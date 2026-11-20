//! Opening and creating projects, and the list of recent ones.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use dea_project::{
    Environment, FileError, MigrationOutcome, Project, ProjectError, create_project, load_project,
    migrate,
};
pub use dea_project::{FileErrorInfo, portable_path};
use serde::{Deserialize, Serialize};

/// How many recent projects are kept (F1-FON-03 #4).
pub const MAX_RECENT: usize = 10;

/// A recent project as stored in the app data folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    /// The project folder, canonical when it was opened.
    pub root: PathBuf,
    /// The project id from `dea.yaml`, so a moved clone keeps one entry.
    pub id: Option<String>,
    pub name: String,
    /// When it was last opened, in seconds since the Unix epoch.
    pub last_opened: u64,
}

/// A recent project as Welcome shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct RecentEntry {
    pub root: String,
    pub name: String,
    /// Request files in the project; 0 when the folder is missing.
    pub request_count: u32,
    /// Seconds since the Unix epoch.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub last_opened: u64,
    /// The folder or its `dea.yaml` is gone.
    pub missing: bool,
}

/// A project read from disk, with the files that could not be read and the
/// upgrade that ran first, if any.
#[derive(Debug, Clone)]
pub struct OpenedProject {
    pub project: Project,
    pub errors: Vec<FileError>,
    pub migrated: Option<MigrationOutcome>,
}

/// What a window knows about its project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct ProjectInfo {
    pub root: String,
    pub name: String,
    /// Keys the project's local data and Vault secrets.
    pub scope: String,
    /// Which window of this project: 0 for the first, 1 for the second.
    pub slot: u8,
    /// Written by a newer Dea: shown, never written.
    pub read_only: bool,
    pub default_environment: Option<String>,
    pub environments: Vec<Environment>,
    /// The file of each environment by name, relative to the root with
    /// forward slashes.
    pub environment_files: BTreeMap<String, String>,
    /// Names of the project's secrets; never their values.
    pub secret_names: Vec<String>,
    pub errors: Vec<FileErrorInfo>,
    /// The schema version the project was upgraded from when it opened.
    pub migrated_from: Option<u32>,
    /// The project folder is gone.
    pub missing: bool,
}

impl ProjectInfo {
    /// The info of an opened project for window `slot`.
    #[must_use]
    pub fn new(opened: &OpenedProject, slot: u8) -> ProjectInfo {
        let p = &opened.project;
        ProjectInfo {
            root: p.root.to_string_lossy().into_owned(),
            name: p.manifest.name.clone(),
            scope: p.vault_scope(),
            slot,
            read_only: p.read_only,
            default_environment: p.manifest.default_environment.clone(),
            environments: p.environments.clone(),
            environment_files: p
                .environment_files
                .iter()
                .map(|(name, path)| (name.clone(), portable_path(path)))
                .collect(),
            secret_names: p.vault.secrets.iter().map(|s| s.name.clone()).collect(),
            errors: opened.errors.iter().map(FileErrorInfo::from).collect(),
            migrated_from: opened
                .migrated
                .as_ref()
                .filter(|m| m.from != m.to)
                .map(|m| m.from),
            missing: false,
        }
    }

    /// The info of a project whose folder is gone; named after the folder.
    #[must_use]
    pub fn missing(root: &Path, scope: &str, slot: u8) -> ProjectInfo {
        ProjectInfo {
            root: root.to_string_lossy().into_owned(),
            name: root
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            scope: scope.to_string(),
            slot,
            read_only: true,
            default_environment: None,
            environments: Vec::new(),
            environment_files: BTreeMap::new(),
            secret_names: Vec::new(),
            errors: Vec::new(),
            migrated_from: None,
            missing: true,
        }
    }

    /// The info of a project whose `dea.yaml` cannot be read, for example
    /// after a merge that left conflict markers. Nothing else is loaded;
    /// the window keeps what it showed before and names the problem.
    #[must_use]
    pub fn unreadable(root: &Path, scope: &str, slot: u8, error: &FileError) -> ProjectInfo {
        ProjectInfo {
            missing: false,
            errors: vec![FileErrorInfo::from(error)],
            ..ProjectInfo::missing(root, scope, slot)
        }
    }
}

/// Sent to a window when files of its project changed on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct ProjectChanged {
    pub info: ProjectInfo,
    /// Paths relative to the project root, with forward slashes.
    pub changed: Vec<String>,
}

/// Why a folder could not be opened or a project not created.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("{} has no dea.yaml, so it is not a Dea project", path.display())]
    NotAProject { path: PathBuf },
    #[error("{} is not a folder", path.display())]
    NotAFolder { path: PathBuf },
    #[error("a project needs a name")]
    EmptyName,
    /// A typed location that is not a full path, such as `~/work/api`,
    /// would land somewhere unexpected.
    #[error("{} is not a full path: choose the folder with Choose…", path.display())]
    RelativeLocation { path: PathBuf },
    #[error(transparent)]
    Project(#[from] ProjectError),
}

/// Opens the project in `root`: upgrades an older format (with a backup)
/// and reads every file.
///
/// # Errors
///
/// [`OpenError::NotAFolder`] or [`OpenError::NotAProject`] for a folder Dea
/// cannot open; [`OpenError::Project`] when the upgrade or `dea.yaml` fails.
pub fn open_project(root: &Path) -> Result<OpenedProject, OpenError> {
    if !root.is_dir() {
        return Err(OpenError::NotAFolder {
            path: root.to_path_buf(),
        });
    }
    let root = fs::canonicalize(root).map_err(ProjectError::Io)?;
    if !root.join("dea.yaml").is_file() {
        return Err(OpenError::NotAProject { path: root });
    }
    let migrated = migrate(&root)?;
    let loaded = load_project(&root)?;
    Ok(OpenedProject {
        project: loaded.project,
        errors: loaded.errors,
        migrated: Some(migrated),
    })
}

/// Creates a project named `name` in `location`, creating the folder when
/// it does not exist, and opens it.
///
/// # Errors
///
/// [`OpenError::EmptyName`] for a blank name, [`OpenError::RelativeLocation`]
/// for a location that is not a full path; [`OpenError::Project`] when a
/// project file already exists there or cannot be written.
pub fn new_project(location: &Path, name: &str) -> Result<OpenedProject, OpenError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(OpenError::EmptyName);
    }
    if !location.is_absolute() {
        return Err(OpenError::RelativeLocation {
            path: location.to_path_buf(),
        });
    }
    fs::create_dir_all(location).map_err(ProjectError::Io)?;
    create_project(location, name)?;
    open_project(location)
}

/// The recent list as Welcome shows it: missing folders marked, requests
/// counted.
#[must_use]
pub fn recent_entries(list: &[RecentProject]) -> Vec<RecentEntry> {
    list.iter()
        .map(|p| {
            let missing = !p.root.join("dea.yaml").is_file();
            RecentEntry {
                root: p.root.to_string_lossy().into_owned(),
                name: p.name.clone(),
                request_count: if missing {
                    0
                } else {
                    count_yaml(&p.root.join("requests"))
                },
                last_opened: p.last_opened,
                missing,
            }
        })
        .collect()
}

/// YAML files under `dir`, without following symbolic links.
fn count_yaml(dir: &Path) -> u32 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let Ok(kind) = entry.file_type() else {
                return 0;
            };
            let path = entry.path();
            if kind.is_dir() {
                count_yaml(&path)
            } else if kind.is_file() && path.extension().is_some_and(|e| e == "yaml") {
                1
            } else {
                0
            }
        })
        .sum()
}
