use std::cell::RefCell;
use std::fs;
use std::path::Path;

use dea_project::{Migration, ProjectError, SCHEMA_VERSION, create_project, migrate, migrate_with};
use tempfile::TempDir;
use time::macros::datetime;

const NOW: time::OffsetDateTime = datetime!(2026-11-17 09:30:00 UTC);

/// A version 0 project: `.yml` environment files, which version 1 renames.
fn project_v0() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("dea.yaml"), "name: x\nschemaVersion: 0\n").unwrap();
    fs::create_dir(root.join("environments")).unwrap();
    fs::write(
        root.join("environments/old.yml"),
        "name: old\nproduction: false\n",
    )
    .unwrap();
    dir
}

fn rename_yml(root: &Path) -> Result<(), ProjectError> {
    for entry in fs::read_dir(root.join("environments"))? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "yml") {
            fs::rename(&path, path.with_extension("yaml"))?;
        }
    }
    Ok(())
}

fn fail(_root: &Path) -> Result<(), ProjectError> {
    Err(ProjectError::Migration("disk full".into()))
}

fn manifest(root: &Path) -> String {
    fs::read_to_string(root.join("dea.yaml")).unwrap()
}

#[test]
fn migrate_noop_when_current() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    create_project(root, "harbor-api").unwrap();
    let o = migrate(root).unwrap();
    assert_eq!((o.from, o.to), (SCHEMA_VERSION, SCHEMA_VERSION));
    assert!(o.backup_dir.is_none());
    assert!(!root.join(".dea-backup").exists());
}

#[test]
fn migrate_with_runs_steps_in_order_and_backs_up() {
    let dir = project_v0();
    let root = dir.path();
    let o = migrate_with(root, 1, &[(0, rename_yml as Migration)], NOW).unwrap();
    assert_eq!((o.from, o.to), (0, 1));
    assert_eq!(
        o.backup_dir.unwrap(),
        root.join(".dea-backup/20261117T093000Z")
    );
    assert!(
        root.join(".dea-backup/20261117T093000Z/environments/old.yml")
            .is_file()
    );
    assert!(root.join(".dea-backup/20261117T093000Z/dea.yaml").is_file());
    assert!(root.join("environments/old.yaml").is_file());
    assert_eq!(manifest(root), "name: x\nschemaVersion: 1\n");
}

thread_local! {
    static RAN: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

fn step0(_: &Path) -> Result<(), ProjectError> {
    RAN.with(|r| r.borrow_mut().push(0));
    Ok(())
}

fn step1(_: &Path) -> Result<(), ProjectError> {
    RAN.with(|r| r.borrow_mut().push(1));
    Ok(())
}

#[test]
fn steps_run_in_version_order_whatever_the_list_order() {
    let dir = project_v0();
    let root = dir.path();
    let steps: [(u32, Migration); 2] = [(1, step1), (0, step0)];
    let o = migrate_with(root, 2, &steps, NOW).unwrap();
    assert_eq!((o.from, o.to), (0, 2));
    assert_eq!(RAN.with(|r| r.borrow().clone()), [0, 1]);
    assert!(manifest(root).contains("schemaVersion: 2"));
}

#[test]
fn migration_failure_keeps_original_version_and_backup() {
    let dir = project_v0();
    let root = dir.path();
    let err = migrate_with(root, 1, &[(0, fail as Migration)], NOW).unwrap_err();
    assert!(matches!(err, ProjectError::Migration(_)), "{err:?}");
    assert!(manifest(root).contains("schemaVersion: 0"));
    assert!(root.join(".dea-backup/20261117T093000Z/dea.yaml").is_file());
}

#[test]
fn failure_in_a_later_step_keeps_the_last_successful_version() {
    let dir = project_v0();
    let root = dir.path();
    let steps: [(u32, Migration); 2] = [(0, rename_yml), (1, fail)];
    assert!(migrate_with(root, 2, &steps, NOW).is_err());
    assert!(manifest(root).contains("schemaVersion: 1"));
    assert!(root.join("environments/old.yaml").is_file());
}

#[test]
fn missing_step_is_an_error_and_changes_nothing() {
    let dir = project_v0();
    let root = dir.path();
    let err = migrate_with(root, 1, &[], NOW).unwrap_err();
    assert!(matches!(err, ProjectError::Migration(_)), "{err:?}");
    assert!(manifest(root).contains("schemaVersion: 0"));
    assert!(!root.join(".dea-backup").exists());
}

#[test]
fn backup_excludes_git_and_previous_backups() {
    let dir = project_v0();
    let root = dir.path();
    fs::create_dir_all(root.join(".git/objects")).unwrap();
    fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::create_dir_all(root.join(".dea-backup/20260101T000000Z")).unwrap();
    fs::write(root.join(".dea-backup/20260101T000000Z/dea.yaml"), "old\n").unwrap();

    let o = migrate_with(root, 1, &[(0, rename_yml as Migration)], NOW).unwrap();
    let backup = o.backup_dir.unwrap();
    assert!(!backup.join(".git").exists());
    assert!(!backup.join(".dea-backup").exists());
    assert!(backup.join("environments/old.yml").is_file());
    // The earlier backup is untouched.
    assert_eq!(
        fs::read_to_string(root.join(".dea-backup/20260101T000000Z/dea.yaml")).unwrap(),
        "old\n"
    );
}

#[test]
fn backups_in_the_same_second_do_not_overwrite_each_other() {
    let dir = project_v0();
    let root = dir.path();
    fs::create_dir_all(root.join(".dea-backup/20261117T093000Z")).unwrap();
    let o = migrate_with(root, 1, &[(0, rename_yml as Migration)], NOW).unwrap();
    assert_eq!(
        o.backup_dir.unwrap(),
        root.join(".dea-backup/20261117T093000Z-2")
    );
}

#[test]
fn migration_keeps_backups_out_of_git() {
    let dir = project_v0();
    let root = dir.path();
    migrate_with(root, 1, &[(0, rename_yml as Migration)], NOW).unwrap();
    let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(gitignore.lines().any(|l| l == ".dea-backup/"));
}

#[test]
fn newer_project_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("dea.yaml"),
        "name: x\nschemaVersion: 9\nlaterField: true\n",
    )
    .unwrap();
    let o = migrate(root).unwrap();
    assert_eq!((o.from, o.to), (9, 9));
    assert!(o.backup_dir.is_none());
    assert_eq!(
        manifest(root),
        "name: x\nschemaVersion: 9\nlaterField: true\n"
    );
}

#[test]
fn missing_manifest_is_not_a_project() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        migrate(dir.path()),
        Err(ProjectError::NotAProject(_))
    ));
}
