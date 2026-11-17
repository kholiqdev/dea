use std::fs;
use std::path::Path;
use std::process::Command;

use checklist_gate::{Finding, FindingKind, evaluate, format_findings, parse_checklist};
use tempfile::TempDir;

fn item(id: &str, title: &str, built: &str, tested: &str) -> String {
    format!(
        "### {id} · {title}\n\n- [{built}] Dibangun\n- [{tested}] Diuji\n\n**Fitur.** Something.\n\n"
    )
}

/// Builds `docs/fase-1-x/CHECKLIST.md` and `docs/fase-2-y/CHECKLIST.md`.
fn docs(phase1: &str, phase2: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (name, body) in [("fase-1-x", phase1), ("fase-2-y", phase2)] {
        let phase_dir = dir.path().join(name);
        fs::create_dir(&phase_dir).unwrap();
        fs::write(
            phase_dir.join("CHECKLIST.md"),
            format!("# Checklist\n\n## Module\n\n{body}"),
        )
        .unwrap();
    }
    dir
}

fn complete_phase(prefix: &str) -> String {
    item(&format!("{prefix}-AAA-01"), "First", "x", "x")
        + &item(&format!("{prefix}-AAA-02"), "Second", "X", "X")
}

#[test]
fn all_complete_passes() {
    let d = docs(&complete_phase("F1"), &complete_phase("F2"));
    assert!(evaluate(d.path(), 1).unwrap().is_empty());
    assert!(evaluate(d.path(), 2).unwrap().is_empty());
}

#[test]
fn unchecked_item_is_reported() {
    let d = docs(&item("F1-AAA-01", "First", "x", " "), "");
    let f = evaluate(d.path(), 1).unwrap();
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].id.as_deref(), Some("F1-AAA-01"));
    assert_eq!(f[0].title, "First");
    assert!(matches!(f[0].kind, FindingKind::NotTested));
}

#[test]
fn tested_without_built_is_reported() {
    let d = docs(&item("F1-AAA-01", "First", " ", "x"), "");
    let f = evaluate(d.path(), 1).unwrap();
    assert_eq!(f.len(), 1);
    assert!(matches!(f[0].kind, FindingKind::TestedWithoutBuilt));
}

#[test]
fn earlier_phase_incomplete_fails_later_phase() {
    let d = docs(&item("F1-AAA-01", "First", " ", " "), &complete_phase("F2"));
    let f = evaluate(d.path(), 2).unwrap();
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].phase, 1);
    assert!(matches!(f[0].kind, FindingKind::NotBuilt));
}

#[test]
fn stray_unchecked_box_in_gate_section_fails() {
    let phase1 = complete_phase("F1") + "## Release gate\n\n- [x] Docs published\n- [ ] CI green\n";
    let d = docs(&phase1, "");
    let f = evaluate(d.path(), 1).unwrap();
    assert_eq!(f.len(), 1);
    assert!(matches!(f[0].kind, FindingKind::UncheckedBox { .. }));
    assert_eq!(f[0].title, "CI green");
}

#[test]
fn indented_unchecked_box_is_reported() {
    let phase1 = complete_phase("F1")
        + "## Release gate\n\n- [x] Security review\n  - [ ] External auditor signed off\n";
    let d = docs(&phase1, "");
    let f = evaluate(d.path(), 1).unwrap();
    assert_eq!(f.len(), 1);
    assert!(matches!(f[0].kind, FindingKind::UncheckedBox { .. }));
}

#[test]
fn missing_phase_dir_is_an_error() {
    let d = docs(&complete_phase("F1"), &complete_phase("F2"));
    assert!(evaluate(d.path(), 3).is_err());
}

fn sample_findings() -> Vec<Finding> {
    let finding = |phase: u8, id: &str, kind| Finding {
        phase,
        id: Some(id.to_string()),
        title: "Title".to_string(),
        kind,
    };
    vec![
        finding(1, "F1-AAA-01", FindingKind::NotBuilt),
        finding(1, "F1-AAA-02", FindingKind::NotTested),
        Finding {
            phase: 1,
            id: None,
            title: "CI green".to_string(),
            kind: FindingKind::UncheckedBox { line: 40 },
        },
        finding(2, "F2-BBB-01", FindingKind::TestedWithoutBuilt),
    ]
}

#[test]
fn summary_output_has_counts_only() {
    let findings = sample_findings();
    let out = format_findings(&findings, true);
    assert_eq!(out, "phase 1: 3 incomplete\nphase 2: 1 incomplete\n");
    assert!(!out.contains("F1-"));
    let full = format_findings(&findings, false);
    assert!(full.contains("F1-AAA-01 · Title: not built"));
    assert!(full.contains("F2-BBB-01 · Title: tested but not built"));
    assert!(full.contains("phase 1, line 40: unchecked box (CI green)"));
}

#[test]
fn real_phase_1_checklist_parses() {
    // docs/ is a private repository and is absent from public checkouts.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/fase-1-mvp/CHECKLIST.md"
    );
    let Ok(md) = fs::read_to_string(path) else {
        eprintln!("skip: {path} not found");
        return;
    };
    let parsed = parse_checklist(&md);
    assert!(parsed.items.len() >= 87);
    let ids: std::collections::HashSet<_> = parsed.items.iter().map(|i| &i.id).collect();
    assert_eq!(ids.len(), parsed.items.len());
    assert!(ids.iter().any(|id| id.as_str() == "F1-REL-14"));
}

fn run_cli(docs_root: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_checklist-gate"))
        .arg("--docs")
        .arg(docs_root)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
    )
}

#[test]
fn cli_fails_on_findings_and_passes_when_complete() {
    let incomplete = docs(&item("F1-AAA-01", "First", " ", " "), "");
    let (code, out) = run_cli(incomplete.path(), &["--phase", "1"]);
    assert_eq!(code, 1);
    assert!(out.contains("F1-AAA-01 · First: not built"));

    let complete = docs(&complete_phase("F1"), "");
    let (code, out) = run_cli(complete.path(), &["--phase", "1"]);
    assert_eq!(code, 0);
    assert!(out.contains("Gate passed"));
}

#[test]
fn cli_report_only_never_fails() {
    let d = docs(&item("F1-AAA-01", "First", " ", " "), "");
    let (code, _) = run_cli(d.path(), &["--phase", "1", "--report-only"]);
    assert_eq!(code, 0);
}

#[test]
fn cli_summary_hides_ids_and_titles() {
    let d = docs(&item("F1-AAA-01", "Secret feature", " ", " "), "");
    let (code, out) = run_cli(d.path(), &["--phase", "1", "--report-only", "--summary"]);
    assert_eq!(code, 0);
    assert_eq!(out, "phase 1: 1 incomplete\n");
}
