//! A secret canary goes through every Vault path and must never show up in
//! the project folder, the app data, `Debug` output or error messages.

mod support;

use std::sync::Arc;

use dea_engine::UseError;
use dea_project::{ResolveContext, SecretKind, SystemDynamicSource, create_project, resolve};
use dea_testkit::{canary, find_canary};
use dea_vault::{OAuthToken, SecretData, SecretKey};
use secrecy::SecretString;
use support::{FakeClipboard, FakePrompter, ManualClock, file_service};

#[test]
fn canary_never_leaves_the_vault() {
    let secret = canary();
    let project_dir = tempfile::tempdir().unwrap();
    let app_data = tempfile::tempdir().unwrap();
    let project = create_project(project_dir.path(), "harbor-api").unwrap();
    let scope = project.vault_scope();

    // The Linux path: encrypted file and Vault password.
    let answers = [Some("correct horse"); 8];
    let (vault, statuses) = file_service(
        app_data.path(),
        FakePrompter::answering(&answers),
        ManualClock::new(),
    );
    vault.unlock("Unlock the Vault").unwrap();
    vault
        .authorize_production("Send GET /v1/me to production", &["token"])
        .unwrap();

    let kinds = [
        ("token", SecretKind::Token),
        ("password", SecretKind::Password),
        ("deploy_key", SecretKind::SshKey),
        ("api_key", SecretKind::ApiKey),
        ("github", SecretKind::OauthToken),
    ];
    let mut debug_text = String::new();
    for environment in ["staging", "production"] {
        let production = environment == "production";
        for (name, kind) in kinds {
            let key = SecretKey {
                scope: scope.clone(),
                environment: environment.into(),
                name: name.into(),
            };
            let data = if kind == SecretKind::OauthToken {
                SecretData::OAuth(OAuthToken {
                    access_token: SecretString::from(secret.clone()),
                    refresh_token: Some(SecretString::from(secret.clone())),
                    expires_at: None,
                })
            } else {
                SecretData::Text(SecretString::from(secret.clone()))
            };
            vault.set_secret(&key, kind, &data).unwrap();
            let read = vault.secret(&key, production).unwrap();
            assert_eq!(read.as_ref().unwrap().value_for_use(), secret);
            debug_text += &format!("{read:?} {data:?}");

            let clipboard = Arc::new(FakeClipboard::default());
            vault
                .copy_secret(
                    &key,
                    production,
                    clipboard,
                    std::time::Duration::from_millis(1),
                )
                .unwrap();
        }

        let lookup = vault.lookup(&scope, production);
        let mut dynamic = SystemDynamicSource;
        let mut ctx = ResolveContext {
            scopes: &[],
            environment,
            secrets: &lookup,
            dynamic: &mut dynamic,
        };
        let resolved = resolve("Bearer {{vault:token}}", &mut ctx).unwrap();
        assert_eq!(resolved.text, format!("Bearer {secret}"));
        debug_text += &format!("{:?}", resolved.used);
    }

    debug_text += &format!(
        "{:?} {:?}",
        vault.statuses(&project),
        statuses.lock().unwrap()
    );
    vault.lock();
    let key = SecretKey {
        scope: scope.clone(),
        environment: "staging".into(),
        name: "token".into(),
    };
    let locked: Result<_, UseError> = vault.secret(&key, false);
    debug_text += &format!("{locked:?}");
    if let Err(e) = locked {
        debug_text += &e.to_string();
    }

    assert!(
        find_canary(&[project_dir.path(), app_data.path()], &secret).is_empty(),
        "the canary was written to disk"
    );
    assert!(!debug_text.contains(&secret), "{debug_text}");
}
