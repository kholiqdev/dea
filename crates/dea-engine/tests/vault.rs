mod support;

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use dea_engine::{PromptProblem, UseError, VaultStatus, production_reason};
use dea_project::{
    ResolveContext, ResolveError, SecretKind, SecretLookupError, SystemDynamicSource, resolve,
};
use dea_vault::{AuthError, OsEvent, SecretData, SecretKey};
use secrecy::SecretString;
use support::{FakeAuthenticator, FakePrompter, Gate, ManualClock, file_service, service};

const MIN: Duration = Duration::from_secs(60);

fn key(environment: &str, name: &str) -> SecretKey {
    SecretKey {
        scope: "project-1".into(),
        environment: environment.into(),
        name: name.into(),
    }
}

fn text(value: &str) -> SecretData {
    SecretData::Text(SecretString::from(value))
}

#[test]
fn starts_locked_and_refuses_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let (auth, reasons) = FakeAuthenticator::native(vec![]);
    let (vault, _) = service(
        dir.path(),
        auth,
        FakePrompter::answering(&[]),
        ManualClock::new(),
    );
    assert_eq!(vault.status(), VaultStatus { locked: true });
    assert!(matches!(
        vault.secret(&key("staging", "t"), false),
        Err(UseError::Locked)
    ));
    assert!(matches!(
        vault.set_secret(&key("staging", "t"), SecretKind::Token, &text("x")),
        Err(UseError::Locked)
    ));
    assert!(reasons.lock().unwrap().is_empty(), "nothing prompted");
}

#[test]
fn unlock_with_native_authentication_reports_status() {
    let dir = tempfile::tempdir().unwrap();
    let (auth, reasons) = FakeAuthenticator::native(vec![]);
    let (vault, statuses) = service(
        dir.path(),
        auth,
        FakePrompter::answering(&[]),
        ManualClock::new(),
    );
    vault.unlock("Unlock the Vault").unwrap();
    assert_eq!(*reasons.lock().unwrap(), ["Unlock the Vault"]);
    assert_eq!(*statuses.lock().unwrap(), [VaultStatus { locked: false }]);

    vault
        .set_secret(&key("staging", "t"), SecretKind::Token, &text("abc"))
        .unwrap();
    let got = vault.secret(&key("staging", "t"), false).unwrap().unwrap();
    assert_eq!(got.value_for_use(), "abc");
    // Already unlocked: no second dialog.
    vault.unlock("Unlock the Vault").unwrap();
    assert_eq!(reasons.lock().unwrap().len(), 1);
}

#[test]
fn production_asks_once_per_window_with_the_action_in_the_reason() {
    assert_eq!(
        production_reason(
            "Send POST /v1/shipments to production",
            &["prod_access_token"]
        ),
        "Send POST /v1/shipments to production using `prod_access_token`"
    );
    assert_eq!(
        production_reason("Connect to harbor_prod", &["db_user", "db_password"]),
        "Connect to harbor_prod using `db_user`, `db_password`"
    );

    let dir = tempfile::tempdir().unwrap();
    let clock = ManualClock::new();
    let (auth, reasons) = FakeAuthenticator::native(vec![]);
    let (vault, _) = service(
        dir.path(),
        auth,
        FakePrompter::answering(&[]),
        clock.clone(),
    );
    vault.unlock("Unlock the Vault").unwrap();
    vault
        .set_secret(
            &key("production", "prod_access_token"),
            SecretKind::Token,
            &text("p"),
        )
        .unwrap();

    let prod = key("production", "prod_access_token");
    assert!(matches!(
        vault.secret(&prod, true),
        Err(UseError::ReauthRequired)
    ));
    let action = "Send POST /v1/shipments to production";
    vault
        .authorize_production(action, &["prod_access_token"])
        .unwrap();
    assert_eq!(
        reasons.lock().unwrap()[1],
        "Send POST /v1/shipments to production using `prod_access_token`"
    );
    assert!(vault.secret(&prod, true).unwrap().is_some());

    clock.advance(9 * MIN);
    vault
        .authorize_production(action, &["prod_access_token"])
        .unwrap();
    assert_eq!(reasons.lock().unwrap().len(), 2, "still inside the window");

    clock.advance(MIN);
    assert!(matches!(
        vault.secret(&prod, true),
        Err(UseError::ReauthRequired)
    ));
    vault
        .authorize_production(action, &["prod_access_token"])
        .unwrap();
    assert_eq!(reasons.lock().unwrap().len(), 3, "the window closed");
}

#[test]
fn cancelling_production_returns_cancelled_and_keeps_the_window_closed() {
    let dir = tempfile::tempdir().unwrap();
    let (auth, _) = FakeAuthenticator::native(vec![Ok(()), Err(AuthError::Cancelled)]);
    let (vault, _) = service(
        dir.path(),
        auth,
        FakePrompter::answering(&[]),
        ManualClock::new(),
    );
    vault.unlock("Unlock the Vault").unwrap();
    assert!(matches!(
        vault.authorize_production("Send GET /v1/me to production", &["token"]),
        Err(UseError::Cancelled)
    ));
    assert!(matches!(
        vault.secret(&key("production", "token"), true),
        Err(UseError::ReauthRequired)
    ));
    // Still unlocked for everything else.
    assert!(!vault.status().locked);
}

#[test]
fn production_while_locked_unlocks_with_one_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let (auth, reasons) = FakeAuthenticator::native(vec![]);
    let (vault, statuses) = service(
        dir.path(),
        auth,
        FakePrompter::answering(&[]),
        ManualClock::new(),
    );
    vault
        .authorize_production("Send GET /v1/me to production", &["token"])
        .unwrap();
    assert_eq!(reasons.lock().unwrap().len(), 1);
    assert!(!vault.status().locked);
    assert_eq!(*statuses.lock().unwrap(), [VaultStatus { locked: false }]);
    assert!(
        vault
            .secret(&key("production", "token"), true)
            .unwrap()
            .is_none()
    );
}

#[test]
fn idle_lock_and_os_events_close_the_window_and_report_status() {
    let dir = tempfile::tempdir().unwrap();
    let clock = ManualClock::new();
    let (auth, _) = FakeAuthenticator::native(vec![]);
    let (vault, statuses) = service(
        dir.path(),
        auth,
        FakePrompter::answering(&[]),
        clock.clone(),
    );

    vault
        .authorize_production("Send GET /v1/me to production", &["token"])
        .unwrap();
    clock.advance(14 * MIN);
    vault.record_activity();
    clock.advance(14 * MIN);
    vault.tick();
    assert!(!vault.status().locked, "activity restarted the idle timer");
    clock.advance(MIN);
    vault.tick();
    assert!(vault.status().locked);

    vault.unlock("Unlock the Vault").unwrap();
    vault.handle_os_event(OsEvent::Sleeping);
    assert!(vault.status().locked);
    assert!(matches!(
        vault.secret(&key("production", "token"), true),
        Err(UseError::Locked)
    ));
    // A second event while locked reports nothing new.
    vault.handle_os_event(OsEvent::ScreenLocked);
    let seen: Vec<bool> = statuses.lock().unwrap().iter().map(|s| s.locked).collect();
    assert_eq!(seen, [false, true, false, true]);
}

#[test]
fn linux_creates_the_vault_password_on_first_unlock() {
    let dir = tempfile::tempdir().unwrap();
    let prompter = FakePrompter::answering(&[Some("short"), Some("correct horse")]);
    let (vault, _) = file_service(dir.path(), prompter.clone(), ManualClock::new());
    vault.unlock("Unlock the Vault").unwrap();

    let prompts = prompter.prompts.lock().unwrap().clone();
    assert_eq!(prompts.len(), 2);
    assert!(
        prompts
            .iter()
            .all(|(kind, _)| *kind == "new_vault_password")
    );
    assert_eq!(prompts[0].1.reason, "Unlock the Vault");
    assert_eq!(prompts[0].1.problem, None);
    assert_eq!(
        prompts[1].1.problem,
        Some(PromptProblem::TooShort { min: 8 })
    );

    // The encrypted file now exists and holds values.
    vault
        .set_secret(&key("staging", "t"), SecretKind::Token, &text("abc"))
        .unwrap();
    assert!(dir.path().join("vault.enc").exists());
    vault.lock();
    let again = FakePrompter::answering(&[Some("correct horse")]);
    let (reopened, _) = file_service(dir.path(), again.clone(), ManualClock::new());
    reopened.unlock("Unlock the Vault").unwrap();
    assert_eq!(again.prompts.lock().unwrap()[0].0, "vault_password");
    assert_eq!(
        reopened
            .secret(&key("staging", "t"), false)
            .unwrap()
            .unwrap()
            .value_for_use(),
        "abc"
    );
}

#[test]
fn wrong_vault_password_asks_again_then_holds() {
    let dir = tempfile::tempdir().unwrap();
    let setup = FakePrompter::answering(&[Some("correct horse")]);
    let (vault, _) = file_service(dir.path(), setup, ManualClock::new());
    vault.unlock("Unlock the Vault").unwrap();
    vault.lock();

    let wrong = FakePrompter::answering(&[
        Some("wrong 1"),
        Some("wrong 2"),
        Some("wrong 3"),
        Some("wrong 4"),
        Some("wrong 5"),
        Some("correct horse"),
    ]);
    let (vault, _) = file_service(dir.path(), wrong.clone(), ManualClock::new());
    let result = vault.unlock("Unlock the Vault");
    assert!(
        matches!(result, Err(UseError::RetryAfter(d)) if d == Duration::from_secs(30)),
        "{result:?}"
    );
    let prompts = wrong.prompts.lock().unwrap().clone();
    assert_eq!(prompts.len(), 6);
    assert_eq!(prompts[0].1.problem, None);
    assert!(
        prompts[1..5]
            .iter()
            .all(|(_, p)| p.problem == Some(PromptProblem::WrongPassword))
    );
    assert_eq!(
        prompts[5].1.problem,
        Some(PromptProblem::RetryAfter(Duration::from_secs(30)))
    );
    assert!(vault.status().locked);
}

#[test]
fn cancelling_the_vault_password_keeps_the_vault_locked() {
    let dir = tempfile::tempdir().unwrap();
    let (vault, _) = file_service(
        dir.path(),
        FakePrompter::answering(&[None]),
        ManualClock::new(),
    );
    assert!(matches!(
        vault.unlock("Unlock the Vault"),
        Err(UseError::Cancelled)
    ));
    assert!(vault.status().locked);
}

#[test]
fn lookup_maps_locked_and_reauth_errors() {
    let dir = tempfile::tempdir().unwrap();
    let (auth, _) = FakeAuthenticator::native(vec![]);
    let (vault, _) = service(
        dir.path(),
        auth,
        FakePrompter::answering(&[]),
        ManualClock::new(),
    );
    let run = |production: bool| {
        let lookup = vault.lookup("project-1", production);
        let mut dynamic = SystemDynamicSource;
        let mut ctx = ResolveContext {
            scopes: &[],
            environment: "production",
            secrets: &lookup,
            dynamic: &mut dynamic,
        };
        resolve("Bearer {{vault:token}}", &mut ctx)
    };
    assert!(matches!(
        run(true),
        Err(ResolveError::Secret {
            error: SecretLookupError::Locked,
            ..
        })
    ));
    vault.unlock("Unlock the Vault").unwrap();
    vault
        .set_secret(
            &key("production", "token"),
            SecretKind::Token,
            &text("t0k3n"),
        )
        .unwrap();
    assert!(matches!(
        run(true),
        Err(ResolveError::Secret {
            error: SecretLookupError::ReauthRequired,
            ..
        })
    ));
    vault
        .authorize_production("Send GET /v1/me to production", &["token"])
        .unwrap();
    assert_eq!(run(true).unwrap().text, "Bearer t0k3n");
}

#[test]
fn concurrent_production_uses_prompt_once() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Arc::new(Gate::default());
    let (mut auth, reasons) = FakeAuthenticator::native(vec![]);
    auth.hold = Some(Arc::clone(&gate));
    let (vault, _) = service(
        dir.path(),
        auth,
        FakePrompter::answering(&[]),
        ManualClock::new(),
    );

    let threads: Vec<_> = (0..2)
        .map(|_| {
            let vault = Arc::clone(&vault);
            thread::spawn(move || {
                vault.authorize_production("Send GET /v1/me to production", &["token"])
            })
        })
        .collect();
    // Both requests are in flight while the dialog is open.
    thread::sleep(Duration::from_millis(200));
    gate.open();
    for t in threads {
        t.join().unwrap().unwrap();
    }
    assert_eq!(reasons.lock().unwrap().len(), 1);
}
