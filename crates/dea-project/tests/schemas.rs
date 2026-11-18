use std::fs;
use std::path::Path;

use dea_project::{create_project, json_schemas};
use serde_json::Value;

fn schema(file: &str) -> Value {
    json_schemas()
        .into_iter()
        .find(|(name, _)| *name == file)
        .unwrap_or_else(|| panic!("no schema named {file}"))
        .1
}

fn yaml(text: &str) -> Value {
    serde_norway::from_str(text).unwrap()
}

fn is_valid(file: &str, text: &str) -> bool {
    jsonschema::validator_for(&schema(file))
        .unwrap()
        .is_valid(&yaml(text))
}

#[test]
fn published_schemas_are_up_to_date() {
    let names: Vec<_> = json_schemas().into_iter().map(|(name, _)| name).collect();
    assert_eq!(
        names,
        [
            "manifest.schema.json",
            "environment.schema.json",
            "vault.schema.json"
        ]
    );
    for (file, schema) in json_schemas() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("schemas")
            .join(file);
        let expected = serde_json::to_string_pretty(&schema).unwrap() + "\n";
        if std::env::var_os("UPDATE_SCHEMAS").is_some() {
            fs::write(&path, &expected).unwrap();
            continue;
        }
        assert_eq!(
            fs::read_to_string(&path).unwrap_or_default(),
            expected,
            "{file} is out of date: run UPDATE_SCHEMAS=1 cargo test -p dea-project --test schemas"
        );
    }
}

#[test]
fn schema_rejects_unknown_fields() {
    let env = schema("environment.schema.json");
    assert_eq!(env["additionalProperties"], Value::Bool(false));
    assert!(is_valid(
        "environment.schema.json",
        "name: local\nproduction: false\n"
    ));
    assert!(!is_valid(
        "environment.schema.json",
        "name: local\nproduction: false\ncolour: red\n"
    ));
}

#[test]
fn files_written_by_dea_match_the_schemas() {
    let dir = tempfile::tempdir().unwrap();
    create_project(dir.path(), "harbor-api").unwrap();
    let read = |rel: &str| fs::read_to_string(dir.path().join(rel)).unwrap();
    assert!(is_valid("manifest.schema.json", &read("dea.yaml")));
    assert!(is_valid("vault.schema.json", &read("vault.yaml")));
    for env in ["local", "staging", "production"] {
        assert!(is_valid(
            "environment.schema.json",
            &read(&format!("environments/{env}.yaml"))
        ));
    }
}

#[test]
fn schemas_follow_the_parser_rules() {
    // Required fields.
    assert!(!is_valid("manifest.schema.json", "name: x\n"));
    assert!(!is_valid("environment.schema.json", "production: true\n"));
    // The production flag is always explicit.
    assert!(!is_valid("environment.schema.json", "name: s\n"));
    // Optional fields may be left out.
    assert!(is_valid(
        "manifest.schema.json",
        "name: x\nschemaVersion: 1\n"
    ));
    // Secret types use the names written to vault.yaml.
    let vault = "secrets:\n- name: access_token\n  type: sshKey\n  description: Deploy key\n";
    assert!(is_valid("vault.schema.json", vault));
    assert!(!is_valid(
        "vault.schema.json",
        "secrets:\n- name: access_token\n  type: certificate\n"
    ));
    // Variables need a name and a value.
    assert!(is_valid(
        "environment.schema.json",
        "name: s\nproduction: false\nvariables:\n- name: baseUrl\n  value: https://x.test\n"
    ));
    assert!(!is_valid(
        "environment.schema.json",
        "name: s\nproduction: false\nvariables:\n- name: baseUrl\n"
    ));
}
