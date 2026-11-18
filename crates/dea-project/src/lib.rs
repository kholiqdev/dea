//! Reads and writes Dea project folders: a manifest, environments and the
//! list of secrets a project needs, stored as plain YAML files for git.

mod create;
mod error;
mod load;
mod migrate;
mod model;
mod names;
mod resolve;
mod yaml;

pub use create::create_project;
pub use error::{FileError, ProjectError};
pub use load::{has_merge_conflict_markers, load_project};
pub use migrate::{Migration, MigrationOutcome, migrate, migrate_with};
pub use model::{
    Environment, LoadedProject, MAX_RELATIVE_PATH_LEN, Manifest, Project, SCHEMA_VERSION,
    SecretKind, SecretSpec, Variable, VaultManifest,
};
pub use names::{check_relative_path, is_production_name, safe_file_stem, unique_file_stem};
pub use resolve::{
    DynamicSource, Resolution, ResolveContext, ResolveError, Scope, ScopeKind, SecretLookup,
    SecretLookupError, SystemDynamicSource, UsedVariable, ValueSource, resolve,
};
pub use yaml::{to_yaml, write_if_changed};
