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
    // Drop events from project creation. macOS can deliver writes made just
    // before the watch started a while later, especially on slow CI
    // machines, so wait until nothing has arrived for half a second.
    let settle = Instant::now();
    while settle.elapsed() < Duration::from_secs(5)
        && rx.recv_timeout(Duration::from_millis(500)).is_ok()
    {}
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
    // No latency requirement here; slow CI machines need more than 2 s.
    let (seen, _) = collect_until(&w.rx, Duration::from_secs(5), |s| {
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

#[test]
fn slow_checkout_is_coalesced_into_few_batches() {
    let w = watched();
    let requests = w.dir.path().join("requests");
    fs::create_dir(&requests).unwrap();
    // A checkout spread over a second or more, like a slow disk or an
    // antivirus scanning every file.
    let started = Instant::now();
    for i in 0..500 {
        fs::write(
            requests.join(format!("r{i:03}.yaml")),
            format!("name: r{i}\n"),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(2));
    }
    let writing = started.elapsed();

    // Every report per path, in order. Batches were sent while writing;
    // the times below are when this test read them.
    let mut history: BTreeMap<PathBuf, Vec<ChangeKind>> = BTreeMap::new();
    let mut batches: Vec<(Duration, usize)> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    fn take(
        batch: Vec<ProjectChange>,
        at: Duration,
        history: &mut BTreeMap<PathBuf, Vec<ChangeKind>>,
        batches: &mut Vec<(Duration, usize)>,
    ) {
        batches.push((at, batch.len()));
        for change in batch {
            history.entry(change.path).or_default().push(change.kind);
        }
    }
    while history.len() < 500 {
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        let Ok(batch) = w.rx.recv_timeout(left) else {
            break;
        };
        take(batch, started.elapsed(), &mut history, &mut batches);
    }
    // Anything still pending would be a late extra batch.
    if let Ok(batch) = w.rx.recv_timeout(Duration::from_millis(800)) {
        take(batch, started.elapsed(), &mut history, &mut batches);
    }

    let others: Vec<_> = history
        .keys()
        .filter(|p| !p.starts_with("requests"))
        .collect();
    assert!(
        others.is_empty(),
        "files the test did not write: {others:?}"
    );
    assert_eq!(history.len(), 500, "batches (time, size): {batches:?}");
    // A file whose content lands just after a batch was sent is reported
    // again as modified in the next one; nothing else may repeat.
    for (path, kinds) in &history {
        assert_eq!(kinds[0], ChangeKind::Created, "{path:?}: {kinds:?}");
        assert!(
            kinds[1..].iter().all(|k| *k == ChangeKind::Modified),
            "{path:?}: {kinds:?}"
        );
    }
    let repeats: usize = history.values().map(|k| k.len() - 1).sum();
    assert!(
        repeats < 25,
        "{repeats} repeated reports for 500 files: {batches:?}"
    );
    // Without coalescing, a batch arrives every 62.5 ms tick while files keep
    // changing. With it, at most one per 2 s while busy plus a final one;
    // allow one per half second of writing to stay robust on slow machines.
    let allowed = 2 + (writing.as_millis() / 500) as usize;
    assert!(
        batches.len() <= allowed,
        "{} batches for {writing:?} of writes, allowed {allowed}: {batches:?}",
        batches.len()
    );
}
