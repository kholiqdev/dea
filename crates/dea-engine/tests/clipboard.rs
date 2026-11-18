mod support;

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use dea_engine::{CLIPBOARD_CLEAR_AFTER, Clipboard, UseError};
use dea_project::SecretKind;
use dea_vault::{SecretData, SecretKey};
use secrecy::SecretString;
use support::{FakeAuthenticator, FakeClipboard, FakePrompter, ManualClock, project, service};

const SHORT: Duration = Duration::from_millis(50);

fn key(environment: &str) -> SecretKey {
    SecretKey {
        scope: "project-1".into(),
        environment: environment.into(),
        name: "token".into(),
    }
}

fn unlocked_with_secret() -> (tempfile::TempDir, Arc<dea_engine::VaultService>) {
    let dir = tempfile::tempdir().unwrap();
    let (auth, _) = FakeAuthenticator::native(vec![]);
    let (vault, _) = service(
        dir.path(),
        auth,
        FakePrompter::answering(&[]),
        ManualClock::new(),
    );
    vault.unlock("Unlock the Vault").unwrap();
    for env in ["staging", "production"] {
        vault
            .set_secret(
                &key(env),
                SecretKind::Token,
                &SecretData::Text(SecretString::from("abc123")),
            )
            .unwrap();
    }
    (dir, vault)
}

#[test]
fn the_default_delay_is_thirty_seconds() {
    assert_eq!(CLIPBOARD_CLEAR_AFTER, Duration::from_secs(30));
}

#[test]
fn copied_secret_is_cleared_after_the_delay() {
    let (_dir, vault) = unlocked_with_secret();
    let clipboard = Arc::new(FakeClipboard::default());
    vault
        .copy_secret(&project(), "staging", "token", clipboard.clone(), SHORT)
        .unwrap();
    assert_eq!(clipboard.text().as_deref(), Some("abc123"));
    thread::sleep(SHORT * 6);
    assert_eq!(clipboard.text(), None);
}

#[test]
fn clipboard_changed_by_the_person_is_left_alone() {
    let (_dir, vault) = unlocked_with_secret();
    let clipboard = Arc::new(FakeClipboard::default());
    vault
        .copy_secret(&project(), "staging", "token", clipboard.clone(), SHORT)
        .unwrap();
    clipboard
        .set_secret_text("something I copied myself")
        .unwrap();
    thread::sleep(SHORT * 6);
    assert_eq!(
        clipboard.text().as_deref(),
        Some("something I copied myself")
    );
}

#[test]
fn copying_needs_an_unlocked_vault_and_production_authentication() {
    let (_dir, vault) = unlocked_with_secret();
    let clipboard = Arc::new(FakeClipboard::default());
    assert!(matches!(
        vault.copy_secret(&project(), "production", "token", clipboard.clone(), SHORT),
        Err(UseError::ReauthRequired)
    ));
    vault.lock();
    assert!(matches!(
        vault.copy_secret(&project(), "staging", "token", clipboard.clone(), SHORT),
        Err(UseError::Locked)
    ));
    assert_eq!(clipboard.text(), None);
}

#[test]
fn copying_a_missing_secret_is_an_error() {
    let (_dir, vault) = unlocked_with_secret();
    let clipboard = Arc::new(FakeClipboard::default());
    assert!(matches!(
        vault.copy_secret(&project(), "staging", "nothing", clipboard.clone(), SHORT),
        Err(UseError::Missing)
    ));
    assert_eq!(clipboard.text(), None);
}
