//! The one error shape commands reject with, `{ kind, message }`.

use std::error::Error;
use std::fmt::Write;

use dea_engine::projects::OpenError;
use dea_engine::store::StoreError;
use dea_engine::vault::{SaveSecretError, UseError, VaultError};
use dea_project::ProjectError;
use serde::Serialize;

use crate::session::Refusal;

/// What kind of failure a command hit, so the UI can react to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IpcErrorKind {
    /// The request broke a rule; the message says which.
    Invalid,
    /// A file could not be read or written.
    Io,
    /// Something inside Dea failed, such as a background task.
    Internal,
    /// The folder has no `dea.yaml`.
    NotAProject,
    /// Another environment already has this name or file name.
    NameTaken,
    /// The project was saved by a newer Dea.
    ReadOnly,
    /// A file has Git conflict markers and is not overwritten.
    MergeConflict,
    /// The project folder is gone.
    Missing,
    /// The Vault is locked.
    VaultLocked,
    /// Production secrets, or turning production off, need authentication
    /// again.
    ReauthRequired,
    /// The person cancelled the authentication dialog.
    Cancelled,
    /// Too many failed attempts; the message says how long to wait.
    RetryAfter,
    /// Authentication failed.
    AuthFailed,
    /// The secret has no value on this device.
    SecretMissing,
}

#[derive(Debug, Serialize)]
pub struct IpcError {
    pub kind: IpcErrorKind,
    /// The error and each underlying cause, as shown to the person.
    pub message: String,
}

impl IpcError {
    fn new(kind: IpcErrorKind, error: &dyn Error) -> Self {
        let mut message = error.to_string();
        let mut cause = error.source();
        while let Some(c) = cause {
            // Writing to a String cannot fail.
            let _ = write!(message, ": {c}");
            cause = c.source();
        }
        Self { kind, message }
    }
}

impl From<StoreError> for IpcError {
    fn from(e: StoreError) -> Self {
        let kind = match e {
            StoreError::Invalid(_) => IpcErrorKind::Invalid,
            StoreError::Io { .. } => IpcErrorKind::Io,
        };
        Self::new(kind, &e)
    }
}

impl From<tauri::Error> for IpcError {
    fn from(e: tauri::Error) -> Self {
        Self::new(IpcErrorKind::Internal, &e)
    }
}

impl From<ProjectError> for IpcError {
    fn from(e: ProjectError) -> Self {
        let kind = match e {
            ProjectError::NotAProject(_) => IpcErrorKind::NotAProject,
            ProjectError::NameTaken(_) => IpcErrorKind::NameTaken,
            ProjectError::ReadOnly => IpcErrorKind::ReadOnly,
            ProjectError::MergeConflict(_) => IpcErrorKind::MergeConflict,
            ProjectError::Io(_) => IpcErrorKind::Io,
            ProjectError::Yaml(_) | ProjectError::Migration(_) => IpcErrorKind::Internal,
            ProjectError::InvalidManifest(_)
            | ProjectError::PathTooLong(_)
            | ProjectError::AlreadyExists(_)
            | ProjectError::InvalidEnvironment(_)
            | ProjectError::InvalidSecret(_) => IpcErrorKind::Invalid,
        };
        Self::new(kind, &e)
    }
}

impl From<OpenError> for IpcError {
    fn from(e: OpenError) -> Self {
        match e {
            OpenError::Project(e) => e.into(),
            OpenError::NotAProject { .. } => Self::new(IpcErrorKind::NotAProject, &e),
            OpenError::NotAFolder { .. }
            | OpenError::EmptyName
            | OpenError::RelativeLocation { .. } => Self::new(IpcErrorKind::Invalid, &e),
        }
    }
}

impl From<UseError> for IpcError {
    fn from(e: UseError) -> Self {
        let kind = match &e {
            UseError::Locked => IpcErrorKind::VaultLocked,
            UseError::ReauthRequired => IpcErrorKind::ReauthRequired,
            UseError::Cancelled => IpcErrorKind::Cancelled,
            UseError::RetryAfter(_) => IpcErrorKind::RetryAfter,
            UseError::AuthFailed => IpcErrorKind::AuthFailed,
            UseError::Missing => IpcErrorKind::SecretMissing,
            UseError::Vault(e) => vault_kind(e),
        };
        Self::new(kind, &e)
    }
}

impl From<VaultError> for IpcError {
    fn from(e: VaultError) -> Self {
        Self::new(vault_kind(&e), &e)
    }
}

fn vault_kind(e: &VaultError) -> IpcErrorKind {
    match e {
        VaultError::Locked => IpcErrorKind::VaultLocked,
        VaultError::WrongPassword => IpcErrorKind::AuthFailed,
        VaultError::PasswordTooShort { .. } | VaultError::InvalidSetting(_) => {
            IpcErrorKind::Invalid
        }
        VaultError::Io { .. } => IpcErrorKind::Io,
        VaultError::NoPassword | VaultError::Corrupt(_) | VaultError::Backend(_) => {
            IpcErrorKind::Internal
        }
    }
}

impl From<SaveSecretError> for IpcError {
    fn from(e: SaveSecretError) -> Self {
        match e {
            SaveSecretError::Project(e) => e.into(),
            SaveSecretError::Use(e) => e.into(),
            SaveSecretError::Invalid(message) => Self {
                kind: IpcErrorKind::Invalid,
                message,
            },
        }
    }
}

impl From<Refusal> for IpcError {
    fn from(r: Refusal) -> Self {
        let (kind, message) = match r {
            Refusal::NoProject => (IpcErrorKind::Invalid, "this window has no project open"),
            Refusal::Missing => (IpcErrorKind::Missing, "the project folder is gone"),
        };
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;

    #[test]
    fn project_errors_keep_a_kind_the_ui_can_act_on() {
        use dea_engine::projects::OpenError;
        use dea_project::ProjectError;

        let kind = |e: IpcError| serde_json::to_value(&e).unwrap()["kind"].clone();
        assert_eq!(
            kind(IpcError::from(OpenError::NotAProject {
                path: PathBuf::from("/x")
            })),
            json!("notAProject")
        );
        assert_eq!(kind(IpcError::from(OpenError::EmptyName)), json!("invalid"));
        assert_eq!(
            kind(IpcError::from(ProjectError::NameTaken("Staging".into()))),
            json!("nameTaken")
        );
        assert_eq!(
            kind(IpcError::from(ProjectError::ReadOnly)),
            json!("readOnly")
        );
        assert_eq!(
            kind(IpcError::from(ProjectError::MergeConflict(PathBuf::from(
                "a.yaml"
            )))),
            json!("mergeConflict")
        );
        assert_eq!(
            kind(IpcError::from(ProjectError::InvalidEnvironment(
                "bad".into()
            ))),
            json!("invalid")
        );
        assert_eq!(kind(IpcError::from(Refusal::Missing)), json!("missing"));
        assert_eq!(kind(IpcError::from(Refusal::NoProject)), json!("invalid"));
    }

    #[test]
    fn store_errors_keep_their_kind_and_their_cause() {
        let io_error = StoreError::Io {
            path: PathBuf::from("/data/settings.json"),
            source: io::Error::new(io::ErrorKind::PermissionDenied, "access denied"),
        };
        let e = IpcError::from(io_error);
        assert_eq!(e.kind, IpcErrorKind::Io);
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({ "kind": "io", "message": "cannot write /data/settings.json: access denied" })
        );

        let e = IpcError::from(StoreError::Invalid("Font size must be 11 to 20".into()));
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({ "kind": "invalid", "message": "Font size must be 11 to 20" })
        );
    }

    #[test]
    fn use_errors_map_to_their_kinds() {
        use std::time::Duration;

        use dea_engine::vault::{UseError, VaultError};

        let json = |e: UseError| serde_json::to_value(IpcError::from(e)).unwrap();
        let kind = |e: UseError| json(e)["kind"].clone();
        assert_eq!(kind(UseError::Locked), json!("vaultLocked"));
        assert_eq!(kind(UseError::ReauthRequired), json!("reauthRequired"));
        assert_eq!(kind(UseError::Cancelled), json!("cancelled"));
        assert_eq!(kind(UseError::AuthFailed), json!("authFailed"));
        assert_eq!(kind(UseError::Missing), json!("secretMissing"));
        assert_eq!(
            json(UseError::RetryAfter(Duration::from_secs(30))),
            json!({ "kind": "retryAfter", "message": "too many failed attempts; try again in 30 s" })
        );
        assert_eq!(
            json(UseError::Vault(VaultError::Backend("keychain gone".into()))),
            json!({ "kind": "internal", "message": "the secret store failed: keychain gone" })
        );
    }

    #[test]
    fn save_secret_errors_keep_the_kind_of_their_cause() {
        use dea_engine::vault::{SaveSecretError, UseError};

        let kind = |e: SaveSecretError| serde_json::to_value(IpcError::from(e)).unwrap();
        assert_eq!(
            kind(SaveSecretError::Invalid(
                "only an OAuth token has an expiry".into()
            )),
            json!({ "kind": "invalid", "message": "only an OAuth token has an expiry" })
        );
        assert_eq!(
            kind(SaveSecretError::Use(UseError::Locked))["kind"],
            json!("vaultLocked")
        );
        assert_eq!(
            kind(SaveSecretError::Project(ProjectError::ReadOnly))["kind"],
            json!("readOnly")
        );
    }
}
