use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use dea_project::{
    DynamicSource, Environment, FieldValue, LocalFiles, NoLocalFiles, Project, RequestContext,
    RequestError, RequestTree, ResolvedBody, ResolvedRequest, ScopeKind, SecretLookup,
    SecretLookupError, ValueSource, Variable, create_project, load_requests, resolve_request,
};

struct Vault(HashMap<String, String>);

impl Vault {
    fn new(pairs: &[(&str, &str)]) -> Self {
        Vault(
            pairs
                .iter()
                .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
                .collect(),
        )
    }
}

impl SecretLookup for Vault {
    fn lookup(&self, _env: &str, name: &str) -> Result<Option<String>, SecretLookupError> {
        Ok(self.0.get(name).cloned())
    }
}

struct NeverAsked;

impl SecretLookup for NeverAsked {
    fn lookup(&self, _env: &str, _name: &str) -> Result<Option<String>, SecretLookupError> {
        panic!("the Vault must not be touched without an environment");
    }
}

struct Fixed;

impl DynamicSource for Fixed {
    fn guid(&mut self) -> String {
        "g-1".into()
    }
    fn unix_timestamp(&mut self) -> i64 {
        1_763_371_800
    }
    fn iso_timestamp(&mut self) -> String {
        "2025-11-17T09:30:00.000Z".into()
    }
    fn random_int(&mut self) -> u32 {
        7
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    project: Project,
    tree: RequestTree,
}

fn fixture(files: &[(&str, &str)]) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut project = create_project(dir.path(), "harbor").unwrap();
    project.environments = vec![Environment {
        name: "staging".into(),
        production: false,
        color: None,
        variables: vec![Variable {
            name: "baseUrl".into(),
            value: "http://localhost:8080".into(),
            description: None,
        }],
    }];
    for (rel, text) in files {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let tree = load_requests(dir.path()).unwrap().tree;
    Fixture {
        _dir: dir,
        project,
        tree,
    }
}

fn run_with(
    f: &Fixture,
    path: &str,
    environment: Option<&str>,
    secrets: &dyn SecretLookup,
) -> Result<ResolvedRequest, RequestError> {
    run_local(f, path, environment, secrets, &NoLocalFiles)
}

fn run_local(
    f: &Fixture,
    path: &str,
    environment: Option<&str>,
    secrets: &dyn SecretLookup,
    local_files: &dyn LocalFiles,
) -> Result<ResolvedRequest, RequestError> {
    let mut dynamic = Fixed;
    let mut ctx = RequestContext {
        project: &f.project,
        tree: &f.tree,
        environment,
        secrets,
        local_files,
        dynamic: &mut dynamic,
    };
    resolve_request(Path::new(path), &mut ctx)
}

fn run(f: &Fixture, path: &str) -> ResolvedRequest {
    run_with(f, path, Some("staging"), &Vault::new(&[])).unwrap()
}

fn header<'a>(r: &'a ResolvedRequest, name: &str) -> Vec<&'a str> {
    r.headers
        .iter()
        .filter(|(n, _)| n.text.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.text.as_str())
        .collect()
}

const REQ: &str = "requests/shipping/tracking/events.yaml";

#[test]
fn path_variables_fill_colon_segments() {
    let f = fixture(&[
        (
            "requests/shipping/tracking/_folder.yaml",
            "variables:\n  - name: id\n    value: shp_1\n",
        ),
        (
            REQ,
            "name: Events\nmethod: GET\n\
             url: '{{baseUrl}}/v1/shipments/:shipmentId/events/:other'\n\
             pathVariables:\n  - name: shipmentId\n    value: '{{id}}'\n",
        ),
    ]);
    let resolved = run(&f, REQ);
    assert_eq!(
        resolved.url.text,
        "http://localhost:8080/v1/shipments/shp_1/events/:other"
    );
    assert_eq!(resolved.unresolved, [":other"]);
}

#[test]
fn enabled_params_are_appended_and_encoded() {
    let f = fixture(&[(
        REQ,
        "name: E\nmethod: GET\nurl: http://h/x?keep=1\nparams:\n  \
         - name: q\n    value: 'a b&c=d#e+f%20g'\n  \
         - name: off\n    value: '1'\n    enabled: false\n",
    )]);
    assert_eq!(
        run(&f, REQ).url.text,
        "http://h/x?keep=1&q=a%20b%26c%3Dd%23e%2Bf%20g"
    );
}

#[test]
fn secrets_are_redacted_everywhere() {
    let canary = dea_testkit::canary();
    let f = fixture(&[(
        REQ,
        "name: E\nmethod: POST\nurl: http://h/x\nparams:\n  \
         - name: k\n    value: '{{vault:api_key}}'\nheaders:\n  \
         - name: X-Key\n    value: '{{vault:api_key}}'\nbody:\n  mode: json\n  \
         text: '{\"k\": \"{{vault:api_key}}\"}'\n",
    )]);
    let resolved = run_with(
        &f,
        REQ,
        Some("staging"),
        &Vault::new(&[("api_key", &canary)]),
    )
    .unwrap();
    let debug = format!("{resolved:?}");
    assert!(!debug.contains(&canary));
    assert!(!resolved.url.redacted.contains(&canary));
    assert!(resolved.url.text.contains(&canary));
    assert!(resolved.url.redacted.contains("[secret:api_key]"));
    assert_eq!(resolved.secret_names(), ["api_key"]);
    let ResolvedBody::Text { text, .. } = &resolved.body else {
        panic!("a text body");
    };
    assert!(text.text.contains(&canary));
    assert!(!text.redacted.contains(&canary));
}

#[test]
fn broken_folder_blocks_requests_below() {
    let f = fixture(&[
        (
            "requests/shipping/tracking/_folder.yaml",
            "<<<<<<< HEAD\nname: a\n=======\nname: b\n>>>>>>> other\n",
        ),
        (REQ, "name: E\nmethod: GET\nurl: http://h\n"),
    ]);
    let result = run_with(&f, REQ, Some("staging"), &Vault::new(&[]));
    assert!(matches!(result, Err(RequestError::BrokenFolder(dir)) if dir.ends_with("tracking")));
}

#[test]
fn missing_request_is_not_found() {
    let f = fixture(&[(REQ, "name: E\nmethod: GET\nurl: http://h\n")]);
    let result = run_with(&f, "requests/shipping/nope.yaml", None, &Vault::new(&[]));
    assert!(matches!(result, Err(RequestError::NotFound(_))));
}

#[test]
fn nearest_scope_wins() {
    let f = fixture(&[
        (
            "requests/shipping/_folder.yaml",
            "variables:\n  - name: who\n    value: collection\n",
        ),
        (
            "requests/shipping/tracking/_folder.yaml",
            "name: Tracking\nvariables:\n  - name: who\n    value: folder\n",
        ),
        (REQ, "name: E\nmethod: GET\nurl: 'http://h/{{who}}'\n"),
    ]);
    let resolved = run(&f, REQ);
    assert_eq!(resolved.url.text, "http://h/folder");
    assert!(matches!(
        &resolved.used[0].source,
        ValueSource::Scope { kind: ScopeKind::Folder, label } if label == "Tracking"
    ));
}

#[test]
fn folder_auth_is_inherited_and_none_turns_it_off() {
    let f = fixture(&[
        (
            "requests/shipping/_folder.yaml",
            "auth:\n  type: bearer\n  token: tok\n",
        ),
        ("requests/shipping/tracking/_folder.yaml", "variables: []\n"),
        (REQ, "name: E\nmethod: GET\nurl: http://h\n"),
        (
            "requests/shipping/off.yaml",
            "name: O\nmethod: GET\nurl: http://h\nauth:\n  type: none\n",
        ),
    ]);
    assert_eq!(header(&run(&f, REQ), "authorization"), ["Bearer tok"]);
    assert!(header(&run(&f, "requests/shipping/off.yaml"), "authorization").is_empty());
}

#[test]
fn explicit_header_beats_auth_and_inherited_headers() {
    let f = fixture(&[
        (
            "requests/shipping/_folder.yaml",
            "headers:\n  - name: X-Team\n    value: ops\n  - name: Accept\n    value: '*/*'\n  \
             - name: ''\n    value: nameless\n  - name: X-Off\n    value: 'no'\n    enabled: false\n",
        ),
        (
            "requests/shipping/r.yaml",
            "name: R\nmethod: GET\nurl: http://h\nauth:\n  type: bearer\n  token: tok\n\
             headers:\n  - name: x-team\n    value: web\n  - name: Authorization\n    value: Custom x\n",
        ),
    ]);
    let resolved = run(&f, "requests/shipping/r.yaml");
    assert_eq!(header(&resolved, "x-team"), ["web"]);
    assert_eq!(header(&resolved, "authorization"), ["Custom x"]);
    assert_eq!(header(&resolved, "accept"), ["*/*"]);
    assert!(header(&resolved, "x-off").is_empty());
    assert_eq!(resolved.headers.len(), 3);
    // The person's own Authorization is not the one auth would add.
    assert_eq!(resolved.auth_header, None);
}

#[test]
fn same_level_headers_with_one_name_are_all_sent() {
    let f = fixture(&[
        (
            "requests/shipments/_folder.yaml",
            "headers:\n  - name: Accept\n    value: '*/*'\n",
        ),
        (
            "requests/shipments/list.yaml",
            "name: List\nmethod: GET\nurl: http://h\nheaders:\n  \
             - name: Accept\n    value: text/plain\n  - name: Accept\n    value: application/json\n",
        ),
        (
            "requests/shipments/wide.yaml",
            "name: Wide\nmethod: GET\nurl: http://h\n",
        ),
    ]);
    assert_eq!(
        header(&run(&f, "requests/shipments/list.yaml"), "accept"),
        ["text/plain", "application/json"]
    );
    // A request without its own Accept still inherits the folder's.
    assert_eq!(
        header(&run(&f, "requests/shipments/wide.yaml"), "accept"),
        ["*/*"]
    );
}

#[test]
fn nearer_level_replaces_every_same_named_header_from_farther_levels() {
    let f = fixture(&[
        (
            "requests/shipments/_folder.yaml",
            "headers:\n  - name: Accept\n    value: '*/*'\n  - name: Accept\n    value: text/html\n",
        ),
        (
            "requests/shipments/one.yaml",
            "name: One\nmethod: GET\nurl: http://h\nheaders:\n  - name: Accept\n    value: application/json\n",
        ),
    ]);
    assert_eq!(
        header(&run(&f, "requests/shipments/one.yaml"), "accept"),
        ["application/json"]
    );
}

#[test]
fn auth_header_is_named() {
    let f = fixture(&[
        (
            "requests/shipping/_folder.yaml",
            "auth:\n  type: bearer\n  token: tok\n",
        ),
        (
            "requests/shipping/key.yaml",
            "name: K\nmethod: GET\nurl: http://h\n\
             auth:\n  type: apiKey\n  key: X-Api-Key\n  value: k\n  in: header\n",
        ),
        (
            "requests/shipping/inherit.yaml",
            "name: I\nmethod: GET\nurl: http://h\n",
        ),
    ]);
    let key = run(&f, "requests/shipping/key.yaml");
    assert_eq!(key.auth_header.as_deref(), Some("X-Api-Key"));
    assert_eq!(header(&key, "x-api-key"), ["k"]);
    let inherited = run(&f, "requests/shipping/inherit.yaml");
    assert_eq!(inherited.auth_header.as_deref(), Some("Authorization"));
}

#[test]
fn api_key_in_query_is_appended() {
    let f = fixture(&[(
        REQ,
        "name: E\nmethod: GET\nurl: http://h/x\nparams:\n  - name: a\n    value: '1'\n\
         auth:\n  type: apiKey\n  key: key\n  value: 'v 1'\n  in: query\n",
    )]);
    let resolved = run(&f, REQ);
    assert_eq!(resolved.url.text, "http://h/x?a=1&key=v%201");
    assert_eq!(resolved.auth_header, None);
}

#[test]
fn basic_auth_is_encoded_and_redacted() {
    let canary = dea_testkit::canary();
    let f = fixture(&[(
        REQ,
        "name: E\nmethod: GET\nurl: http://h\nauth:\n  type: basic\n  username: ops\n  \
         password: '{{vault:ops_password}}'\n",
    )]);
    let resolved = run_with(
        &f,
        REQ,
        Some("staging"),
        &Vault::new(&[("ops_password", &canary)]),
    )
    .unwrap();
    let (name, value) = &resolved.headers[0];
    assert_eq!(name.text, "Authorization");
    assert_eq!(
        value.text,
        format!("Basic {}", STANDARD.encode(format!("ops:{canary}")))
    );
    assert_eq!(
        value.redacted,
        format!("Basic {}", STANDARD.encode("ops:[secret:ops_password]"))
    );
}

#[test]
fn unresolved_names_are_listed_once_in_order() {
    let f = fixture(&[(
        REQ,
        "name: E\nmethod: GET\nurl: 'http://h/{{a}}/{{b}}/{{a}}'\nheaders:\n  \
         - name: X\n    value: '{{b}}'\n",
    )]);
    let resolved = run(&f, REQ);
    assert_eq!(resolved.unresolved, ["a", "b"]);
    assert!(resolved.url.text.contains("{{a}}"));
}

#[test]
fn no_environment_leaves_vault_references_unresolved() {
    let f = fixture(&[(
        REQ,
        "name: E\nmethod: GET\nurl: 'http://h/{{vault:token}}'\n",
    )]);
    let resolved = run_with(&f, REQ, None, &NeverAsked).unwrap();
    assert_eq!(resolved.url.text, "http://h/{{vault:token}}");
    assert_eq!(resolved.unresolved, ["vault:token"]);
}

#[test]
fn body_files_must_stay_in_the_project() {
    let f = fixture(&[
        ("data/payload.bin", "x"),
        (
            "requests/shipping/ok.yaml",
            "name: O\nmethod: POST\nurl: http://h\nbody:\n  mode: binary\n  file: data/payload.bin\n",
        ),
        (
            "requests/shipping/up.yaml",
            "name: U\nmethod: POST\nurl: http://h\nbody:\n  mode: binary\n  file: ../outside.bin\n",
        ),
        (
            "requests/shipping/abs.yaml",
            "name: A\nmethod: POST\nurl: http://h\nbody:\n  mode: formData\n  fields:\n    \
             - name: f\n      file: /etc/passwd\n",
        ),
    ]);
    let ok = run(&f, "requests/shipping/ok.yaml");
    let ResolvedBody::File { path, content_type } = &ok.body else {
        panic!("a file body");
    };
    assert_eq!(*path, f.project.root.join("data/payload.bin"));
    assert!(path.is_absolute());
    assert_eq!(*content_type, "application/octet-stream");
    for bad in ["requests/shipping/up.yaml", "requests/shipping/abs.yaml"] {
        let result = run_with(&f, bad, Some("staging"), &Vault::new(&[]));
        assert!(matches!(result, Err(RequestError::FileOutsideProject(_))));
    }
}

/// Requests numbered `i` that use `path` as a binary body, a form file and,
/// through their folder, a CA certificate.
#[cfg(unix)]
fn uses_of(i: usize, path: &str) -> Vec<(String, String)> {
    let request = |body: &str| format!("name: R\nmethod: POST\nurl: http://h\n{body}");
    vec![
        (
            format!("requests/binary/r{i}.yaml"),
            request(&format!("body:\n  mode: binary\n  file: {path}\n")),
        ),
        (
            format!("requests/form/r{i}.yaml"),
            request(&format!(
                "body:\n  mode: formData\n  fields:\n    - name: f\n      file: {path}\n"
            )),
        ),
        (
            format!("requests/tls{i}/_folder.yaml"),
            format!("tls:\n  caCertificates:\n    - {path}\n"),
        ),
        (format!("requests/tls{i}/r.yaml"), request("")),
    ]
}

#[cfg(unix)]
#[test]
fn symlinks_must_stay_in_the_project() {
    use std::os::unix::fs::symlink;

    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("id_rsa"), "secret").unwrap();
    let paths = [
        "data/escape.bin",
        "data/out/id_rsa",
        "data/link.bin",
        "data/missing.bin",
    ];
    let mut files = vec![("data/real.bin".to_string(), "x".to_string())];
    for (i, path) in paths.iter().enumerate() {
        files.extend(uses_of(i, path));
    }
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(f, t)| (f.as_str(), t.as_str()))
        .collect();
    let f = fixture(&files);
    let root = &f.project.root;
    // A file and a folder that lead outside, and a link to a file inside.
    symlink(outside.path().join("id_rsa"), root.join(paths[0])).unwrap();
    symlink(outside.path(), root.join("data/out")).unwrap();
    symlink("real.bin", root.join(paths[2])).unwrap();

    let resolve = |i: usize| {
        [
            format!("requests/binary/r{i}.yaml"),
            format!("requests/form/r{i}.yaml"),
            format!("requests/tls{i}/r.yaml"),
        ]
        .map(|path| run_with(&f, &path, Some("staging"), &Vault::new(&[])))
    };
    for i in [0, 1] {
        for result in resolve(i) {
            assert!(
                matches!(result, Err(RequestError::FileOutsideProject(_))),
                "{}: {result:?}",
                paths[i]
            );
        }
    }
    // A path that does not exist yet is checked as written; the send
    // reports it missing.
    for i in [2, 3] {
        let expected = root.join(paths[i]);
        let [binary, form, tls] = resolve(i).map(Result::unwrap);
        assert!(matches!(binary.body, ResolvedBody::File { path, .. } if path == expected));
        let ResolvedBody::FormData(fields) = form.body else {
            panic!("a form body");
        };
        assert!(matches!(&fields[0].value, FieldValue::File { path, .. } if *path == expected));
        assert_eq!(tls.tls.ca_certificates, [expected]);
    }
}

#[test]
fn form_data_and_content_types() {
    let f = fixture(&[
        (
            "requests/shipping/form.yaml",
            "name: F\nmethod: POST\nurl: http://h\nbody:\n  mode: formData\n  fields:\n    \
             - name: a\n      value: '1'\n    - name: f\n      file: up/x.png\n      \
             contentType: image/png\n    - name: off\n      value: x\n      enabled: false\n",
        ),
        (
            "requests/shipping/xml.yaml",
            "name: X\nmethod: POST\nurl: http://h\nbody:\n  mode: raw\n  language: xml\n  text: '<a/>'\n",
        ),
    ]);
    let ResolvedBody::FormData(fields) = run(&f, "requests/shipping/form.yaml").body else {
        panic!("form data");
    };
    assert_eq!(fields.len(), 2);
    assert!(matches!(&fields[0].value, FieldValue::Text(v) if v.text == "1"));
    assert!(matches!(
        &fields[1].value,
        FieldValue::File { path, content_type }
            if path.is_absolute() && content_type.as_deref() == Some("image/png")
    ));
    let ResolvedBody::Text { content_type, .. } = run(&f, "requests/shipping/xml.yaml").body else {
        panic!("text body");
    };
    assert_eq!(content_type, "application/xml");
}

#[test]
fn options_take_the_nearest_value_then_defaults() {
    let f = fixture(&[
        (
            "requests/shipping/_folder.yaml",
            "options:\n  timeoutMs: 1000\n  followRedirects: false\n  maxRedirects: 3\n",
        ),
        (
            "requests/shipping/tracking/_folder.yaml",
            "options:\n  timeoutMs: 5000\n",
        ),
        (
            REQ,
            "name: E\nmethod: GET\nurl: http://h\noptions:\n  maxRedirects: 10\n",
        ),
    ]);
    let o = run(&f, REQ).options;
    assert_eq!(
        (
            o.timeout_ms,
            o.follow_redirects,
            o.max_redirects,
            o.verify_tls,
            o.use_system_proxy,
            o.cookies
        ),
        (5000, false, 10, true, true, true)
    );
}

#[test]
fn tls_paths_are_absolute_and_key_is_resolved() {
    let f = fixture(&[
        (
            "requests/shipping/_folder.yaml",
            "tls:\n  caCertificates:\n    - certs/ca.pem\n  clientCertificate:\n    \
             certificate: certs/client.pem\n    key: '{{vault:client_key}}'\n",
        ),
        (REQ, "name: E\nmethod: GET\nurl: http://h\n"),
    ]);
    let resolved = run_with(
        &f,
        REQ,
        Some("staging"),
        &Vault::new(&[("client_key", "KEYDATA")]),
    )
    .unwrap();
    let root: PathBuf = f.project.root.clone();
    assert_eq!(resolved.tls.ca_certificates, [root.join("certs/ca.pem")]);
    let cert = resolved.tls.client_certificate.unwrap();
    assert_eq!(cert.certificate, root.join("certs/client.pem"));
    assert_eq!(cert.key.text, "KEYDATA");
    assert_eq!(cert.key.redacted, "[secret:client_key]");
}

#[test]
fn dynamic_variables_in_a_body() {
    let f = fixture(&[(
        REQ,
        "name: E\nmethod: POST\nurl: http://h\nbody:\n  mode: json\n  \
         text: '{\"id\": \"{{$guid}}\", \"n\": {{$randomInt}}}'\n",
    )]);
    let ResolvedBody::Text { text, content_type } = run(&f, REQ).body else {
        panic!("json body");
    };
    assert_eq!(text.text, "{\"id\": \"g-1\", \"n\": 7}");
    assert_eq!(content_type, "application/json");
}

const LOCAL_ID: &str = "local:0123456789ab";

fn local_fixture() -> Fixture {
    fixture(&[
        (
            "requests/shipments/upload.yaml",
            &format!(
                "name: U\nmethod: POST\nurl: http://h\nbody:\n  mode: binary\n  file: {LOCAL_ID}\n"
            ),
        ),
        (
            "requests/shipments/_folder.yaml",
            &format!(
                "tls:\n  caCertificates:\n    - {LOCAL_ID}\n  clientCertificate:\n    \
                 certificate: {LOCAL_ID}\n    key: '{{{{vault:client_key}}}}'\n"
            ),
        ),
    ])
}

fn local_map(path: &str) -> BTreeMap<String, PathBuf> {
    [(LOCAL_ID.to_string(), PathBuf::from(path))].into()
}

#[test]
fn local_body_file_resolves_through_the_map() {
    let f = local_fixture();
    let resolved = run_local(
        &f,
        "requests/shipments/upload.yaml",
        Some("staging"),
        &Vault::new(&[("client_key", "k")]),
        &local_map("/tmp/x/upload.bin"),
    )
    .unwrap();
    assert_eq!(
        resolved.body,
        ResolvedBody::File {
            path: PathBuf::from("/tmp/x/upload.bin"),
            content_type: "application/octet-stream"
        }
    );
    assert_eq!(
        resolved.tls.ca_certificates,
        [PathBuf::from("/tmp/x/upload.bin")]
    );
    assert_eq!(
        resolved.tls.client_certificate.unwrap().certificate,
        PathBuf::from("/tmp/x/upload.bin")
    );
}

#[test]
fn unknown_local_file_names_it() {
    let f = local_fixture();
    let err = run_local(
        &f,
        "requests/shipments/upload.yaml",
        Some("staging"),
        &Vault::new(&[]),
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(matches!(&err, RequestError::LocalFileMissing(id) if id == LOCAL_ID));
    assert_eq!(
        err.to_string(),
        "the file \"local:0123456789ab\" was picked on another device; choose it again"
    );
}

#[test]
fn local_form_file_resolves_through_the_map() {
    let f = fixture(&[(
        "requests/a/form.yaml",
        &format!(
            "name: F\nmethod: POST\nurl: http://h\nbody:\n  mode: formData\n  fields:\n    \
             - name: f\n      file: {LOCAL_ID}\n"
        ),
    )]);
    let resolved = run_local(
        &f,
        "requests/a/form.yaml",
        None,
        &Vault::new(&[]),
        &local_map("/tmp/x/a.txt"),
    )
    .unwrap();
    let ResolvedBody::FormData(fields) = &resolved.body else {
        panic!("form data");
    };
    assert!(matches!(
        &fields[0].value,
        FieldValue::File { path, .. } if path == Path::new("/tmp/x/a.txt")
    ));
}
