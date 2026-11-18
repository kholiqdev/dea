use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use zeroize::Zeroizing;

use crate::{Backend, VaultError};

/// Keeps values in memory only. For tests and previews.
#[derive(Default)]
pub struct MemoryBackend {
    values: Mutex<HashMap<String, Zeroizing<String>>>,
}

impl Backend for MemoryBackend {
    fn get(&self, account: &str) -> Result<Option<Zeroizing<String>>, VaultError> {
        let values = self.values.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(values.get(account).cloned())
    }

    fn set(&self, account: &str, value: &str) -> Result<(), VaultError> {
        let mut values = self.values.lock().unwrap_or_else(PoisonError::into_inner);
        values.insert(account.to_string(), Zeroizing::new(value.to_string()));
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), VaultError> {
        let mut values = self.values.lock().unwrap_or_else(PoisonError::into_inner);
        values.remove(account);
        Ok(())
    }
}
