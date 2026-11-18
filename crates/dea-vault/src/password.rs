//! The Vault password on Linux (K19), kept as an Argon2id hash.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use secrecy::{ExposeSecret, SecretString};

use crate::index::write_private;
use crate::{KdfParams, VaultError};

/// Shortest accepted Vault password, in characters.
pub const MIN_PASSWORD_CHARS: usize = 8;

pub struct VaultPassword {
    path: PathBuf,
}

impl VaultPassword {
    /// The hash file at `path`. Nothing is read until it is needed.
    pub fn open(path: &Path) -> Self {
        VaultPassword {
            path: path.to_path_buf(),
        }
    }

    pub fn is_set(&self) -> bool {
        self.path.exists()
    }

    /// Stores the hash of `password`, replacing any earlier one.
    pub fn set(&self, password: &SecretString, kdf: KdfParams) -> Result<(), VaultError> {
        if password.expose_secret().chars().count() < MIN_PASSWORD_CHARS {
            return Err(VaultError::PasswordTooShort {
                min: MIN_PASSWORD_CHARS,
            });
        }
        let hash = kdf
            .argon2()?
            .hash_password(password.expose_secret().as_bytes())
            .map_err(|_| VaultError::Backend("the password could not be hashed".into()))?;
        write_private(&self.path, format!("{hash}\n").as_bytes())
    }

    /// Whether `password` is the Vault password.
    pub fn verify(&self, password: &SecretString) -> Result<bool, VaultError> {
        let text = fs::read_to_string(&self.path).map_err(|e| match e.kind() {
            ErrorKind::NotFound => VaultError::NoPassword,
            _ => VaultError::io(&self.path)(e),
        })?;
        let hash = PasswordHash::new(text.trim())
            .map_err(|_| VaultError::Corrupt(format!("{} cannot be read", self.path.display())))?;
        // The cost stored in the hash is used, not the default.
        Ok(KdfParams::default()
            .argon2()?
            .verify_password(password.expose_secret().as_bytes(), &hash)
            .is_ok())
    }
}
