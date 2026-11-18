use std::fs;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use dea_testkit::{canary, find_canary};

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

#[test]
fn every_canary_is_new_and_recognisable() {
    let a = canary();
    let b = canary();
    assert_ne!(a, b);
    assert!(a.starts_with("dea-canary-"), "{a}");
    assert_eq!(a.len(), "dea-canary-".len() + 32, "{a}");
}

#[test]
fn finds_plain_utf16_and_base64_copies() {
    let dir = tempfile::tempdir().unwrap();
    let secret = canary();
    fs::write(dir.path().join("plain.log"), format!("token={secret}\n")).unwrap();
    fs::write(dir.path().join("wide.dat"), utf16le(&format!("x{secret}y"))).unwrap();
    // HTTP basic auth: the canary sits at an unaligned offset of the base64.
    let basic = STANDARD.encode(format!("user:{secret}"));
    fs::write(
        dir.path().join("history.json"),
        format!("\"Basic {basic}\""),
    )
    .unwrap();
    fs::write(dir.path().join("aligned.b64"), STANDARD.encode(&secret)).unwrap();

    let mut found: Vec<_> = find_canary(&[dir.path()], &secret)
        .into_iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    found.sort();
    assert_eq!(
        found,
        ["aligned.b64", "history.json", "plain.log", "wide.dat"]
    );
}

#[test]
fn ignores_files_without_the_canary() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("other.log"), canary()).unwrap();
    fs::write(dir.path().join("empty"), "").unwrap();
    assert!(find_canary(&[dir.path()], &canary()).is_empty());
}

#[test]
fn searches_nested_folders_and_every_root() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let secret = canary();
    fs::create_dir_all(a.path().join("a/b/c")).unwrap();
    fs::write(a.path().join("a/b/c/deep.yaml"), &secret).unwrap();
    fs::write(b.path().join("other-root.txt"), &secret).unwrap();
    assert_eq!(find_canary(&[a.path(), b.path()], &secret).len(), 2);
}

#[cfg(unix)]
#[test]
fn does_not_follow_symlinks() {
    let outside = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let secret = canary();
    fs::write(outside.path().join("secret.txt"), &secret).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
    assert!(find_canary(&[root.path()], &secret).is_empty());
}
