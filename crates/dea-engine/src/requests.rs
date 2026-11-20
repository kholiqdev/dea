//! Sends the requests of a project: asks before anything goes to a
//! production environment, opens the Vault when a secret is needed, and
//! keeps a cookie jar per project and environment (decisions 12 and 14).
//!
//! [`RequestService::prepare`] does everything up to the network and may
//! show dialogs, so it blocks. [`RequestService::send`] is the network
//! part.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use dea_http::{Body, ClientIdentity, Options, Part, PartValue, Request, SendContext, TlsConfig};
use dea_project::{
    FieldValue, LocalFiles, Project, RequestContext, RequestError, RequestFile, RequestTree,
    Resolved, ResolvedBody, ResolvedRequest, ResolvedTls, SecretLookup, SecretLookupError,
    SystemDynamicSource, is_local_ref, resolve_draft, resolve_request,
};
use url::Url;
use zeroize::Zeroizing;

use crate::error::UseError;
use crate::inspection::Inspection;
use crate::prompt::Prompter;
use crate::store::{Store, StoreError};
use crate::vault::{VaultService, is_production, production_reason};

pub use crate::prompt::{SendAnswer, SendConfirmation};
pub use dea_http::{
    CancellationToken, Client, ClientConfig, CookieJar, Event, HttpError, Response,
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

/// What a send that reached the server gives back.
#[derive(Debug)]
pub struct Sent {
    pub response: Response,
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
    jars: Mutex<HashMap<Key, Arc<CookieJar>>>,
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
            jars: Mutex::default(),
            unsaved: Mutex::default(),
            saving: Arc::default(),
        }
    }

    /// Resolves the request at `path` for `environment` (`None` is "No
    /// environment"), in this order:
    ///
    /// 1. a preview that never touches the Vault, for the method, the URL
    ///    and the secrets it needs;
    /// 2. for a production environment, the confirmation, unless the
    ///    person stopped the asking for this `GET`, `HEAD` or `OPTIONS`;
    /// 3. when secrets are used, production authentication, or for another
    ///    environment an unlock of a locked Vault;
    /// 4. when secrets are used, the real resolution with the Vault's
    ///    values; otherwise the preview is the request.
    ///
    /// The Vault is opened only at step 3, so a request without secrets
    /// never reaches the keychain. Blocks: steps 2 and 3 may show dialogs.
    /// Nothing is sent.
    ///
    /// # Errors
    ///
    /// [`SendError::NotConfirmed`] when the person cancels; nothing was
    /// read from the Vault then. [`SendError::Secret`] when the Vault
    /// cannot be opened, [`SendError::Request`] when the request cannot be
    /// resolved, [`SendError::Unresolved`] or [`SendError::InvalidUrl`]
    /// for a URL that cannot be sent to, [`SendError::File`] for a
    /// certificate that cannot be read.
    pub fn prepare(
        &self,
        project: &Project,
        tree: &RequestTree,
        path: &Path,
        environment: Option<&str>,
    ) -> Result<PreparedRequest, SendError> {
        let scope = project.vault_scope();
        let preview = resolve(&self.store, project, tree, path, environment, &Placeholders)?;
        let url = with_scheme(&preview.url).redacted;
        let production = environment.filter(|env| is_production(project, env));
        if let Some(env) = production {
            self.confirm(&scope, env, &preview.method, &url)?;
        }

        let names = preview.secret_names();
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
            let lookup = vault.lookup(project);
            resolve(&self.store, project, tree, path, environment, &lookup)?
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
        })
    }

    /// What the editor shows about `draft`, as if it were saved at `path`:
    /// each variable with its value and source, the placeholders with no
    /// value, the headers Dea adds, and the options that apply. Secrets are
    /// placeholders: the Vault is not read and nothing is asked.
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
            &Placeholders,
            &AnyLocalFile,
            |ctx| resolve_draft(path, draft, ctx),
            tree,
        )?;
        let url = with_scheme(&preview.url).text;
        Ok(Inspection::new(&preview, &url, self.client.user_agent()))
    }

    /// Sends a prepared request with the cookies of its environment, then
    /// saves the jar if the response changed it or an earlier save failed.
    /// `events` hears the response as it arrives.
    ///
    /// A response is never lost to a failed save: it comes back with
    /// [`Sent::cookies_not_saved`] set, and the cookies stay in use.
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
            request, jar: key, ..
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
        Ok(Sent {
            response: result?,
            cookies_not_saved,
        })
    }

    /// The cookie jar of `environment` of `project`, for the cookie
    /// manager. Changes to it are saved by the next send that uses it.
    pub fn cookies(&self, project: &Project, environment: Option<&str>) -> Arc<CookieJar> {
        self.jar(&project.vault_scope(), environment.unwrap_or_default())
    }

    /// Forgets the loaded jars, the unsaved ones included, and the
    /// production choices of project `scope`, after its local data was
    /// cleared with [`Store::clear_project`]. A send still running then
    /// does not save its jar again.
    pub fn forget_project(&self, scope: &str) {
        guard(&self.jars).retain(|(s, _), _| s != scope);
        guard(&self.unsaved).retain(|(s, _)| s != scope);
        guard(&self.skip).retain(|(s, _)| s != scope);
    }

    /// Asks before a request goes to production `environment`.
    fn confirm(
        &self,
        scope: &str,
        environment: &str,
        method: &str,
        url: &str,
    ) -> Result<(), SendError> {
        let can_skip = SKIPPABLE.contains(&method);
        let key = (scope.to_string(), environment.to_string());
        if can_skip && guard(&self.skip).contains(&key) {
            return Ok(());
        }
        let confirmation = SendConfirmation {
            method: method.to_string(),
            url: url.to_string(),
            environment: environment.to_string(),
            can_skip,
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
    /// time.
    fn jar(&self, scope: &str, environment: &str) -> Arc<CookieJar> {
        let key = (scope.to_string(), environment.to_string());
        if let Some(jar) = guard(&self.jars).get(&key) {
            return Arc::clone(jar);
        }
        // Read without the lock; if another thread got there first, its
        // jar is the one kept.
        let loaded = Arc::new(self.store.load_cookies(scope, environment));
        Arc::clone(guard(&self.jars).entry(key).or_insert(loaded))
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
    /// thread.
    async fn save(&self, key: Key, jar: Arc<CookieJar>) -> Result<(), StoreError> {
        let current = guard(&self.jars)
            .get(&key)
            .is_some_and(|cached| Arc::ptr_eq(cached, &jar));
        if !current {
            return Ok(());
        }
        let store = Arc::clone(&self.store);
        let saving = Arc::clone(&self.saving);
        let task = tokio::task::spawn_blocking(move || {
            let _one_at_a_time = guard(&saving);
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
    environment: Option<&str>,
    secrets: &dyn SecretLookup,
) -> Result<ResolvedRequest, RequestError> {
    let local_files = store.local_files(&project.vault_scope());
    resolve_with(
        project,
        environment,
        secrets,
        &local_files,
        |ctx| resolve_request(path, ctx),
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
