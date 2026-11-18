//! The operating system keychain: macOS Keychain, Windows Credential
//! Manager, or the Linux Secret Service, through `keyring-core`.

use std::sync::Arc;

use keyring_core::{CredentialStore, Entry, Error as KeyringError};
use zeroize::{Zeroize, Zeroizing};

use crate::chunk::{chunk_count, join, split};
use crate::{Backend, VaultError};

/// Secret values in the OS keychain, under one service name.
pub struct KeychainBackend {
    service: String,
    store: Arc<CredentialStore>,
}

impl KeychainBackend {
    /// Connects to this OS's keychain. On Linux this fails when no Secret
    /// Service answers on the session bus.
    pub fn new(service: &str) -> Result<Self, VaultError> {
        Ok(KeychainBackend {
            service: service.to_string(),
            store: native_store()?,
        })
    }

    /// Reads an account that never exists, to check the keychain answers.
    pub fn probe(&self) -> Result<(), VaultError> {
        self.read("dea.probe").map(|_| ())
    }

    fn entry(&self, account: &str) -> Result<Entry, VaultError> {
        self.store
            .build(&self.service, account, None)
            .map_err(describe)
    }

    fn read(&self, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>, VaultError> {
        match self.entry(account)?.get_secret() {
            Ok(bytes) => Ok(Some(Zeroizing::new(bytes))),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(e) => Err(describe(e)),
        }
    }

    fn write(&self, account: &str, bytes: &[u8]) -> Result<(), VaultError> {
        self.entry(account)?.set_secret(bytes).map_err(describe)
    }

    fn remove(&self, account: &str) -> Result<(), VaultError> {
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(e) => Err(describe(e)),
        }
    }
}

fn part(account: &str, i: usize) -> String {
    format!("{account}#{i}")
}

impl Backend for KeychainBackend {
    fn get(&self, account: &str) -> Result<Option<Zeroizing<String>>, VaultError> {
        let Some(first) = self.read(account)? else {
            return Ok(None);
        };
        let (mut bytes, _) = join(&first, |i| {
            Ok(self
                .read(&part(account, i))?
                .map(|mut b| std::mem::take(&mut *b)))
        })?;
        match String::from_utf8(std::mem::take(&mut *bytes)) {
            Ok(text) => Ok(Some(Zeroizing::new(text))),
            Err(e) => {
                e.into_bytes().zeroize();
                Err(VaultError::Corrupt(
                    "a value in the keychain is not text".into(),
                ))
            }
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<(), VaultError> {
        let old_count = self.read(account)?.map_or(0, |first| chunk_count(&first));
        let mut pieces = split(value.as_bytes());
        // Later parts first, so the header never points at missing parts.
        let mut result = Ok(());
        for (i, piece) in pieces.iter().enumerate().skip(1).rev() {
            result = result.and_then(|()| self.write(&part(account, i + 1), piece));
        }
        result = result.and_then(|()| self.write(account, &pieces[0]));
        let new_count = pieces.len();
        for piece in &mut pieces {
            piece.zeroize();
        }
        result?;
        for i in (new_count + 1).max(2)..=old_count {
            self.remove(&part(account, i))?;
        }
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), VaultError> {
        let count = self.read(account)?.map_or(0, |first| chunk_count(&first));
        self.remove(account)?;
        for i in 2..=count {
            self.remove(&part(account, i))?;
        }
        Ok(())
    }
}

/// A keychain error as a Vault error. Errors that carry the stored bytes
/// are replaced by a fixed message, so a value never reaches a log.
fn describe(e: KeyringError) -> VaultError {
    match e {
        KeyringError::BadEncoding(mut bytes) | KeyringError::BadDataFormat(mut bytes, _) => {
            bytes.zeroize();
            VaultError::Corrupt("a value in the keychain is damaged".into())
        }
        other => VaultError::Backend(other.to_string()),
    }
}

fn native_store() -> Result<Arc<CredentialStore>, VaultError> {
    #[cfg(target_os = "macos")]
    let store =
        apple_native_keyring_store::keychain::Store::new().map(|s| s as Arc<CredentialStore>);
    #[cfg(windows)]
    let store = windows_native_keyring_store::Store::new().map(|s| s as Arc<CredentialStore>);
    #[cfg(target_os = "linux")]
    let store = zbus_secret_service_keyring_store::Store::new().map(|s| s as Arc<CredentialStore>);
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    let store: Result<Arc<CredentialStore>, KeyringError> = Err(KeyringError::NotSupportedByStore(
        "no keychain on this platform".into(),
    ));
    store.map_err(describe)
}
