//! Files a request sends or uses for TLS, as the editor shows them: a path
//! inside the project, or a `local:` reference to a file outside it that
//! only this device knows.

use std::path::{Path, PathBuf};

use dea_project::is_local_ref;
use serde::Serialize;

use crate::projects::portable_path;
use crate::store::{Store, StoreError};

/// A file field of a request or folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct FileRef {
    /// What the request file holds: a path relative to the project with
    /// `/`, or a `local:<id>` reference.
    pub value: String,
    /// The file name without its folder; the reference itself when this
    /// device does not know the file.
    pub name: String,
    /// The file is outside the project, so `value` is a `local:` reference
    /// and the file is not committed.
    pub outside: bool,
    /// The file is not on this device.
    pub missing: bool,
}

impl FileRef {
    /// The value to store for `picked`, a file the person chose: its path
    /// relative to the project at `root`, or for a file outside the project
    /// a `local:` reference that `store` keeps for project `scope`.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the reference cannot be saved.
    pub fn picked(
        store: &Store,
        root: &Path,
        scope: &str,
        picked: &Path,
    ) -> Result<FileRef, StoreError> {
        let name = file_name(picked);
        if let Some(inside) = relative_to(root, picked) {
            return Ok(FileRef {
                value: portable_path(&inside),
                name,
                outside: false,
                missing: false,
            });
        }
        Ok(FileRef {
            value: store.add_local_file(scope, picked)?,
            name,
            outside: true,
            missing: false,
        })
    }

    /// `value` from a request file of the project at `root` as the editor
    /// shows it. A `local:` reference is looked up in what `store` keeps
    /// for project `scope`.
    #[must_use]
    pub fn describe(store: &Store, root: &Path, scope: &str, value: &str) -> FileRef {
        if is_local_ref(value) {
            let path = store.local_files(scope).remove(value);
            return FileRef {
                value: value.to_string(),
                name: path.as_deref().map_or_else(|| value.to_string(), file_name),
                outside: true,
                missing: !path.is_some_and(|p| p.is_file()),
            };
        }
        let path: PathBuf = value.split('/').collect();
        FileRef {
            value: value.to_string(),
            name: file_name(&path),
            outside: false,
            missing: !root.join(&path).is_file(),
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// `path` relative to `root` when it is inside it. Both are compared as
/// the file system resolves them, so a link to the project still counts.
fn relative_to(root: &Path, path: &Path) -> Option<PathBuf> {
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let inside = canonical(path)
        .strip_prefix(canonical(root))
        .ok()?
        .to_path_buf();
    (!inside.as_os_str().is_empty()).then_some(inside)
}
