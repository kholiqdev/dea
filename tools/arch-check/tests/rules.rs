use std::path::Path;

use arch_check::{DepKind, Edge, Rules, Violation, check, explain, load_rules, workspace_graph};

fn e(from: &str, to: &str, kind: DepKind) -> Edge {
    Edge {
        from: from.into(),
        to: to.into(),
        kind,
    }
}

fn rules() -> Rules {
    load_rules(&Path::new(env!("CARGO_MANIFEST_DIR")).join("rules.toml")).unwrap()
}

/// Every crate named in the folder structure.
fn members() -> Vec<String> {
    [
        "dea-desktop",
        "dea-engine",
        "dea-vault",
        "dea-http",
        "dea-script",
        "dea-import",
        "dea-db",
        "dea-ssh",
        "dea-pty",
        "dea-mobile",
        "dea-project",
        "dea-protocol",
        "dea-testkit",
        "checklist-gate",
        "perf-probe",
        "arch-check",
    ]
    .map(String::from)
    .to_vec()
}

fn forbidden(from: &str, to: &str) -> Vec<Violation> {
    vec![Violation::ForbiddenEdge {
        from: from.into(),
        to: to.into(),
    }]
}

#[test]
fn allowed_edges_pass() {
    let edges = [
        e("dea-desktop", "dea-engine", DepKind::Normal),
        e("dea-desktop", "dea-project", DepKind::Normal),
        e("dea-engine", "dea-http", DepKind::Normal),
        e("dea-engine", "dea-project", DepKind::Normal),
        e("dea-http", "dea-project", DepKind::Normal),
        e("dea-mobile", "dea-protocol", DepKind::Normal),
        e("checklist-gate", "dea-project", DepKind::Normal),
        e("dea-desktop", "tauri", DepKind::Normal),
    ];
    assert_eq!(check(&members(), &edges, &rules()), []);
}

#[test]
fn feature_to_feature_is_forbidden() {
    let v = check(
        &members(),
        &[e("dea-db", "dea-ssh", DepKind::Normal)],
        &rules(),
    );
    assert_eq!(v, forbidden("dea-db", "dea-ssh"));
}

#[test]
fn script_cannot_depend_on_http() {
    let v = check(
        &members(),
        &[e("dea-script", "dea-http", DepKind::Normal)],
        &rules(),
    );
    assert_eq!(v, forbidden("dea-script", "dea-http"));
}

#[test]
fn core_cannot_depend_on_feature() {
    let v = check(
        &members(),
        &[e("dea-project", "dea-vault", DepKind::Normal)],
        &rules(),
    );
    assert_eq!(v, forbidden("dea-project", "dea-vault"));
}

#[test]
fn core_crates_cannot_depend_on_each_other() {
    let v = check(
        &members(),
        &[e("dea-project", "dea-protocol", DepKind::Normal)],
        &rules(),
    );
    assert_eq!(v, forbidden("dea-project", "dea-protocol"));
}

#[test]
fn app_cannot_skip_engine_to_feature() {
    let v = check(
        &members(),
        &[e("dea-desktop", "dea-http", DepKind::Normal)],
        &rules(),
    );
    assert_eq!(v, forbidden("dea-desktop", "dea-http"));
}

#[test]
fn build_dependencies_are_checked_like_normal_ones() {
    let v = check(
        &members(),
        &[e("dea-project", "dea-vault", DepKind::Build)],
        &rules(),
    );
    assert_eq!(v, forbidden("dea-project", "dea-vault"));
}

#[test]
fn other_dev_dependencies_are_not_checked() {
    let v = check(
        &members(),
        &[e("dea-http", "dea-ssh", DepKind::Dev)],
        &rules(),
    );
    assert_eq!(v, []);
}

#[test]
fn testkit_only_as_dev_dependency() {
    let v = check(
        &members(),
        &[e("dea-http", "dea-testkit", DepKind::Normal)],
        &rules(),
    );
    assert_eq!(
        v,
        [Violation::TestkitAsNormalDependency {
            from: "dea-http".into()
        }]
    );
    let v = check(
        &members(),
        &[e("dea-http", "dea-testkit", DepKind::Dev)],
        &rules(),
    );
    assert_eq!(v, []);
}

#[test]
fn unknown_workspace_crate_is_reported() {
    let mut members = members();
    members.push("dea-new".into());
    assert_eq!(
        check(&members, &[], &rules()),
        [Violation::UnknownCrate("dea-new".into())]
    );
}

#[test]
fn tauri_in_feature_crate_is_forbidden() {
    let v = check(
        &members(),
        &[e("dea-http", "tauri", DepKind::Normal)],
        &rules(),
    );
    assert_eq!(
        v,
        [Violation::ForbiddenExternal {
            from: "dea-http".into(),
            dependency: "tauri".into()
        }]
    );
}

#[test]
fn violations_explain_the_layers() {
    let v = check(
        &members(),
        &[e("dea-db", "dea-ssh", DepKind::Normal)],
        &rules(),
    );
    assert_eq!(
        explain(&v[0], &rules()),
        "dea-db -> dea-ssh: feature may not depend on feature"
    );
}

#[test]
fn workspace_graph_reads_members_and_direct_dependencies() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let (members, edges) = workspace_graph(&root).unwrap();
    for name in ["dea-desktop", "dea-project", "arch-check"] {
        assert!(members.iter().any(|m| m == name), "{name} in {members:?}");
    }
    assert!(edges.contains(&e("dea-desktop", "tauri", DepKind::Normal)));
    assert!(edges.contains(&e("dea-desktop", "tauri-build", DepKind::Build)));
    assert!(edges.contains(&e("dea-project", "tempfile", DepKind::Normal)));
}

#[test]
fn real_workspace_passes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let (members, edges) = workspace_graph(&root).unwrap();
    assert_eq!(check(&members, &edges, &rules()), []);
}
