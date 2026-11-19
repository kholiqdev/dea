use std::fmt;
use std::path::PathBuf;

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
        write!(f, "{}", self.path.display())?;
        if let Some(line) = self.line {
            write!(f, ":{line}")?;
            if let Some(column) = self.column {
                write!(f, ":{column}")?;
            }
        }
        write!(f, ": {}", self.message)
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
    #[error("an environment called \"{0}\" already exists")]
    NameTaken(String),
}
