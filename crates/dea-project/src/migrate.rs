//! Upgrades projects written with an older file format, after a backup.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde_norway::Value;
use time::{OffsetDateTime, UtcOffset};

use crate::create::add_backup_ignore;
use crate::load::parse;
use crate::{FileError, ProjectError, SCHEMA_VERSION, to_yaml, write_if_changed};

/// One upgrade step, from the version it is registered under to the next.
pub type Migration = fn(root: &Path) -> Result<(), ProjectError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationOutcome {
    pub from: u32,
    pub to: u32,
    /// Where the project was copied before the first step ran.
    pub backup_dir: Option<PathBuf>,
}

/// Built-in steps. Schema version 1 is the first, so there are none yet.
const MIGRATIONS: &[(u32, Migration)] = &[];

const MANIFEST: &str = "dea.yaml";
const BACKUP_DIR: &str = ".dea-backup";
/// Top-level entries never copied into a backup.
const NOT_BACKED_UP: [&str; 2] = [".git", BACKUP_DIR];

/// Brings a project up to the schema version of this build.
pub fn migrate(root: &Path) -> Result<MigrationOutcome, ProjectError> {
    migrate_with(root, SCHEMA_VERSION, MIGRATIONS, OffsetDateTime::now_utc())
}

/// Runs `migrations` (keyed by the version each step starts from) until the
/// project reaches `target`. The project is copied to
/// `.dea-backup/<timestamp>/` first. `schemaVersion` is written after each
/// successful step, so a failure leaves the last version that worked.
/// Projects already at or above `target` are left untouched.
pub fn migrate_with(
    root: &Path,
    target: u32,
    migrations: &[(u32, Migration)],
    now: OffsetDateTime,
) -> Result<MigrationOutcome, ProjectError> {
    let from = schema_version(&read_manifest(root)?)?;
    if from >= target {
        return Ok(MigrationOutcome {
            from,
            to: from,
            backup_dir: None,
        });
    }

    // Make sure every step exists before touching anything.
    let mut steps = Vec::new();
    for version in from..target {
        let step = migrations
            .iter()
            .find(|(v, _)| *v == version)
            .map(|(_, step)| *step)
            .ok_or_else(|| {
                ProjectError::Migration(format!("no migration from schema version {version}"))
            })?;
        steps.push((version, step));
    }

    let backup_dir = back_up(root, now)?;
    add_backup_ignore(&root.join(".gitignore"))?;

    for (version, step) in steps {
        step(root).map_err(|e| {
            ProjectError::Migration(format!(
                "migration from schema version {version} failed: {e}. \
                 The project before migration is in {}",
                backup_dir.display()
            ))
        })?;
        // A step may rewrite dea.yaml, so read it again before bumping.
        let mut manifest = read_manifest(root)?;
        set_schema_version(&mut manifest, version + 1)?;
        write_if_changed(&root.join(MANIFEST), &to_yaml(&manifest)?)?;
    }

    Ok(MigrationOutcome {
        from,
        to: target,
        backup_dir: Some(backup_dir),
    })
}

/// `dea.yaml` as plain YAML, so any schema version can be read.
fn read_manifest(root: &Path) -> Result<Value, ProjectError> {
    let text = match fs::read_to_string(root.join(MANIFEST)) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(ProjectError::NotAProject(root.to_path_buf()));
        }
        Err(e) => return Err(e.into()),
    };
    parse::<Value>(Path::new(MANIFEST), &text).map_err(ProjectError::InvalidManifest)
}

fn invalid_version() -> ProjectError {
    ProjectError::InvalidManifest(FileError {
        path: PathBuf::from(MANIFEST),
        line: None,
        column: None,
        message: "schemaVersion is missing or is not a whole number".into(),
    })
}

fn schema_version(manifest: &Value) -> Result<u32, ProjectError> {
    manifest
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(invalid_version)
}

fn set_schema_version(manifest: &mut Value, version: u32) -> Result<(), ProjectError> {
    let map = manifest.as_mapping_mut().ok_or_else(invalid_version)?;
    map.insert(Value::from("schemaVersion"), Value::from(version));
    Ok(())
}

/// Copies the project to a new `.dea-backup/<YYYYMMDDTHHMMSSZ>/` folder,
/// numbered when a backup from the same second already exists.
fn back_up(root: &Path, now: OffsetDateTime) -> Result<PathBuf, ProjectError> {
    let t = now.to_offset(UtcOffset::UTC);
    let stamp = format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    );
    let backups = root.join(BACKUP_DIR);
    let mut dir = backups.join(&stamp);
    let mut n = 2;
    while dir.exists() {
        dir = backups.join(format!("{stamp}-{n}"));
        n += 1;
    }
    fs::create_dir_all(&dir)?;

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        if NOT_BACKED_UP.iter().any(|skip| name == *skip) {
            continue;
        }
        copy_entry(&entry.path(), &dir.join(&name))?;
    }
    Ok(dir)
}

/// Copies files and folders. Symbolic links to files are copied as files;
/// links to folders are skipped, so a link loop cannot fill the disk.
fn copy_entry(from: &Path, to: &Path) -> Result<(), ProjectError> {
    let file_type = fs::symlink_metadata(from)?.file_type();
    if file_type.is_dir() {
        fs::create_dir(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_entry(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else if file_type.is_file() || fs::metadata(from).is_ok_and(|m| m.is_file()) {
        fs::copy(from, to)?;
    }
    Ok(())
}
