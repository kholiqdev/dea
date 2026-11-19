//! The `requests/` folder: loading it as a tree and saving one request.
//!
//! The first level under `requests/` holds collections. Every folder may
//! have a `_folder.yaml`; the one at `requests/_folder.yaml` only sets the
//! order of the collections.

use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use crate::load::{io_error, parse};
use crate::request::{Body, ClientCertificate};
use crate::{
    FileError, FolderFile, Project, ProjectError, RequestFile, check_relative_path, to_yaml,
    write_if_changed,
};

const REQUESTS: &str = "requests";
const FOLDER_FILE: &str = "_folder.yaml";

/// A folder or collection of requests.
#[derive(Debug, Clone, PartialEq)]
pub struct Folder {
    /// Relative to the project root, for example `requests/shipments`.
    pub dir: PathBuf,
    pub settings: FolderFile,
    /// Its `_folder.yaml` could not be used: the settings are the defaults.
    pub broken: bool,
    pub items: Vec<TreeItem>,
}

impl Folder {
    /// The name in `_folder.yaml`, or the name of the directory.
    #[must_use]
    pub fn display_name(&self) -> String {
        self.settings
            .name
            .clone()
            .unwrap_or_else(|| dir_name(&self.dir))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TreeItem {
    Folder(Folder),
    Request(RequestEntry),
}

impl TreeItem {
    fn key(&self) -> String {
        match self {
            TreeItem::Folder(f) => dir_name(&f.dir),
            TreeItem::Request(r) => r
                .path
                .file_stem()
                .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestEntry {
    /// Relative to the project root.
    pub path: PathBuf,
    pub request: RequestFile,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RequestTree {
    pub collections: Vec<Folder>,
}

/// Where a request sits: the folders around it and the request itself.
#[derive(Debug, Clone)]
pub struct RequestLocation<'a> {
    /// From the collection to the innermost folder.
    pub folders: Vec<&'a Folder>,
    pub entry: &'a RequestEntry,
}

impl RequestTree {
    /// The request stored at `path`, relative to the project root.
    #[must_use]
    pub fn find(&self, path: &Path) -> Option<RequestLocation<'_>> {
        fn search<'a>(
            folder: &'a Folder,
            path: &Path,
            trail: &mut Vec<&'a Folder>,
        ) -> Option<&'a RequestEntry> {
            trail.push(folder);
            for item in &folder.items {
                match item {
                    TreeItem::Request(entry) if entry.path == path => return Some(entry),
                    TreeItem::Folder(inner) if path.starts_with(&inner.dir) => {
                        if let Some(found) = search(inner, path, trail) {
                            return Some(found);
                        }
                    }
                    _ => {}
                }
            }
            trail.pop();
            None
        }
        for collection in &self.collections {
            if !path.starts_with(&collection.dir) {
                continue;
            }
            let mut folders = Vec::new();
            if let Some(entry) = search(collection, path, &mut folders) {
                return Some(RequestLocation { folders, entry });
            }
        }
        None
    }

    /// Every request, folder by folder in display order.
    pub fn requests(&self) -> impl Iterator<Item = &RequestEntry> {
        fn walk<'a>(folder: &'a Folder, out: &mut Vec<&'a RequestEntry>) {
            for item in &folder.items {
                match item {
                    TreeItem::Request(entry) => out.push(entry),
                    TreeItem::Folder(inner) => walk(inner, out),
                }
            }
        }
        let mut all = Vec::new();
        for collection in &self.collections {
            walk(collection, &mut all);
        }
        all.into_iter()
    }
}

/// The request tree and the files that could not be used.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedRequests {
    pub tree: RequestTree,
    pub errors: Vec<FileError>,
}

fn dir_name(dir: &Path) -> String {
    dir.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

fn is_http_token(method: &str) -> bool {
    !method.is_empty()
        && method
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c))
}

fn is_vault_reference(key: &str) -> bool {
    key.strip_prefix("{{vault:")
        .and_then(|rest| rest.strip_suffix("}}"))
        .is_some_and(|name| {
            let name = name.trim();
            !name.is_empty() && !name.contains(['{', '}'])
        })
}

fn validate_client_certificate(cert: &ClientCertificate) -> Result<(), String> {
    if is_vault_reference(&cert.key) {
        return Ok(());
    }
    Err(
        "the client key must be a Vault reference such as {{vault:client_key}}; \
         a private key is never stored in the project"
            .into(),
    )
}

/// Checks a request. Used when loading and when saving.
fn validate_request(request: &RequestFile) -> Result<(), String> {
    if request.name.trim().is_empty() {
        return Err("a request needs a name".into());
    }
    if !is_http_token(&request.method) {
        return Err(format!(
            "\"{}\" is not an HTTP method: use letters, digits or - _ . without spaces",
            request.method
        ));
    }
    if let Some(Body::FormData { fields }) = &request.body {
        for field in fields {
            if field.value.is_some() == field.file.is_some() {
                return Err(format!(
                    "the form field \"{}\" needs either a value or a file, not both or neither",
                    field.name
                ));
            }
        }
    }
    Ok(())
}

/// Checks a folder file. `root` is `requests/_folder.yaml`, which only sets
/// the order.
fn validate_folder(settings: &FolderFile, root: bool) -> Result<(), String> {
    if root {
        let only_order = FolderFile {
            order: settings.order.clone(),
            ..FolderFile::default()
        };
        if *settings != only_order {
            return Err(
                "requests/_folder.yaml only sets the order of the collections; \
                 put other settings in the folder of a collection"
                    .into(),
            );
        }
        return Ok(());
    }
    if settings
        .name
        .as_deref()
        .is_some_and(|n| n.trim().is_empty())
    {
        return Err("a folder name cannot be blank".into());
    }
    if let Some(cert) = settings
        .tls
        .as_ref()
        .and_then(|t| t.client_certificate.as_ref())
    {
        validate_client_certificate(cert)?;
    }
    Ok(())
}

fn file_error(path: &Path, message: String) -> FileError {
    FileError {
        path: path.to_path_buf(),
        line: None,
        column: None,
        message,
        merge_conflict: false,
    }
}

fn read_parsed<T: serde::de::DeserializeOwned>(root: &Path, rel: &Path) -> Result<T, FileError> {
    let text = fs::read_to_string(root.join(rel)).map_err(|e| io_error(rel, &e))?;
    parse(rel, &text)
}

/// Whether a folder holds a `.yaml` file at any depth. Symlinks are not
/// followed.
fn has_yaml(dir: &Path) -> Result<bool, ProjectError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry.file_type()?;
        if name.starts_with('.') {
            continue;
        }
        if (kind.is_file() && name.ends_with(".yaml"))
            || (kind.is_dir() && has_yaml(&entry.path())?)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Puts the items named in `order` first, in that order; the others follow:
/// folders, then requests, each by name.
fn arrange(items: &mut Vec<TreeItem>, order: &[String]) {
    items.sort_by_key(|item| (matches!(item, TreeItem::Request(_)), item.key()));
    let mut ordered = Vec::with_capacity(items.len());
    for name in order {
        if let Some(i) = items.iter().position(|item| item.key() == *name) {
            ordered.push(items.remove(i));
        }
    }
    ordered.append(items);
    *items = ordered;
}

fn load_folder(
    root: &Path,
    dir: &Path,
    errors: &mut Vec<FileError>,
) -> Result<Option<Folder>, ProjectError> {
    if !has_yaml(&root.join(dir))? {
        return Ok(None);
    }
    let mut settings = FolderFile::default();
    let mut broken = false;
    let folder_rel = dir.join(FOLDER_FILE);
    match read_parsed::<FolderFile>(root, &folder_rel) {
        Ok(parsed) => match validate_folder(&parsed, false) {
            Ok(()) => settings = parsed,
            Err(message) => {
                broken = true;
                errors.push(file_error(&folder_rel, message));
            }
        },
        Err(e) if is_missing(root, &folder_rel) => drop(e),
        Err(e) => {
            broken = true;
            errors.push(e);
        }
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(root.join(dir))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry.file_type()?;
        if name.starts_with('.') {
            continue;
        }
        let rel = dir.join(&name);
        if kind.is_dir() {
            if let Some(inner) = load_folder(root, &rel, errors)? {
                items.push(TreeItem::Folder(inner));
            }
        } else if kind.is_file() && name.ends_with(".yaml") && name != FOLDER_FILE {
            match read_parsed::<RequestFile>(root, &rel) {
                Ok(request) => match validate_request(&request) {
                    Ok(()) => items.push(TreeItem::Request(RequestEntry { path: rel, request })),
                    Err(message) => errors.push(file_error(&rel, message)),
                },
                Err(e) => errors.push(e),
            }
        }
    }
    arrange(&mut items, &settings.order);
    Ok(Some(Folder {
        dir: dir.to_path_buf(),
        settings,
        broken,
        items,
    }))
}

fn is_missing(root: &Path, rel: &Path) -> bool {
    matches!(fs::metadata(root.join(rel)), Err(e) if e.kind() == ErrorKind::NotFound)
}

/// Loads `requests/`. A broken file is reported in `errors` and skipped,
/// except a broken `_folder.yaml`: its folder stays, marked `broken`, with
/// its requests. A missing `requests/` is an empty tree.
///
/// # Errors
///
/// Only when the folder cannot be read.
pub fn load_requests(root: &Path) -> Result<LoadedRequests, ProjectError> {
    let base = Path::new(REQUESTS);
    let mut errors = Vec::new();
    let mut collections = Vec::new();
    let entries = match fs::read_dir(root.join(base)) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Ok(LoadedRequests {
                tree: RequestTree::default(),
                errors,
            });
        }
        Err(e) => return Err(e.into()),
    };

    let mut order = Vec::new();
    let order_rel = base.join(FOLDER_FILE);
    match read_parsed::<FolderFile>(root, &order_rel) {
        Ok(parsed) => {
            if let Err(message) = validate_folder(&parsed, true) {
                errors.push(file_error(&order_rel, message));
            }
            order = parsed.order;
        }
        Err(e) if is_missing(root, &order_rel) => drop(e),
        Err(e) => errors.push(e),
    }

    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry.file_type()?;
        if name.starts_with('.') {
            continue;
        }
        let rel = base.join(&name);
        if kind.is_dir() {
            if let Some(folder) = load_folder(root, &rel, &mut errors)? {
                collections.push(folder);
            }
        } else if kind.is_file() && name.ends_with(".yaml") && name != FOLDER_FILE {
            errors.push(file_error(
                &rel,
                "a request must be inside a collection folder, for example \
                 requests/my-collection/"
                    .into(),
            ));
        }
    }
    collections.sort_by_key(|c| dir_name(&c.dir));
    let mut ordered = Vec::with_capacity(collections.len());
    for name in &order {
        if let Some(i) = collections.iter().position(|c| dir_name(&c.dir) == *name) {
            ordered.push(collections.remove(i));
        }
    }
    ordered.append(&mut collections);
    errors.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(LoadedRequests {
        tree: RequestTree {
            collections: ordered,
        },
        errors,
    })
}

fn check_request_path(path: &Path) -> Result<(), ProjectError> {
    let invalid = |message: &str| Err(ProjectError::InvalidRequest(message.into()));
    let parts: Vec<_> = path.components().collect();
    if !parts.iter().all(|c| matches!(c, Component::Normal(_))) {
        return invalid("a request path is relative to the project and has no .. parts");
    }
    if parts.first().map(|c| c.as_os_str()) != Some(REQUESTS.as_ref()) {
        return invalid("requests are stored under requests/");
    }
    if parts.len() < 3 {
        return invalid("a request must be inside a collection folder, such as requests/orders/");
    }
    let file = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    if !file.ends_with(".yaml") || file.len() == ".yaml".len() {
        return invalid("a request file name ends in .yaml");
    }
    if file == FOLDER_FILE {
        return invalid("_folder.yaml holds folder settings, not a request");
    }
    check_relative_path(path)
}

/// Writes `request` to `path`, relative to the project root, when it
/// differs from the file on disk.
///
/// # Errors
///
/// [`ProjectError::ReadOnly`]; [`ProjectError::InvalidRequest`] for a path
/// outside `requests/<collection>/`, one that does not end in `.yaml` or is
/// `_folder.yaml`, or a request that could not be loaded;
/// [`ProjectError::MergeConflict`] when the file has conflict markers.
pub fn save_request(
    project: &Project,
    path: &Path,
    request: &RequestFile,
) -> Result<(), ProjectError> {
    if project.read_only {
        return Err(ProjectError::ReadOnly);
    }
    check_request_path(path)?;
    validate_request(request).map_err(ProjectError::InvalidRequest)?;
    write_if_changed(&project.root.join(path), &to_yaml(request)?)?;
    Ok(())
}
