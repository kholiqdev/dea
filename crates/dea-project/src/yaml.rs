use std::fs;
use std::io::{ErrorKind, Write as _};
use std::path::Path;

use crate::ProjectError;

/// Serializes a project file: LF line endings and exactly one trailing newline.
pub fn to_yaml<T: serde::Serialize>(value: &T) -> Result<String, ProjectError> {
    let text = serde_norway::to_string(value).map_err(|e| ProjectError::Yaml(e.to_string()))?;
    let mut text = text.replace("\r\n", "\n");
    text.truncate(text.trim_end_matches('\n').len());
    text.push('\n');
    Ok(text)
}

/// Line endings and a leading byte order mark do not count as changes, so a
/// file checked out with CRLF or saved by Notepad is not rewritten.
fn normalize(text: &str) -> String {
    text.strip_prefix('\u{feff}')
        .unwrap_or(text)
        .replace("\r\n", "\n")
}

/// Writes `contents` only when it differs from the file on disk. Returns
/// whether the file was written. The write is atomic: a temporary file in
/// the same folder is renamed over the target.
pub fn write_if_changed(path: &Path, contents: &str) -> Result<bool, ProjectError> {
    match fs::read(path) {
        Ok(existing) => {
            if let Ok(existing) = std::str::from_utf8(&existing)
                && normalize(existing) == normalize(contents)
            {
                return Ok(false);
            }
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    fs::create_dir_all(dir)?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".dea-write-")
        .tempfile_in(dir)?;
    tmp.write_all(contents.as_bytes())?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| ProjectError::Io(e.error))?;
    Ok(true)
}
