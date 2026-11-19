//! Saving, renaming, duplicating and deleting environments, one file each
//! under `environments/`.
//!
//! The files on disk are the source of truth: an environment is found by
//! the name inside its file, so a file named by hand keeps working.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::load::{parse_environment, parse_manifest, yaml_files};
use crate::{
    Environment, Project, ProjectError, has_merge_conflict_markers, safe_file_stem, to_yaml,
    write_if_changed,
};

/// The colours an environment may have. Red is kept for production
/// (F1-FON-07 #2).
pub const ENV_COLORS: [&str; 5] = ["slate", "blue", "green", "amber", "silver"];

const ENVIRONMENTS: &str = "environments";
const MANIFEST: &str = "dea.yaml";

/// Something that uses an environment, shown before it is deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct EnvReference {
    pub label: String,
    pub count: u32,
}

/// Whether `name` can name a variable: a letter or `_`, then letters,
/// digits, `_`, `.` or `-`.
#[must_use]
pub fn is_valid_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// An environment file on disk and the name inside it.
struct EnvFile {
    rel: PathBuf,
    name: String,
    text: String,
}

fn env_files(root: &Path) -> Result<Vec<EnvFile>, ProjectError> {
    let mut files = Vec::new();
    for rel in yaml_files(root, Path::new(ENVIRONMENTS))? {
        let text = fs::read_to_string(root.join(&rel))?;
        let name = parse_environment(&rel, &text, false).map_or_else(
            // A file that does not parse still holds its place: its stem
            // stays taken, and a conflict in it blocks writes to it.
            |_| {
                rel.file_stem()
                    .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
            },
            |env| env.name,
        );
        files.push(EnvFile { rel, name, text });
    }
    Ok(files)
}

fn stem_of(rel: &Path) -> String {
    rel.file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().to_lowercase())
}

fn validate(env: &Environment) -> Result<(), ProjectError> {
    let invalid = |m: String| Err(ProjectError::InvalidEnvironment(m));
    if env.name.trim().is_empty() {
        return invalid("an environment needs a name".into());
    }
    if let Some(color) = env.color.as_deref()
        && !ENV_COLORS.contains(&color)
    {
        return invalid(format!(
            "\"{color}\" is not an environment colour; use one of {}",
            ENV_COLORS.join(", ")
        ));
    }
    let mut seen: Vec<&str> = Vec::new();
    for v in &env.variables {
        if !is_valid_variable_name(&v.name) {
            return invalid(format!(
                "\"{}\" is not a variable name: start with a letter or _, then use letters, digits, _, . or -",
                v.name
            ));
        }
        if seen.contains(&v.name.as_str()) {
            return invalid(format!("the variable \"{}\" is listed twice", v.name));
        }
        seen.push(&v.name);
    }
    Ok(())
}

fn refuse_conflicts(root: &Path, rel: &Path, text: &str) -> Result<(), ProjectError> {
    if has_merge_conflict_markers(text) {
        return Err(ProjectError::MergeConflict(root.join(rel)));
    }
    Ok(())
}

/// Rewrites `defaultEnvironment` in `dea.yaml`, keeping every other field.
fn set_default(root: &Path, from: &str, to: Option<&str>) -> Result<(), ProjectError> {
    let path = root.join(MANIFEST);
    let text = fs::read_to_string(&path)?;
    let mut manifest = parse_manifest(&text).map_err(ProjectError::InvalidManifest)?;
    if manifest.default_environment.as_deref() != Some(from) {
        return Ok(());
    }
    manifest.default_environment = to.map(str::to_string);
    write_if_changed(&path, &to_yaml(&manifest)?)?;
    Ok(())
}

fn check_manifest(root: &Path) -> Result<(), ProjectError> {
    let text = fs::read_to_string(root.join(MANIFEST))?;
    refuse_conflicts(root, Path::new(MANIFEST), &text)
}

/// Saves `env`. With `previous` set, the environment of that name is
/// replaced, and renamed when the names differ: its file moves, and
/// `defaultEnvironment` follows.
///
/// # Errors
///
/// [`ProjectError::ReadOnly`] for a project from a newer Dea;
/// [`ProjectError::InvalidEnvironment`] for a blank name, a colour outside
/// [`ENV_COLORS`] or a bad or repeated variable name;
/// [`ProjectError::NameTaken`] when another environment has the name or
/// its file name; [`ProjectError::MergeConflict`] when a file involved has
/// conflict markers. Nothing is written on error.
pub fn save_environment(
    project: &Project,
    previous: Option<&str>,
    env: &Environment,
) -> Result<(), ProjectError> {
    if project.read_only {
        return Err(ProjectError::ReadOnly);
    }
    validate(env)?;
    let root = &project.root;
    let files = env_files(root)?;
    let old = previous.and_then(|name| files.iter().find(|f| f.name == name));
    let stem = safe_file_stem(&env.name);

    let taken = files.iter().any(|f| {
        !old.is_some_and(|o| o.rel == f.rel)
            && (f.name.eq_ignore_ascii_case(&env.name) || stem_of(&f.rel) == stem)
    });
    if taken {
        return Err(ProjectError::NameTaken(env.name.clone()));
    }

    let target = match old {
        Some(o) if stem_of(&o.rel) == stem || previous == Some(env.name.as_str()) => o.rel.clone(),
        _ => Path::new(ENVIRONMENTS).join(format!("{stem}.yaml")),
    };
    let renamed = old.filter(|o| o.name != env.name);
    if let Some(o) = old {
        refuse_conflicts(root, &o.rel, &o.text)?;
    }
    if renamed.is_some() {
        check_manifest(root)?;
    }

    write_if_changed(&root.join(&target), &to_yaml(env)?)?;
    if let Some(o) = renamed {
        if o.rel != target {
            fs::remove_file(root.join(&o.rel))?;
        }
        set_default(root, &o.name, Some(&env.name))?;
    }
    Ok(())
}

/// Deletes the environment called `name`. When it was the project's
/// default environment, the project no longer has one.
///
/// # Errors
///
/// [`ProjectError::ReadOnly`], or [`ProjectError::MergeConflict`] when its
/// file or `dea.yaml` has conflict markers.
pub fn delete_environment(project: &Project, name: &str) -> Result<(), ProjectError> {
    if project.read_only {
        return Err(ProjectError::ReadOnly);
    }
    let root = &project.root;
    let files = env_files(root)?;
    let Some(file) = files.iter().find(|f| f.name == name) else {
        return Ok(());
    };
    refuse_conflicts(root, &file.rel, &file.text)?;
    check_manifest(root)?;
    fs::remove_file(root.join(&file.rel))?;
    set_default(root, name, None)
}

/// Saves a copy of the environment called `name` as `<name> copy` (or
/// `<name> copy 2`, …) and returns it.
///
/// # Errors
///
/// [`ProjectError::ReadOnly`], [`ProjectError::InvalidEnvironment`] when no
/// environment has that name, or any error of [`save_environment`].
pub fn duplicate_environment(project: &Project, name: &str) -> Result<Environment, ProjectError> {
    if project.read_only {
        return Err(ProjectError::ReadOnly);
    }
    let root = &project.root;
    let files = env_files(root)?;
    let source = files.iter().find(|f| f.name == name).ok_or_else(|| {
        ProjectError::InvalidEnvironment(format!("no environment is called \"{name}\""))
    })?;
    let original = parse_environment(&source.rel, &source.text, false)
        .map_err(|e| ProjectError::InvalidEnvironment(e.to_string()))?;
    let free = |candidate: &str| {
        let stem = safe_file_stem(candidate);
        !files
            .iter()
            .any(|f| f.name.eq_ignore_ascii_case(candidate) || stem_of(&f.rel) == stem)
    };
    let copy_name = std::iter::once(format!("{name} copy"))
        .chain((2..).map(|n| format!("{name} copy {n}")))
        .find(|candidate| free(candidate))
        .expect("an unused number always exists");
    let copy = Environment {
        name: copy_name,
        ..original
    };
    save_environment(project, None, &copy)?;
    Ok(copy)
}

/// What in the project uses the environment called `name`. Modules add
/// their own references here (requests, connections, hosts).
#[must_use]
pub fn environment_references(project: &Project, name: &str) -> Vec<EnvReference> {
    let mut refs = Vec::new();
    if project.manifest.default_environment.as_deref() == Some(name) {
        refs.push(EnvReference {
            label: "Default environment of the project".into(),
            count: 1,
        });
    }
    refs
}
