use std::fs;

use dea_project::{ProjectError, SCHEMA_VERSION, create_project};

#[test]
fn create_project_writes_expected_tree() {
    let dir = tempfile::tempdir().unwrap();
    create_project(dir.path(), "harbor-api").unwrap();
    for f in [
        "dea.yaml",
        "vault.yaml",
        ".gitignore",
        "environments/local.yaml",
        "environments/staging.yaml",
        "environments/production.yaml",
    ] {
        assert!(dir.path().join(f).is_file(), "{f}");
    }
    let manifest = fs::read_to_string(dir.path().join("dea.yaml")).unwrap();
    let lines: Vec<_> = manifest.lines().collect();
    assert_eq!(lines.len(), 4, "{manifest}");
    assert_eq!(lines[0], "name: harbor-api");
    let id = lines[1].strip_prefix("id: ").expect("id after the name");
    assert!(uuid::Uuid::parse_str(id).is_ok(), "{id}");
    assert_eq!(
        lines[2..],
        ["schemaVersion: 1", "defaultEnvironment: local"]
    );
    let prod = fs::read_to_string(dir.path().join("environments/production.yaml")).unwrap();
    assert_eq!(prod, "name: production\nproduction: true\n");
    let staging = fs::read_to_string(dir.path().join("environments/staging.yaml")).unwrap();
    assert_eq!(staging, "name: staging\nproduction: false\n");
    assert_eq!(
        fs::read_to_string(dir.path().join("vault.yaml")).unwrap(),
        "{}\n"
    );
    let gitignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(gitignore.lines().any(|l| l == ".dea-backup/"));
}

#[test]
fn create_project_returns_what_it_wrote() {
    let dir = tempfile::tempdir().unwrap();
    let project = create_project(dir.path(), "harbor-api").unwrap();
    assert_eq!(project.root, dir.path());
    assert_eq!(project.manifest.name, "harbor-api");
    assert_eq!(project.manifest.schema_version, SCHEMA_VERSION);
    let names: Vec<_> = project
        .environments
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    // Sorted by file name, the same order as any loaded project.
    assert_eq!(names, ["local", "production", "staging"]);
    let production: Vec<_> = project.environments.iter().map(|e| e.production).collect();
    assert_eq!(production, [false, true, false]);
    assert!(project.vault.secrets.is_empty());
    assert!(!project.read_only);
}

#[test]
fn create_project_refuses_existing_manifest() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("dea.yaml"), "name: old\n").unwrap();
    assert!(matches!(
        create_project(dir.path(), "harbor-api"),
        Err(ProjectError::AlreadyExists(_))
    ));
    assert_eq!(
        fs::read_to_string(dir.path().join("dea.yaml")).unwrap(),
        "name: old\n"
    );
}

#[test]
fn create_project_refuses_existing_environment_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("environments")).unwrap();
    fs::write(dir.path().join("environments/staging.yaml"), "keep: me\n").unwrap();
    assert!(matches!(
        create_project(dir.path(), "harbor-api"),
        Err(ProjectError::AlreadyExists(_))
    ));
    assert!(!dir.path().join("dea.yaml").exists());
}

#[test]
fn create_project_keeps_an_existing_gitignore() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".gitignore"), "node_modules/\n").unwrap();
    create_project(dir.path(), "harbor-api").unwrap();
    let gitignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert_eq!(gitignore, "node_modules/\n.dea-backup/\n");
}

#[test]
fn create_project_makes_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("new/harbor-api");
    create_project(&root, "harbor-api").unwrap();
    assert!(root.join("dea.yaml").is_file());
}

#[test]
fn new_project_has_an_id() {
    let dir = tempfile::tempdir().unwrap();
    let project = create_project(dir.path(), "harbor-api").unwrap();
    let id = project.manifest.id.clone().expect("id is written");
    assert!(uuid::Uuid::parse_str(&id).is_ok(), "{id}");
    assert_eq!(project.vault_scope(), id);
}

#[test]
fn two_new_projects_get_different_ids() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let a = create_project(a.path(), "a").unwrap();
    let b = create_project(b.path(), "b").unwrap();
    assert_ne!(a.manifest.id, b.manifest.id);
}
