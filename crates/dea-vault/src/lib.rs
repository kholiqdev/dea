//! Secrets for Dea. Values live in the operating system keychain, or in an
//! encrypted file where Linux has no Secret Service. Project files only name
//! the secrets they need (K10).

mod auth;
mod backend;
mod chunk;
mod data;
mod detect;
mod error;
mod file;
mod index;
mod kdf;
mod key;
mod keychain;
mod lock;
mod memory;
mod os_events;
mod password;
mod policy;
mod store;

pub use auth::{AuthError, AuthMethod, Authenticator, WindowHandle, native_authenticator};
pub use backend::Backend;
pub use data::{OAuthToken, SecretData};
pub use detect::{BackendKind, VAULT_FILE, VaultBackend, open_native};
pub use error::VaultError;
pub use file::FileBackend;
pub use kdf::KdfParams;
pub use key::SecretKey;
pub use keychain::KeychainBackend;
pub use lock::{HOLD, LockState, MAX_FAILURES, RESUME_GAP};
pub use memory::MemoryBackend;
#[cfg(windows)]
pub use os_events::WINDOW_CLASS;
pub use os_events::{OsEvent, OsEventsWatch, watch_os_events};
pub use password::{MIN_PASSWORD_CHARS, VaultPassword};
pub use policy::{
    AUTO_LOCK_MINUTES, AutoLock, DEFAULT_AUTO_LOCK_MINUTES, DEFAULT_PRODUCTION_WINDOW_MINUTES,
    PRODUCTION_WINDOW_MINUTES, Policy,
};
pub use store::{SecretState, SecretStatus, SecretStore};
