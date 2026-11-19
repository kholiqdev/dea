//! Types of the files under `requests/`: one YAML file per request and a
//! `_folder.yaml` per folder. Field order is the order in the file.

use std::fmt;

use schemars::JsonSchema;
use serde::de::{Deserializer, Visitor};
use serde::{Deserialize, Serialize};

use crate::Variable;

fn default_true() -> bool {
    true
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_true(value: &bool) -> bool {
    *value
}

/// Reads any YAML scalar as its text: `20` as `"20"`, `true` as `"true"`
/// and an empty value as `""`. The text fields inside `body` and `auth`
/// need it: serde reads those through a buffer that has already turned
/// unquoted scalars into numbers and booleans. A decimal integer comes back
/// as written; a float, or a hex or octal integer, comes back in its plain
/// decimal form (`1.50` as `1.5`): quote it to keep it as written.
fn scalar_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    deserializer.deserialize_any(ScalarText)
}

/// [`scalar_text`] for a field that may be left out.
fn some_scalar_text<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    scalar_text(deserializer).map(Some)
}

struct ScalarText;

impl<'de> Visitor<'de> for ScalarText {
    type Value = String;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a string, number or boolean")
    }

    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
        Ok(value.to_owned())
    }

    fn visit_string<E: serde::de::Error>(self, value: String) -> Result<String, E> {
        Ok(value)
    }

    fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<String, E> {
        Ok(value.to_string())
    }

    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<String, E> {
        Ok(value.to_string())
    }

    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<String, E> {
        Ok(value.to_string())
    }

    fn visit_i128<E: serde::de::Error>(self, value: i128) -> Result<String, E> {
        Ok(value.to_string())
    }

    fn visit_u128<E: serde::de::Error>(self, value: u128) -> Result<String, E> {
        Ok(value.to_string())
    }

    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<String, E> {
        Ok(value.to_string())
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<String, E> {
        Ok(String::new())
    }

    fn visit_none<E: serde::de::Error>(self) -> Result<String, E> {
        Ok(String::new())
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<String, D::Error> {
        scalar_text(deserializer)
    }
}

/// The urlencoded fields, read with [`scalar_text`]. [`KeyValue`] itself
/// keeps reading strings as written, which `params` and `headers` rely on.
fn scalar_rows<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<KeyValue>, D::Error> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Row {
        #[serde(deserialize_with = "scalar_text")]
        name: String,
        #[serde(deserialize_with = "scalar_text")]
        value: String,
        #[serde(default, deserialize_with = "some_scalar_text")]
        description: Option<String>,
        #[serde(default = "default_true")]
        enabled: bool,
    }
    let rows = Vec::<Row>::deserialize(deserializer)?;
    Ok(rows
        .into_iter()
        .map(|row| KeyValue {
            name: row.name,
            value: row.value,
            description: row.description,
            enabled: row.enabled,
        })
        .collect())
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
        #[serde(deserialize_with = "scalar_text")]
        text: String,
    },
    Raw {
        language: RawLanguage,
        #[serde(deserialize_with = "scalar_text")]
        text: String,
    },
    Urlencoded {
        #[serde(deserialize_with = "scalar_rows")]
        fields: Vec<KeyValue>,
    },
    FormData {
        fields: Vec<FormField>,
    },
    Binary {
        /// Path of the file to send, relative to the project.
        #[serde(deserialize_with = "scalar_text")]
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
    #[serde(deserialize_with = "scalar_text")]
    pub name: String,
    #[serde(
        default,
        deserialize_with = "some_scalar_text",
        skip_serializing_if = "Option::is_none"
    )]
    pub value: Option<String>,
    /// Path of the file to upload, relative to the project.
    #[serde(
        default,
        deserialize_with = "some_scalar_text",
        skip_serializing_if = "Option::is_none"
    )]
    pub file: Option<String>,
    #[serde(
        default,
        deserialize_with = "some_scalar_text",
        skip_serializing_if = "Option::is_none"
    )]
    pub content_type: Option<String>,
    #[serde(
        default,
        deserialize_with = "some_scalar_text",
        skip_serializing_if = "Option::is_none"
    )]
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
        #[serde(deserialize_with = "scalar_text")]
        token: String,
    },
    Basic {
        #[serde(deserialize_with = "scalar_text")]
        username: String,
        #[serde(deserialize_with = "scalar_text")]
        password: String,
    },
    ApiKey {
        #[serde(deserialize_with = "scalar_text")]
        key: String,
        #[serde(deserialize_with = "scalar_text")]
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
    /// The longest wait for the response head, then between two pieces of
    /// the body. `0` sets no limit.
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
