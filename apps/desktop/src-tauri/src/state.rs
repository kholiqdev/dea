//! What every command can reach.

use std::path::{Path, PathBuf};

use dea_engine::store::Store;
use tauri::{AppHandle, Manager, Runtime};

pub struct AppState {
    pub store: Store,
}

impl AppState {
    pub fn new(app_data: &Path) -> Self {
        Self {
            store: Store::new(app_data),
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
