//! Release gate for the phase checklists in the planning repository.
//!
//! A phase may only be released when every item of its checklist, and of
//! every earlier phase, has both its "Dibangun" (built) and "Diuji" (tested)
//! boxes checked, and no other unchecked box is left anywhere in the file.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

static ITEM_HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^### (F\d+-[A-Z]+-\d{2}) · (.+)$").unwrap());
static ANY_HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#{1,3} ").unwrap());
static BUILT_BOX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^- \[( |x|X)\] Dibangun").unwrap());
static TESTED_BOX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^- \[( |x|X)\] Diuji").unwrap());
// Any list marker at any indentation, so nested boxes cannot slip through.
static UNCHECKED_BOX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*[-*+] \[ \]\s*(.*)$").unwrap());

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub built: bool,
    pub tested: bool,
    /// 1-based line of the item heading.
    pub line: usize,
}

#[derive(Debug, Default, PartialEq)]
pub struct ChecklistFile {
    pub items: Vec<Item>,
    /// 1-based lines of unchecked boxes that are not an item's own two boxes.
    pub stray_unchecked_lines: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FindingKind {
    NotBuilt,
    NotTested,
    TestedWithoutBuilt,
    UncheckedBox { line: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub phase: u8,
    pub id: Option<String>,
    pub title: String,
    pub kind: FindingKind,
}

#[derive(Debug, thiserror::Error)]
pub enum GateError {
    #[error("phase must be 1 or higher")]
    InvalidPhase,
    #[error("no phase {phase} folder (fase-{phase}-*) in {root}")]
    PhaseDirNotFound { phase: u8, root: PathBuf },
    #[error("more than one phase {phase} folder (fase-{phase}-*) in {root}")]
    PhaseDirAmbiguous { phase: u8, root: PathBuf },
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Which of an item's boxes a line sets, if any.
enum ItemBox {
    Built(bool),
    Tested(bool),
}

fn item_box(line: &str) -> Option<ItemBox> {
    let checked = |c: &regex::Captures| c[1].eq_ignore_ascii_case("x");
    if let Some(c) = BUILT_BOX.captures(line) {
        return Some(ItemBox::Built(checked(&c)));
    }
    TESTED_BOX
        .captures(line)
        .map(|c| ItemBox::Tested(checked(&c)))
}

pub fn parse_checklist(markdown: &str) -> ChecklistFile {
    let mut file = ChecklistFile::default();
    // The item currently being read, with whether each box was seen yet.
    let mut current: Option<(Item, bool, bool)> = None;

    for (index, raw) in markdown.lines().enumerate() {
        let line_no = index + 1;
        let line = raw.trim_end_matches('\r');

        if ANY_HEADING.is_match(line) {
            if let Some((item, _, _)) = current.take() {
                file.items.push(item);
            }
            if let Some(c) = ITEM_HEADING.captures(line) {
                let item = Item {
                    id: c[1].to_string(),
                    title: c[2].trim().to_string(),
                    built: false,
                    tested: false,
                    line: line_no,
                };
                current = Some((item, false, false));
            }
            continue;
        }

        // The first "Dibangun" and "Diuji" boxes of an item are its own.
        if let Some((item, seen_built, seen_tested)) = current.as_mut() {
            match item_box(line) {
                Some(ItemBox::Built(checked)) if !*seen_built => {
                    item.built = checked;
                    *seen_built = true;
                    continue;
                }
                Some(ItemBox::Tested(checked)) if !*seen_tested => {
                    item.tested = checked;
                    *seen_tested = true;
                    continue;
                }
                _ => {}
            }
        }

        if UNCHECKED_BOX.is_match(line) {
            file.stray_unchecked_lines.push(line_no);
        }
    }

    if let Some((item, _, _)) = current {
        file.items.push(item);
    }
    file
}

pub fn phase_dir(docs_root: &Path, phase: u8) -> Result<PathBuf, GateError> {
    let prefix = format!("fase-{phase}-");
    let entries = fs::read_dir(docs_root).map_err(|source| GateError::Io {
        path: docs_root.to_path_buf(),
        source,
    })?;
    let mut matches = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| GateError::Io {
            path: docs_root.to_path_buf(),
            source,
        })?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(&prefix) && entry.path().is_dir() {
            matches.push(entry.path());
        }
    }
    match matches.len() {
        0 => Err(GateError::PhaseDirNotFound {
            phase,
            root: docs_root.to_path_buf(),
        }),
        1 => Ok(matches.remove(0)),
        _ => Err(GateError::PhaseDirAmbiguous {
            phase,
            root: docs_root.to_path_buf(),
        }),
    }
}

/// Checks phases `1..=phase` and returns every incomplete item or stray box.
pub fn evaluate(docs_root: &Path, phase: u8) -> Result<Vec<Finding>, GateError> {
    if phase == 0 {
        return Err(GateError::InvalidPhase);
    }
    let mut findings = Vec::new();
    for p in 1..=phase {
        let path = phase_dir(docs_root, p)?.join("CHECKLIST.md");
        let markdown = fs::read_to_string(&path).map_err(|source| GateError::Io {
            path: path.clone(),
            source,
        })?;
        let parsed = parse_checklist(&markdown);

        for item in parsed.items {
            let kind = if item.tested && !item.built {
                FindingKind::TestedWithoutBuilt
            } else if !item.built {
                FindingKind::NotBuilt
            } else if !item.tested {
                FindingKind::NotTested
            } else {
                continue;
            };
            findings.push(Finding {
                phase: p,
                id: Some(item.id),
                title: item.title,
                kind,
            });
        }

        let lines: Vec<&str> = markdown.lines().collect();
        for line in parsed.stray_unchecked_lines {
            let text = UNCHECKED_BOX
                .captures(lines[line - 1].trim_end_matches('\r'))
                .map(|c| c[1].trim().to_string())
                .unwrap_or_default();
            findings.push(Finding {
                phase: p,
                id: None,
                title: text,
                kind: FindingKind::UncheckedBox { line },
            });
        }
    }
    Ok(findings)
}

/// Full output names every finding. Summary output only counts findings per
/// phase, because CI logs of the public repository are readable by anyone.
pub fn format_findings(findings: &[Finding], summary: bool) -> String {
    let mut out = String::new();
    if summary {
        let mut counts: BTreeMap<u8, usize> = BTreeMap::new();
        for f in findings {
            *counts.entry(f.phase).or_default() += 1;
        }
        for (phase, count) in counts {
            let _ = writeln!(out, "phase {phase}: {count} incomplete");
        }
        return out;
    }
    for f in findings {
        let _ = match (&f.id, &f.kind) {
            (_, FindingKind::UncheckedBox { line }) => writeln!(
                out,
                "phase {}, line {line}: unchecked box ({})",
                f.phase, f.title
            ),
            (id, kind) => {
                let state = match kind {
                    FindingKind::NotBuilt => "not built",
                    FindingKind::NotTested => "not tested",
                    _ => "tested but not built",
                };
                writeln!(
                    out,
                    "{} · {}: {state}",
                    id.as_deref().unwrap_or("?"),
                    f.title
                )
            }
        };
    }
    out
}
