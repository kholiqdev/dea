//! Reports changes made to project files outside Dea, for example by an
//! editor or a `git pull`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime};

use notify::event::EventKind;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::ProjectError;

/// A batch is delivered once the project has been quiet this long...
const QUIET: Duration = Duration::from_millis(300);
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
    _watcher: RecommendedWatcher,
}

impl ProjectWatcher {
    /// Calls `on_batch` from a background thread with every file change,
    /// grouped into batches. Each path appears once per batch. Changes are
    /// held until the project is quiet for 300 ms (at most 2 s), so a large
    /// `git checkout` arrives as one batch whatever the disk speed.
    ///
    /// OS events only say which paths were touched. Their kinds are not
    /// trusted: macOS merges the flags of a file's recent history, so a new
    /// file that is deleted again looks created, and an event deduplicator
    /// would drop the pair. What changed is decided by comparing the disk
    /// with the files known before.
    pub fn start(
        root: &Path,
        on_batch: impl Fn(Vec<ProjectChange>) + Send + 'static,
    ) -> Result<Self, ProjectError> {
        // OS watchers report resolved paths (on macOS, /var is /private/var).
        let root = fs::canonicalize(root)?;
        let mut known = Known::new();
        list_files(&root, &root, &mut known);

        // Dropping the watcher drops `tx`, which ends the batching thread
        // after it delivers anything still pending.
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = tx.send(event);
        })
        .map_err(|e| ProjectError::Io(std::io::Error::other(e)))?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|e| ProjectError::Io(std::io::Error::other(e)))?;
        std::thread::spawn(move || batch_events(&root, known, &rx, on_batch));
        Ok(Self { _watcher: watcher })
    }
}

/// Collects OS events until the project is quiet, then reports the changes.
fn batch_events(
    root: &Path,
    mut known: Known,
    rx: &Receiver<notify::Result<Event>>,
    on_batch: impl Fn(Vec<ProjectChange>),
) {
    let mut pending: Vec<notify::Result<Event>> = Vec::new();
    let mut first: Option<Instant> = None;
    let flush = |pending: &mut Vec<notify::Result<Event>>, known: &mut Known| {
        let batch = process(root, std::mem::take(pending), known);
        if !batch.is_empty() {
            on_batch(batch);
        }
    };
    loop {
        let received = match first {
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(since) => rx.recv_timeout(QUIET.min(MAX_WAIT.saturating_sub(since.elapsed()))),
        };
        match received {
            Ok(event) => {
                pending.push(event);
                let since = *first.get_or_insert_with(Instant::now);
                if since.elapsed() >= MAX_WAIT {
                    flush(&mut pending, &mut known);
                    first = None;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                flush(&mut pending, &mut known);
                first = None;
            }
            Err(RecvTimeoutError::Disconnected) => {
                flush(&mut pending, &mut known);
                return;
            }
        }
    }
}

/// Turns a batch of OS events into changes. When the OS lost events (a full
/// inotify queue, an overflowing Windows buffer, a macOS "rescan" flag) or
/// the watcher failed, the whole tree is compared with the known files
/// instead, so no change is missed silently. Events that only read a file
/// are ignored.
fn process(
    root: &Path,
    events: Vec<notify::Result<Event>>,
    known: &mut Known,
) -> Vec<ProjectChange> {
    let mut touched = BTreeSet::new();
    for event in events {
        match event {
            Ok(event) if event.need_rescan() => return rescan(root, known),
            Ok(event) if matches!(event.kind, EventKind::Access(_)) => {}
            Ok(event) => touched.extend(event.paths),
            Err(_) => return rescan(root, known),
        }
    }
    changes(root, touched.iter(), known)
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
/// rename). A known file is modified only when its size or modification
/// time differs. Files created and removed within one batch are not
/// reported.
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
            Ok(meta) => match known.get(rel) {
                None => {
                    out.insert(rel.to_path_buf(), ChangeKind::Created);
                }
                Some(old) if *old != Stamp::of(&meta) => {
                    out.insert(rel.to_path_buf(), ChangeKind::Modified);
                }
                // An event for a file that did not change, such as one macOS
                // delivers late for a write made before the watch started.
                Some(_) => {}
            },
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
    use notify::event::{AccessKind, CreateKind, Flag, RemoveKind};
    use notify::{Error, Event};

    use super::*;

    fn kinds(changes: Vec<ProjectChange>) -> BTreeMap<String, ChangeKind> {
        changes
            .into_iter()
            .map(|c| (c.path.to_string_lossy().replace('\\', "/"), c.kind))
            .collect()
    }

    /// A known project with `a.yaml`, `b.yaml` and `env/x.yaml`.
    fn project() -> (tempfile::TempDir, PathBuf, Known) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::write(root.join("a.yaml"), "a: 1\n").unwrap();
        fs::write(root.join("b.yaml"), "b: 1\n").unwrap();
        fs::create_dir(root.join("env")).unwrap();
        fs::write(root.join("env/x.yaml"), "x: 1\n").unwrap();
        let mut known = Known::new();
        list_files(&root, &root, &mut known);
        (dir, root, known)
    }

    /// `a.yaml` edited, `b.yaml` deleted, `c.yaml` added, with no OS event
    /// naming them.
    fn change_behind_our_back(root: &Path) {
        fs::write(root.join("a.yaml"), "a: 1\nmore: 2\n").unwrap();
        fs::remove_file(root.join("b.yaml")).unwrap();
        fs::write(root.join("c.yaml"), "c: 1\n").unwrap();
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
        let (_dir, root, mut known) = project();
        change_behind_our_back(&root);
        let events = vec![Err(Error::generic("event queue overflow"))];
        assert_eq!(kinds(process(&root, events, &mut known)), expected());
        // The known files now match the disk, so nothing is reported twice.
        let again = vec![Err(Error::generic("event queue overflow"))];
        assert!(process(&root, again, &mut known).is_empty());
    }

    #[test]
    fn rescan_events_trigger_a_rescan() {
        let (_dir, root, mut known) = project();
        change_behind_our_back(&root);
        let events = vec![Ok(Event::new(EventKind::Other).set_flag(Flag::Rescan))];
        assert_eq!(kinds(process(&root, events, &mut known)), expected());
    }

    #[test]
    fn a_file_reported_as_created_but_gone_is_removed() {
        // macOS reports a recently created file that is deleted with both
        // the created and removed flags.
        let (_dir, root, mut known) = project();
        fs::remove_file(root.join("b.yaml")).unwrap();
        let events = vec![
            Ok(Event::new(EventKind::Create(CreateKind::File)).add_path(root.join("b.yaml"))),
            Ok(Event::new(EventKind::Remove(RemoveKind::File)).add_path(root.join("b.yaml"))),
        ];
        assert_eq!(
            kinds(process(&root, events, &mut known)),
            BTreeMap::from([("b.yaml".to_string(), ChangeKind::Removed)])
        );
    }

    #[test]
    fn a_removed_folder_reports_each_known_file() {
        let (_dir, root, mut known) = project();
        fs::remove_dir_all(root.join("env")).unwrap();
        let events = vec![Ok(
            Event::new(EventKind::Remove(RemoveKind::Folder)).add_path(root.join("env"))
        )];
        assert_eq!(
            kinds(process(&root, events, &mut known)),
            BTreeMap::from([("env/x.yaml".to_string(), ChangeKind::Removed)])
        );
    }

    #[test]
    fn an_event_for_an_unchanged_file_is_not_a_change() {
        // macOS can deliver events late, for example for files written just
        // before the watch started, while the machine is busy.
        let (_dir, root, mut known) = project();
        let events = vec![Ok(
            Event::new(EventKind::Create(CreateKind::File)).add_path(root.join("a.yaml"))
        )];
        assert!(process(&root, events, &mut known).is_empty());
    }

    #[test]
    fn reading_a_file_is_not_a_change() {
        let (_dir, root, mut known) = project();
        let events = vec![Ok(
            Event::new(EventKind::Access(AccessKind::Read)).add_path(root.join("a.yaml"))
        )];
        assert!(process(&root, events, &mut known).is_empty());
    }
}
