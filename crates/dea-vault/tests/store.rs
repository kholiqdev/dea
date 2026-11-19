use std::sync::Arc;

use dea_project::{SecretKind, SecretSpec, VaultManifest};
use dea_testkit::{canary, find_canary};
use dea_vault::{
    Backend, MemoryBackend, OAuthToken, SecretData, SecretKey, SecretState, SecretStatus,
    SecretStore, has_values_in,
};
use secrecy::{ExposeSecret, SecretString};
use time::OffsetDateTime;
use time::macros::datetime;

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

fn store() -> (tempfile::TempDir, Arc<MemoryBackend>, SecretStore) {
    let dir = tempfile::tempdir().unwrap();
    let backend = Arc::new(MemoryBackend::default());
    let store = SecretStore::open(dir.path(), backend.clone()).unwrap();
    (dir, backend, store)
}

fn manifest(names: &[(&str, SecretKind)]) -> VaultManifest {
    VaultManifest {
        secrets: names
            .iter()
            .map(|(name, kind)| SecretSpec {
                name: (*name).into(),
                kind: *kind,
                description: Some(format!("{name} for the API")),
            })
            .collect(),
    }
}

fn envs() -> Vec<String> {
    ["local", "staging", "production"]
        .map(String::from)
        .to_vec()
}

fn status_of<'a>(statuses: &'a [SecretStatus], environment: &str, name: &str) -> &'a SecretStatus {
    statuses
        .iter()
        .find(|s| s.environment == environment && s.name == name)
        .unwrap_or_else(|| panic!("no status for {environment}/{name}: {statuses:?}"))
}

#[test]
fn set_then_get_round_trips_and_records_metadata() {
    let (dir, _, store) = store();
    let k = key("staging", "access_token");
    let before = OffsetDateTime::now_utc();
    store.set(&k, SecretKind::Token, &text("abc123")).unwrap();

    let got = store.get(&k).unwrap().expect("stored");
    assert_eq!(got.value_for_use(), "abc123");

    let statuses = store.statuses(
        "project-1",
        &manifest(&[("access_token", SecretKind::Token)]),
        &envs(),
    );
    let SecretState::Present {
        updated_at,
        expires_at,
    } = status_of(&statuses, "staging", "access_token").state
    else {
        panic!("not present: {statuses:?}");
    };
    assert!(updated_at >= before.replace_nanosecond(0).unwrap());
    assert_eq!(expires_at, None);

    // The index survives reopening.
    let reopened = SecretStore::open(dir.path(), Arc::new(MemoryBackend::default())).unwrap();
    let statuses = reopened.statuses(
        "project-1",
        &manifest(&[("access_token", SecretKind::Token)]),
        &envs(),
    );
    assert!(matches!(
        status_of(&statuses, "staging", "access_token").state,
        SecretState::Present { .. }
    ));
}

#[test]
fn oauth_token_keeps_refresh_token_and_expiry() {
    let (_dir, _, store) = store();
    let k = key("production", "github");
    let expires = datetime!(2026-11-19 10:40 UTC);
    let token = SecretData::OAuth(OAuthToken {
        access_token: SecretString::from("access-1"),
        refresh_token: Some(SecretString::from("refresh-1")),
        expires_at: Some(expires),
    });
    store.set(&k, SecretKind::OauthToken, &token).unwrap();

    let SecretData::OAuth(got) = store.get(&k).unwrap().unwrap() else {
        panic!("not an OAuth token");
    };
    assert_eq!(got.access_token.expose_secret(), "access-1");
    assert_eq!(got.refresh_token.unwrap().expose_secret(), "refresh-1");
    assert_eq!(got.expires_at, Some(expires));

    let statuses = store.statuses(
        "project-1",
        &manifest(&[("github", SecretKind::OauthToken)]),
        &envs(),
    );
    assert!(matches!(
        status_of(&statuses, "production", "github").state,
        SecretState::Present { expires_at: Some(e), .. } if e == expires
    ));
    let data = SecretData::OAuth(OAuthToken {
        access_token: SecretString::from("access-2"),
        refresh_token: None,
        expires_at: None,
    });
    assert_eq!(data.value_for_use(), "access-2");
}

#[test]
fn accounts_never_collide() {
    let k = |env: &str, name: &str| SecretKey {
        scope: "p".into(),
        environment: env.into(),
        name: name.into(),
    };
    let pairs = [
        (k("a/b", "c"), k("a", "b/c")),
        (k("a%2Fb", "c"), k("a/b", "c")),
        (k("a", "b#2"), k("a", "b%232")),
        (k("Produksi 🚀", "token"), k("Produksi", "🚀/token")),
        (k("环境", "x"), k("环", "境/x")),
    ];
    // Windows Credential Manager compares names without regard to case.
    let case_pairs = [
        (k("Staging", "token"), k("staging", "token")),
        (k("local", "API_KEY"), k("local", "api_key")),
        (k("Produksi", "É"), k("produksi", "é")),
    ];
    for (a, b) in pairs.into_iter().chain(case_pairs) {
        assert_ne!(
            a.account().to_lowercase(),
            b.account().to_lowercase(),
            "{a:?} vs {b:?}"
        );
    }
    // Chunk entries of the keychain add "#<n>"; no account ends that way.
    assert!(!k("a", "b#2").account().contains('#'));
}

#[test]
fn value_removed_outside_dea_is_missing() {
    let (_dir, backend, store) = store();
    let k = key("staging", "access_token");
    store.set(&k, SecretKind::Token, &text("abc")).unwrap();
    backend.delete(&k.account()).unwrap();

    assert!(store.get(&k).unwrap().is_none());
    let statuses = store.statuses(
        "project-1",
        &manifest(&[("access_token", SecretKind::Token)]),
        &envs(),
    );
    assert_eq!(
        status_of(&statuses, "staging", "access_token").state,
        SecretState::Missing
    );
}

#[test]
fn deleting_removes_the_value_and_the_metadata() {
    let (_dir, backend, store) = store();
    let k = key("local", "db_password");
    store.set(&k, SecretKind::Password, &text("pw")).unwrap();
    store.delete(&k).unwrap();
    assert!(backend.get(&k.account()).unwrap().is_none());
    assert!(store.get(&k).unwrap().is_none());
    // Deleting again is not an error.
    store.delete(&k).unwrap();
}

#[test]
fn statuses_list_every_required_secret_per_environment() {
    let (_dir, _, store) = store();
    store
        .set(
            &key("staging", "access_token"),
            SecretKind::Token,
            &text("t"),
        )
        .unwrap();
    store
        .set(&key("local", "old_secret"), SecretKind::ApiKey, &text("o"))
        .unwrap();
    // Another project's secret never shows up.
    let other = SecretKey {
        scope: "project-2".into(),
        environment: "local".into(),
        name: "access_token".into(),
    };
    store.set(&other, SecretKind::Token, &text("x")).unwrap();

    let statuses = store.statuses(
        "project-1",
        &manifest(&[
            ("access_token", SecretKind::Token),
            ("db_password", SecretKind::Password),
        ]),
        &envs(),
    );
    let order: Vec<_> = statuses
        .iter()
        .map(|s| {
            (
                s.name.as_str(),
                s.environment.as_str(),
                s.required,
                matches!(s.state, SecretState::Missing),
            )
        })
        .collect();
    assert_eq!(
        order,
        [
            ("access_token", "local", true, true),
            ("access_token", "staging", true, false),
            ("access_token", "production", true, true),
            ("db_password", "local", true, true),
            ("db_password", "staging", true, true),
            ("db_password", "production", true, true),
            ("old_secret", "local", false, false),
        ]
    );
    let first = &statuses[0];
    assert_eq!(first.kind, SecretKind::Token);
    assert_eq!(
        first.description.as_deref(),
        Some("access_token for the API")
    );
    assert_eq!(statuses[6].kind, SecretKind::ApiKey);
}

#[test]
fn values_never_reach_the_index_debug_or_errors() {
    let (dir, _, store) = store();
    let secret = canary();
    store
        .set(&key("staging", "token"), SecretKind::Token, &text(&secret))
        .unwrap();
    let oauth = SecretData::OAuth(OAuthToken {
        access_token: SecretString::from(secret.clone()),
        refresh_token: Some(SecretString::from(secret.clone())),
        expires_at: None,
    });
    store
        .set(&key("staging", "oauth"), SecretKind::OauthToken, &oauth)
        .unwrap();

    let statuses = store.statuses("project-1", &VaultManifest::default(), &envs());
    let debug_text = format!(
        "{:?} {:?} {:?} {:?}",
        store.get(&key("staging", "token")).unwrap(),
        store.get(&key("staging", "oauth")).unwrap(),
        oauth,
        statuses
    );
    let broken = format!("{{\"accessToken\": \"{secret}\", ");
    let error_text = match SecretData::decode(SecretKind::OauthToken, &broken) {
        Err(e) => format!("{e} {e:?}"),
        Ok(_) => panic!("broken JSON was accepted"),
    };

    assert!(find_canary(&[dir.path()], &secret).is_empty());
    assert!(!debug_text.contains(&secret), "{debug_text}");
    assert!(!error_text.contains(&secret), "{error_text}");
}

#[test]
fn a_broken_index_is_reported_and_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("index.json");
    std::fs::write(&path, "{ not json").unwrap();
    let opened = SecretStore::open(dir.path(), Arc::new(MemoryBackend::default()));
    assert!(matches!(opened, Err(dea_vault::VaultError::Corrupt(_))));
    std::fs::write(&path, r#"{"version": 99, "entries": []}"#).unwrap();
    let opened = SecretStore::open(dir.path(), Arc::new(MemoryBackend::default()));
    assert!(matches!(opened, Err(dea_vault::VaultError::Corrupt(_))));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        r#"{"version": 99, "entries": []}"#
    );
}

#[test]
fn values_are_found_from_the_index_alone() {
    let (dir, _, store) = store();
    // No index yet: nothing stored.
    assert!(!has_values_in(dir.path(), "project-1", "staging"));
    store
        .set(&key("staging", "token"), SecretKind::Token, &text("t"))
        .unwrap();
    assert!(has_values_in(dir.path(), "project-1", "staging"));
    assert!(!has_values_in(dir.path(), "project-1", "production"));
    assert!(!has_values_in(dir.path(), "project-2", "staging"));
    // A damaged index, or one from a newer Dea, counts as holding values.
    let path = dir.path().join("index.json");
    std::fs::write(&path, "{ not json").unwrap();
    assert!(has_values_in(dir.path(), "project-1", "production"));
    std::fs::write(&path, r#"{"version": 99, "entries": []}"#).unwrap();
    assert!(has_values_in(dir.path(), "project-1", "production"));
}
