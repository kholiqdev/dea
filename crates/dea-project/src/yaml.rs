use std::fs;
use std::io::{ErrorKind, Write as _};
use std::path::Path;

use crate::{ProjectError, has_merge_conflict_markers};

/// Serializes a project file with LF line endings, ending in a newline.
/// Trailing newlines are never trimmed: a value that ends in blank lines is
/// written as a block that keeps them, and trimming would change the value.
pub fn to_yaml<T: serde::Serialize>(value: &T) -> Result<String, ProjectError> {
    let mut text = serde_norway::to_string(value).map_err(|e| ProjectError::Yaml(e.to_string()))?;
    if !text.ends_with('\n') {
        text.push('\n');
    }
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
/// the same folder is renamed over the target. An existing file keeps its
/// permissions, and a file with unresolved merge conflict markers is never
/// overwritten (F1-FON-05 #5).
pub fn write_if_changed(path: &Path, contents: &str) -> Result<bool, ProjectError> {
    let existing_permissions = match fs::read(path) {
        Ok(existing) => {
            if let Ok(existing) = std::str::from_utf8(&existing) {
                if has_merge_conflict_markers(existing) {
                    return Err(ProjectError::MergeConflict(path.to_path_buf()));
                }
                if normalize(existing) == normalize(contents) {
                    return Ok(false);
                }
            }
            Some(fs::metadata(path)?.permissions())
        }
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };

    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    fs::create_dir_all(dir)?;
    let mut builder = tempfile::Builder::new();
    builder.prefix(".dea-write-");
    // Temporary files are private (0600) by default; a project file should
    // get the same permissions as any new file, after the user's umask.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o666));
    }
    let mut tmp = builder.tempfile_in(dir)?;
    if let Some(permissions) = existing_permissions {
        fs::set_permissions(tmp.path(), permissions)?;
    }
    tmp.write_all(contents.as_bytes())?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| ProjectError::Io(e.error))?;
    Ok(true)
}
