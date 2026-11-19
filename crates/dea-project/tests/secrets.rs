use std::fs;
use std::path::Path;

use dea_project::{
    Project, ProjectError, SecretKind, SecretSpec, create_project, load_project, save_secret_spec,
};

fn project(dir: &Path) -> Project {
    create_project(dir, "harbor").unwrap()
}

fn spec(name: &str, kind: SecretKind, description: Option<&str>) -> SecretSpec {
    SecretSpec {
        name: name.into(),
        kind,
        description: description.map(Into::into),
    }
}

#[test]
fn a_new_secret_is_declared_without_a_value() {
    let dir = tempfile::tempdir().unwrap();
    let p = project(dir.path());
    let declared = spec("access_token", SecretKind::Token, Some("Login token"));
    save_secret_spec(&p, &declared).unwrap();

    let reloaded = load_project(dir.path()).unwrap().project;
    assert_eq!(reloaded.vault.secrets, [declared]);
    let text = fs::read_to_string(dir.path().join("vault.yaml")).unwrap();
    assert!(text.contains("access_token"));
    assert!(!text.contains("value"));
}

#[test]
fn the_same_name_takes_the_new_type_and_description() {
    let dir = tempfile::tempdir().unwrap();
    let p = project(dir.path());
    save_secret_spec(&p, &spec("key", SecretKind::Token, Some("old"))).unwrap();
    save_secret_spec(&p, &spec("other", SecretKind::Password, None)).unwrap();
    let newer = spec("key", SecretKind::ApiKey, None);
    save_secret_spec(&p, &newer).unwrap();

    let secrets = load_project(dir.path()).unwrap().project.vault.secrets;
    assert_eq!(secrets.len(), 2);
    assert_eq!(secrets[0], newer);
    assert_eq!(secrets[1].name, "other");
}

#[test]
fn a_bad_secret_name_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let p = project(dir.path());
    let before = fs::read_to_string(dir.path().join("vault.yaml")).unwrap();
    for name in ["access token", "", "1st"] {
        match save_secret_spec(&p, &spec(name, SecretKind::Token, None)) {
            Err(ProjectError::InvalidSecret(m)) => assert!(m.contains(&format!("\"{name}\""))),
            other => panic!("expected InvalidSecret for {name:?}, got {other:?}"),
        }
    }
    assert_eq!(
        fs::read_to_string(dir.path().join("vault.yaml")).unwrap(),
        before
    );
}

#[test]
fn vault_yaml_with_conflict_markers_is_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let p = project(dir.path());
    let conflicted = "<<<<<<< HEAD\nsecrets: []\n=======\nsecrets: []\n>>>>>>> other\n";
    fs::write(dir.path().join("vault.yaml"), conflicted).unwrap();
    assert!(matches!(
        save_secret_spec(&p, &spec("key", SecretKind::Token, None)),
        Err(ProjectError::MergeConflict(_))
    ));
    assert_eq!(
        fs::read_to_string(dir.path().join("vault.yaml")).unwrap(),
        conflicted
    );
}

#[test]
fn a_read_only_project_refuses_declarations() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = project(dir.path());
    p.read_only = true;
    assert!(matches!(
        save_secret_spec(&p, &spec("key", SecretKind::Token, None)),
        Err(ProjectError::ReadOnly)
    ));
}
