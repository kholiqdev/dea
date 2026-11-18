use std::path::PathBuf;

/// Everything that can go wrong in the Vault. Messages never include a
/// secret value.
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("the Vault is locked")]
    Locked,
    #[error("the Vault password is wrong")]
    WrongPassword,
    #[error("no Vault password has been set")]
    NoPassword,
    #[error("the Vault password must have at least {min} characters")]
    PasswordTooShort { min: usize },
    #[error("invalid setting: {0}")]
    InvalidSetting(String),
    #[error("Vault data is damaged: {0}")]
    Corrupt(String),
    #[error("the secret store failed: {0}")]
    Backend(String),
    #[error("cannot access {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl VaultError {
    pub(crate) fn io(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.into();
        move |source| VaultError::Io { path, source }
    }
}
