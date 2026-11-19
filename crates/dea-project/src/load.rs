use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::{
    Environment, FileError, LoadedProject, Manifest, Project, ProjectError, SCHEMA_VERSION,
    VaultManifest,
};

const MANIFEST: &str = "dea.yaml";
const VAULT: &str = "vault.yaml";
const ENVIRONMENTS: &str = "environments";

/// True when `text` still holds git conflict markers, for example after a
/// `git pull` that could not merge a project file.
pub fn has_merge_conflict_markers(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_end_matches('\r');
        line.starts_with("<<<<<<< ") || line.starts_with(">>>>>>> ") || line == "======="
    })
}

/// Loads a project folder. Only a missing or unreadable `dea.yaml` is fatal;
/// every other broken file is reported in `errors` and skipped.
pub fn load_project(root: &Path) -> Result<LoadedProject, ProjectError> {
    let manifest_path = root.join(MANIFEST);
    let manifest_text = match fs::read_to_string(&manifest_path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(ProjectError::NotAProject(root.to_path_buf()));
        }
        Err(e) => return Err(e.into()),
    };
    let manifest = parse_manifest(&manifest_text).map_err(ProjectError::InvalidManifest)?;
    let read_only = manifest.schema_version > SCHEMA_VERSION;

    let mut errors = Vec::new();

    let vault = match fs::read_to_string(root.join(VAULT)) {
        Ok(text) => parse_vault(&text, read_only).unwrap_or_else(|e| {
            errors.push(e);
            VaultManifest::default()
        }),
        Err(e) if e.kind() == ErrorKind::NotFound => VaultManifest::default(),
        Err(e) => {
            errors.push(io_error(Path::new(VAULT), &e));
            VaultManifest::default()
        }
    };

    let mut environments = Vec::new();
    let mut environment_files = BTreeMap::new();
    for rel in yaml_files(root, Path::new(ENVIRONMENTS))? {
        match fs::read_to_string(root.join(&rel)) {
            Ok(text) => match parse_environment(&rel, &text, read_only) {
                Ok(env) => {
                    environment_files.insert(env.name.clone(), rel);
                    environments.push(env);
                }
                Err(e) => errors.push(e),
            },
            Err(e) => errors.push(io_error(&rel, &e)),
        }
    }

    Ok(LoadedProject {
        project: Project {
            root: root.to_path_buf(),
            manifest,
            environments,
            environment_files,
            vault,
            read_only,
        },
        errors,
    })
}

/// Fields every schema version keeps. Used to open projects written by a
/// newer Dea, whose manifest may have fields this build does not know.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestAnyVersion {
    name: String,
    #[serde(default)]
    id: Option<String>,
    schema_version: u32,
    #[serde(default)]
    default_environment: Option<String>,
}

/// Environment and vault files as any schema version may write them: the
/// same fields, but unknown ones are ignored. Used only for read-only
/// projects from a newer Dea, which adds fields when it bumps the version.
mod any_version {
    use serde::Deserialize;

    use crate::{Environment, SecretKind, SecretSpec, Variable, VaultManifest};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct EnvironmentFile {
        name: String,
        production: bool,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        variables: Vec<VariableEntry>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct VariableEntry {
        name: String,
        value: String,
        #[serde(default)]
        description: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct VaultFile {
        #[serde(default)]
        secrets: Vec<SecretEntry>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SecretEntry {
        name: String,
        #[serde(rename = "type")]
        kind: SecretKind,
        #[serde(default)]
        description: Option<String>,
    }

    impl From<EnvironmentFile> for Environment {
        fn from(e: EnvironmentFile) -> Self {
            Environment {
                name: e.name,
                production: e.production,
                color: e.color,
                variables: e
                    .variables
                    .into_iter()
                    .map(|v| Variable {
                        name: v.name,
                        value: v.value,
                        description: v.description,
                    })
                    .collect(),
            }
        }
    }

    impl From<VaultFile> for VaultManifest {
        fn from(v: VaultFile) -> Self {
            VaultManifest {
                secrets: v
                    .secrets
                    .into_iter()
                    .map(|s| SecretSpec {
                        name: s.name,
                        kind: s.kind,
                        description: s.description,
                    })
                    .collect(),
            }
        }
    }
}

pub(crate) fn parse_environment(
    rel: &Path,
    text: &str,
    read_only: bool,
) -> Result<Environment, FileError> {
    if read_only {
        parse::<any_version::EnvironmentFile>(rel, text).map(Environment::from)
    } else {
        parse(rel, text)
    }
}

fn parse_vault(text: &str, read_only: bool) -> Result<VaultManifest, FileError> {
    let rel = Path::new(VAULT);
    if read_only {
        parse::<any_version::VaultFile>(rel, text).map(VaultManifest::from)
    } else {
        parse(rel, text)
    }
}

pub(crate) fn parse_manifest(text: &str) -> Result<Manifest, FileError> {
    let rel = Path::new(MANIFEST);
    let any: ManifestAnyVersion = parse(rel, text)?;
    if any.schema_version > SCHEMA_VERSION {
        return Ok(Manifest {
            name: any.name,
            id: any.id,
            schema_version: any.schema_version,
            default_environment: any.default_environment,
        });
    }
    parse(rel, text)
}

/// Reads one project file: a byte order mark and CRLF line endings are
/// accepted, conflict markers and empty files are reported clearly.
pub(crate) fn parse<T: DeserializeOwned>(rel: &Path, text: &str) -> Result<T, FileError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = text.replace("\r\n", "\n");
    let error = |message: String, line, column| FileError {
        path: rel.to_path_buf(),
        line,
        column,
        message,
        merge_conflict: false,
    };
    if has_merge_conflict_markers(&text) {
        return Err(FileError {
            merge_conflict: true,
            ..error(
                "unresolved merge conflict: resolve the conflict markers and save".into(),
                None,
                None,
            )
        });
    }
    if text.trim().is_empty() {
        return Err(error("empty file".into(), None, None));
    }
    serde_norway::from_str(&text).map_err(|e| {
        let location = e.location();
        error(
            e.to_string(),
            location.as_ref().map(serde_norway::Location::line),
            location.as_ref().map(serde_norway::Location::column),
        )
    })
}

fn io_error(rel: &Path, e: &std::io::Error) -> FileError {
    FileError {
        path: rel.to_path_buf(),
        line: None,
        column: None,
        message: format!("cannot read file: {e}"),
        merge_conflict: false,
    }
}

/// `*.yaml` files directly inside `dir`, sorted by file name, as paths
/// relative to the project root. A missing folder has no files.
pub(crate) fn yaml_files(root: &Path, dir: &Path) -> Result<Vec<PathBuf>, ProjectError> {
    let entries = match fs::read_dir(root.join(dir)) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".yaml") && !name.starts_with('.') && entry.file_type()?.is_file() {
            names.push(name);
        }
    }
    names.sort();
    Ok(names.into_iter().map(|name| dir.join(name)).collect())
}
