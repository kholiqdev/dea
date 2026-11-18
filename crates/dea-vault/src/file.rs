//! The encrypted file used on Linux when no Secret Service is running.
//!
//! Layout of `vault.enc`:
//!
//! | Bytes | Content |
//! | --- | --- |
//! | 8 | magic `DEAVAULT` |
//! | 1 | format version, `1` |
//! | 12 | Argon2id memory (KiB), iterations, parallelism, u32 little-endian |
//! | 16 | salt |
//! | 12 | AES-GCM nonce, new for every write |
//! | rest | AES-256-GCM ciphertext and tag |
//!
//! Every byte before the ciphertext is authenticated as associated data.
//! The plaintext is JSON: `{"entries": {<account>: <value>}}`.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::index::write_private;
use crate::{Backend, KdfParams, VaultError};

const MAGIC: &[u8; 8] = b"DEAVAULT";
const VERSION: u8 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const HEADER_LEN: usize = 8 + 1 + 12 + SALT_LEN;
const TAG_LEN: usize = 16;

/// Secret values in one file encrypted with a key from the Vault password.
pub struct FileBackend {
    path: PathBuf,
    kdf: KdfParams,
    state: Mutex<Option<Unlocked>>,
}

struct Unlocked {
    key: Zeroizing<[u8; 32]>,
    kdf: KdfParams,
    salt: [u8; SALT_LEN],
    entries: BTreeMap<String, Zeroizing<String>>,
}

#[derive(Serialize)]
struct PlainOut<'a> {
    entries: BTreeMap<&'a str, &'a str>,
}

#[derive(Deserialize)]
struct PlainIn {
    entries: BTreeMap<String, String>,
}

impl FileBackend {
    /// A backend for the file at `path`. Nothing is read until
    /// [`FileBackend::unlock`]. `kdf` is the cost for a new file; an existing
    /// file keeps its own.
    pub fn open(path: &Path, kdf: KdfParams) -> Self {
        FileBackend {
            path: path.to_path_buf(),
            kdf,
            state: Mutex::new(None),
        }
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn is_unlocked(&self) -> bool {
        self.state().is_some()
    }

    fn state(&self) -> MutexGuard<'_, Option<Unlocked>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Writes a new, empty file protected by `password`, and leaves it
    /// unlocked. Refuses to replace an existing file.
    pub fn create(&self, password: &SecretString) -> Result<(), VaultError> {
        if self.exists() {
            return Err(VaultError::Backend(format!(
                "{} already exists",
                self.path.display()
            )));
        }
        let mut salt = [0u8; SALT_LEN];
        rand::fill(&mut salt);
        let unlocked = Unlocked {
            key: self
                .kdf
                .derive_key(password.expose_secret().as_bytes(), &salt)?,
            kdf: self.kdf,
            salt,
            entries: BTreeMap::new(),
        };
        self.write(&unlocked)?;
        *self.state() = Some(unlocked);
        Ok(())
    }

    /// Decrypts the file with `password`. A wrong password and a damaged
    /// file are told apart where the format allows; the file is never
    /// changed.
    pub fn unlock(&self, password: &SecretString) -> Result<(), VaultError> {
        let bytes = fs::read(&self.path).map_err(|e| match e.kind() {
            ErrorKind::NotFound => VaultError::NoPassword,
            _ => VaultError::io(&self.path)(e),
        })?;
        let damaged = |why: &str| VaultError::Corrupt(format!("{}: {why}", self.path.display()));
        if bytes.len() < HEADER_LEN + NONCE_LEN + TAG_LEN || &bytes[..8] != MAGIC {
            return Err(damaged("not a Dea vault file"));
        }
        if bytes[8] != VERSION {
            return Err(damaged("written by a newer version of Dea"));
        }
        let u32_at = |at: usize| {
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let kdf = KdfParams {
            memory_kib: u32_at(9),
            iterations: u32_at(13),
            parallelism: u32_at(17),
        };
        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&bytes[21..HEADER_LEN]);
        let key = kdf.derive_key(password.expose_secret().as_bytes(), &salt)?;

        let aad_end = HEADER_LEN + NONCE_LEN;
        let nonce =
            Nonce::try_from(&bytes[HEADER_LEN..aad_end]).map_err(|_| damaged("bad nonce"))?;
        let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| damaged("bad key"))?;
        let plain = cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &bytes[aad_end..],
                    aad: &bytes[..aad_end],
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| VaultError::WrongPassword)?;
        let parsed: PlainIn =
            serde_json::from_slice(&plain).map_err(|_| damaged("the contents cannot be read"))?;
        let entries = parsed
            .entries
            .into_iter()
            .map(|(account, value)| (account, Zeroizing::new(value)))
            .collect();
        *self.state() = Some(Unlocked {
            key,
            kdf,
            salt,
            entries,
        });
        Ok(())
    }

    /// Forgets the key and every decrypted value.
    pub fn lock(&self) {
        *self.state() = None;
    }

    fn write(&self, unlocked: &Unlocked) -> Result<(), VaultError> {
        let plain = PlainOut {
            entries: unlocked
                .entries
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect(),
        };
        let plain = Zeroizing::new(
            serde_json::to_vec(&plain).map_err(|e| VaultError::Corrupt(e.to_string()))?,
        );
        let mut bytes = Vec::with_capacity(HEADER_LEN + NONCE_LEN + plain.len() + TAG_LEN);
        bytes.extend_from_slice(MAGIC);
        bytes.push(VERSION);
        for n in [
            unlocked.kdf.memory_kib,
            unlocked.kdf.iterations,
            unlocked.kdf.parallelism,
        ] {
            bytes.extend_from_slice(&n.to_le_bytes());
        }
        bytes.extend_from_slice(&unlocked.salt);
        let mut nonce = [0u8; NONCE_LEN];
        rand::fill(&mut nonce);
        bytes.extend_from_slice(&nonce);

        let cipher = Aes256Gcm::new_from_slice(unlocked.key.as_ref())
            .map_err(|_| VaultError::Corrupt("bad key".into()))?;
        let ciphertext = cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: &plain,
                    aad: &bytes,
                },
            )
            .map_err(|_| VaultError::Backend("encryption failed".into()))?;
        bytes.extend_from_slice(&ciphertext);
        write_private(&self.path, &bytes)
    }
}

impl Backend for FileBackend {
    fn get(&self, account: &str) -> Result<Option<Zeroizing<String>>, VaultError> {
        let state = self.state();
        let unlocked = state.as_ref().ok_or(VaultError::Locked)?;
        Ok(unlocked.entries.get(account).cloned())
    }

    fn set(&self, account: &str, value: &str) -> Result<(), VaultError> {
        let mut state = self.state();
        let unlocked = state.as_mut().ok_or(VaultError::Locked)?;
        let previous = unlocked
            .entries
            .insert(account.to_string(), Zeroizing::new(value.to_string()));
        if let Err(e) = self.write(unlocked) {
            // Keep memory in step with the file.
            match previous {
                Some(old) => unlocked.entries.insert(account.to_string(), old),
                None => unlocked.entries.remove(account),
            };
            return Err(e);
        }
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), VaultError> {
        let mut state = self.state();
        let unlocked = state.as_mut().ok_or(VaultError::Locked)?;
        if unlocked.entries.remove(account).is_some() {
            self.write(unlocked)?;
        }
        Ok(())
    }
}
