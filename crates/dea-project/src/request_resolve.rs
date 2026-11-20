//! Turns a request file into what is sent: every `{{...}}` placeholder
//! replaced, with the settings of its folders applied.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

use crate::local_files::{LocalFiles, is_local_ref};
use crate::request::{ApiKeyLocation, Auth, Body, KeyValue, RawLanguage, RequestFile, SendOptions};
use crate::{
    DynamicSource, Folder, Project, RequestTree, ResolveContext, ResolveError, Scope, ScopeKind,
    SecretLookup, SecretLookupError, UsedVariable, ValueSource, Variable, resolve,
};

pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
pub const DEFAULT_MAX_REDIRECTS: u32 = 10;

/// What a query value must not contain as written. `%` is left alone so a
/// value that is already encoded is not encoded twice.
const QUERY: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'&')
    .add(b'=')
    .add(b'#')
    .add(b'+')
    .add(b'"')
    .add(b'<')
    .add(b'>');

/// What the request needs to resolve.
pub struct RequestContext<'a> {
    pub project: &'a Project,
    pub tree: &'a RequestTree,
    /// `None` is "No environment": Vault references stay unresolved and the
    /// Vault is not read.
    pub environment: Option<&'a str>,
    pub secrets: &'a dyn SecretLookup,
    /// The files this device picked from outside the project.
    pub local_files: &'a dyn LocalFiles,
    pub dynamic: &'a mut dyn DynamicSource,
}

/// A resolved text and the same text safe to show.
#[derive(Clone, PartialEq)]
pub struct Resolved {
    /// The text to send.
    pub text: String,
    /// With every secret value replaced by `[secret:<name>]`.
    pub redacted: String,
}

impl Resolved {
    fn plain(text: &str) -> Self {
        Resolved {
            text: text.to_string(),
            redacted: text.to_string(),
        }
    }

    fn map(&self, f: impl Fn(&str) -> String) -> Self {
        Resolved {
            text: f(&self.text),
            redacted: f(&self.redacted),
        }
    }
}

impl fmt::Debug for Resolved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.redacted, f)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedField {
    pub name: Resolved,
    pub value: FieldValue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    Text(Resolved),
    File {
        path: PathBuf,
        content_type: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedBody {
    None,
    Text {
        text: Resolved,
        content_type: &'static str,
    },
    Urlencoded(Vec<(Resolved, Resolved)>),
    FormData(Vec<ResolvedField>),
    File {
        path: PathBuf,
        content_type: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveOptions {
    /// `0` sets no limit.
    pub timeout_ms: u64,
    pub follow_redirects: bool,
    pub max_redirects: u32,
    pub verify_tls: bool,
    pub use_system_proxy: bool,
    pub cookies: bool,
}

impl Default for EffectiveOptions {
    fn default() -> Self {
        EffectiveOptions {
            timeout_ms: DEFAULT_TIMEOUT_MS,
            follow_redirects: true,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            verify_tls: true,
            use_system_proxy: true,
            cookies: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedClientCertificate {
    pub certificate: PathBuf,
    pub key: Resolved,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedTls {
    pub ca_certificates: Vec<PathBuf>,
    pub client_certificate: Option<ResolvedClientCertificate>,
}

/// A request ready to send. Its `Debug` shows redacted text only.
#[derive(Clone, PartialEq)]
pub struct ResolvedRequest {
    pub name: String,
    pub method: String,
    pub url: Resolved,
    pub headers: Vec<(Resolved, Resolved)>,
    /// The name of the header the auth setting added, such as
    /// `Authorization` or an API key header. `None` when it added none.
    pub auth_header: Option<String>,
    pub body: ResolvedBody,
    pub options: EffectiveOptions,
    pub tls: ResolvedTls,
    /// Placeholders and `:name` segments with no value, once each.
    pub unresolved: Vec<String>,
    pub used: Vec<UsedVariable>,
}

impl ResolvedRequest {
    /// Names of the Vault secrets used, in the order they were first used.
    #[must_use]
    pub fn secret_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        for used in &self.used {
            if matches!(used.source, ValueSource::Vault { .. }) && !names.contains(&&*used.name) {
                names.push(&used.name);
            }
        }
        names
    }
}

impl fmt::Debug for ResolvedRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Every `Resolved` inside prints its redacted text only.
        f.debug_struct("ResolvedRequest")
            .field("name", &self.name)
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &self.headers)
            .field("auth_header", &self.auth_header)
            .field("body", &self.body)
            .field("options", &self.options)
            .field("tls", &self.tls)
            .field("unresolved", &self.unresolved)
            .field("used", &self.used)
            .finish()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RequestError {
    #[error("there is no request at {0}")]
    NotFound(PathBuf),
    #[error("the folder {0} has a _folder.yaml that cannot be read: fix it first")]
    BrokenFolder(PathBuf),
    #[error("{0} is outside the project: use a path inside the project folder")]
    FileOutsideProject(String),
    #[error("the file \"{0}\" was picked on another device; choose it again")]
    LocalFileMissing(String),
    #[error(transparent)]
    Resolve(#[from] ResolveError),
}

/// Without an environment there is no Vault to ask.
struct NoSecrets;

impl SecretLookup for NoSecrets {
    fn lookup(&self, _environment: &str, _name: &str) -> Result<Option<String>, SecretLookupError> {
        Ok(None)
    }
}

struct Resolver<'r> {
    scopes: Vec<Scope>,
    environment: &'r str,
    secrets: &'r dyn SecretLookup,
    dynamic: &'r mut dyn DynamicSource,
    unresolved: Vec<String>,
    used: Vec<UsedVariable>,
}

impl Resolver<'_> {
    fn part(&mut self, template: &str) -> Result<Resolved, ResolveError> {
        let mut ctx = ResolveContext {
            scopes: &self.scopes,
            environment: self.environment,
            secrets: self.secrets,
            dynamic: &mut *self.dynamic,
        };
        let resolution = resolve(template, &mut ctx)?;
        for name in resolution.unresolved {
            self.note_unresolved(name);
        }
        for used in resolution.used {
            if !self.used.iter().any(|u| u.same_variable(&used)) {
                self.used.push(used);
            }
        }
        Ok(Resolved {
            text: resolution.text,
            redacted: resolution.redacted_text,
        })
    }

    fn note_unresolved(&mut self, name: String) {
        if !self.unresolved.contains(&name) {
            self.unresolved.push(name);
        }
    }

    fn pair(&mut self, item: &KeyValue) -> Result<(Resolved, Resolved), ResolveError> {
        Ok((self.part(&item.name)?, self.part(&item.value)?))
    }
}

fn scope(kind: ScopeKind, label: String, variables: &[Variable]) -> Scope {
    Scope {
        kind,
        label,
        variables: variables.to_vec(),
    }
}

fn used_rows(rows: &[KeyValue]) -> impl Iterator<Item = &KeyValue> {
    rows.iter()
        .filter(|r| r.enabled && !r.name.trim().is_empty())
}

/// A path relative to the project, made absolute. `..` and absolute paths
/// are refused, and so is a path that symbolic links lead out of the
/// project: a cloned project could otherwise upload `~/.ssh/id_rsa`. A path
/// that does not exist yet is only checked as written; sending it fails.
fn project_path(root: &Path, relative: &str) -> Result<PathBuf, RequestError> {
    let outside = || RequestError::FileOutsideProject(relative.to_string());
    let path = Path::new(relative);
    let inside = path
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        && path.components().any(|c| matches!(c, Component::Normal(_)));
    if !inside {
        return Err(outside());
    }
    let joined = root.join(path);
    if let (Ok(real_root), Ok(real)) = (root.canonicalize(), joined.canonicalize())
        && !real.starts_with(&real_root)
    {
        return Err(outside());
    }
    // The path as written, so a link keeps its own file name.
    Ok(joined)
}

/// The file a request names: a `local:` reference goes through this
/// device's map and is used as it is; anything else is a project path.
fn file_path(
    root: &Path,
    local_files: &dyn LocalFiles,
    value: &str,
) -> Result<PathBuf, RequestError> {
    if is_local_ref(value) {
        return local_files
            .path(value)
            .ok_or_else(|| RequestError::LocalFileMissing(value.to_string()));
    }
    project_path(root, value)
}

fn first_set<T>(values: impl Iterator<Item = Option<T>>) -> Option<T> {
    values.flatten().next()
}

fn effective_options(request: &SendOptions, folders: &[&Folder]) -> EffectiveOptions {
    let chain: Vec<&SendOptions> = std::iter::once(request)
        .chain(folders.iter().rev().map(|f| &f.settings.options))
        .collect();
    let d = EffectiveOptions::default();
    EffectiveOptions {
        timeout_ms: first_set(chain.iter().map(|o| o.timeout_ms)).unwrap_or(d.timeout_ms),
        follow_redirects: first_set(chain.iter().map(|o| o.follow_redirects))
            .unwrap_or(d.follow_redirects),
        max_redirects: first_set(chain.iter().map(|o| o.max_redirects)).unwrap_or(d.max_redirects),
        verify_tls: first_set(chain.iter().map(|o| o.verify_tls)).unwrap_or(d.verify_tls),
        use_system_proxy: first_set(chain.iter().map(|o| o.use_system_proxy))
            .unwrap_or(d.use_system_proxy),
        cookies: first_set(chain.iter().map(|o| o.cookies)).unwrap_or(d.cookies),
    }
}

fn raw_content_type(language: RawLanguage) -> &'static str {
    match language {
        RawLanguage::Text => "text/plain",
        RawLanguage::Xml => "application/xml",
        RawLanguage::Html => "text/html",
        RawLanguage::Javascript => "application/javascript",
    }
}

/// Replaces each `:name` segment of the path (the part after the host).
fn fill_path_variables(
    url: &Resolved,
    r: &mut Resolver,
    variables: &[Variable],
) -> Result<Resolved, ResolveError> {
    let (text, redacted) = (url.text.as_str(), url.redacted.as_str());
    let Some((start, end)) = path_span(text) else {
        return Ok(url.clone());
    };
    let Some((rstart, rend)) = path_span(redacted) else {
        return Ok(url.clone());
    };
    let mut values: Vec<(String, Option<Resolved>)> = Vec::new();
    let mut fill =
        |segment: &str, r: &mut Resolver, text_side: bool| -> Result<String, ResolveError> {
            let Some(name) = segment.strip_prefix(':').filter(|n| !n.is_empty()) else {
                return Ok(segment.to_string());
            };
            if !values.iter().any(|(n, _)| n == name) {
                let value = match variables.iter().find(|v| v.name == name) {
                    Some(v) => Some(r.part(&v.value)?),
                    None => {
                        r.note_unresolved(segment.to_string());
                        None
                    }
                };
                values.push((name.to_string(), value));
            }
            let value = values
                .iter()
                .find(|(n, _)| n == name)
                .and_then(|(_, v)| v.as_ref());
            Ok(match value {
                Some(v) if text_side => v.text.clone(),
                Some(v) => v.redacted.clone(),
                None => segment.to_string(),
            })
        };
    let mut rebuild =
        |whole: &str, s: usize, e: usize, text_side: bool| -> Result<String, ResolveError> {
            let mut out = String::from(&whole[..s]);
            let mut first = true;
            for segment in whole[s..e].split('/') {
                if !first {
                    out.push('/');
                }
                first = false;
                out.push_str(&fill(segment, r, text_side)?);
            }
            out.push_str(&whole[e..]);
            Ok(out)
        };
    Ok(Resolved {
        text: rebuild(text, start, end, true)?,
        redacted: rebuild(redacted, rstart, rend, false)?,
    })
}

/// The byte range of the path in a URL: from the first `/` after the host
/// to the `?` or `#`.
fn path_span(url: &str) -> Option<(usize, usize)> {
    let after_scheme = url.find("://").map_or(0, |i| i + 3);
    let start = after_scheme + url[after_scheme..].find('/')?;
    let end = url[start..]
        .find(['?', '#'])
        .map_or(url.len(), |i| start + i);
    Some((start, end))
}

fn append_query(url: &str, name: &str, value: &str) -> String {
    let (base, fragment) = match url.find('#') {
        Some(i) => url.split_at(i),
        None => (url, ""),
    };
    let separator = if !base.contains('?') {
        "?"
    } else if base.ends_with(['?', '&']) {
        ""
    } else {
        "&"
    };
    format!("{base}{separator}{name}={value}{fragment}")
}

fn encode(value: &str) -> String {
    utf8_percent_encode(value, QUERY).to_string()
}

/// Resolves the request stored at `path`, relative to the project root.
///
/// # Errors
///
/// [`RequestError::NotFound`] when the tree has no such request;
/// [`RequestError::BrokenFolder`] when a folder around it could not be
/// read; [`RequestError::FileOutsideProject`] for a body or certificate
/// path that is absolute, uses `..` or leads out through a symbolic link;
/// [`RequestError::LocalFileMissing`] for a `local:` reference this device
/// has no file for;
/// [`RequestError::Resolve`] for a variable cycle or an unreadable secret.
pub fn resolve_request(
    path: &Path,
    ctx: &mut RequestContext,
) -> Result<ResolvedRequest, RequestError> {
    let location = ctx
        .tree
        .find(path)
        .ok_or_else(|| RequestError::NotFound(path.to_path_buf()))?;
    resolve_draft(path, &location.entry.request, ctx)
}

/// Resolves `draft` as if it were saved at `path`: the folders around
/// `path` apply, but the file itself is not read, and `path` need not
/// exist in the tree yet.
///
/// # Errors
///
/// [`RequestError::NotFound`] when `path` is in no collection; otherwise
/// the errors of [`resolve_request`].
#[allow(clippy::too_many_lines)]
pub fn resolve_draft(
    path: &Path,
    draft: &RequestFile,
    ctx: &mut RequestContext,
) -> Result<ResolvedRequest, RequestError> {
    let folders = path
        .parent()
        .map(|dir| ctx.tree.folder_chain(dir))
        .unwrap_or_default();
    if folders.is_empty() {
        return Err(RequestError::NotFound(path.to_path_buf()));
    }
    if let Some(broken) = folders.iter().find(|f| f.broken) {
        return Err(RequestError::BrokenFolder(broken.dir.clone()));
    }
    let folders = &folders;
    let request = draft;
    let root = &ctx.project.root;
    let local_files = ctx.local_files;

    // Innermost folder first; within a kind the first scope wins.
    let mut scopes = Vec::new();
    for (i, folder) in folders.iter().enumerate().rev() {
        let kind = if i == 0 {
            ScopeKind::Collection
        } else {
            ScopeKind::Folder
        };
        scopes.push(scope(
            kind,
            folder.display_name(),
            &folder.settings.variables,
        ));
    }
    let environment = ctx.environment.unwrap_or("");
    if let Some(name) = ctx.environment
        && let Some(env) = ctx.project.environments.iter().find(|e| e.name == name)
    {
        scopes.push(scope(
            ScopeKind::Environment,
            env.name.clone(),
            &env.variables,
        ));
    }
    let secrets: &dyn SecretLookup = if ctx.environment.is_some() {
        ctx.secrets
    } else {
        &NoSecrets
    };
    let mut r = Resolver {
        scopes,
        environment,
        secrets,
        dynamic: &mut *ctx.dynamic,
        unresolved: Vec::new(),
        used: Vec::new(),
    };

    // URL: text, then `:name` segments, then params.
    let url = r.part(&request.url)?;
    let mut url = fill_path_variables(&url, &mut r, &request.path_variables)?;
    for param in used_rows(&request.params) {
        let (name, value) = r.pair(param)?;
        let name = name.map(encode);
        let value = value.map(encode);
        url = Resolved {
            text: append_query(&url.text, &name.text, &value.text),
            redacted: append_query(&url.redacted, &name.redacted, &value.redacted),
        };
    }

    // Headers: wider first. A level replaces same-named headers from wider
    // levels only; rows within one level all stay, in order.
    let mut headers: Vec<(Resolved, Resolved)> = Vec::new();
    let rows = folders
        .iter()
        .map(|f| &f.settings.headers)
        .chain(std::iter::once(&request.headers));
    for rows in rows {
        let mut level = Vec::new();
        for row in used_rows(rows) {
            level.push(r.pair(row)?);
        }
        headers.retain(|(n, _)| {
            !level
                .iter()
                .any(|(ln, _)| ln.text.eq_ignore_ascii_case(&n.text))
        });
        headers.extend(level);
    }

    // Auth: the request's, else the nearest folder's that is not `inherit`.
    let auth = if request.auth.is_inherit() {
        folders
            .iter()
            .rev()
            .map(|f| &f.settings.auth)
            .find(|a| !a.is_inherit())
            .unwrap_or(&Auth::NoAuth)
    } else {
        &request.auth
    };
    let mut auth_header: Option<(Resolved, Resolved)> = None;
    match auth {
        Auth::Inherit | Auth::NoAuth => {}
        Auth::Bearer { token } => {
            let token = r.part(token)?;
            auth_header = Some((
                Resolved::plain("Authorization"),
                token.map(|t| format!("Bearer {t}")),
            ));
        }
        Auth::Basic { username, password } => {
            let user = r.part(username)?;
            let pass = r.part(password)?;
            let basic = |u: &str, p: &str| format!("Basic {}", STANDARD.encode(format!("{u}:{p}")));
            auth_header = Some((
                Resolved::plain("Authorization"),
                Resolved {
                    text: basic(&user.text, &pass.text),
                    redacted: basic(&user.redacted, &pass.redacted),
                },
            ));
        }
        Auth::ApiKey {
            key,
            value,
            location,
        } => {
            let key = r.part(key)?;
            let value = r.part(value)?;
            match location {
                ApiKeyLocation::Header => auth_header = Some((key, value)),
                ApiKeyLocation::Query => {
                    let (key, value) = (key.map(encode), value.map(encode));
                    url = Resolved {
                        text: append_query(&url.text, &key.text, &value.text),
                        redacted: append_query(&url.redacted, &key.redacted, &value.redacted),
                    };
                }
            }
        }
    }
    let mut added_auth_header = None;
    if let Some((name, value)) = auth_header
        && !headers
            .iter()
            .any(|(n, _)| n.text.eq_ignore_ascii_case(&name.text))
    {
        added_auth_header = Some(name.text.clone());
        headers.push((name, value));
    }

    let body = match &request.body {
        None => ResolvedBody::None,
        Some(Body::Json { text }) => ResolvedBody::Text {
            text: r.part(text)?,
            content_type: "application/json",
        },
        Some(Body::Raw { language, text }) => ResolvedBody::Text {
            text: r.part(text)?,
            content_type: raw_content_type(*language),
        },
        Some(Body::Urlencoded { fields }) => {
            let mut out = Vec::new();
            for field in used_rows(fields) {
                out.push(r.pair(field)?);
            }
            ResolvedBody::Urlencoded(out)
        }
        Some(Body::FormData { fields }) => {
            let mut out = Vec::new();
            for field in fields
                .iter()
                .filter(|f| f.enabled && !f.name.trim().is_empty())
            {
                let name = r.part(&field.name)?;
                let value = match (&field.file, &field.value) {
                    (Some(file), _) => FieldValue::File {
                        path: file_path(root, local_files, file)?,
                        content_type: field.content_type.clone(),
                    },
                    (None, value) => FieldValue::Text(r.part(value.as_deref().unwrap_or(""))?),
                };
                out.push(ResolvedField { name, value });
            }
            ResolvedBody::FormData(out)
        }
        Some(Body::Binary { file }) => ResolvedBody::File {
            path: file_path(root, local_files, file)?,
            content_type: "application/octet-stream",
        },
    };

    // TLS: the nearest folder that sets it.
    let mut tls = ResolvedTls::default();
    if let Some(settings) = folders.iter().rev().find_map(|f| f.settings.tls.as_ref()) {
        for ca in &settings.ca_certificates {
            tls.ca_certificates.push(file_path(root, local_files, ca)?);
        }
        if let Some(cert) = &settings.client_certificate {
            tls.client_certificate = Some(ResolvedClientCertificate {
                certificate: file_path(root, local_files, &cert.certificate)?,
                key: r.part(&cert.key)?,
            });
        }
    }

    Ok(ResolvedRequest {
        name: request.name.clone(),
        method: request.method.clone(),
        url,
        headers,
        auth_header: added_auth_header,
        body,
        options: effective_options(&request.options, folders),
        tls,
        unresolved: r.unresolved,
        used: r.used,
    })
}
