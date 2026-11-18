//! Reports changes made to project files outside Dea, for example by an
//! editor or a `git pull`.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime};

use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

use crate::ProjectError;

const DEBOUNCE: Duration = Duration::from_millis(250);
/// A batch is delivered once the project has been quiet this long...
const QUIET: Duration = Duration::from_millis(250);
/// ...or at the latest this long after its first change, so a long running
/// checkout still shows progress.
const MAX_WAIT: Duration = Duration::from_secs(2);

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

/// Size and modification time of a known file, to notice edits during a
/// rescan when the OS could not say which files changed.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

impl Stamp {
    fn of(meta: &fs::Metadata) -> Self {
        Stamp {
            len: meta.len(),
            modified: meta.modified().ok(),
        }
    }
}

/// Files known to exist, relative to the root.
type Known = HashMap<PathBuf, Stamp>;

/// Watches a project folder until dropped.
pub struct ProjectWatcher {
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl ProjectWatcher {
    /// Calls `on_batch` from a background thread with every file change,
    /// grouped into batches. Each path appears once per batch. Changes are
    /// held until the project is quiet for 250 ms (at most 2 s), so a large
    /// `git checkout` arrives as one batch whatever the disk speed.
    pub fn start(
        root: &Path,
        on_batch: impl Fn(Vec<ProjectChange>) + Send + 'static,
    ) -> Result<Self, ProjectError> {
        // OS watchers report resolved paths (on macOS, /var is /private/var).
        let root = fs::canonicalize(root)?;
        let mut known = Known::new();
        list_files(&root, &root, &mut known);

        // Dropping the watcher drops `tx`, which ends the coalescing thread
        // after it delivers anything still pending.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || coalesce(&rx, on_batch));

        let handler_root = root.clone();
        let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
            let batch = handle(&handler_root, result, &mut known);
            if !batch.is_empty() {
                let _ = tx.send(batch);
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

/// Merges batches from the debouncer and delivers them once quiet.
fn coalesce(rx: &Receiver<Vec<ProjectChange>>, on_batch: impl Fn(Vec<ProjectChange>)) {
    let mut pending: BTreeMap<PathBuf, ChangeKind> = BTreeMap::new();
    let mut first: Option<Instant> = None;
    let flush = |pending: &mut BTreeMap<PathBuf, ChangeKind>| {
        if !pending.is_empty() {
            let batch = std::mem::take(pending)
                .into_iter()
                .map(|(path, kind)| ProjectChange { path, kind })
                .collect();
            on_batch(batch);
        }
    };
    loop {
        let received = match first {
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(since) => rx.recv_timeout(QUIET.min(MAX_WAIT.saturating_sub(since.elapsed()))),
        };
        match received {
            Ok(batch) => {
                first.get_or_insert_with(Instant::now);
                for change in batch {
                    match merge(pending.get(&change.path).cloned(), change.kind) {
                        Some(kind) => pending.insert(change.path, kind),
                        None => pending.remove(&change.path),
                    };
                }
                if pending.is_empty() {
                    first = None;
                } else if first.is_some_and(|since| since.elapsed() >= MAX_WAIT) {
                    flush(&mut pending);
                    first = None;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                flush(&mut pending);
                first = None;
            }
            Err(RecvTimeoutError::Disconnected) => {
                flush(&mut pending);
                return;
            }
        }
    }
}

/// Combines what happened to one path across batches. A file that was
/// created and then removed again never existed as far as Dea knows.
fn merge(before: Option<ChangeKind>, next: ChangeKind) -> Option<ChangeKind> {
    use ChangeKind::*;
    match (before, next) {
        (None, next) => Some(next),
        (Some(Created), Removed) => None,
        (Some(Created), _) => Some(Created),
        (Some(Removed), Removed) => Some(Removed),
        (Some(Removed), _) => Some(Modified),
        (Some(Modified), next) => Some(if next == Removed { Removed } else { Modified }),
    }
}

/// Turns one result from the debouncer into changes. When the OS lost
/// events (a full inotify queue, an overflowing Windows buffer, a macOS
/// "rescan" flag) or the watcher failed, the whole tree is compared with
/// the known files instead, so no change is missed silently.
fn handle(root: &Path, result: DebounceEventResult, known: &mut Known) -> Vec<ProjectChange> {
    match result {
        Ok(events) if !events.iter().any(|e| e.need_rescan()) => {
            let touched = events.iter().flat_map(|e| e.paths.iter());
            changes(root, touched, known)
        }
        _ => rescan(root, known),
    }
}

/// Compares every file on disk with the known files: new files are
/// created, missing ones removed, and a different size or modification
/// time means modified.
fn rescan(root: &Path, known: &mut Known) -> Vec<ProjectChange> {
    let mut now = Known::new();
    list_files(root, root, &mut now);
    let mut out: BTreeMap<PathBuf, ChangeKind> = BTreeMap::new();
    for (path, stamp) in &now {
        match known.get(path) {
            None => {
                out.insert(path.clone(), ChangeKind::Created);
            }
            Some(old) if old != stamp => {
                out.insert(path.clone(), ChangeKind::Modified);
            }
            Some(_) => {}
        }
    }
    for path in known.keys() {
        if !now.contains_key(path) {
            out.insert(path.clone(), ChangeKind::Removed);
        }
    }
    *known = now;
    out.into_iter()
        .map(|(path, kind)| ProjectChange { path, kind })
        .collect()
}

/// Turns the paths touched by a batch of OS events into changes. The kind
/// comes from the file's state now, compared with the files known before,
/// because OS events do not reliably say whether a file is new (macOS
/// reports edits of recent files as creations, and an atomic save is a
/// rename). Files created and removed within one batch are not reported.
fn changes<'p>(
    root: &Path,
    touched: impl Iterator<Item = &'p PathBuf>,
    known: &mut Known,
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
                let mut inside = Known::new();
                list_files(root, path, &mut inside);
                for file in inside.into_keys() {
                    if !known.contains_key(&file) {
                        out.insert(file, ChangeKind::Created);
                    }
                }
            }
            Ok(_) => {
                let kind = if known.contains_key(rel) {
                    ChangeKind::Modified
                } else {
                    ChangeKind::Created
                };
                out.insert(rel.to_path_buf(), kind);
            }
            Err(_) => {
                // Gone. It may be a file, or a folder holding known files.
                for file in known.keys().filter(|f| f.starts_with(rel)) {
                    if !root.join(file).exists() {
                        out.insert(file.clone(), ChangeKind::Removed);
                    }
                }
            }
        }
    }
    for (path, kind) in &out {
        match kind {
            ChangeKind::Removed => {
                known.remove(path);
            }
            _ => {
                if let Ok(meta) = fs::metadata(root.join(path)) {
                    known.insert(path.clone(), Stamp::of(&meta));
                }
            }
        }
    }
    out.into_iter()
        .map(|(path, kind)| ProjectChange { path, kind })
        .collect()
}

/// Adds every file under `dir` that is not ignored, relative to `root`.
fn list_files(root: &Path, dir: &Path, out: &mut Known) {
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
                if let Ok(meta) = entry.metadata() {
                    out.insert(rel.to_path_buf(), Stamp::of(&meta));
                }
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

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use notify_debouncer_full::DebouncedEvent;
    use notify_debouncer_full::notify::event::{EventKind, Flag};
    use notify_debouncer_full::notify::{Error, Event};

    use super::*;

    #[test]
    fn merging_keeps_what_happened_overall() {
        use ChangeKind::*;
        assert_eq!(merge(None, Modified), Some(Modified));
        assert_eq!(merge(Some(Created), Modified), Some(Created));
        assert_eq!(merge(Some(Created), Removed), None);
        assert_eq!(merge(Some(Removed), Created), Some(Modified));
        assert_eq!(merge(Some(Modified), Removed), Some(Removed));
        assert_eq!(merge(Some(Modified), Created), Some(Modified));
    }

    fn kinds(changes: Vec<ProjectChange>) -> BTreeMap<String, ChangeKind> {
        changes
            .into_iter()
            .map(|c| (c.path.to_string_lossy().replace('\\', "/"), c.kind))
            .collect()
    }

    /// A project with `a.yaml` and `b.yaml`, then: `a.yaml` edited,
    /// `b.yaml` deleted, `c.yaml` added, with no OS event naming them.
    fn changed_behind_our_back() -> (tempfile::TempDir, PathBuf, Known) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::write(root.join("a.yaml"), "a: 1\n").unwrap();
        fs::write(root.join("b.yaml"), "b: 1\n").unwrap();
        let mut known = Known::new();
        list_files(&root, &root, &mut known);
        fs::write(root.join("a.yaml"), "a: 1\nmore: 2\n").unwrap();
        fs::remove_file(root.join("b.yaml")).unwrap();
        fs::write(root.join("c.yaml"), "c: 1\n").unwrap();
        (dir, root, known)
    }

    fn expected() -> BTreeMap<String, ChangeKind> {
        BTreeMap::from([
            ("a.yaml".to_string(), ChangeKind::Modified),
            ("b.yaml".to_string(), ChangeKind::Removed),
            ("c.yaml".to_string(), ChangeKind::Created),
        ])
    }

    #[test]
    fn watcher_errors_trigger_a_rescan() {
        let (_dir, root, mut known) = changed_behind_our_back();
        let result = Err(vec![Error::generic("event queue overflow")]);
        assert_eq!(kinds(handle(&root, result, &mut known)), expected());
        // The known files now match the disk, so nothing is reported twice.
        let again = Err(vec![Error::generic("event queue overflow")]);
        assert!(handle(&root, again, &mut known).is_empty());
    }

    #[test]
    fn rescan_events_trigger_a_rescan() {
        let (_dir, root, mut known) = changed_behind_our_back();
        let event = Event::new(EventKind::Other).set_flag(Flag::Rescan);
        let result = Ok(vec![DebouncedEvent::new(event, Instant::now())]);
        assert_eq!(kinds(handle(&root, result, &mut known)), expected());
    }
}
