//! Types stored in the project folder. Field order is the order in the file.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::FileError;

/// Version of the project file format written by this build.
pub const SCHEMA_VERSION: u32 = 1;

/// Longest allowed path of a project file, relative to the project root.
pub const MAX_RELATIVE_PATH_LEN: usize = 200;

/// Project manifest, stored in `dea.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_environment: Option<String>,
}

/// An environment, stored in `environments/<name>.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Environment {
    pub name: String,
    /// Marks the environment as production: Dea asks for confirmation
    /// before sending to it and shows it in red. Required, so a production
    /// environment can never lose its marker by omission.
    pub production: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<Variable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Variable {
    pub name: String,
    /// Plain text, or a reference to a secret such as `{{vault:access_token}}`.
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The secrets a project needs, stored in `vault.yaml`. Values are kept in
/// the operating system keychain and never written here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VaultManifest {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<SecretSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecretSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: SecretKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum SecretKind {
    Token,
    Password,
    SshKey,
    ApiKey,
    OauthToken,
}

/// A project as read from disk.
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub environments: Vec<Environment>,
    pub vault: VaultManifest,
    /// True when the project was written by a newer version of Dea.
    pub read_only: bool,
}

/// A loaded project plus the files that could not be read. One broken file
/// never hides the rest of the project.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedProject {
    pub project: Project,
    pub errors: Vec<FileError>,
}
