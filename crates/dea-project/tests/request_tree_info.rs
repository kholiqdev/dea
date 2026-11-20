use std::fs;
use std::path::Path;

use dea_project::{RequestNode, create_project, load_requests, tree_info};
use serde_json::json;

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn request(name: &str, method: &str) -> String {
    format!("name: {name}\nmethod: {method}\nurl: '{{{{baseUrl}}}}/x'\n")
}

#[test]
fn tree_info_counts_requests_below_each_folder() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    create_project(root, "harbor").unwrap();
    write(root, "requests/shipments/_folder.yaml", "name: Shipments\n");
    write(
        root,
        "requests/shipments/list.yaml",
        &request("List shipments", "GET"),
    );
    write(
        root,
        "requests/shipments/eu/create.yaml",
        &request("Create", "POST"),
    );
    write(
        root,
        "requests/shipments/eu/fr/purge.yaml",
        &request("Purge", "PURGE"),
    );
    write(root, "requests/shipments/eu/broken.yaml", "name: [\n");
    write(
        root,
        "requests/carriers/list.yaml",
        &request("Carriers", "GET"),
    );

    let info = tree_info(&load_requests(root).unwrap());

    let counts: Vec<(&str, u32)> = info
        .collections
        .iter()
        .map(|c| (c.name.as_str(), c.request_count))
        .collect();
    assert_eq!(counts, [("carriers", 1), ("Shipments", 3)]);
    let shipments = &info.collections[1];
    assert_eq!(shipments.path, "requests/shipments");
    let RequestNode::Folder(eu) = &shipments.items[0] else {
        panic!("{:?}", shipments.items[0]);
    };
    assert_eq!(
        (eu.path.as_str(), eu.request_count),
        ("requests/shipments/eu", 2)
    );
    let RequestNode::Folder(fr) = &eu.items[0] else {
        panic!("{:?}", eu.items[0]);
    };
    assert_eq!(fr.request_count, 1);

    // The JSON the sidebar reads: `kind` tells folders from requests.
    assert_eq!(
        serde_json::to_value(&shipments.items[1]).unwrap(),
        json!({
            "kind": "request",
            "path": "requests/shipments/list.yaml",
            "name": "List shipments",
            "method": "GET",
            "url": "{{baseUrl}}/x",
        })
    );
    let eu_json = serde_json::to_value(&shipments.items[0]).unwrap();
    assert_eq!(eu_json["kind"], "folder");
    assert_eq!(eu_json["requestCount"], 2);
    assert_eq!(eu_json["broken"], false);

    // The broken file is reported, not counted.
    assert_eq!(info.errors.len(), 1, "{:?}", info.errors);
    assert_eq!(info.errors[0].path, "requests/shipments/eu/broken.yaml");
    assert!(info.errors[0].line.is_some());
    assert!(!info.errors[0].merge_conflict);
}

#[test]
fn a_broken_folder_file_marks_its_folder() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    create_project(root, "harbor").unwrap();
    write(
        root,
        "requests/shipments/_folder.yaml",
        "<<<<<<< HEAD\nname: A\n=======\nname: B\n>>>>>>> other\n",
    );
    write(
        root,
        "requests/shipments/list.yaml",
        &request("List shipments", "GET"),
    );

    let info = tree_info(&load_requests(root).unwrap());

    let shipments = &info.collections[0];
    assert!(shipments.broken);
    assert_eq!(shipments.name, "shipments");
    assert_eq!(shipments.request_count, 1);
    assert_eq!(info.errors[0].path, "requests/shipments/_folder.yaml");
    assert!(info.errors[0].merge_conflict);
}
