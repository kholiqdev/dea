use std::fs;
use std::path::Path;

use dea_engine::projects::{
    MAX_RECENT, OpenError, ProjectInfo, new_project, open_project, recent_entries,
};
use dea_engine::store::Store;

fn project_at(dir: &Path, name: &str) -> dea_project::Project {
    new_project(dir, name).unwrap().project
}

#[test]
fn recent_projects_keep_the_ten_newest() {
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let store = Store::new(data.path());
    for i in 0..12_u64 {
        let p = project_at(&work.path().join(format!("p{i}")), &format!("p{i}"));
        store.touch_recent(&p, 1_000 + i).unwrap();
    }
    let recent = store.load_recent();
    assert_eq!(recent.len(), MAX_RECENT);
    assert_eq!(recent[0].name, "p11");
    assert_eq!(recent[9].name, "p2");
}

#[test]
fn the_same_project_by_another_path_is_one_recent_entry() {
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let store = Store::new(data.path());
    let root = work.path().join("harbor");
    let p = project_at(&root, "harbor");
    store.touch_recent(&p, 1).unwrap();

    // The same folder with a trailing slash.
    let again = open_project(&work.path().join("harbor/")).unwrap().project;
    store.touch_recent(&again, 2).unwrap();
    #[cfg(unix)]
    {
        let link = work.path().join("link");
        std::os::unix::fs::symlink(&root, &link).unwrap();
        let by_link = open_project(&link).unwrap().project;
        store.touch_recent(&by_link, 3).unwrap();
    }
    assert_eq!(store.load_recent().len(), 1);

    // A copy with the same id in another folder replaces the entry.
    let moved = work.path().join("moved");
    fs::create_dir(&moved).unwrap();
    for f in ["dea.yaml", "vault.yaml"] {
        fs::copy(root.join(f), moved.join(f)).unwrap();
    }
    let copy = open_project(&moved).unwrap().project;
    store.touch_recent(&copy, 4).unwrap();
    let recent = store.load_recent();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].root, fs::canonicalize(&moved).unwrap());
}

#[test]
fn recent_entries_mark_missing_folders_and_count_requests() {
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let store = Store::new(data.path());
    let kept = work.path().join("kept");
    let p = project_at(&kept, "kept");
    fs::create_dir_all(kept.join("requests/a/b")).unwrap();
    for f in [
        "requests/one.yaml",
        "requests/a/two.yaml",
        "requests/a/b/three.yaml",
        "requests/a/notes.txt",
    ] {
        fs::write(kept.join(f), "x").unwrap();
    }
    store.touch_recent(&p, 2).unwrap();
    let gone = work.path().join("gone");
    let q = project_at(&gone, "gone");
    store.touch_recent(&q, 1).unwrap();
    fs::remove_dir_all(&gone).unwrap();

    let entries = recent_entries(&store.load_recent());
    assert_eq!(entries.len(), 2);
    assert_eq!(
        (
            entries[0].name.as_str(),
            entries[0].request_count,
            entries[0].missing
        ),
        ("kept", 3, false)
    );
    assert_eq!(
        (
            entries[1].name.as_str(),
            entries[1].request_count,
            entries[1].missing
        ),
        ("gone", 0, true)
    );

    store.remove_recent(Path::new(&entries[1].root)).unwrap();
    assert_eq!(store.load_recent().len(), 1);
}

#[test]
fn opening_a_folder_without_dea_yaml_names_it() {
    let work = tempfile::tempdir().unwrap();
    let err = open_project(work.path()).unwrap_err();
    assert!(matches!(err, OpenError::NotAProject { .. }), "{err:?}");
    let message = err.to_string();
    assert!(message.contains("dea.yaml"), "{message}");
    assert!(
        message.contains(
            &work
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        ),
        "{message}"
    );

    let file = work.path().join("file.txt");
    fs::write(&file, "x").unwrap();
    assert!(matches!(
        open_project(&file),
        Err(OpenError::NotAFolder { .. })
    ));
}

#[test]
fn a_new_project_gets_local_staging_and_production() {
    let work = tempfile::tempdir().unwrap();
    let location = work.path().join("new/place");
    let opened = new_project(&location, "harbor-api").unwrap();
    let names: Vec<_> = opened
        .project
        .environments
        .iter()
        .map(|e| (e.name.as_str(), e.production))
        .collect();
    assert_eq!(
        names,
        [("local", false), ("production", true), ("staging", false)]
    );
    assert!(location.join("dea.yaml").is_file());
    assert!(matches!(
        new_project(&work.path().join("x"), "  "),
        Err(OpenError::EmptyName)
    ));
}

#[test]
fn project_info_lists_environments_secrets_and_problems() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path().join("p");
    project_at(&root, "p");
    fs::write(root.join("environments/broken.yaml"), "name: [").unwrap();
    let opened = open_project(&root).unwrap();
    let info = ProjectInfo::new(&opened, 1);
    assert_eq!(info.name, "p");
    assert_eq!(info.slot, 1);
    assert_eq!(info.scope, opened.project.vault_scope());
    assert_eq!(info.environments.len(), 3);
    assert_eq!(info.errors.len(), 1);
    assert_eq!(info.errors[0].path, "environments/broken.yaml");
    assert!(!info.missing);

    let missing = ProjectInfo::missing(&root.join("nope"), "scope", 0);
    assert!(missing.missing);
    assert_eq!(missing.name, "nope");
}

#[test]
fn project_info_marks_files_with_conflict_markers() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path().join("p");
    project_at(&root, "p");
    fs::write(
        root.join("environments/staging.yaml"),
        "<<<<<<< HEAD\nname: staging\n=======\nname: stage\n>>>>>>> other\n",
    )
    .unwrap();
    fs::write(root.join("environments/broken.yaml"), "name: [").unwrap();
    let info = ProjectInfo::new(&open_project(&root).unwrap(), 0);
    let flags: Vec<_> = info
        .errors
        .iter()
        .map(|e| (e.path.as_str(), e.merge_conflict))
        .collect();
    assert!(
        flags.contains(&("environments/staging.yaml", true)),
        "{flags:?}"
    );
    assert!(
        flags.contains(&("environments/broken.yaml", false)),
        "{flags:?}"
    );
}

#[test]
fn project_info_names_the_file_of_each_environment() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path().join("p");
    project_at(&root, "p");
    fs::write(
        root.join("environments/Stage Env.yaml"),
        "name: qa\nproduction: false\n",
    )
    .unwrap();
    let info = ProjectInfo::new(&open_project(&root).unwrap(), 0);
    assert_eq!(
        info.environment_files.get("qa").map(String::as_str),
        Some("environments/Stage Env.yaml")
    );
    for env in &info.environments {
        assert!(
            info.environment_files.contains_key(&env.name),
            "{}",
            env.name
        );
    }
}
