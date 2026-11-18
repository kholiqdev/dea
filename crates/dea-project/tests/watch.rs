use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use dea_project::{ChangeKind, ProjectChange, ProjectWatcher, create_project, write_if_changed};
use tempfile::TempDir;

struct Watched {
    dir: TempDir,
    rx: Receiver<Vec<ProjectChange>>,
    _watcher: ProjectWatcher,
}

/// A fresh project with a running watcher and no pending events.
fn watched() -> Watched {
    let dir = tempfile::tempdir().unwrap();
    create_project(dir.path(), "harbor-api").unwrap();
    let (tx, rx) = channel();
    let watcher = ProjectWatcher::start(dir.path(), move |batch| {
        let _ = tx.send(batch);
    })
    .unwrap();
    // Let the OS watcher settle and drop anything from project creation.
    std::thread::sleep(Duration::from_millis(400));
    while rx.try_recv().is_ok() {}
    Watched {
        dir,
        rx,
        _watcher: watcher,
    }
}

/// Collects changes until `done` holds or `limit` passes. Returns the last
/// kind per path and the number of batches.
fn collect_until(
    rx: &Receiver<Vec<ProjectChange>>,
    limit: Duration,
    done: impl Fn(&BTreeMap<PathBuf, ChangeKind>) -> bool,
) -> (BTreeMap<PathBuf, ChangeKind>, usize) {
    let start = Instant::now();
    let mut seen = BTreeMap::new();
    let mut batches = 0;
    while !done(&seen) {
        let Some(left) = limit.checked_sub(start.elapsed()) else {
            break;
        };
        match rx.recv_timeout(left) {
            Ok(batch) => {
                batches += 1;
                for change in batch {
                    seen.insert(change.path, change.kind);
                }
            }
            Err(_) => break,
        }
    }
    (seen, batches)
}

fn rel(p: &str) -> PathBuf {
    PathBuf::from(p)
}

#[test]
fn modification_reported_within_one_second() {
    let w = watched();
    let start = Instant::now();
    fs::write(
        w.dir.path().join("environments/local.yaml"),
        "name: local\nproduction: false\ncolor: blue\n",
    )
    .unwrap();
    let target = rel("environments/local.yaml");
    let (seen, _) = collect_until(&w.rx, Duration::from_secs(1), |s| s.contains_key(&target));
    assert_eq!(seen.get(&target), Some(&ChangeKind::Modified), "{seen:?}");
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn atomic_save_is_a_modification() {
    let w = watched();
    write_if_changed(
        &w.dir.path().join("environments/staging.yaml"),
        "name: staging\nproduction: false\ncolor: green\n",
    )
    .unwrap();
    let target = rel("environments/staging.yaml");
    let (seen, _) = collect_until(&w.rx, Duration::from_secs(1), |s| s.contains_key(&target));
    assert_eq!(seen.get(&target), Some(&ChangeKind::Modified), "{seen:?}");
    // The temporary file used for the atomic write is never reported.
    assert!(
        seen.keys()
            .all(|p| !p.to_string_lossy().contains(".dea-write-")),
        "{seen:?}"
    );
}

#[test]
fn new_file_is_created() {
    let w = watched();
    fs::write(
        w.dir.path().join("environments/qa.yaml"),
        "name: qa\nproduction: false\n",
    )
    .unwrap();
    let target = rel("environments/qa.yaml");
    let (seen, _) = collect_until(&w.rx, Duration::from_secs(1), |s| s.contains_key(&target));
    assert_eq!(seen.get(&target), Some(&ChangeKind::Created), "{seen:?}");
}

#[test]
fn removal_reported() {
    let w = watched();
    fs::remove_file(w.dir.path().join("environments/staging.yaml")).unwrap();
    let target = rel("environments/staging.yaml");
    let (seen, _) = collect_until(&w.rx, Duration::from_secs(1), |s| s.contains_key(&target));
    assert_eq!(seen.get(&target), Some(&ChangeKind::Removed), "{seen:?}");
}

#[test]
fn removing_a_folder_reports_each_file() {
    let w = watched();
    fs::remove_dir_all(w.dir.path().join("environments")).unwrap();
    let files = [
        rel("environments/local.yaml"),
        rel("environments/production.yaml"),
        rel("environments/staging.yaml"),
    ];
    let (seen, _) = collect_until(&w.rx, Duration::from_secs(2), |s| {
        files.iter().all(|f| s.contains_key(f))
    });
    for f in &files {
        assert_eq!(seen.get(f), Some(&ChangeKind::Removed), "{f:?} in {seen:?}");
    }
}

#[test]
fn checkout_of_500_files_arrives_as_few_batches() {
    let w = watched();
    let requests = w.dir.path().join("requests");
    fs::create_dir(&requests).unwrap();
    let start = Instant::now();
    for i in 0..500 {
        fs::write(
            requests.join(format!("r{i:03}.yaml")),
            format!("name: r{i}\n"),
        )
        .unwrap();
    }
    let (seen, batches) = collect_until(&w.rx, Duration::from_secs(3), |s| {
        s.keys().filter(|p| p.starts_with("requests")).count() >= 500
    });
    let files = seen.keys().filter(|p| p.starts_with("requests")).count();
    assert_eq!(files, 500);
    assert!(batches <= 5, "{batches} batches");
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(seen.values().all(|k| *k == ChangeKind::Created), "{seen:?}");
}

#[test]
fn ignores_git_backup_and_editor_temp_files() {
    let w = watched();
    let root = w.dir.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".git/index"), "x").unwrap();
    fs::create_dir_all(root.join(".dea-backup/x")).unwrap();
    fs::write(root.join(".dea-backup/x/dea.yaml"), "x").unwrap();
    fs::write(root.join("environments/.local.yaml.swp"), "x").unwrap();
    fs::write(root.join("environments/local.yaml~"), "x").unwrap();
    fs::write(root.join("environments/.#local.yaml"), "x").unwrap();
    fs::write(root.join("environments/4913"), "x").unwrap();
    if let Ok(batch) = w.rx.recv_timeout(Duration::from_secs(1)) {
        panic!("unexpected batch: {batch:?}");
    }
}
