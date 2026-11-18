//! The secret canary: a value stored as a secret in tests, then searched
//! for everywhere a secret must never be written. Later plans add the log,
//! history and crash report folders to the places that are searched.

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

/// A new canary value, different on every call.
pub fn canary() -> String {
    format!("dea-canary-{}", uuid::Uuid::new_v4().simple())
}

/// Every file under `roots` that holds `canary` as UTF-8, UTF-16LE or
/// standard base64 (at any alignment, so a canary inside a longer encoded
/// value such as an HTTP basic auth header is found too). Symlinks are not
/// followed.
pub fn find_canary(roots: &[&Path], canary: &str) -> Vec<PathBuf> {
    let needles = needles(canary);
    let mut found = Vec::new();
    let mut pending: Vec<PathBuf> = roots.iter().map(|r| r.to_path_buf()).collect();
    while let Some(path) = pending.pop() {
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            if let Ok(entries) = fs::read_dir(&path) {
                pending.extend(entries.flatten().map(|e| e.path()));
            }
        } else if meta.is_file()
            && let Ok(bytes) = fs::read(&path)
            && needles.iter().any(|n| contains(&bytes, n))
        {
            found.push(path);
        }
    }
    found.sort();
    found
}

/// The byte patterns that reveal the canary.
fn needles(canary: &str) -> Vec<Vec<u8>> {
    let mut out = vec![
        canary.as_bytes().to_vec(),
        canary.encode_utf16().flat_map(u16::to_le_bytes).collect(),
    ];
    // Base64 turns 3 bytes into 4 characters, so the characters for the
    // canary depend on where it starts. Encode it after 0, 1 and 2 filler
    // bytes and keep only the characters that come from the canary alone.
    for offset in 0..3usize {
        let mut input = vec![0u8; offset];
        input.extend_from_slice(canary.as_bytes());
        let encoded = STANDARD.encode(&input);
        let encoded = encoded.trim_end_matches('=');
        let skip = (offset * 4).div_ceil(3);
        let drop_last = usize::from(input.len() % 3 != 0);
        out.push(encoded.as_bytes()[skip..encoded.len() - drop_last].to_vec());
    }
    out
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}
