use std::fs;
use std::path::Path;

use dea_project::{
    FolderFile, Project, ProjectError, RequestFile, create_folder, create_project, create_request,
    duplicate_item, load_requests, rename_item, save_folder,
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
