use std::fmt;
use std::path::PathBuf;

use serde::Serialize;

use crate::portable_path;

/// A problem in one project file, pointing at the place to fix it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileError {
    /// Path relative to the project root.
    pub path: PathBuf,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
    /// The file still holds git conflict markers.
    pub merge_conflict: bool,
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", portable_path(&self.path))?;
        if let Some(line) = self.line {
            write!(f, ":{line}")?;
            if let Some(column) = self.column {
                write!(f, ":{column}")?;
            }
        }
        write!(f, ": {}", self.message)
    }
}

/// A project file that could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct FileErrorInfo {
    /// Path relative to the project root, with forward slashes.
    pub path: String,
    pub line: Option<u32>,
    pub message: String,
    /// The file still holds git conflict markers; saving over it is refused.
    pub merge_conflict: bool,
}

impl From<&FileError> for FileErrorInfo {
    fn from(e: &FileError) -> Self {
        FileErrorInfo {
            path: portable_path(&e.path),
            line: e.line.and_then(|l| u32::try_from(l).ok()),
            message: e.message.clone(),
            merge_conflict: e.merge_conflict,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ProjectError {
    #[error("{0} is not a Dea project: dea.yaml is missing")]
    NotAProject(PathBuf),
    #[error("invalid project manifest: {0}")]
    InvalidManifest(FileError),
    #[error("path is longer than 200 characters: {0}")]
    PathTooLong(PathBuf),
    #[error("{0} already exists")]
    AlreadyExists(PathBuf),
    /// The request or folder asked for is not on disk, as after a delete.
    #[error("{} does not exist", portable_path(.0))]
    NotFound(PathBuf),
    #[error("cannot write YAML: {0}")]
    Yaml(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("cannot migrate the project: {0}")]
    Migration(String),
    #[error("{0} has unresolved merge conflict markers; resolve them before saving")]
    MergeConflict(PathBuf),
    #[error("this project was saved by a newer Dea and is open read-only")]
    ReadOnly,
    #[error("{0}")]
    InvalidEnvironment(String),
    #[error("{0}")]
    InvalidSecret(String),
    #[error("{0}")]
    InvalidRequest(String),
    #[error("an environment called \"{0}\" already exists")]
    NameTaken(String),
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{FileError, ProjectError};
    use crate::RequestError;

    // A path built from its parts, as a walk of the project builds it, has
    // `\` between them on Windows. Messages name files the way the project
    // tree and git do on every system.
    #[test]
    fn messages_name_project_files_with_forward_slashes() {
        let path: PathBuf = ["requests", "shipments", "broken.yaml"].iter().collect();
        let error = FileError {
            path: path.clone(),
            line: Some(1),
            column: Some(7),
            message: "bad".into(),
            merge_conflict: false,
        };
        assert_eq!(error.to_string(), "requests/shipments/broken.yaml:1:7: bad");
        assert_eq!(
            ProjectError::NotFound(path.clone()).to_string(),
            "requests/shipments/broken.yaml does not exist"
        );
        assert_eq!(
            RequestError::NotFound(path).to_string(),
            "there is no request at requests/shipments/broken.yaml"
        );
    }
}
