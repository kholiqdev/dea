//! Declaring the secrets a project needs in `vault.yaml`. Values never
//! appear here; they live in the operating system keychain.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use crate::load::parse_vault;
use crate::{
    Project, ProjectError, SecretSpec, VaultManifest, has_merge_conflict_markers,
    is_valid_variable_name, to_yaml, write_if_changed,
};

const VAULT: &str = "vault.yaml";

/// Declares `spec` in `vault.yaml`. A declaration with the same name has its
/// kind and description replaced, in its place; otherwise `spec` is added at
/// the end.
///
/// # Errors
///
/// [`ProjectError::ReadOnly`] for a project from a newer Dea;
/// [`ProjectError::InvalidSecret`] when the name is not a valid variable
/// name; [`ProjectError::MergeConflict`] when `vault.yaml` has conflict
/// markers; [`ProjectError::InvalidManifest`] when it cannot be read as a
/// vault. Nothing is written on error.
pub fn save_secret_spec(project: &Project, spec: &SecretSpec) -> Result<(), ProjectError> {
    if project.read_only {
        return Err(ProjectError::ReadOnly);
    }
    if !is_valid_variable_name(&spec.name) {
        return Err(ProjectError::InvalidSecret(format!(
            "\"{}\" is not a secret name: start with a letter or _, then use letters, digits, _, . or -",
            spec.name
        )));
    }
    let path = project.root.join(VAULT);
    let mut vault = match fs::read_to_string(&path) {
        Ok(text) => {
            if has_merge_conflict_markers(&text) {
                return Err(ProjectError::MergeConflict(path));
            }
            parse_vault(&text, false).map_err(ProjectError::InvalidManifest)?
        }
        Err(e) if e.kind() == ErrorKind::NotFound => VaultManifest::default(),
        Err(e) => return Err(e.into()),
    };
    match vault.secrets.iter_mut().find(|s| s.name == spec.name) {
        Some(existing) => *existing = spec.clone(),
        None => vault.secrets.push(spec.clone()),
    }
    write_if_changed(Path::new(&path), &to_yaml(&vault)?)?;
    Ok(())
}
