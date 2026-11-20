//! Files a request uses from outside the project. The request file holds a
//! `local:<12 hex>` reference; the real path lives in the app's local data
//! of each device, so it is never committed.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// What a reference to a file outside the project starts with.
pub const LOCAL_PREFIX: &str = "local:";

const ID_LEN: usize = 12;

/// Whether `value` is `local:` followed by exactly 12 lowercase hex digits.
#[must_use]
pub fn is_local_ref(value: &str) -> bool {
    value.strip_prefix(LOCAL_PREFIX).is_some_and(|id| {
        id.len() == ID_LEN && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

/// Checks a file field of a request or folder: a plain path is left to
/// resolving, but a value that starts like a local reference must be one.
pub(crate) fn check_file_ref(value: &str) -> Result<(), String> {
    if value.starts_with(LOCAL_PREFIX) && !is_local_ref(value) {
        return Err(format!(
            "\"{value}\" is not a file reference: \"{LOCAL_PREFIX}\" is followed by 12 characters \
             of 0-9 and a-f"
        ));
    }
    Ok(())
}

/// Where this device keeps the files that `local:` references name.
pub trait LocalFiles {
    /// The path of the file `id` (the whole `local:<12 hex>` reference).
    fn path(&self, id: &str) -> Option<PathBuf>;
}

impl LocalFiles for BTreeMap<String, PathBuf> {
    fn path(&self, id: &str) -> Option<PathBuf> {
        self.get(id).cloned()
    }
}

/// A device that picked no files.
pub struct NoLocalFiles;

impl LocalFiles for NoLocalFiles {
    fn path(&self, _id: &str) -> Option<PathBuf> {
        None
    }
}
