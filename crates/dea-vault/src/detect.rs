use std::path::Path;
use std::sync::Arc;

use crate::{Backend, FileBackend, KdfParams, KeychainBackend, MemoryBackend, VaultError};

/// File name of the encrypted fallback inside the Vault folder.
pub const VAULT_FILE: &str = "vault.enc";

/// The store holding secret values on this device.
#[derive(Clone)]
pub enum VaultBackend {
    Keychain(Arc<KeychainBackend>),
    File(Arc<FileBackend>),
    Memory(Arc<MemoryBackend>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Keychain,
    EncryptedFile,
    Memory,
}

impl VaultBackend {
    pub fn as_backend(&self) -> Arc<dyn Backend> {
        match self {
            VaultBackend::Keychain(b) => b.clone(),
            VaultBackend::File(b) => b.clone(),
            VaultBackend::Memory(b) => b.clone(),
        }
    }

    pub fn kind(&self) -> BackendKind {
        match self {
            VaultBackend::Keychain(_) => BackendKind::Keychain,
            VaultBackend::File(_) => BackendKind::EncryptedFile,
            VaultBackend::Memory(_) => BackendKind::Memory,
        }
    }
}

/// The OS keychain, or on Linux without a working Secret Service the
/// encrypted file `<vault_dir>/vault.enc` (F1-FON-08 #3, #4). On macOS and
/// Windows a keychain failure is an error.
pub fn open_native(
    service: &str,
    vault_dir: &Path,
    kdf: KdfParams,
) -> Result<VaultBackend, VaultError> {
    let keychain = KeychainBackend::new(service).and_then(|k| k.probe().map(|()| k));
    let probe = match &keychain {
        Ok(_) => Ok(()),
        Err(e) => Err(VaultError::Backend(e.to_string())),
    };
    match choose(cfg!(target_os = "linux"), probe) {
        BackendKind::EncryptedFile => Ok(VaultBackend::File(Arc::new(FileBackend::open(
            &vault_dir.join(VAULT_FILE),
            kdf,
        )))),
        _ => keychain.map(|k| VaultBackend::Keychain(Arc::new(k))),
    }
}

fn choose(os_is_linux: bool, probe: Result<(), VaultError>) -> BackendKind {
    if os_is_linux && probe.is_err() {
        BackendKind::EncryptedFile
    } else {
        BackendKind::Keychain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_without_secret_service_uses_the_encrypted_file() {
        assert_eq!(
            choose(true, Err(VaultError::Backend("no bus".into()))),
            BackendKind::EncryptedFile
        );
        assert_eq!(choose(true, Ok(())), BackendKind::Keychain);
        assert_eq!(
            choose(false, Err(VaultError::Backend("x".into()))),
            BackendKind::Keychain
        );
    }
}
