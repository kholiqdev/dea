use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

use crate::VaultError;

/// Argon2id cost. Stored next to what it protects, so the defaults can be
/// raised later without breaking files written earlier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl Default for KdfParams {
    /// 64 MiB, 3 iterations, 1 lane.
    fn default() -> Self {
        KdfParams {
            memory_kib: 65_536,
            iterations: 3,
            parallelism: 1,
        }
    }
}

impl KdfParams {
    /// The cheapest valid cost. Only for tests, where the default takes
    /// seconds per call in debug builds.
    pub fn insecure_for_tests() -> Self {
        KdfParams {
            memory_kib: 64,
            iterations: 1,
            parallelism: 1,
        }
    }

    pub(crate) fn argon2(self) -> Result<Argon2<'static>, VaultError> {
        let params = Params::new(self.memory_kib, self.iterations, self.parallelism, Some(32))
            .map_err(|_| VaultError::Corrupt("invalid key derivation parameters".into()))?;
        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
    }

    /// A 256-bit key from `password` and `salt`.
    pub(crate) fn derive_key(
        self,
        password: &[u8],
        salt: &[u8],
    ) -> Result<Zeroizing<[u8; 32]>, VaultError> {
        let mut key = Zeroizing::new([0u8; 32]);
        self.argon2()?
            .hash_password_into(password, salt, key.as_mut())
            .map_err(|_| VaultError::Corrupt("the key could not be derived".into()))?;
        Ok(key)
    }
}
