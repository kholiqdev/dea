//! Secrets for Dea. Values live in the operating system keychain, or in an
//! encrypted file where Linux has no Secret Service. Project files only name
//! the secrets they need (K10).

mod backend;
mod data;
mod error;
mod index;
mod key;
mod memory;
mod store;

pub use backend::Backend;
pub use data::{OAuthToken, SecretData};
pub use error::VaultError;
pub use key::SecretKey;
pub use memory::MemoryBackend;
pub use store::{SecretState, SecretStatus, SecretStore};
