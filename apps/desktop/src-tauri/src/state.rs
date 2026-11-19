//! What every command can reach.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use dea_engine::store::Store;
use dea_project::ProjectWatcher;
use tauri::{AppHandle, Manager, Runtime};

use crate::session::Sessions;

pub struct AppState {
    pub store: Store,
    sessions: Mutex<Sessions>,
    // One watcher per window with a project, by window label.
    watchers: Mutex<HashMap<String, ProjectWatcher>>,
}

impl AppState {
    pub fn new(app_data: PathBuf) -> Self {
        Self {
            store: Store::new(app_data),
            sessions: Mutex::new(Sessions::default()),
            watchers: Mutex::new(HashMap::new()),
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

/// In an end-to-end run, the folder the next folder pick returns: a native
/// dialog cannot be driven, so the test writes the path to the file that
/// `DEA_E2E_PICK_FILE` names. Each path is used once.
pub fn e2e_picked_folder() -> Option<String> {
    if !e2e() {
        return None;
    }
    take_line(Path::new(&std::env::var_os("DEA_E2E_PICK_FILE")?))
}

/// The first line of `file`, which is then removed.
fn take_line(file: &Path) -> Option<String> {
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
