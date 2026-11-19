use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dea_project::{SecretKind, VaultManifest};
use time::OffsetDateTime;

use crate::index::{Index, IndexEntry};
use crate::{Backend, SecretData, SecretKey, VaultError};

/// Secret values in a [`Backend`], plus metadata about them in
/// `index.json`. The index decides which secrets exist on this device.
pub struct SecretStore {
    backend: Arc<dyn Backend>,
    index: Mutex<Index>,
}

/// One secret in one environment, as the Vault screen lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct SecretStatus {
    pub name: String,
    pub kind: SecretKind,
    pub description: Option<String>,
    pub environment: String,
    /// Listed in `vault.yaml` for an environment of the project. A value
    /// left over after the secret or environment was removed is not.
    pub required: bool,
    pub state: SecretState,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SecretState {
    /// Required, but there is no value on this device.
    Missing,
    Present {
        updated_at: OffsetDateTime,
        expires_at: Option<OffsetDateTime>,
    },
}

impl SecretStore {
    /// Opens the store whose metadata lives in `dir`. Nothing is written
    /// until a secret is set.
    pub fn open(dir: &Path, backend: Arc<dyn Backend>) -> Result<Self, VaultError> {
        Ok(SecretStore {
            backend,
            index: Mutex::new(Index::load(dir)?),
        })
    }

    fn index(&self) -> MutexGuard<'_, Index> {
        self.index.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set(
        &self,
        key: &SecretKey,
        kind: SecretKind,
        data: &SecretData,
    ) -> Result<(), VaultError> {
        let mut index = self.index();
        self.backend.set(&key.account(), &data.encode())?;
        index.upsert(IndexEntry {
            scope: key.scope.clone(),
            environment: key.environment.clone(),
            name: key.name.clone(),
            kind,
            updated_at: OffsetDateTime::now_utc(),
            expires_at: data.expires_at(),
        });
        index.save()
    }

    /// The value, or `None` when this device has none. A value that was
    /// removed outside Dea is dropped from the index and reported as `None`.
    pub fn get(&self, key: &SecretKey) -> Result<Option<SecretData>, VaultError> {
        let mut index = self.index();
        let Some(kind) = index.find(key).map(|e| e.kind) else {
            return Ok(None);
        };
        match self.backend.get(&key.account())? {
            Some(text) => SecretData::decode(kind, &text).map(Some),
            None => {
                index.remove(key);
                index.save()?;
                Ok(None)
            }
        }
    }

    pub fn delete(&self, key: &SecretKey) -> Result<(), VaultError> {
        let mut index = self.index();
        self.backend.delete(&key.account())?;
        if index.remove(key) {
            index.save()?;
        }
        Ok(())
    }

    /// Whether `environment` of `scope` has a value on this device. Reads
    /// only the index, so it works while the backend is locked.
    pub fn has_values(&self, scope: &str, environment: &str) -> bool {
        self.index()
            .entries
            .iter()
            .any(|e| e.scope == scope && e.environment == environment)
    }

    /// Moves every value of environment `from` in `scope` to `to`, keeping
    /// type and dates, for a renamed environment. Values `to` already had
    /// are removed first, so `to` ends with exactly what `from` had and
    /// gains nothing left behind by an earlier environment of that name.
    pub fn move_environment(&self, scope: &str, from: &str, to: &str) -> Result<(), VaultError> {
        if from == to {
            return Ok(());
        }
        let mut index = self.index();
        // The index is saved even after a failure, so it keeps matching the
        // backend for every value handled so far.
        let moved = self.remove_in(&mut index, scope, to).and_then(|()| {
            let from_keys = keys_in(&index, scope, from);
            for from_key in from_keys {
                let to_key = SecretKey {
                    environment: to.to_string(),
                    ..from_key.clone()
                };
                if let Some(text) = self.backend.get(&from_key.account())? {
                    self.backend.set(&to_key.account(), &text)?;
                    if let Some(entry) = index.entries.iter_mut().find(|e| e.is(&from_key)) {
                        entry.environment = to.to_string();
                    }
                } else {
                    // Removed outside Dea: nothing to move.
                    index.remove(&from_key);
                }
                self.backend.delete(&from_key.account())?;
            }
            Ok(())
        });
        let saved = index.save();
        moved.and(saved)
    }

    /// Removes every value of `environment` in `scope`, for a deleted
    /// environment.
    pub fn delete_environment(&self, scope: &str, environment: &str) -> Result<(), VaultError> {
        let mut index = self.index();
        let removed = self.remove_in(&mut index, scope, environment);
        let saved = index.save();
        removed.and(saved)
    }

    /// Removes the values of `environment` from the backend and `index`,
    /// without saving the index.
    fn remove_in(
        &self,
        index: &mut Index,
        scope: &str,
        environment: &str,
    ) -> Result<(), VaultError> {
        for key in keys_in(index, scope, environment) {
            self.backend.delete(&key.account())?;
            index.remove(&key);
        }
        Ok(())
    }

    /// Every secret `manifest` requires in each of `environments`, in
    /// manifest order then environment order, followed by stored values of
    /// this project that are no longer required.
    pub fn statuses(
        &self,
        scope: &str,
        manifest: &VaultManifest,
        environments: &[String],
    ) -> Vec<SecretStatus> {
        let index = self.index();
        let state = |entry: Option<&IndexEntry>| match entry {
            Some(e) => SecretState::Present {
                updated_at: e.updated_at,
                expires_at: e.expires_at,
            },
            None => SecretState::Missing,
        };
        let mut out = Vec::new();
        for spec in &manifest.secrets {
            for environment in environments {
                let entry = index.entries.iter().find(|e| {
                    e.scope == scope && &e.environment == environment && e.name == spec.name
                });
                out.push(SecretStatus {
                    name: spec.name.clone(),
                    kind: spec.kind,
                    description: spec.description.clone(),
                    environment: environment.clone(),
                    required: true,
                    state: state(entry),
                });
            }
        }
        let mut extra: Vec<&IndexEntry> = index
            .entries
            .iter()
            .filter(|e| {
                e.scope == scope
                    && !(environments.contains(&e.environment)
                        && manifest.secrets.iter().any(|s| s.name == e.name))
            })
            .collect();
        extra.sort_by(|a, b| (&a.environment, &a.name).cmp(&(&b.environment, &b.name)));
        out.extend(extra.into_iter().map(|e| SecretStatus {
            name: e.name.clone(),
            kind: e.kind,
            description: None,
            environment: e.environment.clone(),
            required: false,
            state: state(Some(e)),
        }));
        out
    }
}

/// The keys of every value of `environment` in `scope`.
fn keys_in(index: &Index, scope: &str, environment: &str) -> Vec<SecretKey> {
    index
        .entries
        .iter()
        .filter(|e| e.scope == scope && e.environment == environment)
        .map(|e| SecretKey {
            scope: e.scope.clone(),
            environment: e.environment.clone(),
            name: e.name.clone(),
        })
        .collect()
}
