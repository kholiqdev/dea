//! What the app needs from the Vault: rows for the Vault screen, adding a
//! secret, the production window and the unlock method.

mod support;

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use dea_engine::vault::{
    SaveSecretError, SecretInput, SecretKey, SecretString, UnlockMethod, UseError, VaultService,
    VaultStatus, production_reason, save_secret, unlock_method,
};
use dea_project::{Project, ProjectError, SecretKind, SecretSpec, create_project, load_project};
use dea_vault::{OAuthToken, SecretData};
use secrecy::ExposeSecret;
use support::{FakeAuthenticator, FakePrompter, ManualClock, project, service};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const MIN: Duration = Duration::from_secs(60);
const EXPIRY: &str = "2026-12-31T23:59:00Z";

fn spec(name: &str, kind: SecretKind) -> SecretSpec {
    SecretSpec {
        name: name.into(),
        kind,
        description: None,
    }
}

fn key(environment: &str, name: &str) -> SecretKey {
    SecretKey {
        scope: "project-1".into(),
        environment: environment.into(),
        name: name.into(),
    }
}

fn input(name: &str, kind: SecretKind, value: &str) -> SecretInput {
    SecretInput {
        name: name.into(),
        kind,
        environment: "staging".into(),
        description: None,
        value: value.into(),
        refresh_token: None,
        expires_at: None,
    }
}

/// An unlocked Vault over memory, its folder, and its clock.
fn unlocked() -> (Arc<VaultService>, tempfile::TempDir, Arc<ManualClock>) {
    let dir = tempfile::tempdir().unwrap();
    let clock = ManualClock::new();
    let (auth, _) = FakeAuthenticator::native(vec![]);
    let (vault, _) = service(
        dir.path(),
        auth,
        FakePrompter::answering(&[]),
        clock.clone(),
    );
    vault.unlock("unlock the Vault").unwrap();
    (vault, dir, clock)
}

fn on_disk(dir: &Path) -> Project {
    create_project(dir, "harbor-api").unwrap()
}

#[test]
fn rows_mark_missing_values_per_environment() {
    let (vault, _dir, _) = unlocked();
    let mut p = project();
    p.vault.secrets = vec![
        spec("pg_password", SecretKind::Password),
        spec("access_token", SecretKind::Token),
    ];
    vault
        .set_secret(
            &key("staging", "access_token"),
            SecretKind::Token,
            &SecretData::Text(SecretString::from("abc")),
        )
        .unwrap();
    vault.lock();

    let rows = vault.rows(&p);
    let order: Vec<(&str, &str)> = rows
        .iter()
        .map(|r| (r.environment.as_str(), r.name.as_str()))
        .collect();
    assert_eq!(
        order,
        [
            ("local", "access_token"),
            ("local", "pg_password"),
            ("production", "access_token"),
            ("production", "pg_password"),
            ("staging", "access_token"),
            ("staging", "pg_password"),
        ]
    );
    for row in &rows {
        let stored = (row.environment.as_str(), row.name.as_str()) == ("staging", "access_token");
        assert_eq!(row.missing, !stored, "{row:?}");
        assert_eq!(row.updated_at.is_some(), stored, "{row:?}");
        assert_eq!(row.expires_at, None);
    }
    let staging = &rows[4];
    assert_eq!(staging.kind, SecretKind::Token);
    let updated = staging.updated_at.as_deref().unwrap();
    assert!(
        OffsetDateTime::parse(updated, &Rfc3339).is_ok(),
        "{updated}"
    );
    assert_eq!(rows[5].kind, SecretKind::Password);
}

#[test]
fn rows_carry_the_expiry_of_an_oauth_token() {
    let (vault, _dir, _) = unlocked();
    let mut p = project();
    p.vault.secrets = vec![SecretSpec {
        description: Some("GitHub login".into()),
        ..spec("github", SecretKind::OauthToken)
    }];
    let token = OAuthToken {
        access_token: SecretString::from("access"),
        refresh_token: None,
        expires_at: Some(OffsetDateTime::parse(EXPIRY, &Rfc3339).unwrap()),
    };
    vault
        .set_secret(
            &key("staging", "github"),
            SecretKind::OauthToken,
            &SecretData::OAuth(token),
        )
        .unwrap();

    let rows = vault.rows(&p);
    let row = rows.iter().find(|r| r.environment == "staging").unwrap();
    assert_eq!(row.kind, SecretKind::OauthToken);
    assert_eq!(row.description.as_deref(), Some("GitHub login"));
    assert_eq!(row.expires_at.as_deref(), Some(EXPIRY));
    assert!(!row.missing);
}

#[test]
fn rows_and_unlock_methods_use_camel_case_in_json() {
    let (vault, _dir, _) = unlocked();
    let mut p = project();
    p.vault.secrets = vec![spec("access_token", SecretKind::Token)];
    let json = serde_json::to_value(&vault.rows(&p)[0]).unwrap();
    for field in [
        "name",
        "kind",
        "description",
        "environment",
        "missing",
        "updatedAt",
        "expiresAt",
    ] {
        assert!(json.get(field).is_some(), "{field} in {json}");
    }
    assert_eq!(
        serde_json::to_value(UnlockMethod::DeviceOwner).unwrap(),
        "deviceOwner"
    );
    assert_eq!(
        serde_json::to_value(UnlockMethod::VaultPassword).unwrap(),
        "vaultPassword"
    );
    assert_eq!(
        serde_json::to_value(VaultStatus { locked: true }).unwrap(),
        serde_json::json!({ "locked": true })
    );
}

#[test]
fn saving_a_secret_declares_it_and_stores_the_value() {
    let (vault, _app, _) = unlocked();
    let dir = tempfile::tempdir().unwrap();
    let p = on_disk(dir.path());
    let new = SecretInput {
        description: Some("Login token".into()),
        ..input("access_token", SecretKind::Token, "abc")
    };
    save_secret(&vault, &p, new).unwrap();

    let reloaded = load_project(dir.path()).unwrap().project;
    assert_eq!(
        reloaded.vault.secrets,
        [SecretSpec {
            description: Some("Login token".into()),
            ..spec("access_token", SecretKind::Token)
        }]
    );
    let value = vault
        .secret(&reloaded, "staging", "access_token")
        .unwrap()
        .unwrap();
    assert_eq!(value.value_for_use(), "abc");
    let text = fs::read_to_string(dir.path().join("vault.yaml")).unwrap();
    assert!(!text.contains("abc"), "{text}");
}

#[test]
fn a_secret_is_not_stored_when_its_declaration_cannot_be_written() {
    let (vault, _app, _) = unlocked();
    let dir = tempfile::tempdir().unwrap();
    let p = on_disk(dir.path());
    let conflicted = "<<<<<<< HEAD\nsecrets: []\n=======\nsecrets: []\n>>>>>>> theirs\n";
    fs::write(dir.path().join("vault.yaml"), conflicted).unwrap();

    match save_secret(&vault, &p, input("access_token", SecretKind::Token, "abc")) {
        Err(SaveSecretError::Project(ProjectError::MergeConflict(_))) => {}
        other => panic!("expected a merge conflict, got {other:?}"),
    }
    assert!(
        vault
            .secret(&p, "staging", "access_token")
            .unwrap()
            .is_none()
    );

    let read_only = Project {
        read_only: true,
        ..p.clone()
    };
    match save_secret(
        &vault,
        &read_only,
        input("access_token", SecretKind::Token, "abc"),
    ) {
        Err(SaveSecretError::Project(ProjectError::ReadOnly)) => {}
        other => panic!("expected read-only, got {other:?}"),
    }
    assert!(
        vault
            .secret(&p, "staging", "access_token")
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_locked_vault_declares_nothing() {
    let (vault, _app, _) = unlocked();
    vault.lock();
    let dir = tempfile::tempdir().unwrap();
    let p = on_disk(dir.path());
    let before = fs::read_to_string(dir.path().join("vault.yaml")).unwrap();

    assert!(matches!(
        save_secret(&vault, &p, input("access_token", SecretKind::Token, "abc")),
        Err(SaveSecretError::Use(UseError::Locked))
    ));
    assert_eq!(
        fs::read_to_string(dir.path().join("vault.yaml")).unwrap(),
        before
    );
}

#[test]
fn only_oauth_tokens_take_a_refresh_token_and_expiry() {
    let (vault, _app, _) = unlocked();
    let dir = tempfile::tempdir().unwrap();
    let p = on_disk(dir.path());

    let oauth = SecretInput {
        refresh_token: Some("refresh".into()),
        expires_at: Some(EXPIRY.into()),
        ..input("github", SecretKind::OauthToken, "access")
    };
    save_secret(&vault, &p, oauth).unwrap();
    let reloaded = load_project(dir.path()).unwrap().project;
    match vault.secret(&reloaded, "staging", "github").unwrap() {
        Some(SecretData::OAuth(token)) => {
            assert_eq!(token.access_token.expose_secret(), "access");
            assert_eq!(
                token
                    .refresh_token
                    .as_ref()
                    .map(ExposeSecret::expose_secret),
                Some("refresh")
            );
            assert_eq!(
                token.expires_at,
                Some(OffsetDateTime::parse(EXPIRY, &Rfc3339).unwrap())
            );
        }
        other => panic!("expected an OAuth token, got {other:?}"),
    }

    // Empty optional fields count as not given.
    let blank = SecretInput {
        description: Some(String::new()),
        refresh_token: Some(String::new()),
        expires_at: Some(" ".into()),
        ..input("api_key", SecretKind::ApiKey, "k")
    };
    save_secret(&vault, &p, blank).unwrap();

    let refused = [
        SecretInput {
            refresh_token: Some("refresh".into()),
            ..input("token", SecretKind::Token, "t")
        },
        SecretInput {
            expires_at: Some(EXPIRY.into()),
            ..input("password", SecretKind::Password, "p")
        },
        SecretInput {
            expires_at: Some("next tuesday".into()),
            ..input("later", SecretKind::OauthToken, "a")
        },
    ];
    for bad in refused {
        let name = bad.name.clone();
        match save_secret(&vault, &p, bad) {
            Err(SaveSecretError::Invalid(message)) => {
                assert!(message.starts_with(char::is_lowercase), "{message}");
            }
            other => panic!("expected {name} to be refused, got {other:?}"),
        }
        let declared = load_project(dir.path()).unwrap().project.vault.secrets;
        assert!(declared.iter().all(|s| s.name != name), "{name} declared");
        assert!(vault.secret(&p, "staging", &name).unwrap().is_none());
    }
    let declared = load_project(dir.path()).unwrap().project.vault.secrets;
    assert_eq!(
        declared,
        [
            spec("github", SecretKind::OauthToken),
            spec("api_key", SecretKind::ApiKey)
        ]
    );
}

#[test]
fn secret_input_debug_hides_the_value() {
    let secret = dea_testkit::canary();
    let given = SecretInput {
        refresh_token: Some(secret.clone()),
        expires_at: Some(EXPIRY.into()),
        ..input("github", SecretKind::OauthToken, &secret)
    };
    let text = format!("{given:?}");
    assert!(!text.contains(&secret), "{text}");
    assert!(text.contains("github"), "{text}");
}

#[test]
fn production_open_follows_the_window() {
    let (vault, _dir, clock) = unlocked();
    assert!(!vault.production_open());
    vault
        .authorize_production("reveal a value in production", &["token"])
        .unwrap();
    assert!(vault.production_open());
    clock.advance(9 * MIN);
    assert!(vault.production_open());
    clock.advance(MIN);
    assert!(!vault.production_open());
}

#[test]
fn a_production_check_without_secrets_names_only_the_action() {
    assert_eq!(
        production_reason("turn off production for staging", &[]),
        "turn off production for staging"
    );
    assert_eq!(
        production_reason("copy a value in production", &["token"]),
        "copy a value in production using `token`"
    );
}

#[test]
fn the_unlock_method_follows_the_os() {
    let method = unlock_method();
    if cfg!(target_os = "macos") {
        assert_eq!(method, UnlockMethod::DeviceOwner);
    } else if cfg!(windows) {
        assert!(matches!(
            method,
            UnlockMethod::WindowsHello | UnlockMethod::WindowsPassword
        ));
    } else {
        assert_eq!(method, UnlockMethod::VaultPassword);
    }
}
