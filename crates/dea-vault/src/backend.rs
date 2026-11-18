use zeroize::Zeroizing;

use crate::VaultError;

/// Where secret values are kept, one text value per account.
pub trait Backend: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<Zeroizing<String>>, VaultError>;
    fn set(&self, account: &str, value: &str) -> Result<(), VaultError>;
    /// Deleting an account that does not exist is not an error.
    fn delete(&self, account: &str) -> Result<(), VaultError>;
}
