//! The perf-probe command line, with a shell script standing in for the app.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
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

fn probe(s: &Setup, extra: &[&str]) -> Output {
    let root = s.dir.path();
    let budgets = Path::new(env!("CARGO_MANIFEST_DIR")).join("budgets.toml");
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
