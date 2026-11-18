//! The perf-probe command line, with a shell script standing in for the app.
#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use tempfile::TempDir;

struct Setup {
    dir: TempDir,
    app: PathBuf,
}

/// A fake app with the given body, an empty dist and bundle folder.
fn setup(body: &str) -> Setup {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("fake-app.sh");
    fs::write(&app, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&app, fs::Permissions::from_mode(0o755)).unwrap();
    fs::create_dir(dir.path().join("dist")).unwrap();
    fs::write(dir.path().join("dist/index.html"), "<html></html>").unwrap();
    fs::create_dir(dir.path().join("bundle")).unwrap();
    Setup { dir, app }
}

/// Budgets no test app can miss. These tests are about the command line;
/// the budget checks themselves are tested in `check.rs`, and the real
/// budgets would make a slow, busy machine fail them.
const GENEROUS_BUDGETS: &str = r#"
startup_js_gzip_kb = 100000
regression_pct = 1000

[start_ms]
macos = 600000
windows = 600000
linux = 600000

[idle_mb]
macos = 100000
windows = 100000
linux = 100000

[installer_mb]
"#;

fn probe(s: &Setup, extra: &[&str]) -> Output {
    let root = s.dir.path();
    let budgets = root.join("budgets.toml");
    fs::write(&budgets, GENEROUS_BUDGETS).unwrap();
    Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .env("LAUNCH_LOG", root.join("launches.log"))
        .arg("--app")
        .arg(&s.app)
        .arg("--dist")
        .arg(root.join("dist"))
        .arg("--bundle-dir")
        .arg(root.join("bundle"))
        .arg("--budgets")
        .arg(budgets)
        .arg("--out")
        .arg(root.join("out.json"))
        .args(["--idle-wait-secs", "0"])
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn warmup_launches_are_not_counted() {
    let s = setup(
        r#"echo launch >> "$LAUNCH_LOG"
echo DEA_READY
exec sleep 30"#,
    );
    let out = probe(&s, &["--runs", "1", "--warmup", "2"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let launches = fs::read_to_string(s.dir.path().join("launches.log")).unwrap();
    assert_eq!(
        launches.lines().count(),
        3,
        "two warm-ups and one measured run"
    );
    assert_eq!(stdout.matches("run 1:").count(), 1, "{stdout}");
    assert!(!stdout.contains("run 2:"), "{stdout}");
    assert!(stdout.contains("warm-up"), "{stdout}");
}

#[test]
fn app_errors_reach_the_probe_output() {
    let s = setup("echo 'boom from the webview' >&2\nexit 1");
    let out = probe(&s, &["--runs", "1"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("boom from the webview"), "{stderr}");
}

#[test]
fn app_marks_are_shown_with_their_time() {
    let s = setup(
        r#"sleep 0.1
echo "DEA_MARK mounted"
sleep 0.1
echo DEA_READY
exec sleep 30"#,
    );
    let out = probe(&s, &["--runs", "1"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    let line = stdout
        .lines()
        .find(|l| l.contains("mounted"))
        .unwrap_or_else(|| panic!("no mounted mark in {stdout}"));
    assert!(line.contains(" ms"), "{line}");
}

#[test]
fn other_app_output_is_shown() {
    let s = setup(
        r#"echo "Gtk-Message: hello from the toolkit"
echo DEA_READY
exec sleep 30"#,
    );
    let out = probe(&s, &["--runs", "1"]);
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("hello from the toolkit"), "{stderr}");
}

#[test]
fn a_slow_first_launch_gets_the_longer_warmup_wait() {
    let s = setup(
        r#"if [ ! -f "$LAUNCH_LOG" ]; then sleep 2; fi
echo launch >> "$LAUNCH_LOG"
echo DEA_READY
exec sleep 30"#,
    );
    let out = probe(
        &s,
        &[
            "--runs",
            "1",
            "--warmup",
            "1",
            "--ready-timeout-secs",
            "1",
            "--warmup-timeout-secs",
            "10",
        ],
    );
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_measured_launch_over_the_ready_wait_fails() {
    let s = setup("sleep 3\necho DEA_READY\nexec sleep 30");
    let out = probe(&s, &["--runs", "1", "--ready-timeout-secs", "1"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("within 1s"), "{stderr}");
}
