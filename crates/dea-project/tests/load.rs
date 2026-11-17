use std::fs;
use std::path::Path;

use dea_project::{ProjectError, create_project, has_merge_conflict_markers, load_project};
use tempfile::TempDir;

/// A fresh project with the environments local, staging and production.
fn project() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    create_project(dir.path(), "harbor-api").unwrap();
    dir
}

fn write(dir: &TempDir, rel: &str, text: &str) {
    fs::write(dir.path().join(rel), text).unwrap();
}

fn env_names(dir: &TempDir) -> Vec<String> {
    load_project(dir.path())
        .unwrap()
        .project
        .environments
        .into_iter()
        .map(|e| e.name)
        .collect()
}

#[test]
fn missing_manifest_is_not_a_project() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        load_project(dir.path()),
        Err(ProjectError::NotAProject(_))
    ));
}

#[test]
fn invalid_environment_reports_line_and_keeps_others() {
    let dir = project();
    write(
        &dir,
        "environments/staging.yaml",
        "name: staging\nproduction: maybe\n",
    );
    let l = load_project(dir.path()).unwrap();
    assert_eq!(l.errors.len(), 1);
    assert_eq!(l.errors[0].path, Path::new("environments/staging.yaml"));
    assert_eq!(l.errors[0].line, Some(2));
    let names: Vec<_> = l
        .project
        .environments
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, ["local", "production"]);
}

#[test]
fn unknown_field_is_an_error() {
    let dir = project();
    write(
        &dir,
        "environments/local.yaml",
        "name: local\nproduction: false\ncolour: red\n",
    );
    let l = load_project(dir.path()).unwrap();
    assert_eq!(l.errors.len(), 1);
    assert!(l.errors[0].message.contains("colour"), "{}", l.errors[0]);
}

#[test]
fn merge_conflict_file_is_flagged() {
    let dir = project();
    write(
        &dir,
        "environments/staging.yaml",
        "<<<<<<< HEAD\nname: a\n=======\nname: b\n>>>>>>> main\n",
    );
    let l = load_project(dir.path()).unwrap();
    assert_eq!(l.errors.len(), 1);
    assert!(
        l.errors[0].message.contains("merge conflict"),
        "{}",
        l.errors[0]
    );
    assert_eq!(l.project.environments.len(), 2);
}

#[test]
fn merge_conflict_markers_need_whole_marker_lines() {
    assert!(has_merge_conflict_markers("a: 1\n<<<<<<< HEAD\n"));
    assert!(has_merge_conflict_markers("=======\n"));
    assert!(has_merge_conflict_markers(">>>>>>> feature/x\n"));
    assert!(!has_merge_conflict_markers("value: =======\n"));
    assert!(!has_merge_conflict_markers(
        "note: '<<<<<<< not a marker'\n"
    ));
}

#[test]
fn newer_schema_opens_read_only() {
    let dir = project();
    write(&dir, "dea.yaml", "name: harbor-api\nschemaVersion: 2\n");
    let l = load_project(dir.path()).unwrap();
    assert!(l.project.read_only);
    assert!(l.errors.is_empty(), "{:?}", l.errors);
    assert_eq!(l.project.manifest.schema_version, 2);
}

#[test]
fn newer_schema_with_new_manifest_fields_still_opens_read_only() {
    let dir = project();
    write(
        &dir,
        "dea.yaml",
        "name: harbor-api\nschemaVersion: 3\nfeatureAddedLater: true\n",
    );
    let l = load_project(dir.path()).unwrap();
    assert!(l.project.read_only);
    assert_eq!(l.project.manifest.name, "harbor-api");
}

#[test]
fn current_schema_is_writable() {
    let dir = project();
    assert!(!load_project(dir.path()).unwrap().project.read_only);
}

#[test]
fn invalid_manifest_is_fatal() {
    let dir = project();
    write(&dir, "dea.yaml", "name: harbor-api\nschemaVersion: two\n");
    match load_project(dir.path()) {
        Err(ProjectError::InvalidManifest(e)) => {
            assert_eq!(e.path, Path::new("dea.yaml"));
            assert_eq!(e.line, Some(2));
        }
        other => panic!("expected InvalidManifest, got {other:?}"),
    }
}

#[test]
fn loads_crlf_files() {
    let dir = project();
    write(&dir, "dea.yaml", "name: harbor-api\r\nschemaVersion: 1\r\n");
    write(
        &dir,
        "environments/staging.yaml",
        "name: staging\r\nproduction: false\r\nvariables:\r\n- name: baseUrl\r\n  value: https://staging.harbor.test\r\n",
    );
    let l = load_project(dir.path()).unwrap();
    assert!(l.errors.is_empty(), "{:?}", l.errors);
    let staging = l
        .project
        .environments
        .iter()
        .find(|e| e.name == "staging")
        .unwrap();
    assert_eq!(staging.variables[0].value, "https://staging.harbor.test");
}

#[test]
fn loads_file_with_utf8_bom() {
    let dir = project();
    write(
        &dir,
        "environments/local.yaml",
        "\u{feff}name: local\nproduction: false\n",
    );
    let l = load_project(dir.path()).unwrap();
    assert!(l.errors.is_empty(), "{:?}", l.errors);
    assert_eq!(l.project.environments[0].name, "local");
}

#[test]
fn empty_environment_file_reports_error() {
    let dir = project();
    write(&dir, "environments/staging.yaml", "  \n");
    let l = load_project(dir.path()).unwrap();
    assert_eq!(l.errors.len(), 1);
    assert_eq!(l.errors[0].message, "empty file");
    assert_eq!(l.project.environments.len(), 2);
}

#[test]
fn environments_are_sorted_by_file_name_and_other_files_ignored() {
    let dir = project();
    write(
        &dir,
        "environments/alpha.yaml",
        "name: alpha\nproduction: false\n",
    );
    write(&dir, "environments/notes.txt", "not an environment\n");
    write(&dir, "environments/.DS_Store", "");
    assert_eq!(env_names(&dir), ["alpha", "local", "production", "staging"]);
}

#[test]
fn missing_environments_folder_is_not_an_error() {
    let dir = project();
    fs::remove_dir_all(dir.path().join("environments")).unwrap();
    let l = load_project(dir.path()).unwrap();
    assert!(l.errors.is_empty());
    assert!(l.project.environments.is_empty());
}

#[test]
fn invalid_vault_file_is_reported_and_not_fatal() {
    let dir = project();
    write(
        &dir,
        "vault.yaml",
        "secrets:\n- name: access_token\n  type: certificate\n",
    );
    let l = load_project(dir.path()).unwrap();
    assert_eq!(l.errors.len(), 1);
    assert_eq!(l.errors[0].path, Path::new("vault.yaml"));
    assert!(l.project.vault.secrets.is_empty());
    assert_eq!(l.project.environments.len(), 3);
}

#[test]
fn create_then_load_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let p = create_project(dir.path(), "harbor-api").unwrap();
    assert_eq!(p.environments.len(), 3);
    assert!(
        p.environments
            .iter()
            .find(|e| e.name == "production")
            .unwrap()
            .production
    );
    assert_eq!(load_project(dir.path()).unwrap().project, p);
}
