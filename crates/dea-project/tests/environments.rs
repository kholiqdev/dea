use std::fs;
use std::path::Path;

use dea_project::{
    ENV_COLORS, EnvReference, Environment, Project, ProjectError, Variable, create_project,
    delete_environment, duplicate_environment, environment_references, is_valid_variable_name,
    load_project, save_environment,
};

fn project(dir: &Path) -> Project {
    create_project(dir, "harbor").unwrap()
}

fn reload(p: &Project) -> Project {
    load_project(&p.root).unwrap().project
}

fn env(name: &str) -> Environment {
    Environment {
        name: name.into(),
        production: false,
        color: None,
        variables: vec![Variable {
            name: "baseUrl".into(),
            value: "https://api.test".into(),
            description: None,
        }],
    }
}

fn names(p: &Project) -> Vec<String> {
    let mut n: Vec<_> = p.environments.iter().map(|e| e.name.clone()).collect();
    n.sort();
    n
}

#[test]
fn saving_writes_one_file_per_environment_and_keeps_others() {
    let dir = tempfile::tempdir().unwrap();
    let p = project(dir.path());
    let before = fs::read_to_string(dir.path().join("environments/production.yaml")).unwrap();
    save_environment(&p, None, &env("qa")).unwrap();
    let mut staging = env("staging");
    staging.color = Some("blue".into());
    save_environment(&p, Some("staging"), &staging).unwrap();

    let p = reload(&p);
    assert_eq!(names(&p), ["local", "production", "qa", "staging"]);
    assert!(dir.path().join("environments/qa.yaml").is_file());
    let saved = p.environments.iter().find(|e| e.name == "staging").unwrap();
    assert_eq!(saved, &staging);
    assert_eq!(
        fs::read_to_string(dir.path().join("environments/production.yaml")).unwrap(),
        before
    );
}

#[test]
fn a_name_that_maps_to_a_taken_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let p = project(dir.path());
    let files_before: Vec<_> = fs::read_dir(dir.path().join("environments"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();

    let err = save_environment(&p, None, &env("Staging")).unwrap_err();
    assert!(
        matches!(&err, ProjectError::NameTaken(n) if n == "Staging"),
        "{err:?}"
    );
    let err = save_environment(&p, Some("local"), &env("staging")).unwrap_err();
    assert!(
        matches!(&err, ProjectError::NameTaken(n) if n == "staging"),
        "{err:?}"
    );
    // "eu/prod" and "eu prod" map to the same file stem.
    save_environment(&p, None, &env("eu/prod")).unwrap();
    let p = reload(&p);
    let err = save_environment(&p, None, &env("eu prod")).unwrap_err();
    assert!(matches!(err, ProjectError::NameTaken(_)), "{err:?}");

    let files_after: Vec<_> = fs::read_dir(dir.path().join("environments"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(files_after.len(), files_before.len() + 1);
}

#[test]
fn renaming_moves_the_file_and_the_default_environment() {
    let dir = tempfile::tempdir().unwrap();
    let p = project(dir.path());
    assert_eq!(p.manifest.default_environment.as_deref(), Some("local"));
    save_environment(&p, Some("local"), &env("dev")).unwrap();

    let p = reload(&p);
    assert_eq!(names(&p), ["dev", "production", "staging"]);
    assert!(!dir.path().join("environments/local.yaml").exists());
    assert!(dir.path().join("environments/dev.yaml").is_file());
    assert_eq!(p.manifest.default_environment.as_deref(), Some("dev"));
}

#[test]
fn renaming_finds_a_file_named_by_hand() {
    let dir = tempfile::tempdir().unwrap();
    let p = project(dir.path());
    fs::rename(
        dir.path().join("environments/staging.yaml"),
        dir.path().join("environments/stg-old.yaml"),
    )
    .unwrap();
    let p = reload(&p);
    save_environment(&p, Some("staging"), &env("stage")).unwrap();
    let p = reload(&p);
    assert_eq!(names(&p), ["local", "production", "stage"]);
    assert!(!dir.path().join("environments/stg-old.yaml").exists());
}

#[test]
fn variables_need_valid_unique_names() {
    let dir = tempfile::tempdir().unwrap();
    let p = project(dir.path());
    for bad in ["", "1x", "a b", "x{"] {
        let mut e = env("qa");
        e.variables[0].name = bad.into();
        assert!(
            matches!(
                save_environment(&p, None, &e),
                Err(ProjectError::InvalidEnvironment(_))
            ),
            "{bad:?}"
        );
    }
    let mut twice = env("qa");
    twice.variables.push(twice.variables[0].clone());
    assert!(matches!(
        save_environment(&p, None, &twice),
        Err(ProjectError::InvalidEnvironment(_))
    ));
    let mut blank = env("  ");
    blank.variables.clear();
    assert!(matches!(
        save_environment(&p, None, &blank),
        Err(ProjectError::InvalidEnvironment(_))
    ));
    for good in ["baseUrl", "_x", "api.v2", "a-b_c"] {
        assert!(is_valid_variable_name(good), "{good}");
    }
}

#[test]
fn red_is_not_a_colour_choice() {
    let dir = tempfile::tempdir().unwrap();
    let p = project(dir.path());
    let mut e = env("qa");
    e.color = Some("red".into());
    assert!(matches!(
        save_environment(&p, None, &e),
        Err(ProjectError::InvalidEnvironment(_))
    ));
    e.color = None;
    save_environment(&p, None, &e).unwrap();
    for color in ENV_COLORS {
        e.color = Some(color.into());
        save_environment(&p, Some("qa"), &e).unwrap();
    }
}

#[test]
fn a_file_with_conflict_markers_is_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let p = project(dir.path());
    let path = dir.path().join("environments/staging.yaml");
    let conflicted = "<<<<<<< HEAD\nname: staging\n=======\nname: stage\n>>>>>>> other\n";
    fs::write(&path, conflicted).unwrap();
    let err = save_environment(&p, Some("staging"), &env("staging")).unwrap_err();
    assert!(matches!(err, ProjectError::MergeConflict(_)), "{err:?}");
    assert!(matches!(
        delete_environment(&p, "staging"),
        Err(ProjectError::MergeConflict(_))
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), conflicted);
}

#[test]
fn a_read_only_project_is_never_written() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = project(dir.path());
    p.read_only = true;
    assert!(matches!(
        save_environment(&p, None, &env("qa")),
        Err(ProjectError::ReadOnly)
    ));
    assert!(matches!(
        delete_environment(&p, "local"),
        Err(ProjectError::ReadOnly)
    ));
    assert!(matches!(
        duplicate_environment(&p, "local"),
        Err(ProjectError::ReadOnly)
    ));
    assert!(!dir.path().join("environments/qa.yaml").exists());
}

#[test]
fn deleting_removes_the_file_and_clears_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let p = project(dir.path());
    delete_environment(&p, "local").unwrap();
    let p = reload(&p);
    assert_eq!(names(&p), ["production", "staging"]);
    assert_eq!(p.manifest.default_environment, None);
}

#[test]
fn duplicating_picks_a_free_name() {
    let dir = tempfile::tempdir().unwrap();
    let p = project(dir.path());
    let first = duplicate_environment(&p, "staging").unwrap();
    assert_eq!(first.name, "staging copy");
    let p = reload(&p);
    let second = duplicate_environment(&p, "staging").unwrap();
    assert_eq!(second.name, "staging copy 2");
    let prod = duplicate_environment(&reload(&p), "production").unwrap();
    assert!(prod.production, "a copy of production stays production");
}

#[test]
fn the_default_environment_is_a_reference() {
    let dir = tempfile::tempdir().unwrap();
    let p = project(dir.path());
    assert_eq!(
        environment_references(&p, "local"),
        [EnvReference {
            label: "Default environment of the project".into(),
            count: 1,
        }]
    );
    assert_eq!(environment_references(&p, "staging"), []);
}

#[test]
fn a_colour_already_in_the_file_still_saves() {
    let dir = tempfile::tempdir().unwrap();
    let p = project(dir.path());
    fs::write(
        p.root.join("environments/staging.yaml"),
        "name: staging\nproduction: false\ncolor: teal\n",
    )
    .unwrap();
    let p = reload(&p);
    // Editing a variable keeps a colour this Dea does not know.
    let mut staging = env("staging");
    staging.color = Some("teal".into());
    save_environment(&p, Some("staging"), &staging).unwrap();
    // Choosing it for another environment is still refused.
    let mut local = env("local");
    local.color = Some("teal".into());
    assert!(matches!(
        save_environment(&p, Some("local"), &local),
        Err(ProjectError::InvalidEnvironment(_))
    ));
}
