//! Secrets for Dea. Values live in the operating system keychain, or in an
//! encrypted file where Linux has no Secret Service. Project files only name
//! the secrets they need (K10).

mod backend;
mod data;
mod error;
mod file;
mod index;
mod kdf;
mod key;
mod memory;
mod password;
mod store;

pub use backend::Backend;
pub use data::{OAuthToken, SecretData};
pub use error::VaultError;
pub use file::FileBackend;
pub use kdf::KdfParams;
pub use key::SecretKey;
pub use memory::MemoryBackend;
pub use password::{MIN_PASSWORD_CHARS, VaultPassword};
pub use store::{SecretState, SecretStatus, SecretStore};
