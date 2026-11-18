use std::path::Path;
use std::sync::Arc;

use crate::index::Index;
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

/// The OS keychain, or on Linux the encrypted file `<vault_dir>/vault.enc`
/// (F1-FON-08 #3, #4). On Linux the file is used once it exists, and is
/// chosen only on a device that never stored a value in the keychain, so a
/// Secret Service that is slow to start never hides stored secrets. On
/// macOS and Windows a keychain failure is an error.
pub fn open_native(
    service: &str,
    vault_dir: &Path,
    kdf: KdfParams,
) -> Result<VaultBackend, VaultError> {
    let file_path = vault_dir.join(VAULT_FILE);
    let linux = cfg!(target_os = "linux");
    if linux && file_path.exists() {
        return Ok(VaultBackend::File(Arc::new(FileBackend::open(
            &file_path, kdf,
        ))));
    }
    let keychain = KeychainBackend::new(service).and_then(|k| k.probe().map(|()| k));
    let probe = match &keychain {
        Ok(_) => Ok(()),
        Err(e) => Err(VaultError::Backend(e.to_string())),
    };
    // A damaged index counts as holding entries: never switch on doubt.
    let has_entries = Index::load(vault_dir).map_or(true, |i| !i.entries.is_empty());
    match choose(linux, false, has_entries, probe)? {
        BackendKind::EncryptedFile => Ok(VaultBackend::File(Arc::new(FileBackend::open(
            &file_path, kdf,
        )))),
        _ => keychain.map(|k| VaultBackend::Keychain(Arc::new(k))),
    }
}

fn choose(
    os_is_linux: bool,
    file_exists: bool,
    has_index_entries: bool,
    probe: Result<(), VaultError>,
) -> Result<BackendKind, VaultError> {
    if !os_is_linux {
        return probe.map(|()| BackendKind::Keychain);
    }
    if file_exists {
        return Ok(BackendKind::EncryptedFile);
    }
    match probe {
        Ok(()) => Ok(BackendKind::Keychain),
        Err(_) if !has_index_entries => Ok(BackendKind::EncryptedFile),
        Err(e) => Err(VaultError::Backend(format!(
            "the Secret Service is not answering, and secrets are stored there: {e}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down() -> Result<(), VaultError> {
        Err(VaultError::Backend("no bus".into()))
    }

    #[test]
    fn linux_without_secret_service_uses_the_encrypted_file_on_a_new_device() {
        assert_eq!(
            choose(true, false, false, down()).unwrap(),
            BackendKind::EncryptedFile
        );
        assert_eq!(
            choose(true, false, false, Ok(())).unwrap(),
            BackendKind::Keychain
        );
    }

    #[test]
    fn an_existing_vault_file_is_always_used() {
        assert_eq!(
            choose(true, true, true, Ok(())).unwrap(),
            BackendKind::EncryptedFile
        );
        assert_eq!(
            choose(true, true, false, down()).unwrap(),
            BackendKind::EncryptedFile
        );
    }

    #[test]
    fn a_keychain_that_stops_answering_is_an_error_not_a_switch() {
        // Values were stored in the keychain: switching would hide them and
        // drop their entries from the index.
        assert!(matches!(
            choose(true, false, true, down()),
            Err(VaultError::Backend(_))
        ));
    }

    #[test]
    fn macos_and_windows_always_use_the_keychain() {
        assert_eq!(
            choose(false, false, false, Ok(())).unwrap(),
            BackendKind::Keychain
        );
        assert!(choose(false, false, false, down()).is_err());
    }
}
