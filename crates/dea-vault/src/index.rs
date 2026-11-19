//! Metadata about stored secrets, kept next to the secret store: which
//! secrets have a value on this device, their type and their dates. Never
//! holds a value, so the Vault screen can list secrets while locked.

use std::fs;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use dea_project::SecretKind;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{SecretKey, VaultError};

const FILE: &str = "index.json";
const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IndexEntry {
    pub scope: String,
    pub environment: String,
    pub name: String,
    pub kind: SecretKind,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
}

impl IndexEntry {
    pub(crate) fn is(&self, key: &SecretKey) -> bool {
        self.scope == key.scope && self.environment == key.environment && self.name == key.name
    }
}

#[derive(Serialize, Deserialize)]
struct IndexFile {
    version: u32,
    entries: Vec<IndexEntry>,
}

#[derive(Deserialize)]
struct VersionOnly {
    version: u32,
}

pub(crate) struct Index {
    dir: PathBuf,
    pub entries: Vec<IndexEntry>,
}

impl Index {
    /// Reads `<dir>/index.json`. A missing file is an empty index. A damaged
    /// file, or one from a newer Dea, is an error and is left untouched.
    pub fn load(dir: &Path) -> Result<Self, VaultError> {
        let path = dir.join(FILE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Ok(Index {
                    dir: dir.to_path_buf(),
                    entries: Vec::new(),
                });
            }
            Err(e) => return Err(VaultError::io(&path)(e)),
        };
        let damaged = || VaultError::Corrupt(format!("{} cannot be read", path.display()));
        let version: VersionOnly = serde_json::from_str(&text).map_err(|_| damaged())?;
        if version.version > VERSION {
            return Err(VaultError::Corrupt(format!(
                "{} was written by a newer version of Dea",
                path.display()
            )));
        }
        let file: IndexFile = serde_json::from_str(&text).map_err(|_| damaged())?;
        Ok(Index {
            dir: dir.to_path_buf(),
            entries: file.entries,
        })
    }

    /// Whether `environment` of `scope` has an entry.
    pub fn has_values(&self, scope: &str, environment: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.scope == scope && e.environment == environment)
    }

    pub fn find(&self, key: &SecretKey) -> Option<&IndexEntry> {
        self.entries.iter().find(|e| e.is(key))
    }

    pub fn upsert(&mut self, entry: IndexEntry) {
        match self.entries.iter_mut().find(|e| {
            e.scope == entry.scope && e.environment == entry.environment && e.name == entry.name
        }) {
            Some(existing) => *existing = entry,
            None => self.entries.push(entry),
        }
    }

    /// Returns whether an entry was removed.
    pub fn remove(&mut self, key: &SecretKey) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| !e.is(key));
        self.entries.len() != before
    }

    /// Writes the index atomically, readable by the owner only.
    pub fn save(&self) -> Result<(), VaultError> {
        let path = self.dir.join(FILE);
        let file = IndexFile {
            version: VERSION,
            entries: self.entries.clone(),
        };
        let text = serde_json::to_string_pretty(&file)
            .map_err(|e| VaultError::Corrupt(e.to_string()))?
            + "\n";
        write_private(&path, text.as_bytes())
    }
}

/// Creates `dir` and its parents if missing; on Unix new folders are 0700.
fn create_private_dir(dir: &Path) -> Result<(), VaultError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir).map_err(VaultError::io(dir))
}

/// Replaces `path` with `bytes` through a temporary file in the same folder,
/// so a crash never leaves half a file. On Unix the file is mode 0600.
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> Result<(), VaultError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    create_private_dir(dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(VaultError::io(dir))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tmp.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(VaultError::io(tmp.path()))?;
    }
    tmp.write_all(bytes).map_err(VaultError::io(tmp.path()))?;
    tmp.as_file()
        .sync_all()
        .map_err(VaultError::io(tmp.path()))?;
    tmp.persist(path)
        .map_err(|e| VaultError::io(path)(e.error))?;
    Ok(())
}
