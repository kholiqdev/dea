//! Sends the requests of a project: asks before anything goes to a
//! production environment, opens the Vault when a secret is needed, and
//! keeps a cookie jar per project and environment (decisions 12 and 14).
//!
//! [`RequestService::prepare`] takes the draft to send, saved or not, like
//! [`RequestService::inspect`], does everything up to the network and may
//! show dialogs, so it blocks. [`RequestService::send`] is the network
//! part.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use dea_http::{Body, ClientIdentity, Options, Part, PartValue, Request, SendContext, TlsConfig};
use dea_project::{
    FieldValue, HIDDEN, LocalFiles, Project, RequestContext, RequestError, RequestFile,
    RequestTree, Resolved, ResolvedBody, ResolvedRequest, ResolvedTls, SecretLookup,
    SecretLookupError, SystemDynamicSource, VaultManifest, encode_query_value, is_local_ref,
    resolve_draft,
};
use url::Url;
use zeroize::Zeroizing;

use crate::error::UseError;
use crate::exchange::CookieInfo;
use crate::inspection::Inspection;
use crate::prompt::Prompter;
use crate::store::{Store, StoreError};
use crate::vault::{VaultService, is_production, production_reason};

pub use crate::prompt::{SendAnswer, SendConfirmation};
pub use dea_http::{
    CancellationToken, Client, ClientConfig, CookieJar, Event, HttpError, Response, TlsError,
};

/// The methods whose confirmation may be skipped for the rest of the
/// session. Compared as written: HTTP methods are case-sensitive.
const SKIPPABLE: [&str; 3] = ["GET", "HEAD", "OPTIONS"];

/// A project scope and an environment name, `""` for "No environment".
type Key = (String, String);

fn guard<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Why a request was not sent. Messages never hold a secret value.
#[derive(Debug, thiserror::Error)]
pub enum SendError {
    /// The request goes to production and the person did not confirm it.
    #[error("the request to production was not confirmed")]
    NotConfirmed,
    #[error(transparent)]
    Request(#[from] RequestError),
    /// The URL cannot be used, and these placeholders have no value.
    #[error("the URL is not complete: no value for {}", .0.join(", "))]
    Unresolved(Vec<String>),
    /// The URL cannot be used; it holds the redacted URL.
    #[error("{0} is not a URL Dea can send to")]
    InvalidUrl(String),
    #[error(transparent)]
    Secret(#[from] UseError),
    #[error("could not read {}: {message}", path.display())]
    File { path: PathBuf, message: String },
    #[error(transparent)]
    Http(#[from] HttpError),
}

/// What a send that reached the server gives back. Every secret value the
/// request used is shown as `••••••••` in `url`, in `response.url` and in
/// the URLs of `response.redirects`.
#[derive(Debug)]
pub struct Sent {
    pub response: Response,
    /// The URL of the final hop.
    pub url: String,
    /// Cookies from this exchange that could not be saved; they stay in
    /// memory and are saved again with the next send.
    pub cookies_not_saved: Option<StoreError>,
}

/// A request resolved, confirmed and authorized, ready for
/// [`RequestService::send`].
pub struct PreparedRequest {
    pub method: String,
    /// The URL as sent, every secret shown as `[secret:<name>]`.
    pub url_redacted: String,
    pub environment: Option<String>,
    /// Placeholders with no value; they go out as written.
    pub unresolved: Vec<String>,
    request: Request,
    /// The jar to use, `None` when the request has cookies off.
    jar: Option<Key>,
    /// What `send` hides in what it reports.
    secrets: SecretValues,
}

/// Shows the redacted URL only: the request itself holds secrets.
impl fmt::Debug for PreparedRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedRequest")
            .field("method", &self.method)
            .field("url_redacted", &self.url_redacted)
            .field("environment", &self.environment)
            .field("unresolved", &self.unresolved)
            .field("cookies", &self.jar.is_some())
            .finish_non_exhaustive()
    }
}

/// Gives a [`RequestService`] the Vault when a send first needs it, so the
/// service can exist before the Vault is opened: opening it reaches the
/// keychain, which starting Dea or inspecting a request never does.
pub trait VaultProvider: Send + Sync {
    /// The Vault, opened the first time.
    ///
    /// # Errors
    ///
    /// [`UseError::Vault`] when the Vault cannot be opened.
    fn open(&self) -> Result<Arc<VaultService>, UseError>;
}

/// A Vault that is already open.
impl VaultProvider for Arc<VaultService> {
    fn open(&self) -> Result<Arc<VaultService>, UseError> {
        Ok(Arc::clone(self))
    }
}

/// Sends project requests. One per app, shared by every window.
pub struct RequestService {
    client: Client,
    store: Arc<Store>,
    vault: Box<dyn VaultProvider>,
    prompter: Arc<dyn Prompter>,
    /// Production environments whose `GET`, `HEAD` and `OPTIONS` requests
    /// go out without asking, until Dea quits.
    skip: Mutex<HashSet<Key>>,
    /// The jars loaded so far.
    jars: Arc<Mutex<HashMap<Key, Arc<CookieJar>>>>,
    /// Jars whose last save failed, saved again after the next send.
    unsaved: Mutex<HashSet<Key>>,
    /// Held while a jar is written, so the newest save is the last one.
    saving: Arc<Mutex<()>>,
}

impl RequestService {
    /// A service that opens the Vault with `vault` only when a send needs
    /// it, never to inspect a request.
    #[must_use]
    pub fn new(
        client: Client,
        store: Arc<Store>,
        vault: impl VaultProvider + 'static,
        prompter: Arc<dyn Prompter>,
    ) -> RequestService {
        RequestService {
            client,
            store,
            vault: Box::new(vault),
            prompter,
            skip: Mutex::default(),
            jars: Arc::default(),
            unsaved: Mutex::default(),
            saving: Arc::default(),
        }
    }

    /// Resolves `draft`, as if it were saved at `path`, for `environment`
    /// (`None` is "No environment"), in this order:
    ///
    /// 1. a preview of the draft, resolved the way `inspect` does, that
    ///    never touches the Vault, for the method, the URL and the secrets
    ///    it needs;
    /// 2. for a production environment, the confirmation, unless the
    ///    person stopped the asking for this `GET`, `HEAD` or `OPTIONS`;
    ///    [`SendConfirmation::secrets`] tells whether the request uses any
    ///    Vault secret;
    /// 3. when secrets are used, production authentication, or for another
    ///    environment an unlock of a locked Vault;
    /// 4. when secrets are used, the real resolution with the Vault's
    ///    values; otherwise the preview is the request.
    ///
    /// `path` places the draft among the folders it inherits from; the
    /// file there, if any, is not read. The Vault is opened only at step 3,
    /// so a request without secrets never reaches the keychain. Blocks:
    /// steps 2 and 3 may show dialogs. Nothing is sent.
    ///
    /// # Errors
    ///
    /// [`SendError::NotConfirmed`] when the person cancels; nothing was
    /// read from the Vault then. [`SendError::Secret`] when the Vault
    /// cannot be opened, [`SendError::Request`] when the draft cannot be
    /// resolved, [`SendError::Unresolved`] or [`SendError::InvalidUrl`]
    /// for a URL that cannot be sent to, [`SendError::File`] for a
    /// certificate that cannot be read.
    pub fn prepare(
        &self,
        project: &Project,
        tree: &RequestTree,
        path: &Path,
        draft: &RequestFile,
        environment: Option<&str>,
    ) -> Result<PreparedRequest, SendError> {
        let scope = project.vault_scope();
        let preview = resolve(
            &self.store,
            project,
            tree,
            path,
            draft,
            environment,
            &Placeholders,
        )?;
        let url = with_scheme(&preview.url).redacted;
        let production = environment.filter(|env| is_production(project, env));
        let names = preview.secret_names();
        if let Some(env) = production {
            self.confirm(
                &scope,
                env,
                &preview.method,
                &url,
                !names.is_empty(),
                &preview.unresolved,
            )?;
        }

        let mut secrets = SecretValues::default();
        let resolved = if names.is_empty() {
            // Placeholders only stand in for secrets, so without any the
            // preview is the request, and the Vault stays closed.
            preview
        } else {
            // Opened only now, so a cancelled confirmation leaves it
            // untouched.
            let vault = self.vault.open()?;
            let action = format!(
                "Send {} {} to {}",
                preview.method,
                url_path(&url),
                environment.unwrap_or_default()
            );
            if production.is_some() {
                vault.authorize_production(&action, &names)?;
            } else if vault.status().locked {
                vault.unlock(&production_reason(&action, &names))?;
            }
            let vault_lookup = vault.lookup(project);
            let lookup = Remembering {
                inner: &vault_lookup,
                values: RefCell::default(),
            };
            let resolved = resolve(
                &self.store,
                project,
                tree,
                path,
                draft,
                environment,
                &lookup,
            )?;
            secrets = SecretValues::new(lookup.values.into_inner());
            resolved
        };
        let jar = resolved
            .options
            .cookies
            .then(|| (scope, environment.unwrap_or_default().to_string()));
        if let Some((scope, env)) = &jar {
            // Read now, while blocking is fine, rather than in `send`.
            self.jar(scope, env);
        }
        let url_redacted = with_scheme(&resolved.url).redacted;
        let (request, unresolved) = to_http(resolved)?;
        Ok(PreparedRequest {
            method: request.method.clone(),
            url_redacted,
            environment: environment.map(str::to_string),
            unresolved,
            request,
            jar,
            secrets,
        })
    }

    /// What the editor shows about `draft`, as if it were saved at `path`:
    /// each variable with its value and source, the placeholders with no
    /// value, the headers Dea adds, and the options that apply. Secrets are
    /// placeholders: the Vault is not read and nothing is asked. A Vault
    /// name `vault.yaml` does not declare has no value.
    ///
    /// # Errors
    ///
    /// [`SendError::Request`] when the draft cannot be resolved, for
    /// example a variable that refers to itself or a folder whose
    /// `_folder.yaml` cannot be read.
    pub fn inspect(
        &self,
        project: &Project,
        tree: &RequestTree,
        path: &Path,
        draft: &RequestFile,
        environment: Option<&str>,
    ) -> Result<Inspection, SendError> {
        let preview = resolve_with(
            project,
            environment,
            &Declared(&project.vault),
            &AnyLocalFile,
            |ctx| resolve_draft(path, draft, ctx),
            tree,
        )?;
        let with = with_scheme(&preview.url);
        let shown = if preview.url.text.is_empty() {
            String::new()
        } else {
            hide_secrets(&with.redacted, &preview.secret_names())
        };
        Ok(Inspection::new(
            &preview,
            &with.text,
            shown,
            self.client.user_agent(),
        ))
    }

    /// Sends a prepared request with the cookies of its environment, then
    /// saves the jar if the response changed it or an earlier save failed.
    /// `events` hears the response as it arrives.
    ///
    /// A response is never lost to a failed save: it comes back with
    /// [`Sent::cookies_not_saved`] set, and the cookies stay in use.
    ///
    /// Every secret value the request used is shown as `••••••••` in the
    /// URLs of the response and in the text of an error.
    ///
    /// # Errors
    ///
    /// [`SendError::Http`] when the exchange fails. A save that fails then
    /// is not reported; it is tried again after the next send.
    pub async fn send(
        &self,
        prepared: PreparedRequest,
        cancel: &CancellationToken,
        events: Option<&mut (dyn FnMut(Event) + Send)>,
    ) -> Result<Sent, SendError> {
        let PreparedRequest {
            request,
            jar: key,
            secrets,
            ..
        } = prepared;
        let jar = key.as_ref().map(|(scope, env)| self.jar(scope, env));
        // The client takes a shared callback; the lock is only held while
        // one event is handed over.
        let events = events.map(Mutex::new);
        let forward = |event: Event| {
            if let Some(sink) = &events {
                let mut sink = guard(sink);
                (*sink)(event);
            }
        };
        let mut ctx = SendContext::new(cancel);
        if events.is_some() {
            ctx = ctx.events(&forward);
        }
        if let Some(jar) = &jar {
            ctx = ctx.cookies(jar);
        }
        let result = self.client.send(request, ctx).await;
        let cookies_not_saved = match (key, jar) {
            (Some(key), Some(jar)) => self.save_pending(key, jar).await,
            _ => None,
        };
        let mut response = result.map_err(|e| SendError::Http(secrets.hide_error(e)))?;
        for redirect in &mut response.redirects {
            redirect.url = secrets.hide(&redirect.url);
        }
        response.url = secrets.hide(&response.url);
        Ok(Sent {
            url: response.url.clone(),
            response,
            cookies_not_saved,
        })
    }

    /// The cookie jar of `environment` of `project`. Changes made to it
    /// directly are saved by the next send that uses it; to change cookies
    /// and save them at once, use [`RequestService::change_cookies`].
    pub fn cookies(&self, project: &Project, environment: Option<&str>) -> Arc<CookieJar> {
        self.jar(&project.vault_scope(), environment.unwrap_or_default())
    }

    /// The unexpired cookies of `environment` of `project`, sorted by
    /// domain, path and name.
    pub fn list_cookies(&self, project: &Project, environment: Option<&str>) -> Vec<CookieInfo> {
        self.cookies(project, environment)
            .list()
            .iter()
            .map(CookieInfo::from)
            .collect()
    }

    /// Runs `change` on the jar of `environment` of `project`, and saves
    /// the jar at once when `change` altered it. Held under the same lock
    /// as every other save and as [`RequestService::clear_project`].
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the jar could not be saved. The change stays in
    /// memory and is saved again by the next send.
    pub fn change_cookies<R>(
        &self,
        project: &Project,
        environment: Option<&str>,
        change: impl FnOnce(&CookieJar) -> R,
    ) -> Result<R, StoreError> {
        let key = (
            project.vault_scope(),
            environment.unwrap_or_default().to_string(),
        );
        let _saving = guard(&self.saving);
        let jar = self.cached_jar(&key);
        let result = change(&jar);
        if jar.take_changed() || guard(&self.unsaved).contains(&key) {
            match self.store.save_cookies(&key.0, &key.1, &jar) {
                Ok(()) => {
                    guard(&self.unsaved).remove(&key);
                }
                Err(e) => {
                    guard(&self.unsaved).insert(key);
                    return Err(e);
                }
            }
        }
        Ok(result)
    }

    /// Clears the local data of project `scope` with `clear`, normally
    /// [`Store::clear_project`], and forgets what was loaded from it: the
    /// jars, the unsaved ones included, and the production choices.
    ///
    /// Everything runs under the lock that every jar save and load takes,
    /// so a send that finishes meanwhile either saved before the clear
    /// (and the clear removed the file) or finds its jar forgotten and
    /// writes nothing.
    ///
    /// # Errors
    ///
    /// What `clear` returns.
    pub fn clear_project(
        &self,
        scope: &str,
        clear: impl FnOnce() -> Result<(), StoreError>,
    ) -> Result<(), StoreError> {
        let _saving = guard(&self.saving);
        guard(&self.jars).retain(|(s, _), _| s != scope);
        guard(&self.unsaved).retain(|(s, _)| s != scope);
        guard(&self.skip).retain(|(s, _)| s != scope);
        clear()
    }

    /// Asks before a request goes to production `environment`.
    fn confirm(
        &self,
        scope: &str,
        environment: &str,
        method: &str,
        url: &str,
        secrets: bool,
        unresolved: &[String],
    ) -> Result<(), SendError> {
        // A placeholder with no value is never waved through: the request
        // would go out half written.
        let can_skip = SKIPPABLE.contains(&method) && unresolved.is_empty();
        let key = (scope.to_string(), environment.to_string());
        if can_skip && guard(&self.skip).contains(&key) {
            return Ok(());
        }
        let confirmation = SendConfirmation {
            method: method.to_string(),
            url: url.to_string(),
            host: host_of(url).to_string(),
            environment: environment.to_string(),
            undefined: unresolved.to_vec(),
            can_skip,
            secrets,
        };
        match self.prompter.confirm_send(&confirmation) {
            SendAnswer::Cancel => Err(SendError::NotConfirmed),
            SendAnswer::Send => Ok(()),
            SendAnswer::SendAndStopAsking => {
                if can_skip {
                    guard(&self.skip).insert(key);
                }
                Ok(())
            }
        }
    }

    /// The jar of `scope` and `environment`, read from the store the first
    /// time. The read waits for [`RequestService::clear_project`], so it
    /// never caches a jar older than a clear.
    fn jar(&self, scope: &str, environment: &str) -> Arc<CookieJar> {
        let key = (scope.to_string(), environment.to_string());
        if let Some(jar) = guard(&self.jars).get(&key) {
            return Arc::clone(jar);
        }
        let _saving = guard(&self.saving);
        self.cached_jar(&key)
    }

    /// Like `jar`, for a caller that holds `saving`.
    fn cached_jar(&self, key: &Key) -> Arc<CookieJar> {
        if let Some(jar) = guard(&self.jars).get(key) {
            return Arc::clone(jar);
        }
        let loaded = Arc::new(self.store.load_cookies(&key.0, &key.1));
        guard(&self.jars).insert(key.clone(), Arc::clone(&loaded));
        loaded
    }

    /// Saves `jar` when the exchange changed it or its last save failed,
    /// and remembers a failure so the next send tries again. `take_changed`
    /// cannot be undone, hence the set.
    async fn save_pending(&self, key: Key, jar: Arc<CookieJar>) -> Option<StoreError> {
        if !jar.take_changed() && !guard(&self.unsaved).contains(&key) {
            return None;
        }
        match self.save(key.clone(), jar).await {
            Ok(()) => {
                guard(&self.unsaved).remove(&key);
                None
            }
            Err(e) => {
                guard(&self.unsaved).insert(key);
                Some(e)
            }
        }
    }

    /// Writes `jar`, unless its project's local data was cleared since it
    /// was loaded. The write waits for the disk, so it runs on a blocking
    /// thread, and checks that the jar is still the loaded one only once it
    /// holds `saving`, so a clear cannot slip in between.
    async fn save(&self, key: Key, jar: Arc<CookieJar>) -> Result<(), StoreError> {
        let store = Arc::clone(&self.store);
        let saving = Arc::clone(&self.saving);
        let jars = Arc::clone(&self.jars);
        let task = tokio::task::spawn_blocking(move || {
            let _one_at_a_time = guard(&saving);
            let current = guard(&jars)
                .get(&key)
                .is_some_and(|cached| Arc::ptr_eq(cached, &jar));
            if !current {
                return Ok(());
            }
            store.save_cookies(&key.0, &key.1, &jar)
        });
        match task.await {
            Ok(saved) => saved,
            Err(e) => match e.try_into_panic() {
                Ok(panic) => std::panic::resume_unwind(panic),
                // The runtime is shutting down: Dea is quitting.
                Err(_) => Ok(()),
            },
        }
    }
}

/// Stands in for the Vault before the person confirmed: every secret
/// reads as `[secret:<name>]`, and the Vault is never asked.
struct Placeholders;

impl SecretLookup for Placeholders {
    fn lookup(&self, _environment: &str, name: &str) -> Result<Option<String>, SecretLookupError> {
        Ok(Some(format!("[secret:{name}]")))
    }
}

/// Reads secrets from `inner` and keeps each value it gives, so `send` can
/// hide them.
struct Remembering<'a> {
    inner: &'a dyn SecretLookup,
    values: RefCell<Vec<Zeroizing<String>>>,
}

impl SecretLookup for Remembering<'_> {
    fn lookup(&self, environment: &str, name: &str) -> Result<Option<String>, SecretLookupError> {
        let value = self.inner.lookup(environment, name)?;
        if let Some(value) = &value {
            self.values.borrow_mut().push(Zeroizing::new(value.clone()));
        }
        Ok(value)
    }
}

/// The secret values a request uses, as written, as a query parameter
/// writes them, and as the URL parser writes them in each part of a URL.
/// Longest first, so a value inside a longer one does not leave the rest
/// of that one showing.
#[derive(Default)]
struct SecretValues(Vec<Zeroizing<String>>);

impl SecretValues {
    fn new(values: Vec<Zeroizing<String>>) -> Self {
        let mut forms: Vec<Zeroizing<String>> = Vec::new();
        for value in &values {
            let mut candidates = vec![value.to_string(), encode_query_value(value)];
            candidates.extend(url_forms(value));
            for form in candidates.into_iter().map(Zeroizing::new) {
                if !form.is_empty() && !forms.contains(&form) {
                    forms.push(form);
                }
            }
        }
        forms.sort_by_key(|form| std::cmp::Reverse(form.len()));
        SecretValues(forms)
    }

    /// `text` with every secret shown as `••••••••`.
    fn hide(&self, text: &str) -> String {
        let mut text = text.to_string();
        for secret in &self.0 {
            if text.contains(secret.as_str()) {
                text = text.replace(secret.as_str(), HIDDEN);
            }
        }
        text
    }

    /// `error` with every secret in its text shown as `••••••••`.
    fn hide_error(&self, error: HttpError) -> HttpError {
        match error {
            HttpError::InvalidRequest(message) => HttpError::InvalidRequest(self.hide(&message)),
            HttpError::Dns { host, message } => HttpError::Dns {
                host: self.hide(&host),
                message: self.hide(&message),
            },
            HttpError::ConnectionRefused { addr } => HttpError::ConnectionRefused {
                addr: self.hide(&addr),
            },
            HttpError::Connect { addr, message } => HttpError::Connect {
                addr: self.hide(&addr),
                message: self.hide(&message),
            },
            HttpError::Tls(TlsError { kind, message }) => HttpError::Tls(TlsError {
                kind,
                message: self.hide(&message),
            }),
            HttpError::Proxy(message) => HttpError::Proxy(self.hide(&message)),
            HttpError::File { path, message } => {
                let shown = path.to_string_lossy();
                let hidden = self.hide(&shown);
                let path = if hidden == shown {
                    path
                } else {
                    PathBuf::from(hidden)
                };
                HttpError::File {
                    path,
                    message: self.hide(&message),
                }
            }
            HttpError::Protocol(message) => HttpError::Protocol(self.hide(&message)),
            // No text: a duration or a count.
            error @ (HttpError::Timeout(_)
            | HttpError::Cancelled
            | HttpError::TooManyRedirects(_)) => error,
        }
    }
}

/// `value` as the URL parser writes it in the userinfo, the path, the
/// query and the fragment, and as a host when it is one: lower case,
/// punycode, IP addresses in their usual form.
fn url_forms(value: &str) -> Vec<String> {
    let mut forms = Vec::new();
    let Ok(mut scratch) = Url::parse("http://h/") else {
        return forms;
    };
    if scratch.set_password(Some(value)).is_ok() {
        forms.extend(scratch.password().map(str::to_string));
    }
    scratch.set_path(value);
    let path = scratch.path();
    // `set_path` adds the leading `/` when `value` has none.
    let path = if value.starts_with('/') {
        path
    } else {
        path.strip_prefix('/').unwrap_or(path)
    };
    forms.push(path.to_string());
    scratch.set_query(Some(value));
    forms.extend(scratch.query().map(str::to_string));
    scratch.set_fragment(Some(value));
    forms.extend(scratch.fragment().map(str::to_string));
    // Only when `value` is a host and nothing else, so `ab/cd` does not
    // add `ab`.
    if let Ok(url) = Url::parse(&format!("http://{value}/"))
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
    {
        forms.extend(url.host_str().map(str::to_string));
    }
    forms
}

/// Stands in for the Vault when inspecting: a secret `vault.yaml` declares
/// reads as `[secret:<name>]`, any other name has no value, so a misspelt
/// reference shows as undefined. The Vault is never asked.
struct Declared<'a>(&'a VaultManifest);

impl SecretLookup for Declared<'_> {
    fn lookup(&self, environment: &str, name: &str) -> Result<Option<String>, SecretLookupError> {
        if self.0.secrets.iter().any(|secret| secret.name == name) {
            Placeholders.lookup(environment, name)
        } else {
            Ok(None)
        }
    }
}

/// Stands in for this device's file map when inspecting: every well-formed
/// `local:` reference has a path, because nothing is read. A file picked on
/// another device is shown elsewhere.
struct AnyLocalFile;

impl LocalFiles for AnyLocalFile {
    fn path(&self, id: &str) -> Option<PathBuf> {
        is_local_ref(id).then(|| PathBuf::from(id))
    }
}

fn resolve(
    store: &Store,
    project: &Project,
    tree: &RequestTree,
    path: &Path,
    draft: &RequestFile,
    environment: Option<&str>,
    secrets: &dyn SecretLookup,
) -> Result<ResolvedRequest, RequestError> {
    let local_files = store.local_files(&project.vault_scope());
    resolve_with(
        project,
        environment,
        secrets,
        &local_files,
        |ctx| resolve_draft(path, draft, ctx),
        tree,
    )
}

/// Runs `resolver` with the context of `project`.
fn resolve_with(
    project: &Project,
    environment: Option<&str>,
    secrets: &dyn SecretLookup,
    local_files: &dyn LocalFiles,
    resolver: impl FnOnce(&mut RequestContext) -> Result<ResolvedRequest, RequestError>,
    tree: &RequestTree,
) -> Result<ResolvedRequest, RequestError> {
    let mut dynamic = SystemDynamicSource;
    let mut ctx = RequestContext {
        project,
        tree,
        environment,
        secrets,
        local_files,
        dynamic: &mut dynamic,
    };
    resolver(&mut ctx)
}

/// `url` with every `[secret:<name>]` for `names` shown as `••••••••`.
fn hide_secrets(url: &str, names: &[&str]) -> String {
    names.iter().fold(url.to_string(), |url, name| {
        url.replace(&format!("[secret:{name}]"), HIDDEN)
    })
}

/// The host of `url` and its port, as written: after `://`, without
/// userinfo, up to the first `/`, `?` or `#`. Empty when there is none.
fn host_of(url: &str) -> &str {
    let Some((_, rest)) = url.split_once("://") else {
        return "";
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    authority.rsplit('@').next().unwrap_or_default()
}

/// `url` with `http://` in front when it has no scheme (decision 16).
fn with_scheme(url: &Resolved) -> Resolved {
    let has_scheme = url.text.split_once("://").is_some_and(|(scheme, _)| {
        let mut chars = scheme.chars();
        chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    });
    if has_scheme {
        return url.clone();
    }
    Resolved {
        text: format!("http://{}", url.text),
        redacted: format!("http://{}", url.redacted),
    }
}

/// The path of `url`, without query or fragment, for the authentication
/// dialog. Works on a URL that does not parse yet.
fn url_path(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = rest.find(['?', '#']).map_or(rest, |end| &rest[..end]);
    rest.find('/').map_or("/", |start| &rest[start..])
}

/// The request for the client, and the placeholders left unresolved.
fn to_http(resolved: ResolvedRequest) -> Result<(Request, Vec<String>), SendError> {
    let ResolvedRequest {
        method,
        url,
        headers,
        auth_header,
        body,
        options,
        tls,
        unresolved,
        ..
    } = resolved;
    let url = with_scheme(&url);
    // `{{name}}` is a valid host to the URL parser, but never one to send
    // to.
    let target = Url::parse(&url.text)
        .ok()
        .filter(|u| u.host_str().is_some_and(|host| !host.contains(['{', '}'])));
    let Some(target) = target else {
        return Err(if unresolved.is_empty() {
            SendError::InvalidUrl(url.redacted)
        } else {
            SendError::Unresolved(unresolved)
        });
    };
    // What auth added and whatever holds a Vault value are credentials,
    // which a redirect to another origin leaves behind.
    let from_vault = |part: &Resolved| part.text != part.redacted;
    let credential_headers = headers
        .iter()
        .filter(|(name, value)| from_vault(name) || from_vault(value))
        .map(|(name, _)| name.text.clone())
        .chain(auth_header)
        .collect();
    let request = Request {
        method,
        url: target,
        headers: headers
            .into_iter()
            .map(|(name, value)| (name.text, value.text))
            .collect(),
        credential_headers,
        body: to_body(body),
        options: Options {
            // `timeoutMs: 0` sets no limit, as in Postman.
            timeout: (options.timeout_ms > 0).then(|| Duration::from_millis(options.timeout_ms)),
            follow_redirects: options.follow_redirects,
            max_redirects: options.max_redirects,
            verify_tls: options.verify_tls,
            use_system_proxy: options.use_system_proxy,
            ..Options::default()
        },
        tls: to_tls(&tls)?,
    };
    Ok((request, unresolved))
}

fn to_body(body: ResolvedBody) -> Body {
    match body {
        ResolvedBody::None => Body::Empty,
        ResolvedBody::Text { text, content_type } => Body::Bytes {
            data: text.text.into(),
            content_type: Some(content_type.to_string()),
        },
        ResolvedBody::Urlencoded(fields) => Body::Form(
            fields
                .into_iter()
                .map(|(name, value)| (name.text, value.text))
                .collect(),
        ),
        ResolvedBody::FormData(fields) => Body::Multipart(
            fields
                .into_iter()
                .map(|field| Part {
                    name: field.name.text,
                    value: match field.value {
                        FieldValue::Text(text) => PartValue::Text(text.text),
                        FieldValue::File { path, content_type } => {
                            PartValue::File { path, content_type }
                        }
                    },
                })
                .collect(),
        ),
        ResolvedBody::File { path, content_type } => Body::File {
            path,
            content_type: Some(content_type.to_string()),
        },
    }
}

/// Reads the CA and client certificate files; the key comes from the Vault.
fn to_tls(tls: &ResolvedTls) -> Result<TlsConfig, SendError> {
    let read = |path: &Path| {
        fs::read(path).map_err(|e| SendError::File {
            path: path.to_path_buf(),
            message: e.to_string(),
        })
    };
    let mut config = TlsConfig::default();
    for path in &tls.ca_certificates {
        config.extra_roots_pem.push(read(path)?);
    }
    if let Some(client) = &tls.client_certificate {
        config.client_identity = Some(ClientIdentity {
            certificate_pem: read(&client.certificate)?,
            key_pem: Zeroizing::new(client.key.text.as_bytes().to_vec()),
        });
    }
    Ok(config)
}
