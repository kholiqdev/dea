//! Types of the files under `requests/`: one YAML file per request and a
//! `_folder.yaml` per folder. Field order is the order in the file.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Variable;

fn default_true() -> bool {
    true
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_true(value: &bool) -> bool {
    *value
}

/// One request, stored in `requests/<collection>/.../<name>.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestFile {
    pub name: String,
    /// An HTTP method: `GET`, `POST`, or any other token such as `PURGE`.
    pub method: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_variables: Vec<Variable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Body>,
    #[serde(default, skip_serializing_if = "Auth::is_inherit")]
    pub auth: Auth,
    #[serde(default, skip_serializing_if = "SendOptions::is_empty")]
    pub options: SendOptions,
}

/// A parameter or header. A disabled row stays in the file but is not sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyValue {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "camelCase", deny_unknown_fields)]
pub enum Body {
    Json {
        text: String,
    },
    Raw {
        language: RawLanguage,
        text: String,
    },
    Urlencoded {
        fields: Vec<KeyValue>,
    },
    FormData {
        fields: Vec<FormField>,
    },
    Binary {
        /// Path of the file to send, relative to the project.
        file: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum RawLanguage {
    Text,
    Xml,
    Html,
    Javascript,
}

/// A multipart field: it has either a `value` or a `file`, never both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormField {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Path of the file to upload, relative to the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

/// How a request signs in. `inherit` takes the setting of the nearest
/// folder that has one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Auth {
    #[default]
    Inherit,
    #[serde(rename = "none")]
    NoAuth,
    Bearer {
        token: String,
    },
    Basic {
        username: String,
        password: String,
    },
    ApiKey {
        key: String,
        value: String,
        #[serde(rename = "in")]
        location: ApiKeyLocation,
    },
}

impl Auth {
    #[must_use]
    pub fn is_inherit(&self) -> bool {
        matches!(self, Auth::Inherit)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ApiKeyLocation {
    Header,
    Query,
}

/// Per-request or per-folder send settings. A field left out inherits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_redirects: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_tls: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_system_proxy: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookies: Option<bool>,
}

impl SendOptions {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A folder's settings, stored in its `_folder.yaml`. The file at
/// `requests/_folder.yaml` only sets the order of the collections.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FolderFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// File names (without `.yaml`) and folder names, in display order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<Variable>,
    #[serde(default, skip_serializing_if = "Auth::is_inherit")]
    pub auth: Auth,
    #[serde(default, skip_serializing_if = "SendOptions::is_empty")]
    pub options: SendOptions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsSettings>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TlsSettings {
    /// Paths of extra certificate authorities, relative to the project.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ca_certificates: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_certificate: Option<ClientCertificate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientCertificate {
    /// Path of the certificate, relative to the project.
    pub certificate: String,
    /// A single `{{vault:<name>}}` reference: a private key never lives in
    /// the project folder.
    pub key: String,
}
