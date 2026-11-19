//! The Vault as the desktop app uses it: opening the one Vault of the app,
//! the rows of the Vault screen, and adding a secret. Reached through
//! [`crate::vault`].

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use dea_project::{Project, ProjectError, SecretKind, SecretSpec, save_secret_spec};
use dea_vault::{
    AuthMethod, KdfParams, OAuthToken, Policy, SecretData, SecretKey, SecretState, SecretStatus,
    VaultError, has_values_in, native_authenticator, open_native,
};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{Prompter, SystemClock, UseError, VaultService, VaultStatus};

/// The keychain service name Dea stores its secrets under.
const KEYCHAIN_SERVICE: &str = "dev.dea.desktop";
/// The Vault folder inside the app data folder.
const VAULT_DIR: &str = "vault";

/// Opens the one Vault of the app in `<app_data>/vault`, locked, with the
/// authenticator of this OS. The idle timer and the screen lock watcher
/// run for as long as the returned service lives.
///
/// # Errors
///
/// [`VaultError`] when the keychain cannot be reached, the Vault folder
/// cannot be read, or the background timer cannot start.
pub fn open_app_vault(
    app_data: &Path,
    prompter: Arc<dyn Prompter>,
    policy: Policy,
    on_status: Box<dyn Fn(VaultStatus) + Send + Sync>,
) -> Result<Arc<VaultService>, VaultError> {
    let dir = app_data.join(VAULT_DIR);
    let kdf = KdfParams::default();
    let vault = VaultService::new(
        &dir,
        open_native(KEYCHAIN_SERVICE, &dir, kdf)?,
        native_authenticator(None),
        prompter,
        policy,
        kdf,
        Arc::new(SystemClock),
        on_status,
    )?;
    vault.run_in_background()?;
    Ok(vault)
}

/// Whether `environment` of `project` has values in the one Vault of the
/// app in `<app_data>/vault`, read from its index without opening it, so
/// the keychain is not reached. A damaged index counts as having values.
#[must_use]
pub fn app_vault_has_values(app_data: &Path, project: &Project, environment: &str) -> bool {
    has_values_in(
        &app_data.join(VAULT_DIR),
        &project.vault_scope(),
        environment,
    )
}

/// How this device unlocks the Vault, which decides the unlock button's
/// label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub enum UnlockMethod {
    /// macOS: Touch ID, or the account password without Touch ID.
    DeviceOwner,
    WindowsHello,
    /// Windows without Windows Hello: the account password.
    WindowsPassword,
    /// Linux: the Vault password, asked for in Dea.
    VaultPassword,
}

impl From<AuthMethod> for UnlockMethod {
    fn from(method: AuthMethod) -> Self {
        match method {
            AuthMethod::DeviceOwner => UnlockMethod::DeviceOwner,
            AuthMethod::WindowsHello => UnlockMethod::WindowsHello,
            AuthMethod::WindowsPassword => UnlockMethod::WindowsPassword,
            AuthMethod::VaultPassword => UnlockMethod::VaultPassword,
        }
    }
}

/// How this device unlocks the Vault. Opens no Vault and touches no
/// keychain, so the app can show it before the Vault is first used.
#[must_use]
pub fn unlock_method() -> UnlockMethod {
    native_authenticator(None).method().into()
}

/// One secret in one environment, as the Vault screen lists it. Never holds
/// the value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct SecretRow {
    pub name: String,
    pub kind: SecretKind,
    pub description: Option<String>,
    pub environment: String,
    /// No value on this device.
    pub missing: bool,
    /// When the value was last saved, in RFC 3339.
    pub updated_at: Option<String>,
    /// When an OAuth token stops working, in RFC 3339, if known.
    pub expires_at: Option<String>,
}

impl SecretRow {
    fn from_status(status: SecretStatus) -> SecretRow {
        let (missing, updated_at, expires_at) = match status.state {
            SecretState::Missing => (true, None, None),
            SecretState::Present {
                updated_at,
                expires_at,
            } => (false, rfc3339(updated_at), expires_at.and_then(rfc3339)),
        };
        SecretRow {
            name: status.name,
            kind: status.kind,
            description: status.description,
            environment: status.environment,
            missing,
            updated_at,
            expires_at,
        }
    }
}

fn rfc3339(time: OffsetDateTime) -> Option<String> {
    time.format(&Rfc3339).ok()
}

impl VaultService {
    /// The secrets of `project` in every environment, sorted by
    /// environment and then name, one row each. A name declared twice in
    /// `vault.yaml`, as after a merge, takes its first declaration. Works
    /// while locked: no value is read.
    pub fn rows(&self, project: &Project) -> Vec<SecretRow> {
        let mut rows: Vec<SecretRow> = self
            .statuses(project)
            .into_iter()
            .map(SecretRow::from_status)
            .collect();
        // A stable sort keeps the declarations in file order, so the first
        // one stays.
        rows.sort_by(|a, b| (&a.environment, &a.name).cmp(&(&b.environment, &b.name)));
        rows.dedup_by(|later, first| {
            later.environment == first.environment && later.name == first.name
        });
        rows
    }
}

/// A secret to add from the `Add a secret` form. `Debug` hides the value
/// and the refresh token.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct SecretInput {
    pub name: String,
    pub kind: SecretKind,
    pub environment: String,
    /// Empty counts as none.
    pub description: Option<String>,
    pub value: String,
    /// Only for an OAuth token. Empty counts as none.
    pub refresh_token: Option<String>,
    /// Only for an OAuth token, in RFC 3339. Empty counts as none.
    pub expires_at: Option<String>,
}

impl fmt::Debug for SecretInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretInput")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("environment", &self.environment)
            .field("description", &self.description)
            .field("value", &"***")
            .field("refresh_token", &self.refresh_token.as_ref().map(|_| "***"))
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Why a secret could not be added. Messages never include a value.
#[derive(Debug, thiserror::Error)]
pub enum SaveSecretError {
    /// `vault.yaml` could not take the declaration; no value was stored.
    #[error(transparent)]
    Project(#[from] ProjectError),
    /// The value could not be stored.
    #[error(transparent)]
    Use(#[from] UseError),
    /// The input breaks a rule, such as an expiry on a token that is not
    /// an OAuth token. Nothing was written.
    #[error("{0}")]
    Invalid(String),
}

/// Declares the secret in `vault.yaml` of `project`, then stores its value
/// in `environment` on this device. A name already declared gets the new
/// type and description.
///
/// # Errors
///
/// [`SaveSecretError::Invalid`] when a refresh token or expiry is given for
/// a type other than an OAuth token, the expiry is not RFC 3339, or
/// `project` has no environment called `environment`, as when a form
/// opened before it was renamed or deleted.
/// [`SaveSecretError::Use`] with [`UseError::Locked`] when the Vault is
/// locked. [`SaveSecretError::Project`] when the declaration cannot be
/// written, for example in a read-only project or a `vault.yaml` with
/// conflict markers. In all these cases nothing is written. Errors from
/// storing the value come after the declaration was written, so the secret
/// then shows as missing.
pub fn save_secret(
    vault: &VaultService,
    project: &Project,
    input: SecretInput,
) -> Result<(), SaveSecretError> {
    let SecretInput {
        name,
        kind,
        environment,
        description,
        value,
        refresh_token,
        expires_at,
    } = input;
    let refresh_token = given(refresh_token);
    let expires_at = given(expires_at);
    if kind != SecretKind::OauthToken {
        if refresh_token.is_some() {
            return Err(invalid("only an OAuth token has a refresh token"));
        }
        if expires_at.is_some() {
            return Err(invalid("only an OAuth token has an expiry"));
        }
    }
    let expires_at = expires_at
        .map(|text| OffsetDateTime::parse(text.trim(), &Rfc3339))
        .transpose()
        .map_err(|_| {
            invalid("the expiry must be an RFC 3339 time, such as 2026-12-31T23:59:00Z")
        })?;
    // A form opened before the environment was renamed or deleted must not
    // store a value under the freed name.
    if !project.environments.iter().any(|e| e.name == environment) {
        return Err(SaveSecretError::Invalid(format!(
            "the environment \"{environment}\" no longer exists"
        )));
    }
    if vault.status().locked {
        return Err(UseError::Locked.into());
    }

    let spec = SecretSpec {
        name: name.clone(),
        kind,
        description: given(description),
    };
    save_secret_spec(project, &spec)?;

    let value = SecretString::from(value);
    let data = if kind == SecretKind::OauthToken {
        SecretData::OAuth(OAuthToken {
            access_token: value,
            refresh_token: refresh_token.map(SecretString::from),
            expires_at,
        })
    } else {
        SecretData::Text(value)
    };
    let key = SecretKey {
        scope: project.vault_scope(),
        environment,
        name,
    };
    vault.set_secret(&key, kind, &data)?;
    Ok(())
}

/// `None` for a field left empty.
fn given(field: Option<String>) -> Option<String> {
    field.filter(|text| !text.trim().is_empty())
}

fn invalid(message: &str) -> SaveSecretError {
    SaveSecretError::Invalid(message.to_string())
}
