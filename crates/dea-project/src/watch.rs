//! Reports changes made to project files outside Dea, for example by an
//! editor or a `git pull`.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

use crate::ProjectError;

const DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeKind {
    Created,
    Modified,
    Removed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectChange {
    /// Path relative to the project root.
    pub path: PathBuf,
    pub kind: ChangeKind,
}

/// Watches a project folder until dropped.
pub struct ProjectWatcher {
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl ProjectWatcher {
    /// Calls `on_batch` from a background thread with every file change,
    /// grouped into batches. Each path appears once per batch.
    pub fn start(
        root: &Path,
        on_batch: impl Fn(Vec<ProjectChange>) + Send + 'static,
    ) -> Result<Self, ProjectError> {
        // OS watchers report resolved paths (on macOS, /var is /private/var).
        let root = fs::canonicalize(root)?;
        let mut known = HashSet::new();
        list_files(&root, &root, &mut known);

        let handler_root = root.clone();
        let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
            let Ok(events) = result else {
                return;
            };
            let touched = events.iter().flat_map(|e| e.paths.iter());
            let batch = changes(&handler_root, touched, &mut known);
            if !batch.is_empty() {
                on_batch(batch);
            }
        })
        .map_err(|e| ProjectError::Io(std::io::Error::other(e)))?;
        debouncer
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|e| ProjectError::Io(std::io::Error::other(e)))?;
        Ok(Self {
            _debouncer: debouncer,
        })
    }
}

/// Turns the paths touched by a batch of OS events into changes. The kind
/// comes from the file's state now, compared with the files known before,
/// because OS events do not reliably say whether a file is new (macOS
/// reports edits of recent files as creations, and an atomic save is a
/// rename). Files created and removed within one batch are not reported.
fn changes<'p>(
    root: &Path,
    touched: impl Iterator<Item = &'p PathBuf>,
    known: &mut HashSet<PathBuf>,
) -> Vec<ProjectChange> {
    let mut out: BTreeMap<PathBuf, ChangeKind> = BTreeMap::new();
    for path in touched {
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        if rel.as_os_str().is_empty() || is_ignored(rel) {
            continue;
        }
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.is_dir() => {
                // A folder that appeared, for example by a rename: its files.
                let mut inside = HashSet::new();
                list_files(root, path, &mut inside);
                for file in inside {
                    if !known.contains(&file) {
                        out.insert(file, ChangeKind::Created);
                    }
                }
            }
            Ok(_) => {
                let kind = if known.contains(rel) {
                    ChangeKind::Modified
                } else {
                    ChangeKind::Created
                };
                out.insert(rel.to_path_buf(), kind);
            }
            Err(_) => {
                // Gone. It may be a file, or a folder holding known files.
                for file in known.iter().filter(|f| f.starts_with(rel)) {
                    if !root.join(file).exists() {
                        out.insert(file.clone(), ChangeKind::Removed);
                    }
                }
            }
        }
    }
    for (path, kind) in &out {
        match kind {
            ChangeKind::Removed => known.remove(path),
            _ => known.insert(path.clone()),
        };
    }
    out.into_iter()
        .map(|(path, kind)| ProjectChange { path, kind })
        .collect()
}

/// Adds every file under `dir` that is not ignored, relative to `root`.
fn list_files(root: &Path, dir: &Path, out: &mut HashSet<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        if is_ignored(rel) {
            continue;
        }
        match entry.file_type() {
            Ok(t) if t.is_dir() => list_files(root, &path, out),
            Ok(t) if t.is_file() => {
                out.insert(rel.to_path_buf());
            }
            _ => {}
        }
    }
}

/// Git data, migration backups, and temporary files of editors and of Dea's
/// own atomic writes.
fn is_ignored(rel: &Path) -> bool {
    let in_ignored_folder = rel
        .components()
        .any(|c| matches!(c, Component::Normal(name) if name == ".git" || name == ".dea-backup"));
    if in_ignored_folder {
        return true;
    }
    let Some(name) = rel.file_name().map(|n| n.to_string_lossy()) else {
        return false;
    };
    name.ends_with(".swp")
        || name.ends_with('~')
        || name.starts_with(".#")
        || name == "4913"
        || name == ".DS_Store"
        || name.starts_with(".dea-write-")
}
