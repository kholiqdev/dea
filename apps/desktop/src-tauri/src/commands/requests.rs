//! The request files of a window's project: the tree, reading and saving
//! requests and folders, the tree operations with a short Undo for delete,
//! inspection, and picking files for a request. Paths cross as text
//! relative to the project root, with `/`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use dea_engine::projects::portable_path;
use dea_engine::{FileRef, Inspection};
use dea_project::{
    DeletedItem, FolderFile, LoadedRequests, Project, ProjectError, RequestError, RequestFile,
    RequestTreeInfo, load_project, load_requests, tree_info,
};
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::commands::project::writable;
use crate::error::{IpcError, IpcErrorKind};
use crate::session::{Refusal, Session};
use crate::state::{AppState, e2e_picked_path};

/// A path from the UI, relative to the project root with `/`.
pub(crate) fn rel(path: &str) -> PathBuf {
    path.split('/').collect()
}

/// The window's session, for a command that only reads.
pub(crate) fn session_of(app: &AppHandle, window: &str) -> Result<Session, IpcError> {
    let session = app.state::<AppState>().sessions().get(window).cloned();
    Ok(session.ok_or(Refusal::NoProject)?)
}

/// The request tree of the window's project, read the first time.
pub(crate) fn tree_of(
    app: &AppHandle,
    window: &str,
    session: &Session,
) -> Result<Arc<LoadedRequests>, IpcError> {
    Ok(app
        .state::<AppState>()
        .tree(window, &session.root, || load_requests(&session.root))?)
}

/// The request at `path` in `loaded`, or why it is not there.
fn request_in(loaded: &LoadedRequests, path: &str) -> Result<RequestFile, IpcError> {
    let path = rel(path);
    if let Some(found) = loaded.tree.find(&path) {
        return Ok(found.entry.request.clone());
    }
    if let Some(error) = loaded.errors.iter().find(|e| e.path == path) {
        let kind = if error.merge_conflict {
            IpcErrorKind::MergeConflict
        } else {
            IpcErrorKind::Invalid
        };
        return Err(IpcError {
            kind,
            message: error.to_string(),
        });
    }
    Err(RequestError::NotFound(path).into())
}

/// Runs `change` on the window's project off the main thread, then drops
/// the window's request tree so the next one shows the change at once.
async fn change<T: Send + 'static>(
    app: &AppHandle,
    window: &WebviewWindow,
    change: impl FnOnce(&Project) -> Result<T, ProjectError> + Send + 'static,
) -> Result<T, IpcError> {
    change_in(app, window, writable(app, window.label())?, change).await
}

/// [`change`] on the project of `session`, a writable session of the
/// window.
async fn change_in<T: Send + 'static>(
    app: &AppHandle,
    window: &WebviewWindow,
    session: Session,
    change: impl FnOnce(&Project) -> Result<T, ProjectError> + Send + 'static,
) -> Result<T, IpcError> {
    let app = app.clone();
    let label = window.label().to_string();
    spawn_blocking(move || {
        let project = load_project(&session.root)?.project;
        let changed = change(&project);
        // A failed operation may still have written some files.
        app.state::<AppState>().forget_tree(&label);
        Ok(changed?)
    })
    .await?
}

/// The item `window` deleted as `id` from the project at `root`, while Undo
/// can still restore it.
fn kept_item(
    state: &AppState,
    window: &str,
    root: &Path,
    id: u64,
    now: Instant,
) -> Result<DeletedItem, IpcError> {
    let item = state.deleted_item(window, root, id, now);
    item.ok_or_else(|| IpcError {
        kind: IpcErrorKind::Missing,
        message: "the deleted item is no longer kept".into(),
    })
}

#[tauri::command]
pub async fn request_tree(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<RequestTreeInfo, IpcError> {
    let session = session_of(&app, window.label())?;
    spawn_blocking(move || tree_of(&app, window.label(), &session).map(|loaded| tree_info(&loaded)))
        .await?
}

#[tauri::command]
pub async fn read_request(
    app: AppHandle,
    window: WebviewWindow,
    path: String,
) -> Result<RequestFile, IpcError> {
    let session = session_of(&app, window.label())?;
    spawn_blocking(move || {
        tree_of(&app, window.label(), &session).and_then(|loaded| request_in(&loaded, &path))
    })
    .await?
}

#[tauri::command]
pub async fn save_request(
    app: AppHandle,
    window: WebviewWindow,
    path: String,
    request: RequestFile,
) -> Result<(), IpcError> {
    change(&app, &window, move |project| {
        dea_project::save_request(project, &rel(&path), &request)
    })
    .await
}

#[tauri::command]
pub async fn read_folder(
    app: AppHandle,
    window: WebviewWindow,
    path: String,
) -> Result<FolderFile, IpcError> {
    let session = session_of(&app, window.label())?;
    spawn_blocking(move || {
        let project = load_project(&session.root)?.project;
        Ok(dea_project::read_folder(&project, &rel(&path))?)
    })
    .await?
}

#[tauri::command]
pub async fn save_folder(
    app: AppHandle,
    window: WebviewWindow,
    path: String,
    settings: FolderFile,
) -> Result<(), IpcError> {
    change(&app, &window, move |project| {
        dea_project::save_folder(project, &rel(&path), &settings)
    })
    .await
}

#[tauri::command]
pub async fn create_request(
    app: AppHandle,
    window: WebviewWindow,
    folder: String,
    name: String,
) -> Result<String, IpcError> {
    let created = change(&app, &window, move |project| {
        dea_project::create_request(project, &rel(&folder), &name)
    });
    Ok(portable_path(&created.await?))
}

#[tauri::command]
pub async fn create_folder(
    app: AppHandle,
    window: WebviewWindow,
    parent: String,
    name: String,
) -> Result<String, IpcError> {
    let created = change(&app, &window, move |project| {
        dea_project::create_folder(project, &rel(&parent), &name)
    });
    Ok(portable_path(&created.await?))
}

#[tauri::command]
pub async fn rename_request_item(
    app: AppHandle,
    window: WebviewWindow,
    path: String,
    name: String,
) -> Result<String, IpcError> {
    let renamed = change(&app, &window, move |project| {
        dea_project::rename_item(project, &rel(&path), &name)
    });
    Ok(portable_path(&renamed.await?))
}

#[tauri::command]
pub async fn duplicate_request_item(
    app: AppHandle,
    window: WebviewWindow,
    path: String,
) -> Result<String, IpcError> {
    let copy = change(&app, &window, move |project| {
        dea_project::duplicate_item(project, &rel(&path))
    });
    Ok(portable_path(&copy.await?))
}

/// Moves a request or folder into `folder` at `index` of its order.
#[tauri::command]
pub async fn move_request_item(
    app: AppHandle,
    window: WebviewWindow,
    path: String,
    folder: String,
    index: u32,
) -> Result<String, IpcError> {
    let index = usize::try_from(index).unwrap_or(usize::MAX);
    let moved = change(&app, &window, move |project| {
        dea_project::move_item(project, &rel(&path), &rel(&folder), index)
    });
    Ok(portable_path(&moved.await?))
}

/// Deletes a request or folder and returns the id that restores it while
/// Undo is offered.
#[tauri::command]
pub async fn delete_request_item(
    app: AppHandle,
    window: WebviewWindow,
    path: String,
) -> Result<u64, IpcError> {
    let session = writable(&app, window.label())?;
    let root = session.root.clone();
    let item = change_in(&app, &window, session, move |project| {
        dea_project::delete_item(project, &rel(&path))
    })
    .await?;
    let state = app.state::<AppState>();
    Ok(state.keep_deleted(window.label(), &root, item, Instant::now()))
}

#[tauri::command]
pub async fn restore_request_item(
    app: AppHandle,
    window: WebviewWindow,
    id: u64,
) -> Result<(), IpcError> {
    let session = writable(&app, window.label())?;
    let state = app.state::<AppState>();
    let item = kept_item(&state, window.label(), &session.root, id, Instant::now())?;
    // Restored into the project it was deleted from, which `session` is.
    change_in(&app, &window, session, move |project| {
        dea_project::restore_item(project, &item)
    })
    .await?;
    app.state::<AppState>().forget_deleted(id);
    Ok(())
}

/// What the editor shows about `request`, unsaved, as if it were at
/// `path`, in the window's active environment. Never reads the Vault.
#[tauri::command]
pub async fn inspect_request(
    app: AppHandle,
    window: WebviewWindow,
    path: String,
    request: RequestFile,
) -> Result<Inspection, IpcError> {
    let session = session_of(&app, window.label())?;
    spawn_blocking(move || {
        let project = load_project(&session.root)?.project;
        let loaded = tree_of(&app, window.label(), &session)?;
        let inspection = app.state::<AppState>().requests.inspect(
            &project,
            &loaded.tree,
            &rel(&path),
            &request,
            session.active_environment.as_deref(),
        )?;
        Ok(inspection)
    })
    .await?
}

/// Asks for a file for a request: a file inside the project is stored as
/// its path, one outside it as a `local:` reference. `None` when the
/// person cancels.
#[tauri::command]
pub async fn pick_request_file(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Option<FileRef>, IpcError> {
    let session = session_of(&app, window.label())?;
    let picked = match e2e_picked_path() {
        Some(path) => Some(PathBuf::from(path)),
        None => spawn_blocking(move || {
            window
                .dialog()
                .file()
                .set_parent(&window)
                .blocking_pick_file()
        })
        .await?
        .and_then(|p| p.into_path().ok()),
    };
    let Some(picked) = picked else {
        return Ok(None);
    };
    let store = app.state::<AppState>().store.clone();
    let chosen =
        spawn_blocking(move || FileRef::picked(&store, &session.root, &session.scope, &picked));
    Ok(Some(chosen.await??))
}

/// A file field's `value` as the editor shows it.
#[tauri::command]
pub async fn describe_file_ref(
    app: AppHandle,
    window: WebviewWindow,
    value: String,
) -> Result<FileRef, IpcError> {
    let session = session_of(&app, window.label())?;
    let store = app.state::<AppState>().store.clone();
    Ok(
        spawn_blocking(move || FileRef::describe(&store, &session.root, &session.scope, &value))
            .await?,
    )
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use dea_engine::FileRef;
    use dea_engine::vault::VaultService;
    use dea_engine::{UseError, VaultProvider};
    use dea_project::{
        ChangeKind, ProjectChange, create_project, delete_item, is_local_ref, load_requests,
    };
    use serde_json::json;

    use super::*;
    use crate::state::{AppState, take_line};
    use crate::vault::{NoWindows, UiPrompter};

    /// No test sends a request: opening the Vault fails the test.
    struct NeverOpened;

    impl VaultProvider for NeverOpened {
        fn open(&self) -> Result<Arc<VaultService>, UseError> {
            panic!("the Vault was opened")
        }
    }

    fn state_in(app_data: &Path) -> AppState {
        AppState::new(
            app_data.to_path_buf(),
            UiPrompter::new(NoWindows),
            NeverOpened,
        )
    }

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// A project with `requests/shipments/list.yaml` and `by-id.yaml`.
    fn shipments(root: &Path) -> Project {
        let project = create_project(root, "harbor").unwrap();
        for (file, name) in [("list", "List shipments"), ("by-id", "Shipment by ID")] {
            write(
                root,
                &format!("requests/shipments/{file}.yaml"),
                &format!("name: {name}\nmethod: GET\nurl: /x\n"),
            );
        }
        project
    }

    /// Gives `window` the project at `root`, as opening it does.
    fn open_in(state: &AppState, window: &str, root: &Path) {
        let scope = root.to_string_lossy().into_owned();
        state.sessions().bind(window, root.to_path_buf(), scope);
        state.forget_window(window);
    }

    fn change(path: &str) -> ProjectChange {
        ProjectChange {
            path: path.into(),
            kind: ChangeKind::Modified,
        }
    }

    #[test]
    fn tree_is_cached_until_requests_change() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("p");
        shipments(&root);
        let state = state_in(&dir.path().join("app-data"));
        open_in(&state, "main", &root);
        open_in(&state, "project-1", &root);
        let loads = Cell::new(0);
        let load = || {
            loads.set(loads.get() + 1);
            load_requests(&root)
        };

        let first = state.tree("main", &root, load).unwrap();
        let second = state.tree("main", &root, load).unwrap();
        assert_eq!(loads.get(), 1);
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.tree.requests().count(), 2);

        // Another window keeps a tree of its own.
        state.tree("project-1", &root, load).unwrap();
        assert_eq!(loads.get(), 2);

        // Only a change under requests/ drops the tree.
        state.requests_changed("main", &[change("environments/staging.yaml")]);
        state.tree("main", &root, load).unwrap();
        assert_eq!(loads.get(), 2);

        state.requests_changed(
            "main",
            &[
                change("environments/staging.yaml"),
                change("requests/x.yaml"),
            ],
        );
        state.tree("main", &root, load).unwrap();
        assert_eq!(loads.get(), 3);
        state.tree("project-1", &root, load).unwrap();
        assert_eq!(loads.get(), 3);
    }

    #[test]
    fn a_tree_read_for_another_project_is_not_kept() {
        let dir = tempfile::tempdir().unwrap();
        let (harbor, fleet) = (dir.path().join("harbor"), dir.path().join("fleet"));
        shipments(&harbor);
        create_project(&fleet, "fleet").unwrap();
        write(
            &fleet,
            "requests/trucks/list.yaml",
            "name: Trucks\nmethod: GET\nurl: /t\n",
        );
        let state = state_in(&dir.path().join("app-data"));
        let (harbor_loads, fleet_loads) = (Cell::new(0), Cell::new(0));
        let load_harbor = || {
            harbor_loads.set(harbor_loads.get() + 1);
            load_requests(&harbor)
        };
        let load_fleet = || {
            fleet_loads.set(fleet_loads.get() + 1);
            load_requests(&fleet)
        };
        open_in(&state, "main", &harbor);
        state.tree("main", &harbor, load_harbor).unwrap();

        // The window opens fleet while a command still holds the harbor
        // session: the harbor tree it reads is not kept for the window.
        open_in(&state, "main", &fleet);
        let stale = state.tree("main", &harbor, load_harbor).unwrap();
        assert_eq!(stale.tree.requests().count(), 2);
        let fresh = state.tree("main", &fleet, load_fleet).unwrap();
        let names: Vec<&str> = fresh
            .tree
            .requests()
            .map(|r| r.request.name.as_str())
            .collect();
        assert_eq!(names, ["Trucks"]);
        state.tree("main", &fleet, load_fleet).unwrap();
        assert_eq!(fleet_loads.get(), 1);

        // Asking with the harbor root never gets the fleet tree.
        state.tree("main", &harbor, load_harbor).unwrap();
        assert_eq!(harbor_loads.get(), 3);
    }

    #[test]
    fn a_tree_changed_while_loading_is_not_kept() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("p");
        shipments(&root);
        let state = state_in(&dir.path().join("app-data"));
        open_in(&state, "main", &root);
        let loads = Cell::new(0);
        let load = || {
            loads.set(loads.get() + 1);
            load_requests(&root)
        };

        // The watcher reports a change while the tree is read.
        state
            .tree("main", &root, || {
                state.requests_changed("main", &[change("requests/x.yaml")]);
                load()
            })
            .unwrap();
        state.tree("main", &root, load).unwrap();
        state.tree("main", &root, load).unwrap();
        assert_eq!(loads.get(), 2);
    }

    #[test]
    fn an_item_deleted_in_another_project_is_not_restored() {
        let dir = tempfile::tempdir().unwrap();
        let (harbor, fleet) = (dir.path().join("harbor"), dir.path().join("fleet"));
        let project = shipments(&harbor);
        let state = state_in(&dir.path().join("app-data"));
        let list = delete_item(&project, Path::new("requests/shipments/list.yaml")).unwrap();
        let now = Instant::now();

        // Deleted in harbor; the window has opened fleet since.
        let id = state.keep_deleted("main", &harbor, list, now);

        let refused = kept_item(&state, "main", &fleet, id, now).unwrap_err();
        assert_eq!(
            serde_json::to_value(&refused).unwrap(),
            json!({ "kind": "missing", "message": "the deleted item is no longer kept" })
        );
        assert!(kept_item(&state, "main", &harbor, id, now).is_ok());
    }

    #[test]
    fn a_request_is_read_from_the_tree_or_says_why_it_is_not_there() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("p");
        shipments(&root);
        write(&root, "requests/shipments/broken.yaml", "name: [\n");
        let loaded = load_requests(&root).unwrap();

        let list = request_in(&loaded, "requests/shipments/list.yaml").unwrap();
        assert_eq!(list.name, "List shipments");
        let broken = request_in(&loaded, "requests/shipments/broken.yaml").unwrap_err();
        assert_eq!(broken.kind, IpcErrorKind::Invalid);
        assert!(
            broken.message.starts_with("requests/shipments/broken.yaml"),
            "{}",
            broken.message
        );
        let gone = request_in(&loaded, "requests/shipments/gone.yaml").unwrap_err();
        assert_eq!(gone.kind, IpcErrorKind::Missing);
    }

    #[test]
    fn deleted_items_expire_after_fifteen_seconds() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("p");
        let project = shipments(&root);
        let state = state_in(&dir.path().join("app-data"));
        let list = delete_item(&project, Path::new("requests/shipments/list.yaml")).unwrap();
        let by_id = delete_item(&project, Path::new("requests/shipments/by-id.yaml")).unwrap();
        let start = Instant::now();
        let seconds = |n| start + Duration::from_secs(n);

        let first = state.keep_deleted("main", &root, list, start);
        let second = state.keep_deleted("main", &root, by_id, seconds(10));
        assert_ne!(first, second);

        let kept = kept_item(&state, "main", &root, first, seconds(14)).unwrap();
        assert_eq!(kept.path, Path::new("requests/shipments/list.yaml"));
        // Another window cannot restore it.
        assert!(kept_item(&state, "project-1", &root, first, seconds(14)).is_err());

        // 16 s after the delete the first is gone; the second was kept 6 s
        // ago.
        let gone = kept_item(&state, "main", &root, first, seconds(16)).unwrap_err();
        assert_eq!(
            serde_json::to_value(&gone).unwrap(),
            json!({ "kind": "missing", "message": "the deleted item is no longer kept" })
        );
        assert!(kept_item(&state, "main", &root, second, seconds(16)).is_ok());
        // It was dropped, not only hidden.
        assert!(kept_item(&state, "main", &root, first, start).is_err());

        state.forget_deleted(second);
        assert!(kept_item(&state, "main", &root, second, seconds(16)).is_err());
    }

    #[test]
    fn outside_file_becomes_a_local_ref() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("p");
        let project = create_project(&root, "harbor").unwrap();
        let scope = project.vault_scope();
        let outside = tempfile::tempdir().unwrap();
        let file = outside.path().join("upload.bin");
        fs::write(&file, b"\x00\x01").unwrap();
        let state = state_in(&dir.path().join("app-data"));
        // What an end-to-end run writes for the next pick.
        let pick = dir.path().join("pick.txt");
        fs::write(&pick, format!("{}\n", file.display())).unwrap();

        let picked = take_line(&pick).unwrap();
        let chosen = FileRef::picked(&state.store, &root, &scope, Path::new(&picked)).unwrap();

        assert!(is_local_ref(&chosen.value), "{chosen:?}");
        assert_eq!(chosen.name, "upload.bin");
        assert!(chosen.outside);
        assert!(!chosen.missing);
        // The project holds only the reference; this device keeps the path.
        assert_eq!(
            state.store.local_files(&scope).get(&chosen.value),
            Some(&file)
        );
        assert_eq!(
            FileRef::describe(&state.store, &root, &scope, &chosen.value),
            chosen
        );
        // Picking the same file again gives the same reference.
        let again = FileRef::picked(&state.store, &root, &scope, &file).unwrap();
        assert_eq!(again.value, chosen.value);
    }

    #[test]
    fn a_file_inside_the_project_becomes_a_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("p");
        let project = create_project(&root, "harbor").unwrap();
        let scope = project.vault_scope();
        write(&root, "fixtures/labels/label.pdf", "%PDF");
        let state = state_in(&dir.path().join("app-data"));

        let chosen = FileRef::picked(
            &state.store,
            &root,
            &scope,
            &root.join("fixtures/labels/label.pdf"),
        )
        .unwrap();

        assert_eq!(
            chosen,
            FileRef {
                value: "fixtures/labels/label.pdf".into(),
                name: "label.pdf".into(),
                outside: false,
                missing: false,
            }
        );
        assert!(state.store.local_files(&scope).is_empty());
    }

    #[test]
    fn a_file_that_is_not_on_this_device_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("p");
        let scope = create_project(&root, "harbor").unwrap().vault_scope();
        let state = state_in(&dir.path().join("app-data"));

        let other_device = FileRef::describe(&state.store, &root, &scope, "local:0123456789ab");
        assert!(other_device.outside);
        assert!(other_device.missing);

        let removed = FileRef::describe(&state.store, &root, &scope, "fixtures/gone.pdf");
        assert_eq!(removed.name, "gone.pdf");
        assert!(!removed.outside);
        assert!(removed.missing);
    }
}
