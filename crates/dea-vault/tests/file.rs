use std::fs;

use dea_testkit::{canary, find_canary};
use dea_vault::{Backend, FileBackend, KdfParams, VaultError};
use secrecy::SecretString;

fn password(text: &str) -> SecretString {
    SecretString::from(text)
}

/// A new, unlocked vault file in a temporary folder.
fn vault() -> (tempfile::TempDir, std::path::PathBuf, FileBackend) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.enc");
    let file = FileBackend::open(&path, KdfParams::insecure_for_tests());
    assert!(!file.exists());
    file.create(&password("correct horse")).unwrap();
    assert!(file.exists() && file.is_unlocked());
    (dir, path, file)
}

#[test]
fn values_survive_lock_and_unlock() {
    let (_dir, path, file) = vault();
    file.set("p/staging/token", "abc").unwrap();
    file.set("p/staging/key", "line 1\nline 2").unwrap();
    file.lock();
    assert!(!file.is_unlocked());

    let reopened = FileBackend::open(&path, KdfParams::insecure_for_tests());
    reopened.unlock(&password("correct horse")).unwrap();
    assert_eq!(
        reopened.get("p/staging/token").unwrap().unwrap().as_str(),
        "abc"
    );
    assert_eq!(
        reopened.get("p/staging/key").unwrap().unwrap().as_str(),
        "line 1\nline 2"
    );
    assert!(reopened.get("p/staging/none").unwrap().is_none());
    reopened.delete("p/staging/token").unwrap();
    reopened.delete("p/staging/token").unwrap();
    assert!(reopened.get("p/staging/token").unwrap().is_none());
}

#[test]
fn wrong_password_is_reported_and_keeps_the_file() {
    let (_dir, path, file) = vault();
    file.set("a", "1").unwrap();
    let before = fs::read(&path).unwrap();
    let reopened = FileBackend::open(&path, KdfParams::insecure_for_tests());
    assert!(matches!(
        reopened.unlock(&password("wrong")),
        Err(VaultError::WrongPassword)
    ));
    assert!(!reopened.is_unlocked());
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn truncated_or_foreign_file_is_corrupt_and_kept() {
    let (_dir, path, file) = vault();
    file.set("a", "1").unwrap();
    let full = fs::read(&path).unwrap();
    for bytes in [full[..full.len() / 2].to_vec(), b"{}".to_vec(), Vec::new()] {
        fs::write(&path, &bytes).unwrap();
        let reopened = FileBackend::open(&path, KdfParams::insecure_for_tests());
        let result = reopened.unlock(&password("correct horse"));
        assert!(matches!(result, Err(VaultError::Corrupt(_))), "{result:?}");
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn a_file_from_a_newer_version_is_refused() {
    let (_dir, path, file) = vault();
    file.set("a", "1").unwrap();
    let mut bytes = fs::read(&path).unwrap();
    bytes[8] = 2; // the version byte after the magic
    fs::write(&path, &bytes).unwrap();
    let reopened = FileBackend::open(&path, KdfParams::insecure_for_tests());
    assert!(matches!(
        reopened.unlock(&password("correct horse")),
        Err(VaultError::Corrupt(_))
    ));
}

#[test]
fn locked_backend_refuses_reads_and_writes() {
    let (_dir, _path, file) = vault();
    file.lock();
    assert!(matches!(file.get("a"), Err(VaultError::Locked)));
    assert!(matches!(file.set("a", "1"), Err(VaultError::Locked)));
    assert!(matches!(file.delete("a"), Err(VaultError::Locked)));
}

#[test]
fn every_write_uses_a_new_nonce() {
    let (_dir, path, file) = vault();
    file.set("a", "1").unwrap();
    let first = fs::read(&path).unwrap();
    file.set("a", "1").unwrap();
    let second = fs::read(&path).unwrap();
    assert_eq!(first.len(), second.len());
    // Same header and salt, different nonce.
    assert_eq!(first[..37], second[..37]);
    assert_ne!(first[37..49], second[37..49]);
}

#[test]
fn kdf_parameters_are_read_from_the_file() {
    let (_dir, path, file) = vault();
    file.set("a", "1").unwrap();
    // Opening with different defaults still uses what the file says.
    let reopened = FileBackend::open(&path, KdfParams::default());
    reopened.unlock(&password("correct horse")).unwrap();
    assert_eq!(reopened.get("a").unwrap().unwrap().as_str(), "1");
}

#[test]
fn file_never_contains_the_value() {
    let (dir, _path, file) = vault();
    let secret = canary();
    file.set("p/production/token", &secret).unwrap();
    assert!(find_canary(&[dir.path()], &secret).is_empty());
}

#[cfg(unix)]
#[test]
fn files_are_readable_by_the_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, path, file) = vault();
    file.set("a", "1").unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn a_new_vault_file_creates_its_folder() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app-data").join("vault").join("vault.enc");
    let file = FileBackend::open(&path, KdfParams::insecure_for_tests());
    file.create(&password("correct horse")).unwrap();
    assert!(path.exists());
}
