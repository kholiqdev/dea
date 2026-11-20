//! File operations on the request tree: create, rename, duplicate, move,
//! delete and restore requests and folders, and save folder settings.
//!
//! Paths are relative to the project root. A folder's order lives in its
//! `_folder.yaml`; every operation that adds, renames or copies an item
//! writes the full displayed order back, so the item lands where the user
//! sees it.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::requests::{
    FOLDER_FILE, REQUESTS, check_request_path, dir_name, is_missing, read_parsed, validate_folder,
    validate_request,
};
use crate::{
    FolderFile, Project, ProjectError, RequestFile, check_relative_path, load_requests,
    safe_file_stem, to_yaml, unique_file_stem, write_if_changed,
};

fn invalid(message: impl Into<String>) -> ProjectError {
    ProjectError::InvalidRequest(message.into())
}

fn guard(project: &Project) -> Result<(), ProjectError> {
    if project.read_only {
        return Err(ProjectError::ReadOnly);
    }
    Ok(())
}

/// A folder path: `requests` itself when `allow_root`, else a folder below it.
fn check_dir_path(dir: &Path, allow_root: bool) -> Result<(), ProjectError> {
    let parts: Vec<_> = dir.components().collect();
    if !parts.iter().all(|c| matches!(c, Component::Normal(_)))
        || parts.first().map(|c| c.as_os_str()) != Some(REQUESTS.as_ref())
    {
        return Err(invalid("a folder is inside requests/"));
    }
    if parts.len() < 2 && !allow_root {
        return Err(invalid("requests/ itself cannot be changed this way"));
    }
    check_relative_path(dir)
}

fn require_exists(project: &Project, path: &Path) -> Result<(), ProjectError> {
    if project.root.join(path).exists() {
        Ok(())
    } else {
        Err(invalid(format!("{} does not exist", path.display())))
    }
}

fn is_root(dir: &Path) -> bool {
    dir == Path::new(REQUESTS)
}

fn key_of(path: &Path) -> String {
    if path.extension().is_some_and(|e| e == "yaml") {
        path.file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
    } else {
        dir_name(path)
    }
}

/// The order a folder shows now: the keys of its items as the tree lists
/// them, whether or not `order` names them.
pub(crate) fn displayed_order(project: &Project, dir: &Path) -> Result<Vec<String>, ProjectError> {
    let tree = load_requests(&project.root)?.tree;
    if is_root(dir) {
        return Ok(tree.collections.iter().map(|c| dir_name(&c.dir)).collect());
    }
    Ok(tree
        .folder(dir)
        .map(|f| f.items.iter().map(crate::TreeItem::key).collect())
        .unwrap_or_default())
}

fn read_folder_file(project: &Project, dir: &Path) -> Result<FolderFile, ProjectError> {
    let rel = dir.join(FOLDER_FILE);
    match read_parsed::<FolderFile>(&project.root, &rel) {
        Ok(settings) => Ok(settings),
        Err(_) if is_missing(&project.root, &rel) => Ok(FolderFile::default()),
        Err(e) => Err(invalid(e.to_string())),
    }
}

/// Replaces the `order` of `dir` with `order`, keeping its other settings.
pub(crate) fn write_order(
    project: &Project,
    dir: &Path,
    order: Vec<String>,
) -> Result<(), ProjectError> {
    let settings = FolderFile {
        order,
        ..read_folder_file(project, dir)?
    };
    save_folder(project, dir, &settings)
}

/// The keys of the files and folders in `dir`, for choosing a free name.
fn sibling_keys(project: &Project, dir: &Path, except: Option<&str>) -> Vec<String> {
    let Ok(entries) = fs::read_dir(project.root.join(dir)) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let kind = entry.file_type().ok()?;
            if kind.is_dir() {
                Some(name)
            } else if name != FOLDER_FILE {
                name.strip_suffix(".yaml").map(str::to_owned)
            } else {
                None
            }
        })
        .filter(|key| Some(key.as_str()) != except)
        .collect()
}

fn check_name(name: &str, what: &str) -> Result<(), ProjectError> {
    if name.trim().is_empty() {
        return Err(invalid(format!("a {what} needs a name")));
    }
    Ok(())
}

/// Moves `from` to `to`; a move that only changes the case goes through a
/// temporary name, because macOS and Windows treat both names as one.
fn move_path(from: &Path, to: &Path) -> Result<(), ProjectError> {
    let case_only = from != to
        && from.parent() == to.parent()
        && from
            .file_name()
            .zip(to.file_name())
            .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b));
    if case_only {
        let tmp = from.with_file_name(format!(".dea-rename-{}", std::process::id()));
        fs::rename(from, &tmp)?;
        fs::rename(&tmp, to)?;
    } else {
        fs::rename(from, to)?;
    }
    Ok(())
}

/// Creates a request in `folder` and puts it last in the folder's order.
/// Returns its path.
///
/// # Errors
///
/// [`ProjectError::ReadOnly`]; [`ProjectError::InvalidRequest`] for a
/// folder outside `requests/<collection>/`, one that does not exist or a
/// blank name; [`ProjectError::PathTooLong`].
pub fn create_request(
    project: &Project,
    folder: &Path,
    name: &str,
) -> Result<PathBuf, ProjectError> {
    guard(project)?;
    check_dir_path(folder, false)?;
    require_exists(project, folder)?;
    check_name(name, "request")?;
    let stem = unique_file_stem(&sibling_keys(project, folder, None), &safe_file_stem(name));
    let path = folder.join(format!("{stem}.yaml"));
    check_relative_path(&path)?;
    let request = RequestFile {
        name: name.to_owned(),
        method: "GET".into(),
        url: String::new(),
        path_variables: Vec::new(),
        params: Vec::new(),
        headers: Vec::new(),
        body: None,
        auth: crate::Auth::Inherit,
        options: crate::SendOptions::default(),
    };
    let mut order = displayed_order(project, folder)?;
    write_if_changed(&project.root.join(&path), &to_yaml(&request)?)?;
    order.push(stem);
    write_order(project, folder, order)?;
    Ok(path)
}

/// Creates a folder in `parent`, which is `requests` for a collection, and
/// puts it last in the order of `parent`. Returns its path.
///
/// # Errors
///
/// Like [`create_request`].
pub fn create_folder(
    project: &Project,
    parent: &Path,
    name: &str,
) -> Result<PathBuf, ProjectError> {
    guard(project)?;
    check_dir_path(parent, true)?;
    if !is_root(parent) {
        require_exists(project, parent)?;
    }
    check_name(name, "folder")?;
    let stem = unique_file_stem(&sibling_keys(project, parent, None), &safe_file_stem(name));
    let dir = parent.join(&stem);
    check_relative_path(&dir.join(FOLDER_FILE))?;
    let settings = FolderFile {
        name: Some(name.to_owned()),
        ..FolderFile::default()
    };
    let mut order = displayed_order(project, parent)?;
    write_if_changed(
        &project.root.join(&dir).join(FOLDER_FILE),
        &to_yaml(&settings)?,
    )?;
    order.push(stem);
    write_order(project, parent, order)?;
    Ok(dir)
}

/// Renames a request or folder: its `name` changes, and so does its file or
/// directory name unless the new name maps to the same one. Returns the new
/// path. The key in the `order` of the parent folder follows.
///
/// # Errors
///
/// [`ProjectError::ReadOnly`]; [`ProjectError::InvalidRequest`] for a path
/// outside `requests/`, one that does not exist or a blank name.
pub fn rename_item(project: &Project, path: &Path, name: &str) -> Result<PathBuf, ProjectError> {
    guard(project)?;
    let is_folder = item_is_folder(project, path)?;
    check_name(name, if is_folder { "folder" } else { "request" })?;
    let root = &project.root;
    let parent = path.parent().unwrap_or(Path::new(REQUESTS));
    let old_key = key_of(path);

    let (mut request, mut settings) = (None, None);
    if is_folder {
        let current = read_folder_file(project, path)?;
        if current.name.as_deref().unwrap_or(&old_key) == name {
            return Ok(path.to_path_buf());
        }
        settings = Some(current);
    } else {
        let current: RequestFile = read_parsed(root, path).map_err(|e| invalid(e.to_string()))?;
        if current.name == name {
            return Ok(path.to_path_buf());
        }
        request = Some(current);
    }

    let stem = unique_file_stem(
        &sibling_keys(project, parent, Some(&old_key)),
        &safe_file_stem(name),
    );
    let target = if is_folder {
        parent.join(&stem)
    } else {
        parent.join(format!("{stem}.yaml"))
    };
    check_relative_path(&target)?;
    let mut order = displayed_order(project, parent)?;

    if target != path {
        move_path(&root.join(path), &root.join(&target))?;
    }
    if let Some(mut request) = request {
        request.name = name.to_owned();
        validate_request(&request).map_err(invalid)?;
        write_if_changed(&root.join(&target), &to_yaml(&request)?)?;
    }
    if let Some(mut settings) = settings {
        settings.name = Some(name.to_owned());
        validate_folder(&settings, false).map_err(invalid)?;
        write_if_changed(&root.join(&target).join(FOLDER_FILE), &to_yaml(&settings)?)?;
    }
    if stem != old_key
        && let Some(slot) = order.iter_mut().find(|k| **k == old_key)
    {
        *slot = stem;
        write_order(project, parent, order)?;
    }
    Ok(target)
}

/// Copies a request, or a folder with everything in it, as `<name> copy`,
/// and places the copy right after the original. Returns its path.
///
/// # Errors
///
/// Like [`rename_item`].
pub fn duplicate_item(project: &Project, path: &Path) -> Result<PathBuf, ProjectError> {
    guard(project)?;
    let is_folder = item_is_folder(project, path)?;
    let root = &project.root;
    let parent = path.parent().unwrap_or(Path::new(REQUESTS));
    let old_key = key_of(path);
    let taken = sibling_keys(project, parent, None);

    let target = if is_folder {
        let mut settings = read_folder_file(project, path)?;
        let copy_name = format!("{} copy", settings.name.as_deref().unwrap_or(&old_key));
        let stem = unique_file_stem(&taken, &safe_file_stem(&copy_name));
        let target = parent.join(&stem);
        check_relative_path(&target.join(FOLDER_FILE))?;
        copy_dir(&root.join(path), &root.join(&target))?;
        settings.name = Some(copy_name);
        write_if_changed(&root.join(&target).join(FOLDER_FILE), &to_yaml(&settings)?)?;
        target
    } else {
        let mut request: RequestFile =
            read_parsed(root, path).map_err(|e| invalid(e.to_string()))?;
        request.name = format!("{} copy", request.name);
        let stem = unique_file_stem(&taken, &safe_file_stem(&request.name));
        let target = parent.join(format!("{stem}.yaml"));
        check_relative_path(&target)?;
        write_if_changed(&root.join(&target), &to_yaml(&request)?)?;
        target
    };

    let mut order = displayed_order(project, parent)?;
    order.retain(|k| *k != key_of(&target));
    let at = order
        .iter()
        .position(|k| *k == old_key)
        .map_or(order.len(), |i| i + 1);
    order.insert(at, key_of(&target));
    write_order(project, parent, order)?;
    Ok(target)
}

/// Moves a request or folder into the folder `to` (`requests` for a
/// collection) at position `index` of its order, counted after the item left
/// its old place; an `index` past the end puts it last. A name taken in `to`
/// gets a number. Returns the new path.
///
/// # Errors
///
/// Like [`rename_item`]; also [`ProjectError::InvalidRequest`] for a request
/// moved to `requests` itself, or a folder moved into itself or below itself.
pub fn move_item(
    project: &Project,
    path: &Path,
    to: &Path,
    index: usize,
) -> Result<PathBuf, ProjectError> {
    guard(project)?;
    let is_folder = item_is_folder(project, path)?;
    check_dir_path(to, true)?;
    if is_folder {
        if to.starts_with(path) {
            return Err(invalid("a folder cannot move into itself"));
        }
    } else if is_root(to) {
        return Err(invalid("a request is inside a collection"));
    }
    if !is_root(to) {
        require_exists(project, to)?;
    }
    let from = path.parent().unwrap_or(Path::new(REQUESTS));
    let same = from == to;
    let old_key = key_of(path);
    let stem = unique_file_stem(
        &sibling_keys(project, to, same.then_some(old_key.as_str())),
        &old_key,
    );
    let target = if is_folder {
        to.join(&stem)
    } else {
        to.join(format!("{stem}.yaml"))
    };
    check_relative_path(&target)?;

    // Read both orders and both folder files before touching the disk.
    let mut from_order = displayed_order(project, from)?;
    from_order.retain(|k| *k != old_key);
    read_folder_file(project, from)?;
    let to_order = if same {
        None
    } else {
        read_folder_file(project, to)?;
        Some(displayed_order(project, to)?)
    };

    if target != path {
        move_between(&project.root.join(path), &project.root.join(&target))?;
    }
    let place = |order: &mut Vec<String>| {
        order.retain(|k| *k != stem);
        order.insert(index.min(order.len()), stem.clone());
    };
    match to_order {
        None => {
            place(&mut from_order);
            write_order(project, from, from_order)?;
        }
        Some(mut to_order) => {
            write_order(project, from, from_order)?;
            place(&mut to_order);
            write_order(project, to, to_order)?;
        }
    }
    Ok(target)
}

/// A request or folder taken off the disk, kept whole in memory so that
/// [`restore_item`] can put it back.
#[derive(Clone)]
pub struct DeletedItem {
    /// Where the item was, relative to the project root.
    pub path: PathBuf,
    /// Its position in the order of its folder.
    pub index: usize,
    files: Vec<(PathBuf, Vec<u8>)>,
}

impl std::fmt::Debug for DeletedItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeletedItem")
            .field("path", &self.path)
            .field("index", &self.index)
            .field("files", &self.files.len())
            .finish()
    }
}

/// Deletes a request, or a folder with everything in it, and takes its key
/// out of the order of its folder. The files are kept in the result.
///
/// # Errors
///
/// Like [`rename_item`].
pub fn delete_item(project: &Project, path: &Path) -> Result<DeletedItem, ProjectError> {
    guard(project)?;
    let is_folder = item_is_folder(project, path)?;
    let parent = path.parent().unwrap_or(Path::new(REQUESTS));
    let key = key_of(path);
    read_folder_file(project, parent)?;
    let mut order = displayed_order(project, parent)?;
    let index = order.iter().position(|k| *k == key).unwrap_or(order.len());
    order.retain(|k| *k != key);

    let mut files = Vec::new();
    collect_files(&project.root, path, &mut files)?;
    if is_folder {
        fs::remove_dir_all(project.root.join(path))?;
    } else {
        fs::remove_file(project.root.join(path))?;
    }
    write_order(project, parent, order)?;
    Ok(DeletedItem {
        path: path.to_path_buf(),
        index,
        files,
    })
}

/// Puts back what [`delete_item`] took: every file, and its key at the old
/// position in the order of its folder (last if the folder got shorter).
///
/// # Errors
///
/// [`ProjectError::ReadOnly`]; [`ProjectError::InvalidRequest`] when the
/// path exists again (`<name> already exists`) or its folder is gone.
pub fn restore_item(project: &Project, item: &DeletedItem) -> Result<(), ProjectError> {
    guard(project)?;
    check_dir_path(&item.path, false)?;
    let root = &project.root;
    if root.join(&item.path).exists() {
        return Err(invalid(format!("{} already exists", dir_name(&item.path))));
    }
    let parent = item.path.parent().unwrap_or(Path::new(REQUESTS));
    if !is_root(parent) {
        require_exists(project, parent)?;
    }
    read_folder_file(project, parent)?;
    let key = key_of(&item.path);
    let mut order = displayed_order(project, parent)?;
    order.retain(|k| *k != key);
    order.insert(item.index.min(order.len()), key);

    for (rel, bytes) in &item.files {
        let file = root.join(rel);
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(file, bytes)?;
    }
    write_order(project, parent, order)
}

/// Reads every file at or below `rel` into `files`, with paths relative to
/// `root`.
fn collect_files(
    root: &Path,
    rel: &Path,
    files: &mut Vec<(PathBuf, Vec<u8>)>,
) -> Result<(), ProjectError> {
    let full = root.join(rel);
    if full.is_dir() {
        for entry in fs::read_dir(&full)? {
            collect_files(root, &rel.join(entry?.file_name()), files)?;
        }
    } else {
        files.push((rel.to_path_buf(), fs::read(full)?));
    }
    Ok(())
}

/// Renames `from` to `to`; across volumes it copies and then removes.
fn move_between(from: &Path, to: &Path) -> Result<(), ProjectError> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            if from.is_dir() {
                copy_dir(from, to)?;
                fs::remove_dir_all(from)?;
            } else {
                fs::copy(from, to)?;
                fs::remove_file(from)?;
            }
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// Whether `path` is a folder (else a request), after checking that it is an
/// existing item under `requests/<collection>/`.
fn item_is_folder(project: &Project, path: &Path) -> Result<bool, ProjectError> {
    check_dir_path(path, false)?;
    require_exists(project, path)?;
    if project.root.join(path).is_dir() {
        Ok(true)
    } else {
        check_request_path(path)?;
        Ok(false)
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), ProjectError> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Reads the settings of the folder at `dir` (`requests` for the order of
/// the collections): the defaults when it has no `_folder.yaml`.
///
/// # Errors
///
/// [`ProjectError::InvalidRequest`] for a path outside `requests/`, a
/// folder that does not exist or a `_folder.yaml` that cannot be read.
pub fn read_folder(project: &Project, dir: &Path) -> Result<FolderFile, ProjectError> {
    check_dir_path(dir, true)?;
    if !is_root(dir) {
        require_exists(project, dir)?;
    }
    read_folder_file(project, dir)
}

/// Saves the settings of the folder at `dir` (`requests` for the order of
/// the collections, which takes nothing else), checked as when loading.
///
/// # Errors
///
/// [`ProjectError::ReadOnly`]; [`ProjectError::InvalidRequest`] for a path
/// outside `requests/`, a folder that does not exist or settings that could
/// not be loaded; [`ProjectError::MergeConflict`].
pub fn save_folder(
    project: &Project,
    dir: &Path,
    settings: &FolderFile,
) -> Result<(), ProjectError> {
    guard(project)?;
    check_dir_path(dir, true)?;
    if !is_root(dir) {
        require_exists(project, dir)?;
    }
    validate_folder(settings, is_root(dir)).map_err(invalid)?;
    let file = dir.join(FOLDER_FILE);
    check_relative_path(&file)?;
    write_if_changed(&project.root.join(file), &to_yaml(settings)?)?;
    Ok(())
}
