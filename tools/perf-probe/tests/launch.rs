//! Launch measurement with a small shell script standing in for the app.
#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::time::Duration;

use perf_probe::measure_launch_with;

fn script(dir: &Path, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("fake-app.sh");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn launch_measures_time_to_ready_and_memory() {
    let dir = tempfile::tempdir().unwrap();
    let app = script(
        dir.path(),
        r#"[ "$DEA_PERF_PROBE" = 1 ] || exit 3
sleep 0.2
echo DEA_READY
exec sleep 30"#,
    );
    let started = std::time::Instant::now();
    let (start_ms, idle_mb) =
        measure_launch_with(&app, Duration::from_secs(5), Duration::from_millis(100)).unwrap();
    assert!((150.0..3000.0).contains(&start_ms), "{start_ms}");
    assert!(idle_mb > 0.0, "{idle_mb}");
    // The app is stopped instead of running its full 30 seconds.
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn app_that_exits_before_ready_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let app = script(dir.path(), "echo starting\nexit 1");
    let started = std::time::Instant::now();
    assert!(measure_launch_with(&app, Duration::from_secs(5), Duration::ZERO).is_err());
    assert!(started.elapsed() < Duration::from_secs(4));
}

#[test]
fn app_that_never_gets_ready_times_out() {
    let dir = tempfile::tempdir().unwrap();
    let app = script(dir.path(), "exec sleep 30");
    let started = std::time::Instant::now();
    assert!(measure_launch_with(&app, Duration::from_millis(500), Duration::ZERO).is_err());
    assert!(started.elapsed() < Duration::from_secs(4));
}
