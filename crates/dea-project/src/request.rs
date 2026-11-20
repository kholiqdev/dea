//! Types of the files under `requests/`: one YAML file per request and a
//! `_folder.yaml` per folder. Field order is the order in the file.

use schemars::JsonSchema;
use serde::de::{Deserializer, Error as _};
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

// `Deserialize` is written by hand, see `BodyRepr`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
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
// `Deserialize` is written by hand, see `AuthRepr`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, JsonSchema)]
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

// Reading `body` and `auth`.
//
// A derived `Deserialize` for an internally tagged enum buffers the mapping
// before it knows the variant, and the buffer has already read unquoted
// scalars as numbers and booleans: `1.50` would come back as `1.5`. A plain
// struct is not buffered, so `body` and `auth` are read as flat structs
// first. Their text fields then get the scalar's text as written, as the
// fields of `params` and `headers` do.

/// A field that is present gets its text as written: an empty value is `""`
/// and `null` is `"null"`. With `#[serde(default)]`, a field left out is
/// `None`. A plain `Option<String>` would read an empty value as `None`.
fn present_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

/// The error of the checks below. Its messages read like serde's own, such
/// as ``missing field `text` ``.
type ReprError = serde::de::value::Error;

/// Refuses a field that is present but not one of `allowed`.
fn only(
    present: &[(&'static str, bool)],
    allowed: &'static [&'static str],
) -> Result<(), ReprError> {
    match present
        .iter()
        .find(|(field, is_present)| *is_present && !allowed.contains(field))
    {
        Some((field, _)) => Err(ReprError::unknown_field(field, allowed)),
        None => Ok(()),
    }
}

fn required<T>(value: Option<T>, field: &'static str) -> Result<T, ReprError> {
    value.ok_or_else(|| ReprError::missing_field(field))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum BodyMode {
    Json,
    Raw,
    Urlencoded,
    FormData,
    Binary,
}

/// `body` with the fields of every mode.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BodyRepr {
    mode: BodyMode,
    #[serde(default)]
    language: Option<RawLanguage>,
    #[serde(default, deserialize_with = "present_text")]
    text: Option<String>,
    #[serde(default, deserialize_with = "present_text")]
    file: Option<String>,
    #[serde(default)]
    fields: Option<Vec<RowRepr>>,
}

/// A row of `fields`, with the fields of both an urlencoded row and a
/// [`FormField`].
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RowRepr {
    name: String,
    #[serde(default, deserialize_with = "present_text")]
    value: Option<String>,
    #[serde(default, deserialize_with = "present_text")]
    description: Option<String>,
    #[serde(default, deserialize_with = "present_text")]
    file: Option<String>,
    #[serde(default, deserialize_with = "present_text")]
    content_type: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
}

impl TryFrom<RowRepr> for KeyValue {
    type Error = ReprError;

    fn try_from(row: RowRepr) -> Result<Self, Self::Error> {
        only(
            &[
                ("file", row.file.is_some()),
                ("contentType", row.content_type.is_some()),
            ],
            &["name", "value", "description", "enabled"],
        )?;
        Ok(KeyValue {
            name: row.name,
            value: required(row.value, "value")?,
            description: row.description,
            enabled: row.enabled,
        })
    }
}

impl From<RowRepr> for FormField {
    fn from(row: RowRepr) -> Self {
        FormField {
            name: row.name,
            value: row.value,
            file: row.file,
            content_type: row.content_type,
            description: row.description,
            enabled: row.enabled,
        }
    }
}

impl TryFrom<BodyRepr> for Body {
    type Error = ReprError;

    fn try_from(repr: BodyRepr) -> Result<Self, Self::Error> {
        let BodyRepr {
            mode,
            language,
            text,
            file,
            fields,
        } = repr;
        let present = [
            ("language", language.is_some()),
            ("text", text.is_some()),
            ("file", file.is_some()),
            ("fields", fields.is_some()),
        ];
        Ok(match mode {
            BodyMode::Json => {
                only(&present, &["text"])?;
                Body::Json {
                    text: required(text, "text")?,
                }
            }
            BodyMode::Raw => {
                only(&present, &["language", "text"])?;
                Body::Raw {
                    language: required(language, "language")?,
                    text: required(text, "text")?,
                }
            }
            BodyMode::Urlencoded => {
                only(&present, &["fields"])?;
                Body::Urlencoded {
                    fields: required(fields, "fields")?
                        .into_iter()
                        .map(KeyValue::try_from)
                        .collect::<Result<_, _>>()?,
                }
            }
            BodyMode::FormData => {
                only(&present, &["fields"])?;
                Body::FormData {
                    fields: required(fields, "fields")?
                        .into_iter()
                        .map(FormField::from)
                        .collect(),
                }
            }
            BodyMode::Binary => {
                only(&present, &["file"])?;
                Body::Binary {
                    file: required(file, "file")?,
                }
            }
        })
    }
}

impl<'de> Deserialize<'de> for Body {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let repr = BodyRepr::deserialize(deserializer)?;
        Body::try_from(repr).map_err(D::Error::custom)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum AuthType {
    Inherit,
    #[serde(rename = "none")]
    NoAuth,
    Bearer,
    Basic,
    ApiKey,
}

/// `auth` with the fields of every type.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthRepr {
    #[serde(rename = "type")]
    kind: AuthType,
    #[serde(default, deserialize_with = "present_text")]
    token: Option<String>,
    #[serde(default, deserialize_with = "present_text")]
    username: Option<String>,
    #[serde(default, deserialize_with = "present_text")]
    password: Option<String>,
    #[serde(default, deserialize_with = "present_text")]
    key: Option<String>,
    #[serde(default, deserialize_with = "present_text")]
    value: Option<String>,
    #[serde(default, rename = "in")]
    location: Option<ApiKeyLocation>,
}

impl TryFrom<AuthRepr> for Auth {
    type Error = ReprError;

    fn try_from(repr: AuthRepr) -> Result<Self, Self::Error> {
        let AuthRepr {
            kind,
            token,
            username,
            password,
            key,
            value,
            location,
        } = repr;
        let present = [
            ("token", token.is_some()),
            ("username", username.is_some()),
            ("password", password.is_some()),
            ("key", key.is_some()),
            ("value", value.is_some()),
            ("in", location.is_some()),
        ];
        Ok(match kind {
            AuthType::Inherit => {
                only(&present, &[])?;
                Auth::Inherit
            }
            AuthType::NoAuth => {
                only(&present, &[])?;
                Auth::NoAuth
            }
            AuthType::Bearer => {
                only(&present, &["token"])?;
                Auth::Bearer {
                    token: required(token, "token")?,
                }
            }
            AuthType::Basic => {
                only(&present, &["username", "password"])?;
                Auth::Basic {
                    username: required(username, "username")?,
                    password: required(password, "password")?,
                }
            }
            AuthType::ApiKey => {
                only(&present, &["key", "value", "in"])?;
                Auth::ApiKey {
                    key: required(key, "key")?,
                    value: required(value, "value")?,
                    location: required(location, "in")?,
                }
            }
        })
    }
}

impl<'de> Deserialize<'de> for Auth {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let repr = AuthRepr::deserialize(deserializer)?;
        Auth::try_from(repr).map_err(D::Error::custom)
    }
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
