use std::fs;
use std::time::Duration;

use dea_project::{
    Environment, Manifest, SecretKind, SecretSpec, Variable, VaultManifest, to_yaml,
    write_if_changed,
};

fn sample_environment() -> Environment {
    Environment {
        name: "staging".into(),
        production: false,
        color: Some("blue".into()),
        variables: vec![
            Variable {
                name: "baseUrl".into(),
                value: "https://staging.harbor.test".into(),
                description: Some("API root".into()),
            },
            Variable {
                name: "token".into(),
                value: "{{vault:access_token}}".into(),
                description: None,
            },
        ],
    }
}

fn sample_vault() -> VaultManifest {
    VaultManifest {
        secrets: vec![
            SecretSpec {
                name: "access_token".into(),
                kind: SecretKind::Token,
                description: Some("Staging API token".into()),
            },
            SecretSpec {
                name: "deploy_key".into(),
                kind: SecretKind::SshKey,
                description: None,
            },
        ],
    }
}

#[test]
fn yaml_output_is_lf_with_single_trailing_newline() {
    let s = to_yaml(&sample_environment()).unwrap();
    assert!(!s.contains('\r'));
    assert!(s.ends_with('\n') && !s.ends_with("\n\n"));
}

#[test]
fn yaml_field_names_and_order_are_stable() {
    let manifest = Manifest {
        name: "harbor-api".into(),
        id: Some("0b6f3c1e-2f43-4c43-9a49-5d0e2d2c8a11".into()),
        schema_version: 1,
        default_environment: Some("local".into()),
    };
    assert_eq!(
        to_yaml(&manifest).unwrap(),
        "name: harbor-api\nid: 0b6f3c1e-2f43-4c43-9a49-5d0e2d2c8a11\nschemaVersion: 1\ndefaultEnvironment: local\n"
    );
    let without_id = Manifest {
        id: None,
        ..manifest
    };
    assert_eq!(
        to_yaml(&without_id).unwrap(),
        "name: harbor-api\nschemaVersion: 1\ndefaultEnvironment: local\n"
    );
    let vault = to_yaml(&sample_vault()).unwrap();
    assert!(vault.contains("type: token"), "{vault}");
    assert!(vault.contains("type: sshKey"), "{vault}");
    assert_eq!(to_yaml(&VaultManifest::default()).unwrap(), "{}\n");
}

#[test]
fn production_is_always_written_and_empty_fields_are_not() {
    let env = Environment {
        name: "local".into(),
        production: false,
        color: None,
        variables: vec![],
    };
    assert_eq!(to_yaml(&env).unwrap(), "name: local\nproduction: false\n");
}

#[test]
fn round_trip_is_byte_identical() {
    fn check<T: serde::Serialize + serde::de::DeserializeOwned>(value: &T) {
        let first = to_yaml(value).unwrap();
        let parsed: T = serde_norway::from_str(&first).unwrap();
        assert_eq!(to_yaml(&parsed).unwrap(), first);
    }
    check(&Manifest {
        name: "harbor-api".into(),
        id: None,
        schema_version: 1,
        default_environment: Some("local".into()),
    });
    check(&sample_environment());
    check(&sample_vault());
}

#[test]
fn unknown_fields_are_rejected() {
    let err = serde_norway::from_str::<Manifest>("name: x\nschemaVersion: 1\nextra: 1\n");
    assert!(err.is_err());
}

#[test]
fn write_if_changed_skips_identical() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.yaml");
    assert!(write_if_changed(&path, "a: 1\n").unwrap());
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    std::thread::sleep(Duration::from_millis(20));
    assert!(!write_if_changed(&path, "a: 1\n").unwrap());
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
}

#[test]
fn write_if_changed_treats_crlf_as_identical() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.yaml");
    fs::write(&path, "a: 1\r\n").unwrap();
    assert!(!write_if_changed(&path, "a: 1\n").unwrap());
    assert_eq!(fs::read(&path).unwrap(), b"a: 1\r\n");
}

#[test]
fn write_if_changed_treats_bom_as_identical() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.yaml");
    fs::write(&path, "\u{feff}a: 1\n").unwrap();
    assert!(!write_if_changed(&path, "a: 1\n").unwrap());
}

#[test]
fn write_if_changed_replaces_different_content_and_creates_folders() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/a.yaml");
    assert!(write_if_changed(&path, "a: 1\n").unwrap());
    assert!(write_if_changed(&path, "a: 2\n").unwrap());
    assert_eq!(fs::read_to_string(&path).unwrap(), "a: 2\n");
    // No temporary files are left next to the target.
    assert_eq!(fs::read_dir(dir.path().join("nested")).unwrap().count(), 1);
}

#[test]
fn awkward_values_survive_a_round_trip() {
    for value in [
        "line1\nline2\n\n",
        "trailing blank lines\n\n\n",
        "multi\nline\n",
        "tab\there",
        "yes",
        "no",
        "null",
        "~",
        "123",
        " leading space",
        "trailing space ",
        "#not a comment",
        "key: value",
        "{{vault:token}}",
    ] {
        let env = Environment {
            name: "x".into(),
            production: false,
            color: None,
            variables: vec![Variable {
                name: "v".into(),
                value: value.into(),
                description: None,
            }],
        };
        let text = to_yaml(&env).unwrap();
        assert!(text.ends_with('\n'), "{text:?}");
        let back: Environment = serde_norway::from_str(&text).unwrap();
        assert_eq!(back.variables[0].value, value, "written as {text:?}");
    }
}

#[test]
fn conflicted_file_is_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.yaml");
    let conflicted = "<<<<<<< HEAD\na: 1\n=======\na: 2\n>>>>>>> main\n";
    fs::write(&path, conflicted).unwrap();
    assert!(matches!(
        write_if_changed(&path, "a: 3\n"),
        Err(dea_project::ProjectError::MergeConflict(_))
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), conflicted);
}

#[cfg(unix)]
#[test]
fn written_files_keep_normal_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |p: &std::path::Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    let dir = tempfile::tempdir().unwrap();
    // What the user's umask gives an ordinary new file.
    let probe = dir.path().join("probe");
    fs::File::create(&probe).unwrap();
    let path = dir.path().join("a.yaml");
    write_if_changed(&path, "a: 1\n").unwrap();
    assert_eq!(mode(&path), mode(&probe), "new file");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    write_if_changed(&path, "a: 2\n").unwrap();
    assert_eq!(mode(&path), 0o640, "existing file");
}
