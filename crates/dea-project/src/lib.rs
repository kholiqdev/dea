//! Reads and writes Dea project folders: a manifest, environments and the
//! list of secrets a project needs, stored as plain YAML files for git.

mod create;
mod environments;
mod error;
mod load;
mod migrate;
mod model;
mod names;
mod request;
mod request_resolve;
mod requests;
mod resolve;
mod schema;
mod secrets;
mod watch;
mod yaml;

pub use create::create_project;
pub use environments::{
    ENV_COLORS, EnvReference, delete_environment, duplicate_environment, environment_references,
    is_valid_variable_name, save_environment,
};
pub use error::{FileError, ProjectError};
pub use load::{has_merge_conflict_markers, load_project};
pub use migrate::{Migration, MigrationOutcome, migrate, migrate_with};
pub use model::{
    Environment, LoadedProject, MAX_RELATIVE_PATH_LEN, Manifest, Project, SCHEMA_VERSION,
    SecretKind, SecretSpec, Variable, VaultManifest,
};
pub use names::{check_relative_path, is_production_name, safe_file_stem, unique_file_stem};
pub use request::{
    ApiKeyLocation, Auth, Body, ClientCertificate, FolderFile, FormField, KeyValue, RawLanguage,
    RequestFile, SendOptions, TlsSettings,
};
pub use request_resolve::{
    DEFAULT_MAX_REDIRECTS, DEFAULT_TIMEOUT_MS, EffectiveOptions, FieldValue, RequestContext,
    RequestError, Resolved, ResolvedBody, ResolvedClientCertificate, ResolvedField,
    ResolvedRequest, ResolvedTls, resolve_request,
};
pub use requests::{
    Folder, LoadedRequests, RequestEntry, RequestLocation, RequestTree, TreeItem, load_requests,
    save_request,
};
pub use resolve::{
    DynamicSource, Resolution, ResolveContext, ResolveError, Scope, ScopeKind, SecretLookup,
    SecretLookupError, SystemDynamicSource, UsedVariable, ValueSource, resolve,
};
pub use schema::json_schemas;
pub use secrets::save_secret_spec;
pub use watch::{ChangeKind, ProjectChange, ProjectWatcher};
pub use yaml::{to_yaml, write_if_changed};
