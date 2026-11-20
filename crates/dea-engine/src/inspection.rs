//! What the request editor shows about a draft while it is being written:
//! the value and source of each variable, the placeholders with no value,
//! the headers Dea adds, and the connection options that apply. Built from
//! a preview resolution, so no secret value is ever in it.

use dea_http::automatic_headers;
use dea_project::{
    EffectiveOptions, HIDDEN, ResolvedBody, ResolvedRequest, ScopeKind, UsedVariable, ValueSource,
};
use serde::Serialize;
use url::Url;

/// The URL used for the automatic headers while the real one cannot be
/// parsed yet; its host is not shown.
const STAND_IN_URL: &str = "http://localhost/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct Inspection {
    /// Every variable the draft uses, in the order it was first used.
    pub variables: Vec<VariableInfo>,
    /// Placeholders with no value, as written inside the braces.
    pub undefined: Vec<String>,
    pub automatic_headers: Vec<AutomaticHeader>,
    pub options: OptionsInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct VariableInfo {
    /// The text inside `{{…}}`: `vault:<name>` for a Vault reference.
    pub name: String,
    /// The final value, or `••••••••` when it is or holds a secret.
    pub shown: String,
    pub source: String,
    pub secret: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct AutomaticHeader {
    pub name: String,
    /// `None` when the value is only known when the request is sent.
    pub value: Option<String>,
    /// The request has its own header of this name, which is sent instead.
    pub replaced: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct OptionsInfo {
    /// `0` sets no limit.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub timeout_ms: u64,
    pub follow_redirects: bool,
    pub max_redirects: u32,
    pub verify_tls: bool,
    pub use_system_proxy: bool,
    pub cookies: bool,
}

impl From<EffectiveOptions> for OptionsInfo {
    fn from(o: EffectiveOptions) -> Self {
        OptionsInfo {
            timeout_ms: o.timeout_ms,
            follow_redirects: o.follow_redirects,
            max_redirects: o.max_redirects,
            verify_tls: o.verify_tls,
            use_system_proxy: o.use_system_proxy,
            cookies: o.cookies,
        }
    }
}

impl VariableInfo {
    fn new(used: &UsedVariable) -> Self {
        let (name, source) = match &used.source {
            ValueSource::Scope { kind, label } => (
                used.name.clone(),
                match kind {
                    ScopeKind::Request => "This request".to_string(),
                    ScopeKind::Folder => format!("Folder {label}"),
                    ScopeKind::Collection => format!("Collection {label}"),
                    ScopeKind::Environment => format!("Environment {label}"),
                    ScopeKind::Global => "Global".to_string(),
                },
            ),
            ValueSource::Vault { environment, name } => (
                format!("vault:{name}"),
                format!("Vault, {environment}. The value stays hidden"),
            ),
            ValueSource::Dynamic(name) => (name.clone(), "Dynamic, new on each send".to_string()),
        };
        VariableInfo {
            name,
            // Never more than the resolver shows: secrets are already hidden.
            shown: if used.secret {
                HIDDEN.to_string()
            } else {
                used.shown.clone()
            },
            source,
            secret: used.secret,
        }
    }
}

/// Whether the connection sends any bytes for `body`. A form always does:
/// it ends with its closing boundary.
fn has_body(body: &ResolvedBody) -> bool {
    match body {
        ResolvedBody::None => false,
        ResolvedBody::Text { text, .. } => !text.text.is_empty(),
        ResolvedBody::Urlencoded(fields) => !fields.is_empty(),
        ResolvedBody::FormData(_) | ResolvedBody::File { .. } => true,
    }
}

/// The `Content-Type` the connection adds for `body`.
fn content_type(body: &ResolvedBody) -> Option<&str> {
    match body {
        ResolvedBody::None => None,
        ResolvedBody::Text { content_type, .. } | ResolvedBody::File { content_type, .. } => {
            Some(content_type)
        }
        ResolvedBody::Urlencoded(_) => Some("application/x-www-form-urlencoded"),
        // The boundary is chosen when the request is sent.
        ResolvedBody::FormData(_) => Some("multipart/form-data"),
    }
}

impl Inspection {
    /// `resolved` is a preview resolution of the draft, `url` its URL with a
    /// scheme. `user_agent` is the one the client sends.
    pub(crate) fn new(resolved: &ResolvedRequest, url: &str, user_agent: &str) -> Inspection {
        // While the URL is still being typed the host is unknown, so the
        // `Host` value is left to be worked out on send.
        let parsed = Url::parse(url)
            .ok()
            .filter(|u| u.host_str().is_some_and(|host| !host.contains(['{', '}'])));
        let known = parsed.is_some();
        let target = parsed.unwrap_or_else(|| Url::parse(STAND_IN_URL).expect("a valid URL"));
        let automatic_headers = automatic_headers(
            &target,
            content_type(&resolved.body),
            has_body(&resolved.body),
            user_agent,
        )
        .into_iter()
        .map(|(name, value)| AutomaticHeader {
            replaced: resolved
                .headers
                .iter()
                .any(|(own, _)| own.text.eq_ignore_ascii_case(name)),
            value: if name == "Host" && !known {
                None
            } else {
                value
            },
            name: name.to_string(),
        })
        .collect();
        Inspection {
            variables: resolved.used.iter().map(VariableInfo::new).collect(),
            undefined: resolved.unresolved.clone(),
            automatic_headers,
            options: resolved.options.into(),
        }
    }
}
