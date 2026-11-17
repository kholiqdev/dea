use std::path::Path;

use crate::{MAX_RELATIVE_PATH_LEN, ProjectError};

const MAX_STEM_CHARS: usize = 80;

/// Names Windows refuses as a file name, with or without an extension.
const WINDOWS_RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

fn trim_edges(s: &str) -> &str {
    s.trim_matches(|c| c == '-' || c == '.' || c == ' ')
}

/// Turns a display name into a file name that is valid on Windows, macOS and
/// Linux. Letters from any script are kept.
pub fn safe_file_stem(name: &str) -> String {
    let mut stem = String::with_capacity(name.len());
    for c in name.to_lowercase().chars() {
        let c = if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
            c
        } else {
            '-'
        };
        if !(c == '-' && stem.ends_with('-')) {
            stem.push(c);
        }
    }
    let mut stem = trim_edges(&stem).to_string();
    if stem.is_empty() {
        stem = "untitled".to_string();
    }

    let base_len = stem.find('.').unwrap_or(stem.len());
    if WINDOWS_RESERVED.contains(&&stem[..base_len]) {
        stem.insert_str(base_len, "-1");
    }

    if stem.chars().count() > MAX_STEM_CHARS {
        let truncated: String = stem.chars().take(MAX_STEM_CHARS).collect();
        stem = trim_edges(&truncated).to_string();
    }
    stem
}

/// Returns `stem` in lower case, numbered (`-2`, `-3`, ...) when a file with
/// the same name exists. Names are compared without regard to case, because
/// macOS and Windows file systems do not distinguish them.
pub fn unique_file_stem(existing: &[String], stem: &str) -> String {
    let taken: Vec<String> = existing.iter().map(|s| s.to_lowercase()).collect();
    let base = stem.to_lowercase();
    if !taken.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| {
            let suffix = format!("-{n}");
            let keep = MAX_STEM_CHARS.saturating_sub(suffix.chars().count());
            let head: String = base.chars().take(keep).collect();
            format!("{head}{suffix}")
        })
        .find(|candidate| !taken.contains(candidate))
        .expect("an unused number always exists")
}

/// Rejects project paths longer than 200 characters, which break on Windows
/// once the project folder itself is nested a few levels deep.
pub fn check_relative_path(rel: &Path) -> Result<(), ProjectError> {
    if rel.to_string_lossy().chars().count() > MAX_RELATIVE_PATH_LEN {
        return Err(ProjectError::PathTooLong(rel.to_path_buf()));
    }
    Ok(())
}

/// Environment names that are marked production when a project is created
/// or imported.
pub fn is_production_name(name: &str) -> bool {
    let name = name.trim();
    ["prod", "production", "prd"]
        .iter()
        .any(|p| name.eq_ignore_ascii_case(p))
}
