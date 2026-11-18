use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    Environment, Manifest, Project, ProjectError, SCHEMA_VERSION, VaultManifest,
    check_relative_path, is_production_name, load_project, safe_file_stem, to_yaml,
    write_if_changed,
};

const DEFAULT_ENVIRONMENTS: [&str; 3] = ["local", "staging", "production"];
const BACKUP_IGNORE: &str = ".dea-backup/";

/// Creates a new project in `root` with the environments `local`, `staging`
/// and `production`. Refuses to overwrite any existing project file.
pub fn create_project(root: &Path, name: &str) -> Result<Project, ProjectError> {
    let manifest = Manifest {
        name: name.to_string(),
        schema_version: SCHEMA_VERSION,
        default_environment: Some(DEFAULT_ENVIRONMENTS[0].to_string()),
    };
    let environments: Vec<Environment> = DEFAULT_ENVIRONMENTS
        .iter()
        .map(|env| Environment {
            name: env.to_string(),
            production: is_production_name(env),
            color: None,
            variables: Vec::new(),
        })
        .collect();
    let vault = VaultManifest::default();

    let mut files: Vec<(PathBuf, String)> = vec![
        (PathBuf::from("dea.yaml"), to_yaml(&manifest)?),
        (PathBuf::from("vault.yaml"), to_yaml(&vault)?),
    ];
    for env in &environments {
        let rel = Path::new("environments").join(format!("{}.yaml", safe_file_stem(&env.name)));
        files.push((rel, to_yaml(env)?));
    }

    // Check everything before writing anything, so a refusal leaves no trace.
    for (rel, _) in &files {
        check_relative_path(rel)?;
        let path = root.join(rel);
        if path.exists() {
            return Err(ProjectError::AlreadyExists(path));
        }
    }

    fs::create_dir_all(root)?;
    for (rel, contents) in &files {
        write_if_changed(&root.join(rel), contents)?;
    }
    add_backup_ignore(&root.join(".gitignore"))?;

    // Return what is actually on disk, read the same way as any project.
    let mut loaded = load_project(root)?;
    if !loaded.errors.is_empty() {
        return Err(ProjectError::InvalidManifest(loaded.errors.remove(0)));
    }
    Ok(loaded.project)
}

/// Makes sure migration backups never end up in git, keeping any existing
/// `.gitignore` lines.
pub(crate) fn add_backup_ignore(path: &Path) -> Result<(), ProjectError> {
    let existing = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    if existing.lines().any(|l| l.trim() == BACKUP_IGNORE) {
        return Ok(());
    }
    let mut updated = existing.replace("\r\n", "\n");
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(BACKUP_IGNORE);
    updated.push('\n');
    write_if_changed(path, &updated)?;
    Ok(())
}
