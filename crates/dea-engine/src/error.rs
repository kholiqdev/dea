use std::time::Duration;

use dea_vault::VaultError;

/// Why a secret could not be used. Messages never include a value.
#[derive(Debug, thiserror::Error)]
pub enum UseError {
    #[error("the Vault is locked")]
    Locked,
    #[error("this production secret needs authentication again")]
    ReauthRequired,
    #[error("authentication was cancelled")]
    Cancelled,
    #[error("too many failed attempts; try again in {} s", .0.as_secs().max(1))]
    RetryAfter(Duration),
    #[error("authentication failed")]
    AuthFailed,
    #[error("this secret has no value on this device")]
    Missing,
    #[error(transparent)]
    Vault(#[from] VaultError),
}
