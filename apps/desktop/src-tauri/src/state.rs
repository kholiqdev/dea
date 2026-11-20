//! What every command can reach.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use dea_engine::requests::{Client, ClientConfig, RequestService, VaultProvider};
use dea_engine::store::Store;
use dea_project::{DeletedItem, LoadedRequests, ProjectChange, ProjectError, ProjectWatcher};
use tauri::{AppHandle, Manager, Runtime};

use crate::session::Sessions;
use crate::vault::{AppVault, UiPrompter};

/// How long a deleted request or folder is kept for Undo. The toast offers
/// it for 10 s; the rest covers a slow click.
const KEEP_DELETED: Duration = Duration::from_secs(15);

/// A deleted item: the window that deleted it, the item, and when.
type Deleted = (String, DeletedItem, Instant);

fn guard<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct AppState {
    pub store: Store,
    /// The one Vault of the app, shared by every window.
    pub vault: AppVault,
    /// Resolves and sends requests for every window.
    pub requests: RequestService,
    sessions: Mutex<Sessions>,
    // One watcher per window with a project, by window label.
    watchers: Mutex<HashMap<String, ProjectWatcher>>,
    /// The request tree of each window's project, by window label, until a
    /// file under `requests/` changes.
    trees: Mutex<HashMap<String, Arc<LoadedRequests>>>,
    /// Deleted items that Undo can still restore, by id.
    deleted: Mutex<HashMap<u64, Deleted>>,
    next_deleted: AtomicU64,
}

impl AppState {
    /// State kept in `app_data`; the Vault asks for passwords with
    /// `prompter`, and requests get the Vault from `vault` when a send
    /// first needs it.
    pub fn new(
        app_data: PathBuf,
        prompter: UiPrompter,
        vault: impl VaultProvider + 'static,
    ) -> Self {
        let store = Store::new(app_data.clone());
        // Large response bodies are written here. A folder that cannot be
        // made is reported by the send that needs it.
        let spill_dir = app_data.join("responses");
        let _ = std::fs::create_dir_all(&spill_dir);
        let client = Client::new(ClientConfig {
            spill_dir,
            ..ClientConfig::default()
        })
        .expect("the default user agent is a valid header");
        let app_vault = AppVault::new(app_data, prompter);
        let requests = RequestService::new(
            client,
            Arc::new(store.clone()),
            vault,
            app_vault.shared_prompter(),
        );
        Self {
            store,
            vault: app_vault,
            requests,
            sessions: Mutex::new(Sessions::default()),
            watchers: Mutex::new(HashMap::new()),
            trees: Mutex::new(HashMap::new()),
            deleted: Mutex::new(HashMap::new()),
            next_deleted: AtomicU64::new(1),
        }
    }

    pub fn sessions(&self) -> MutexGuard<'_, Sessions> {
        self.sessions.lock().expect("sessions lock poisoned")
    }

    /// Replaces the window's watcher; `None` stops watching.
    pub fn set_watcher(&self, window: &str, watcher: Option<ProjectWatcher>) {
        let mut watchers = self.watchers.lock().expect("watchers lock poisoned");
        match watcher {
            Some(w) => watchers.insert(window.to_string(), w),
            None => watchers.remove(window),
        };
    }

    /// The request tree of `window`, loaded with `load` the first time.
    /// The lock is held while loading, so a change reported meanwhile
    /// drops the tree after it is stored, never before.
    pub fn tree(
        &self,
        window: &str,
        load: impl FnOnce() -> Result<LoadedRequests, ProjectError>,
    ) -> Result<Arc<LoadedRequests>, ProjectError> {
        let mut trees = guard(&self.trees);
        if let Some(tree) = trees.get(window) {
            return Ok(Arc::clone(tree));
        }
        let tree = Arc::new(load()?);
        trees.insert(window.to_string(), Arc::clone(&tree));
        Ok(tree)
    }

    /// Drops the request tree of `window`, so the next one is read again.
    pub fn forget_tree(&self, window: &str) {
        guard(&self.trees).remove(window);
    }

    /// Drops the request tree of `window` when one of `changes` is under
    /// `requests/`.
    pub fn requests_changed(&self, window: &str, changes: &[ProjectChange]) {
        if changes.iter().any(|c| c.path.starts_with("requests")) {
            self.forget_tree(window);
        }
    }

    /// Forgets what `window` kept of its project: its request tree and the
    /// items it can still restore. Called when it opens or closes one.
    pub fn forget_window(&self, window: &str) {
        self.forget_tree(window);
        guard(&self.deleted).retain(|_, (owner, _, _)| owner != window);
    }

    /// Keeps `item`, deleted by `window` at `now`, for Undo; returns its id.
    pub fn keep_deleted(&self, window: &str, item: DeletedItem, now: Instant) -> u64 {
        let id = self.next_deleted.fetch_add(1, Ordering::Relaxed);
        let mut deleted = self.deleted_at(now);
        deleted.insert(id, (window.to_string(), item, now));
        id
    }

    /// The item `window` deleted as `id`, if it is still kept at `now`.
    pub fn deleted_item(&self, window: &str, id: u64, now: Instant) -> Option<DeletedItem> {
        self.deleted_at(now)
            .get(&id)
            .filter(|(owner, _, _)| owner == window)
            .map(|(_, item, _)| item.clone())
    }

    /// Stops keeping item `id`, once it is restored.
    pub fn forget_deleted(&self, id: u64) {
        guard(&self.deleted).remove(&id);
    }

    /// The deleted items, without those kept longer than [`KEEP_DELETED`]
    /// at `now`.
    fn deleted_at(&self, now: Instant) -> MutexGuard<'_, HashMap<u64, Deleted>> {
        let mut deleted = guard(&self.deleted);
        deleted.retain(|_, (_, _, at)| now.saturating_duration_since(*at) <= KEEP_DELETED);
        deleted
    }
}

/// The app data folder. A debug build takes `DEA_APP_DATA_DIR` instead, so
/// each end-to-end run starts from an empty folder; a release build never
/// reads it.
pub fn app_data_dir<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<PathBuf> {
    if cfg!(debug_assertions)
        && let Some(dir) = std::env::var_os("DEA_APP_DATA_DIR")
    {
        return Ok(PathBuf::from(dir));
    }
    app.path().app_data_dir()
}

/// Whether this is a debug build started for the end-to-end tests.
pub fn e2e() -> bool {
    cfg!(debug_assertions) && std::env::var_os("DEA_E2E").is_some_and(|v| v == "1")
}

/// In an end-to-end run, the path the next folder or file pick returns: a
/// native dialog cannot be driven, so the test writes the path to the file
/// that `DEA_E2E_PICK_FILE` names. Each path is used once.
pub fn e2e_picked_path() -> Option<String> {
    if !e2e() {
        return None;
    }
    take_line(Path::new(&std::env::var_os("DEA_E2E_PICK_FILE")?))
}

/// The first line of `file`, which is then removed.
pub(crate) fn take_line(file: &Path) -> Option<String> {
    let text = std::fs::read_to_string(file).ok()?;
    let _ = std::fs::remove_file(file);
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_string())
}

#[cfg(test)]
mod tests {
    use super::take_line;

    #[test]
    fn a_picked_folder_is_used_once() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pick.txt");
        std::fs::write(&file, "/work/plain\n").unwrap();
        assert_eq!(take_line(&file).as_deref(), Some("/work/plain"));
        assert!(!file.exists());
        assert_eq!(take_line(&file), None);
    }
}
