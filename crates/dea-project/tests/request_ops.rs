use std::fs;
use std::path::Path;

use dea_project::{
    FolderFile, Project, ProjectError, RequestFile, create_folder, create_project, create_request,
    delete_item, duplicate_item, load_requests, move_item, read_folder, rename_item, restore_item,
    save_folder,
};

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn simple(name: &str) -> String {
    format!("name: {name}\nmethod: GET\nurl: '{{{{baseUrl}}}}/x'\n")
}

/// `shipments/` has list.yaml and by-id.yaml, no _folder.yaml.
fn shipments(dir: &Path) -> Project {
    let project = create_project(dir, "harbor").unwrap();
    write(
        dir,
        "requests/shipments/list.yaml",
        &simple("List shipments"),
    );
    write(
        dir,
        "requests/shipments/by-id.yaml",
        &simple("Shipment by ID"),
    );
    project
}

fn folder_file(root: &Path, dir: &str) -> FolderFile {
    let text = fs::read_to_string(root.join(dir).join("_folder.yaml")).unwrap();
    serde_norway::from_str(&text).unwrap()
}

fn read_request(root: &Path, path: &Path) -> RequestFile {
    serde_norway::from_str(&fs::read_to_string(root.join(path)).unwrap()).unwrap()
}

fn keys(root: &Path, dir: &str) -> Vec<String> {
    let loaded = load_requests(root).unwrap();
    assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
    let folder = loaded
        .tree
        .collections
        .iter()
        .find(|c| c.dir == Path::new(dir))
        .unwrap();
    folder
        .items
        .iter()
        .map(|i| match i {
            dea_project::TreeItem::Folder(f) => f.dir.file_name().unwrap().to_string_lossy().into(),
            dea_project::TreeItem::Request(r) => {
                r.path.file_stem().unwrap().to_string_lossy().into()
            }
        })
        .collect()
}

#[test]
fn created_request_goes_last_in_the_folder_order() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    let path =
        create_request(&project, Path::new("requests/shipments"), "Create shipment").unwrap();
    assert_eq!(path, Path::new("requests/shipments/create-shipment.yaml"));
    let folder = folder_file(dir.path(), "requests/shipments");
    assert_eq!(folder.order, ["by-id", "list", "create-shipment"]);
    let request = read_request(dir.path(), &path);
    assert_eq!(
        (
            request.name.as_str(),
            request.method.as_str(),
            request.url.as_str()
        ),
        ("Create shipment", "GET", "")
    );
}

#[test]
fn created_folder_writes_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    let path = create_folder(&project, Path::new("requests/shipments"), "Status & events").unwrap();
    assert_eq!(path, Path::new("requests/shipments/status-events"));
    let text = fs::read_to_string(
        dir.path()
            .join("requests/shipments/status-events/_folder.yaml"),
    )
    .unwrap();
    assert_eq!(text, "name: Status & events\n");
    assert_eq!(
        folder_file(dir.path(), "requests/shipments").order,
        ["by-id", "list", "status-events"]
    );
}

#[test]
fn new_collection_is_added_to_the_root_order() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    let path = create_folder(&project, Path::new("requests"), "Billing").unwrap();
    assert_eq!(path, Path::new("requests/billing"));
    assert_eq!(
        folder_file(dir.path(), "requests").order,
        ["shipments", "billing"]
    );
}

#[test]
fn renaming_only_the_case_keeps_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    let path = rename_item(&project, Path::new("requests/shipments/list.yaml"), "List").unwrap();
    assert_eq!(path, Path::new("requests/shipments/list.yaml"));
    assert_eq!(read_request(dir.path(), &path).name, "List");
    assert!(!dir.path().join("requests/shipments/list-2.yaml").exists());
}

#[test]
fn renaming_a_request_moves_the_file_and_its_order_key() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    create_request(&project, Path::new("requests/shipments"), "Create shipment").unwrap();
    let path = rename_item(
        &project,
        Path::new("requests/shipments/list.yaml"),
        "All shipments",
    )
    .unwrap();
    assert_eq!(path, Path::new("requests/shipments/all-shipments.yaml"));
    assert!(!dir.path().join("requests/shipments/list.yaml").exists());
    assert_eq!(read_request(dir.path(), &path).name, "All shipments");
    assert_eq!(
        folder_file(dir.path(), "requests/shipments").order,
        ["by-id", "all-shipments", "create-shipment"]
    );
}

#[test]
fn renaming_a_folder_renames_the_directory_and_keeps_its_requests() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    let path = rename_item(&project, Path::new("requests/shipments"), "Freight").unwrap();
    assert_eq!(path, Path::new("requests/freight"));
    assert!(!dir.path().join("requests/shipments").exists());
    assert!(dir.path().join("requests/freight/list.yaml").exists());
    assert_eq!(
        folder_file(dir.path(), "requests/freight").name.as_deref(),
        Some("Freight")
    );
}

#[test]
fn renaming_a_collection_updates_the_root_order() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    create_folder(&project, Path::new("requests"), "Billing").unwrap();
    rename_item(&project, Path::new("requests/shipments"), "Freight").unwrap();
    assert_eq!(
        folder_file(dir.path(), "requests").order,
        ["freight", "billing"]
    );
}

#[test]
fn rename_to_a_taken_name_numbers_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    let path = rename_item(&project, Path::new("requests/shipments/by-id.yaml"), "List").unwrap();
    assert_eq!(path, Path::new("requests/shipments/list-2.yaml"));
    assert_eq!(read_request(dir.path(), &path).name, "List");
}

#[test]
fn duplicate_goes_right_after_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    let path = duplicate_item(&project, Path::new("requests/shipments/by-id.yaml")).unwrap();
    assert_eq!(
        path,
        Path::new("requests/shipments/shipment-by-id-copy.yaml")
    );
    assert_eq!(read_request(dir.path(), &path).name, "Shipment by ID copy");
    assert_eq!(
        folder_file(dir.path(), "requests/shipments").order,
        ["by-id", "shipment-by-id-copy", "list"]
    );
    assert_eq!(
        keys(dir.path(), "requests/shipments"),
        ["by-id", "shipment-by-id-copy", "list"]
    );
}

#[test]
fn duplicating_a_folder_copies_every_file() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    write(
        dir.path(),
        "requests/shipments/inner/deep.yaml",
        &simple("Deep"),
    );
    write(
        dir.path(),
        "requests/shipments/inner/body.json",
        "{\"a\":1}",
    );
    let path = duplicate_item(&project, Path::new("requests/shipments")).unwrap();
    assert_eq!(path, Path::new("requests/shipments-copy"));
    let copy = dir.path().join("requests/shipments-copy");
    assert!(copy.join("list.yaml").exists());
    assert!(copy.join("by-id.yaml").exists());
    assert!(copy.join("inner/deep.yaml").exists());
    assert_eq!(
        fs::read_to_string(copy.join("inner/body.json")).unwrap(),
        "{\"a\":1}"
    );
    assert_eq!(
        folder_file(dir.path(), "requests/shipments-copy")
            .name
            .as_deref(),
        Some("shipments copy")
    );
    assert_eq!(
        folder_file(dir.path(), "requests").order,
        ["shipments", "shipments-copy"]
    );
}

#[test]
fn read_folder_gives_the_settings_or_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    let shipments = Path::new("requests/shipments");
    assert_eq!(
        read_folder(&project, shipments).unwrap(),
        FolderFile::default()
    );
    write(dir.path(), "requests/_folder.yaml", "order: [shipments]\n");
    assert_eq!(
        read_folder(&project, Path::new("requests")).unwrap().order,
        ["shipments"]
    );
    write(
        dir.path(),
        "requests/shipments/_folder.yaml",
        "name: Freight\nheaders:\n  - name: X-Trace\n    value: on\n",
    );
    let settings = read_folder(&project, shipments).unwrap();
    assert_eq!(settings.name.as_deref(), Some("Freight"));
    assert_eq!(settings.headers[0].value, "on");

    write(dir.path(), "requests/shipments/_folder.yaml", "name: [\n");
    assert!(matches!(
        read_folder(&project, shipments),
        Err(ProjectError::InvalidRequest(_))
    ));
    assert!(matches!(
        read_folder(&project, Path::new("requests/gone")),
        Err(ProjectError::InvalidRequest(_))
    ));
    assert!(matches!(
        read_folder(&project, Path::new("environments")),
        Err(ProjectError::InvalidRequest(_))
    ));
}

#[test]
fn save_folder_writes_settings_and_rejects_them_on_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    let settings = FolderFile {
        name: Some("Freight".into()),
        order: vec!["list".into()],
        ..FolderFile::default()
    };
    save_folder(&project, Path::new("requests/shipments"), &settings).unwrap();
    assert_eq!(folder_file(dir.path(), "requests/shipments"), settings);
    let err = save_folder(&project, Path::new("requests"), &settings).unwrap_err();
    assert!(matches!(err, ProjectError::InvalidRequest(_)));
    let blank = FolderFile {
        name: Some(" ".into()),
        ..FolderFile::default()
    };
    assert!(matches!(
        save_folder(&project, Path::new("requests/shipments"), &blank),
        Err(ProjectError::InvalidRequest(_))
    ));
}

#[test]
fn operations_refuse_read_only_projects_and_paths_outside_requests() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = shipments(dir.path());
    let list = Path::new("requests/shipments/list.yaml");
    let outside = Path::new("environments");
    assert!(matches!(
        create_request(&project, outside, "x"),
        Err(ProjectError::InvalidRequest(_))
    ));
    assert!(matches!(
        create_folder(&project, Path::new("../x"), "x"),
        Err(ProjectError::InvalidRequest(_))
    ));
    assert!(matches!(
        rename_item(&project, Path::new("dea.yaml"), "x"),
        Err(ProjectError::InvalidRequest(_))
    ));
    assert!(matches!(
        duplicate_item(&project, outside),
        Err(ProjectError::InvalidRequest(_))
    ));
    assert!(matches!(
        save_folder(&project, outside, &FolderFile::default()),
        Err(ProjectError::InvalidRequest(_))
    ));
    let missing = Path::new("requests/shipments/nope.yaml");
    let err = rename_item(&project, missing, "x").unwrap_err();
    assert_eq!(
        err.to_string(),
        "requests/shipments/nope.yaml does not exist"
    );

    project.read_only = true;
    assert!(matches!(
        create_request(&project, Path::new("requests/shipments"), "x"),
        Err(ProjectError::ReadOnly)
    ));
    assert!(matches!(
        create_folder(&project, Path::new("requests"), "x"),
        Err(ProjectError::ReadOnly)
    ));
    assert!(matches!(
        rename_item(&project, list, "x"),
        Err(ProjectError::ReadOnly)
    ));
    assert!(matches!(
        duplicate_item(&project, list),
        Err(ProjectError::ReadOnly)
    ));
    assert!(matches!(
        save_folder(
            &project,
            Path::new("requests/shipments"),
            &FolderFile::default()
        ),
        Err(ProjectError::ReadOnly)
    ));
    assert!(!dir.path().join("requests/shipments/_folder.yaml").exists());
}

fn with_tracking(dir: &Path) -> Project {
    let project = shipments(dir);
    write(dir, "requests/tracking/events.yaml", &simple("Events"));
    project
}

#[test]
fn moving_into_another_folder_updates_both_orders() {
    let dir = tempfile::tempdir().unwrap();
    let project = with_tracking(dir.path());
    let path = move_item(
        &project,
        Path::new("requests/shipments/list.yaml"),
        Path::new("requests/tracking"),
        0,
    )
    .unwrap();
    assert_eq!(path, Path::new("requests/tracking/list.yaml"));
    assert!(!dir.path().join("requests/shipments/list.yaml").exists());
    assert_eq!(
        folder_file(dir.path(), "requests/shipments").order,
        ["by-id"]
    );
    assert_eq!(
        folder_file(dir.path(), "requests/tracking").order,
        ["list", "events"]
    );
}

#[test]
fn moving_within_a_folder_reorders_it() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    let path = move_item(
        &project,
        Path::new("requests/shipments/by-id.yaml"),
        Path::new("requests/shipments"),
        1,
    )
    .unwrap();
    assert_eq!(path, Path::new("requests/shipments/by-id.yaml"));
    assert_eq!(
        folder_file(dir.path(), "requests/shipments").order,
        ["list", "by-id"]
    );
}

#[test]
fn a_folder_cannot_move_into_its_child() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    create_folder(&project, Path::new("requests/shipments"), "Inner").unwrap();
    for to in ["requests/shipments", "requests/shipments/inner"] {
        let err =
            move_item(&project, Path::new("requests/shipments"), Path::new(to), 0).unwrap_err();
        assert_eq!(err.to_string(), "a folder cannot move into itself", "{to}");
    }
    assert!(dir.path().join("requests/shipments/list.yaml").exists());
}

#[test]
fn a_request_cannot_move_to_the_requests_root() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    let err = move_item(
        &project,
        Path::new("requests/shipments/list.yaml"),
        Path::new("requests"),
        0,
    )
    .unwrap_err();
    assert!(matches!(err, ProjectError::InvalidRequest(_)), "{err}");
    assert!(dir.path().join("requests/shipments/list.yaml").exists());
}

#[test]
fn moving_onto_a_taken_name_numbers_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let project = with_tracking(dir.path());
    write(dir.path(), "requests/tracking/list.yaml", &simple("Other"));
    let path = move_item(
        &project,
        Path::new("requests/shipments/list.yaml"),
        Path::new("requests/tracking"),
        5,
    )
    .unwrap();
    assert_eq!(path, Path::new("requests/tracking/list-2.yaml"));
    assert_eq!(
        read_request(dir.path(), Path::new("requests/tracking/list.yaml")).name,
        "Other"
    );
    assert_eq!(
        folder_file(dir.path(), "requests/tracking").order,
        ["events", "list", "list-2"]
    );
}

#[test]
fn moving_a_folder_takes_its_files_along() {
    let dir = tempfile::tempdir().unwrap();
    let project = with_tracking(dir.path());
    let path = move_item(
        &project,
        Path::new("requests/shipments"),
        Path::new("requests/tracking"),
        0,
    )
    .unwrap();
    assert_eq!(path, Path::new("requests/tracking/shipments"));
    assert!(
        dir.path()
            .join("requests/tracking/shipments/by-id.yaml")
            .exists()
    );
    assert_eq!(
        folder_file(dir.path(), "requests/tracking").order,
        ["shipments", "events"]
    );
    assert_eq!(folder_file(dir.path(), "requests").order, ["tracking"]);
}

#[test]
fn restore_refuses_to_overwrite_a_new_item() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    let deleted = delete_item(&project, Path::new("requests/shipments/list.yaml")).unwrap();
    create_request(&project, Path::new("requests/shipments"), "List").unwrap();
    let err = restore_item(&project, &deleted).unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
    assert_eq!(
        read_request(dir.path(), Path::new("requests/shipments/list.yaml")).name,
        "List"
    );
}

#[test]
fn deleted_request_comes_back_at_its_index() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    let before = fs::read(dir.path().join("requests/shipments/by-id.yaml")).unwrap();
    let deleted = delete_item(&project, Path::new("requests/shipments/by-id.yaml")).unwrap();
    assert_eq!(deleted.index, 0);
    assert!(!dir.path().join("requests/shipments/by-id.yaml").exists());
    assert_eq!(
        folder_file(dir.path(), "requests/shipments").order,
        ["list"]
    );
    restore_item(&project, &deleted).unwrap();
    assert_eq!(
        fs::read(dir.path().join("requests/shipments/by-id.yaml")).unwrap(),
        before
    );
    assert_eq!(
        folder_file(dir.path(), "requests/shipments").order,
        ["by-id", "list"]
    );
}

#[test]
fn deleted_folder_comes_back_with_every_file_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let project = with_tracking(dir.path());
    write(
        dir.path(),
        "requests/shipments/deep/more.yaml",
        &simple("More"),
    );
    write(
        dir.path(),
        "requests/shipments/_folder.yaml",
        "name: Ships\n",
    );
    save_folder(
        &project,
        Path::new("requests"),
        &FolderFile {
            order: vec!["shipments".into(), "tracking".into()],
            ..FolderFile::default()
        },
    )
    .unwrap();
    let deleted = delete_item(&project, Path::new("requests/shipments")).unwrap();
    assert_eq!(deleted.index, 0);
    assert!(!dir.path().join("requests/shipments").exists());
    assert_eq!(folder_file(dir.path(), "requests").order, ["tracking"]);
    restore_item(&project, &deleted).unwrap();
    for file in ["list.yaml", "by-id.yaml", "deep/more.yaml", "_folder.yaml"] {
        assert!(
            dir.path().join("requests/shipments").join(file).exists(),
            "{file}"
        );
    }
    assert_eq!(
        fs::read_to_string(dir.path().join("requests/shipments/_folder.yaml")).unwrap(),
        "name: Ships\n"
    );
    assert_eq!(
        folder_file(dir.path(), "requests").order,
        ["shipments", "tracking"]
    );
}

#[test]
fn deleted_item_debug_hides_contents() {
    let dir = tempfile::tempdir().unwrap();
    let project = shipments(dir.path());
    let deleted = delete_item(&project, Path::new("requests/shipments/list.yaml")).unwrap();
    let text = format!("{deleted:?}");
    assert!(!text.contains("List shipments"), "{text}");
    assert!(!text.contains("baseUrl"), "{text}");
}

#[test]
fn a_failed_move_leaves_the_project_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let project = with_tracking(dir.path());
    write(
        dir.path(),
        "requests/tracking/_folder.yaml",
        "order: [unclosed\n",
    );
    let err = move_item(
        &project,
        Path::new("requests/shipments/list.yaml"),
        Path::new("requests/tracking"),
        0,
    );
    assert!(err.is_err());
    assert!(dir.path().join("requests/shipments/list.yaml").exists());
}
