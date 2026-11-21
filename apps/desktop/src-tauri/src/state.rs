//! What every command can reach.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use dea_engine::ResponseStore;
use dea_engine::requests::{
    CancellationToken, Client, ClientConfig, RequestService, VaultProvider,
};
use dea_engine::store::Store;
use dea_project::{DeletedItem, LoadedRequests, ProjectChange, ProjectError, ProjectWatcher};
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, Runtime};

use crate::session::Sessions;
use crate::vault::{AppVault, UiPrompter};

/// How long a deleted request or folder is kept for Undo. The toast offers
/// it for 10 s; the rest covers a slow click.
const KEEP_DELETED: Duration = Duration::from_secs(15);

/// A deleted item: the window that deleted it, the root of the project it
/// was deleted from, the item, and when.
type Deleted = (String, PathBuf, DeletedItem, Instant);

/// A window's request tree and the root of the project it was read from.
struct CachedTree {
    root: PathBuf,
    tree: Arc<LoadedRequests>,
}

fn guard<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct AppState {
    pub store: Store,
    /// The one Vault of the app, shared by every window.
    pub vault: AppVault,
    /// Resolves and sends requests for every window.
    pub requests: RequestService,
    /// The sends in flight, by `"<window>/<tab>"`.
    pub sends: Sends,
    /// The last response of each tab, by `"<window>/<tab>"`.
    pub responses: ResponseStore,
    sessions: Mutex<Sessions>,
    // One watcher per window with a project, by window label.
    watchers: Mutex<HashMap<String, ProjectWatcher>>,
    /// The request tree of each window's project, by window label, until a
    /// file under `requests/` changes.
    trees: Mutex<HashMap<String, CachedTree>>,
    /// How many trees were dropped, so a tree read while one was dropped is
    /// not kept. Changed only while `trees` is locked.
    tree_drops: AtomicU64,
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
            sends: Sends::default(),
            responses: ResponseStore::default(),
            sessions: Mutex::new(Sessions::default()),
            watchers: Mutex::new(HashMap::new()),
            trees: Mutex::new(HashMap::new()),
            tree_drops: AtomicU64::new(0),
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

    /// The request tree of the project at `root` for `window`, read with
    /// `load` unless the window keeps one of that project. The tree is read
    /// without holding the lock, and kept only while `root` is still the
    /// window's project and no tree was dropped meanwhile, so a tree read
    /// before a change or for a project the window has left is never kept.
    pub fn tree(
        &self,
        window: &str,
        root: &Path,
        load: impl FnOnce() -> Result<LoadedRequests, ProjectError>,
    ) -> Result<Arc<LoadedRequests>, ProjectError> {
        let drops = {
            let trees = guard(&self.trees);
            if let Some(cached) = trees.get(window).filter(|c| c.root == root) {
                return Ok(Arc::clone(&cached.tree));
            }
            self.tree_drops.load(Ordering::SeqCst)
        };
        let tree = Arc::new(load()?);
        let current = self.sessions().get(window).is_some_and(|s| s.root == root);
        let mut trees = guard(&self.trees);
        if current && self.tree_drops.load(Ordering::SeqCst) == drops {
            let cached = CachedTree {
                root: root.to_path_buf(),
                tree: Arc::clone(&tree),
            };
            trees.insert(window.to_string(), cached);
        }
        Ok(tree)
    }

    /// Drops the request tree of `window`, so the next one is read again.
    pub fn forget_tree(&self, window: &str) {
        let mut trees = guard(&self.trees);
        trees.remove(window);
        self.tree_drops.fetch_add(1, Ordering::SeqCst);
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
        self.deleted_at(Instant::now())
            .retain(|_, (owner, _, _, _)| owner != window);
    }

    /// Ends what `window` still waits on as it closes or loads a new page:
    /// the prompts shown in it, its sends, and the responses its tabs kept.
    pub fn window_closed(&self, window: &str) {
        let prefix = format!("{window}/");
        // Sends first, so one woken from its prompt ends as cancelled.
        self.sends.cancel_prefix(&prefix);
        self.vault.prompter().cancel_window(window);
        self.responses.release_prefix(&prefix);
    }

    /// A page of `window` is loading. Opening or closing a project, or a
    /// reload, starts a new page in the same window: the old page's dialogs
    /// and listeners are gone, so what it waited on ends as if the window
    /// closed. The window keeps its project.
    pub fn page_load(&self, window: &str, event: PageLoadEvent) {
        if event == PageLoadEvent::Started {
            self.window_closed(window);
        }
    }

    /// Keeps `item`, deleted by `window` at `now` from the project at
    /// `root`, for Undo; returns its id.
    pub fn keep_deleted(&self, window: &str, root: &Path, item: DeletedItem, now: Instant) -> u64 {
        let id = self.next_deleted.fetch_add(1, Ordering::Relaxed);
        let entry = (window.to_string(), root.to_path_buf(), item, now);
        self.deleted_at(now).insert(id, entry);
        id
    }

    /// The item `window` deleted as `id` from the project at `root`, if it
    /// is still kept at `now`.
    pub fn deleted_item(
        &self,
        window: &str,
        root: &Path,
        id: u64,
        now: Instant,
    ) -> Option<DeletedItem> {
        self.deleted_at(now)
            .get(&id)
            .filter(|(owner, from, _, _)| owner == window && from == root)
            .map(|(_, _, item, _)| item.clone())
    }

    /// Stops keeping item `id`, once it is restored.
    pub fn forget_deleted(&self, id: u64) {
        self.deleted_at(Instant::now()).remove(&id);
    }

    /// The deleted items, without those kept longer than [`KEEP_DELETED`]
    /// at `now`.
    fn deleted_at(&self, now: Instant) -> MutexGuard<'_, HashMap<u64, Deleted>> {
        let mut deleted = guard(&self.deleted);
        deleted.retain(|_, (_, _, _, at)| now.saturating_duration_since(*at) <= KEEP_DELETED);
        deleted
    }
}

/// The sends in flight, each with the token that cancels it, by key. The
/// desktop uses `"<window>/<tab>"`.
#[derive(Default)]
pub struct Sends {
    running: Mutex<HashMap<String, CancellationToken>>,
}

impl Sends {
    /// Starts a send under `key`, cancelling the one it replaces.
    pub fn start(&self, key: &str) -> CancellationToken {
        let token = CancellationToken::new();
        if let Some(old) = guard(&self.running).insert(key.to_string(), token.clone()) {
            old.cancel();
        }
        token
    }

    /// Cancels the send under `key`. An unknown key is ignored.
    pub fn cancel(&self, key: &str) {
        if let Some(token) = guard(&self.running).remove(key) {
            token.cancel();
        }
    }

    /// Cancels every send whose key starts with `prefix`, such as
    /// `"<window>/"`.
    pub fn cancel_prefix(&self, prefix: &str) {
        let mut running = guard(&self.running);
        for (_, token) in running.extract_if(|key, _| key.starts_with(prefix)) {
            token.cancel();
        }
    }

    /// Runs `f` while `token` is still the send under `key`, holding the
    /// lock the cancels take, so nothing is kept for a send cancelled or
    /// replaced meanwhile. `None`, without running `f`, otherwise.
    pub fn if_running<T>(
        &self,
        key: &str,
        token: &CancellationToken,
        f: impl FnOnce() -> T,
    ) -> Option<T> {
        let running = guard(&self.running);
        (running.get(key) == Some(token) && !token.is_cancelled()).then(f)
    }

    /// Forgets the send under `key` once it ended, unless a newer one
    /// took its place.
    pub fn finish(&self, key: &str, token: &CancellationToken) {
        let mut running = guard(&self.running);
        if running.get(key) == Some(token) {
            running.remove(key);
        }
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

/// In an end-to-end run, the path the next save dialog returns: the test
/// writes it to the file that `DEA_E2E_SAVE_FILE` names. Each path is used
/// once.
pub fn e2e_save_path() -> Option<String> {
    if !e2e() {
        return None;
    }
    take_line(Path::new(&std::env::var_os("DEA_E2E_SAVE_FILE")?))
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
    use dea_engine::{Prompter, ResponseError, SendAnswer};

    use super::*;
    use crate::testing::{RecordingWindows, WAIT, confirmation, kept, on_thread, state_with};

    #[test]
    fn a_picked_folder_is_used_once() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pick.txt");
        std::fs::write(&file, "/work/plain\n").unwrap();
        assert_eq!(take_line(&file).as_deref(), Some("/work/plain"));
        assert!(!file.exists());
        assert_eq!(take_line(&file), None);
    }

    #[test]
    fn a_new_send_with_the_same_key_cancels_the_old_one() {
        let sends = Sends::default();
        let first = sends.start("main/t1");
        let second = sends.start("main/t1");
        assert!(first.is_cancelled());
        sends.finish("main/t1", &first); // stale: must not remove the second
        sends.cancel("main/t1");
        assert!(second.is_cancelled());
    }

    #[test]
    fn a_send_keeps_its_response_only_while_it_runs() {
        let sends = Sends::default();
        let first = sends.start("main/t1");
        assert_eq!(sends.if_running("main/t1", &first, || 1), Some(1));

        let second = sends.start("main/t1");
        assert_eq!(sends.if_running("main/t1", &first, || 2), None);
        sends.cancel("main/t1");
        assert_eq!(sends.if_running("main/t1", &second, || 3), None);

        let third = sends.start("main/t1");
        sends.finish("main/t1", &third);
        assert_eq!(sends.if_running("main/t1", &third, || 4), None);
        assert!(!third.is_cancelled(), "a send that ended is not cancelled");
    }

    #[test]
    fn cancelling_a_prefix_spares_other_windows() {
        let sends = Sends::default();
        let ours = [sends.start("project-1/t1"), sends.start("project-1/t2")];
        let theirs = [sends.start("project-10/t1"), sends.start("main/t1")];
        sends.cancel_prefix("project-1/");
        assert!(ours.iter().all(CancellationToken::is_cancelled));
        assert!(!theirs.iter().any(CancellationToken::is_cancelled));
    }

    #[test]
    fn closing_a_window_cancels_its_sends_and_confirmations() {
        let dir = tempfile::tempdir().unwrap();
        let (windows, _shown, confirms, _) = RecordingWindows::new(&["main", "project-1"], None);
        let state = state_with(dir.path(), windows);
        let confirm_from = |window: &'static str, key: &'static str| {
            let state = Arc::clone(&state);
            let send = state.sends.start(key);
            let asked = on_thread(move || {
                let prompter = state.vault.prompter();
                prompter.sending(window, &send, || prompter.confirm_send(&confirmation()))
            });
            let (_, event) = confirms.recv_timeout(WAIT).unwrap();
            (asked, event)
        };
        let closing = state.sends.start("project-1/t1");
        let (closing_asked, _) = confirm_from("project-1", "project-1/t2");
        let staying = state.sends.start("main/t1");
        let (staying_asked, waiting) = confirm_from("main", "main/t2");

        state.window_closed("project-1");
        assert!(closing.is_cancelled());
        assert_eq!(
            closing_asked.recv_timeout(WAIT).unwrap(),
            SendAnswer::Cancel
        );

        // The other window goes on.
        assert!(!staying.is_cancelled());
        state
            .vault
            .prompter()
            .answer_send(waiting.id, SendAnswer::Send);
        assert_eq!(staying_asked.recv_timeout(WAIT).unwrap(), SendAnswer::Send);
    }

    #[test]
    fn a_new_page_ends_what_the_old_one_sent() {
        let dir = tempfile::tempdir().unwrap();
        let (windows, _shown, confirms, _) = RecordingWindows::new(&["main", "project-1"], None);
        let state = state_with(dir.path(), windows);
        let reloading = state.sends.start("project-1/t1");
        let asking = state.sends.start("project-1/t2");
        let asked = {
            let state = Arc::clone(&state);
            on_thread(move || {
                let prompter = state.vault.prompter();
                prompter.sending("project-1", &asking, || {
                    prompter.confirm_send(&confirmation())
                })
            })
        };
        confirms.recv_timeout(WAIT).unwrap();
        let staying = state.sends.start("main/t1");
        kept(&state.responses, "project-1/t1", "[1]");
        kept(&state.responses, "main/t1", "[2]");

        // A page that finished loading keeps all of it.
        state.page_load("project-1", PageLoadEvent::Finished);
        assert!(!reloading.is_cancelled());
        assert!(state.responses.bytes("project-1/t1").is_ok());

        // Opening or closing a project, or a reload, loads a new page in
        // the same window: the old page's sends and responses end with it.
        state.page_load("project-1", PageLoadEvent::Started);
        assert!(reloading.is_cancelled());
        assert_eq!(asked.recv_timeout(WAIT).unwrap(), SendAnswer::Cancel);
        assert!(matches!(
            state.responses.bytes("project-1/t1"),
            Err(ResponseError::NotFound)
        ));

        // The other window goes on.
        assert!(!staying.is_cancelled());
        assert!(state.responses.bytes("main/t1").is_ok());
    }
}
