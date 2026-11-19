use std::fs;
use std::path::Path;

use dea_project::{
    ApiKeyLocation, Auth, Body, FormField, KeyValue, Project, ProjectError, RawLanguage,
    RequestFile, SendOptions, TreeItem, Variable, create_project, load_requests, save_request,
};

fn project(dir: &Path) -> Project {
    create_project(dir, "harbor").unwrap()
}

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn simple(name: &str) -> String {
    format!("name: {name}\nmethod: GET\nurl: '{{{{baseUrl}}}}/x'\n")
}

fn kv(name: &str, value: &str) -> KeyValue {
    KeyValue {
        name: name.into(),
        value: value.into(),
        description: None,
        enabled: true,
    }
}

fn request() -> RequestFile {
    RequestFile {
        name: "Get shipment".into(),
        method: "GET".into(),
        url: "{{baseUrl}}/shipments/:id".into(),
        path_variables: vec![Variable {
            name: "id".into(),
            value: "42".into(),
            description: Some("Shipment id".into()),
        }],
        params: vec![
            kv("expand", "events"),
            KeyValue {
                description: Some("Not needed".into()),
                enabled: false,
                ..kv("debug", "1")
            },
        ],
        headers: vec![kv("Accept", "application/json")],
        body: Some(Body::FormData {
            fields: vec![
                FormField {
                    name: "note".into(),
                    value: Some("hello".into()),
                    file: None,
                    content_type: None,
                    description: None,
                    enabled: true,
                },
                FormField {
                    name: "doc".into(),
                    value: None,
                    file: Some("fixtures/doc.pdf".into()),
                    content_type: Some("application/pdf".into()),
                    description: None,
                    enabled: false,
                },
            ],
        }),
        auth: Auth::ApiKey {
            key: "api_key".into(),
            value: "{{vault:api_key}}".into(),
            location: ApiKeyLocation::Query,
        },
        options: SendOptions {
            timeout_ms: Some(5000),
            follow_redirects: Some(false),
            max_redirects: Some(3),
            verify_tls: Some(true),
            use_system_proxy: Some(false),
            cookies: Some(true),
        },
    }
}

fn names(items: &[TreeItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item {
            TreeItem::Folder(f) => f.dir.file_name().unwrap().to_string_lossy().into_owned(),
            TreeItem::Request(r) => r.path.file_stem().unwrap().to_string_lossy().into_owned(),
        })
        .collect()
}

#[test]
fn request_file_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let project = project(dir.path());
    let root = dir.path();
    let request = request();
    save_request(
        &project,
        Path::new("requests/shipments/get-shipment.yaml"),
        &request,
    )
    .unwrap();
    let text = fs::read_to_string(root.join("requests/shipments/get-shipment.yaml")).unwrap();
    assert!(text.starts_with("name: Get shipment\nmethod: GET\nurl: "));
    let loaded = load_requests(root).unwrap();
    assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
    assert_eq!(
        loaded
            .tree
            .find(Path::new("requests/shipments/get-shipment.yaml"))
            .unwrap()
            .entry
            .request,
        request
    );
}

#[test]
fn minimal_request_writes_only_filled_fields() {
    let dir = tempfile::tempdir().unwrap();
    let project = project(dir.path());
    let request = RequestFile {
        name: "Ping".into(),
        method: "GET".into(),
        url: "https://x.test".into(),
        path_variables: vec![],
        params: vec![],
        headers: vec![],
        body: None,
        auth: Auth::Inherit,
        options: SendOptions::default(),
    };
    save_request(&project, Path::new("requests/a/ping.yaml"), &request).unwrap();
    let text = fs::read_to_string(dir.path().join("requests/a/ping.yaml")).unwrap();
    assert_eq!(text, "name: Ping\nmethod: GET\nurl: https://x.test\n");
}

#[test]
fn auth_none_and_bodies_use_their_file_names() {
    let dir = tempfile::tempdir().unwrap();
    let project = project(dir.path());
    let mut r = request();
    r.auth = Auth::NoAuth;
    r.body = Some(Body::Raw {
        language: RawLanguage::Xml,
        text: "<a/>".into(),
    });
    save_request(&project, Path::new("requests/a/r.yaml"), &r).unwrap();
    let text = fs::read_to_string(dir.path().join("requests/a/r.yaml")).unwrap();
    assert!(text.contains("auth:\n  type: none\n"), "{text}");
    assert!(text.contains("mode: raw\n"), "{text}");
    assert!(text.contains("language: xml\n"), "{text}");
}

#[test]
fn items_follow_the_folder_order() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "requests/shipments/_folder.yaml",
        "order:\n- create\n- list\n",
    );
    for n in ["create", "list", "by-id"] {
        write(root, &format!("requests/shipments/{n}.yaml"), &simple(n));
    }
    write(
        root,
        "requests/shipments/tracking/events.yaml",
        &simple("e"),
    );
    let loaded = load_requests(root).unwrap();
    assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
    let shipments = &loaded.tree.collections[0];
    assert_eq!(
        names(&shipments.items),
        ["create", "list", "tracking", "by-id"]
    );
}

#[test]
fn collections_follow_the_root_order() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "requests/_folder.yaml",
        "order:\n- tracking\n- shipments\n",
    );
    write(root, "requests/shipments/a.yaml", &simple("a"));
    write(root, "requests/tracking/b.yaml", &simple("b"));
    write(root, "requests/billing/c.yaml", &simple("c"));
    let loaded = load_requests(root).unwrap();
    let dirs: Vec<_> = loaded
        .tree
        .collections
        .iter()
        .map(|c| c.dir.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        dirs,
        [
            "requests/tracking",
            "requests/shipments",
            "requests/billing"
        ]
    );
}

#[test]
fn broken_request_is_reported_and_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "requests/a/good.yaml", &simple("Good"));
    write(
        root,
        "requests/a/bad.yaml",
        "name: Bad\nmethod: GET\nurl: [unclosed\n",
    );
    let loaded = load_requests(root).unwrap();
    assert_eq!(loaded.tree.requests().count(), 1);
    assert_eq!(loaded.errors.len(), 1);
    assert_eq!(loaded.errors[0].path, Path::new("requests/a/bad.yaml"));
    assert!(loaded.errors[0].line.is_some());
}

#[test]
fn broken_folder_file_keeps_its_requests_and_is_marked() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "requests/a/_folder.yaml",
        "<<<<<<< HEAD\nname: A\n=======\nname: B\n>>>>>>> other\n",
    );
    write(root, "requests/a/good.yaml", &simple("Good"));
    let loaded = load_requests(root).unwrap();
    let folder = &loaded.tree.collections[0];
    assert!(folder.broken);
    assert_eq!(names(&folder.items), ["good"]);
    assert_eq!(loaded.errors.len(), 1);
    assert!(loaded.errors[0].merge_conflict);
    assert_eq!(loaded.errors[0].path, Path::new("requests/a/_folder.yaml"));
}

#[test]
fn folders_without_yaml_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "requests/shipments/a.yaml", &simple("a"));
    write(root, "requests/shipments/fixtures/payload.json", "{}");
    write(root, "requests/empty/readme.txt", "x");
    let loaded = load_requests(root).unwrap();
    assert_eq!(loaded.tree.collections.len(), 1);
    assert_eq!(names(&loaded.tree.collections[0].items), ["a"]);
}

#[test]
fn missing_requests_folder_is_an_empty_tree() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = load_requests(dir.path()).unwrap();
    assert!(loaded.tree.collections.is_empty());
    assert!(loaded.errors.is_empty());
}

#[test]
fn folder_display_name_falls_back_to_the_directory() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "requests/a/_folder.yaml", "name: Shipments\n");
    write(root, "requests/a/x.yaml", &simple("x"));
    write(root, "requests/b/y.yaml", &simple("y"));
    let loaded = load_requests(root).unwrap();
    assert_eq!(loaded.tree.collections[0].display_name(), "Shipments");
    assert_eq!(loaded.tree.collections[1].display_name(), "b");
}

#[test]
fn find_lists_the_folders_from_the_collection_inwards() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "requests/a/b/c/deep.yaml", &simple("deep"));
    let loaded = load_requests(root).unwrap();
    let found = loaded
        .tree
        .find(Path::new("requests/a/b/c/deep.yaml"))
        .unwrap();
    let dirs: Vec<_> = found
        .folders
        .iter()
        .map(|f| f.dir.to_string_lossy().into_owned())
        .collect();
    assert_eq!(dirs, ["requests/a", "requests/a/b", "requests/a/b/c"]);
    assert!(
        loaded
            .tree
            .find(Path::new("requests/a/nope.yaml"))
            .is_none()
    );
}

#[test]
fn request_outside_a_collection_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "requests/ping.yaml", &simple("Ping"));
    let loaded = load_requests(root).unwrap();
    assert_eq!(loaded.tree.requests().count(), 0);
    assert_eq!(loaded.errors.len(), 1);
    assert!(
        loaded.errors[0]
            .message
            .contains("inside a collection folder"),
        "{}",
        loaded.errors[0]
    );
}

#[test]
fn root_folder_file_only_sets_order() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "requests/a/x.yaml", &simple("x"));
    write(
        root,
        "requests/_folder.yaml",
        "order:\n- a\nheaders:\n- name: X\n  value: y\n",
    );
    let loaded = load_requests(root).unwrap();
    assert_eq!(loaded.errors.len(), 1);
    assert_eq!(loaded.errors[0].path, Path::new("requests/_folder.yaml"));
}

fn folder_with_key(key: &str) -> String {
    format!("tls:\n  clientCertificate:\n    certificate: certs/client.pem\n    key: {key}\n")
}

#[test]
fn client_key_must_come_from_the_vault() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "requests/a/x.yaml", &simple("x"));
    write(
        root,
        "requests/a/_folder.yaml",
        &folder_with_key("'-----BEGIN PRIVATE KEY-----\n...'"),
    );
    let loaded = load_requests(root).unwrap();
    assert_eq!(loaded.errors.len(), 1);
    assert!(loaded.tree.collections[0].broken);
    assert!(
        loaded.errors[0].message.contains("vault"),
        "{}",
        loaded.errors[0]
    );

    write(
        root,
        "requests/a/_folder.yaml",
        &folder_with_key("'{{vault:client_key}}'"),
    );
    let loaded = load_requests(root).unwrap();
    assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
    assert!(!loaded.tree.collections[0].broken);
    assert!(loaded.tree.collections[0].settings.tls.is_some());
}

fn form_field(value: Option<&str>, file: Option<&str>) -> RequestFile {
    let mut r = request();
    r.body = Some(Body::FormData {
        fields: vec![FormField {
            name: "f".into(),
            value: value.map(Into::into),
            file: file.map(Into::into),
            content_type: None,
            description: None,
            enabled: true,
        }],
    });
    r
}

#[test]
fn form_field_has_a_value_or_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let project = project(dir.path());
    let path = Path::new("requests/a/r.yaml");
    for bad in [form_field(Some("v"), Some("f.txt")), form_field(None, None)] {
        assert!(matches!(
            save_request(&project, path, &bad),
            Err(ProjectError::InvalidRequest(_))
        ));
    }
    save_request(&project, path, &form_field(Some("v"), None)).unwrap();
    save_request(&project, path, &form_field(None, Some("f.txt"))).unwrap();

    write(
        dir.path(),
        "requests/a/loaded.yaml",
        "name: L\nmethod: POST\nurl: u\nbody:\n  mode: formData\n  fields:\n  - name: f\n",
    );
    let loaded = load_requests(dir.path()).unwrap();
    assert_eq!(loaded.errors.len(), 1);
    assert_eq!(loaded.errors[0].path, Path::new("requests/a/loaded.yaml"));
}

#[test]
fn method_must_be_an_http_token() {
    let dir = tempfile::tempdir().unwrap();
    let project = project(dir.path());
    let path = Path::new("requests/a/r.yaml");
    for ok in ["GET", "PURGE"] {
        let mut r = request();
        r.method = ok.into();
        save_request(&project, path, &r).unwrap();
    }
    let mut r = request();
    r.method = "BAD METHOD".into();
    assert!(matches!(
        save_request(&project, path, &r),
        Err(ProjectError::InvalidRequest(_))
    ));
    write(
        dir.path(),
        "requests/a/loaded.yaml",
        "name: L\nmethod: BAD METHOD\nurl: u\n",
    );
    assert_eq!(load_requests(dir.path()).unwrap().errors.len(), 1);
}

#[test]
fn save_refuses_read_only_projects_and_paths_outside_requests() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = project(dir.path());
    let r = request();
    for bad in [
        "requests/ping.yaml",
        "requests/a/ping.json",
        "requests/a/_folder.yaml",
        "environments/x.yaml",
        "dea.yaml",
        "requests/../x/ping.yaml",
        "/tmp/requests/a/ping.yaml",
    ] {
        assert!(
            matches!(
                save_request(&project, Path::new(bad), &r),
                Err(ProjectError::InvalidRequest(_))
            ),
            "{bad}"
        );
    }
    assert!(!dir.path().join("requests").exists());
    project.read_only = true;
    assert!(matches!(
        save_request(&project, Path::new("requests/a/ping.yaml"), &r),
        Err(ProjectError::ReadOnly)
    ));
}

fn load_one(root: &Path, rel: &str) -> RequestFile {
    let loaded = load_requests(root).unwrap();
    assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
    loaded
        .tree
        .find(Path::new(rel))
        .unwrap()
        .entry
        .request
        .clone()
}

fn pairs(fields: &[KeyValue]) -> Vec<(&str, &str)> {
    fields
        .iter()
        .map(|f| (f.name.as_str(), f.value.as_str()))
        .collect()
}

#[test]
fn unquoted_scalars_in_body_and_auth_are_text() {
    let dir = tempfile::tempdir().unwrap();
    let project = project(dir.path());
    let root = dir.path();
    let head = "name: R\nmethod: POST\nurl: http://h\n";
    let scalars = "    - name: 7\n      value: 20\n    - name: b\n      value: true\n    \
                   - name: e\n      value:\n    - name: z\n      value: 007\n";
    write(
        root,
        "requests/a/urlencoded.yaml",
        &format!(
            "{head}body:\n  mode: urlencoded\n  fields:\n{scalars}\
             auth:\n  type: apiKey\n  key: 12\n  value: 12345\n  in: header\n"
        ),
    );
    write(
        root,
        "requests/a/form.yaml",
        &format!(
            "{head}body:\n  mode: formData\n  fields:\n{scalars}\
             auth:\n  type: basic\n  username: 1000\n  password: false\n"
        ),
    );
    write(
        root,
        "requests/a/text.yaml",
        &format!("{head}body:\n  mode: json\n  text: 20\nauth:\n  type: bearer\n  token: 12345\n"),
    );
    write(
        root,
        "requests/a/binary.yaml",
        &format!("{head}body:\n  mode: binary\n  file: 2024\n"),
    );
    write(
        root,
        "requests/b/_folder.yaml",
        "auth:\n  type: apiKey\n  key: k\n  value: 12345\n  in: query\n",
    );
    write(root, "requests/b/r.yaml", &simple("r"));

    let expected = [("7", "20"), ("b", "true"), ("e", ""), ("z", "007")];
    let urlencoded = load_one(root, "requests/a/urlencoded.yaml");
    let Some(Body::Urlencoded { fields }) = &urlencoded.body else {
        panic!("{:?}", urlencoded.body);
    };
    assert_eq!(pairs(fields), expected);
    assert_eq!(
        urlencoded.auth,
        Auth::ApiKey {
            key: "12".into(),
            value: "12345".into(),
            location: ApiKeyLocation::Header
        }
    );

    let form = load_one(root, "requests/a/form.yaml");
    let Some(Body::FormData { fields }) = &form.body else {
        panic!("{:?}", form.body);
    };
    let values: Vec<(&str, Option<&str>)> = fields
        .iter()
        .map(|f| (f.name.as_str(), f.value.as_deref()))
        .collect();
    assert_eq!(values, expected.map(|(name, value)| (name, Some(value))));
    assert_eq!(
        form.auth,
        Auth::Basic {
            username: "1000".into(),
            password: "false".into()
        }
    );

    let text = load_one(root, "requests/a/text.yaml");
    assert_eq!(text.body, Some(Body::Json { text: "20".into() }));
    assert_eq!(
        text.auth,
        Auth::Bearer {
            token: "12345".into()
        }
    );
    let binary = load_one(root, "requests/a/binary.yaml");
    assert_eq!(
        binary.body,
        Some(Body::Binary {
            file: "2024".into()
        })
    );
    let loaded = load_requests(root).unwrap();
    let folder = loaded
        .tree
        .collections
        .iter()
        .find(|c| c.dir.ends_with("b"))
        .unwrap();
    assert!(!folder.broken);
    assert!(matches!(&folder.settings.auth, Auth::ApiKey { value, .. } if value == "12345"));

    // Saving writes strings.
    save_request(
        &project,
        Path::new("requests/a/urlencoded.yaml"),
        &urlencoded,
    )
    .unwrap();
    let saved = fs::read_to_string(root.join("requests/a/urlencoded.yaml")).unwrap();
    for quoted in [
        "name: '7'",
        "value: '20'",
        "value: 'true'",
        "value: ''",
        "value: '12345'",
    ] {
        assert!(saved.contains(quoted), "{quoted} in {saved}");
    }
    assert_eq!(load_one(root, "requests/a/urlencoded.yaml"), urlencoded);
}
