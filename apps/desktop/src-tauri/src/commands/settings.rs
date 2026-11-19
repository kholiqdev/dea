//! Settings and UI state. File work runs off the main thread.

use dea_engine::store::{SETTINGS_VERSION, Settings, UiState};
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::IpcError;
use crate::state::AppState;

/// The event every window gets when the settings are saved, with the saved
/// `Settings`, so no window keeps an older copy and saves it back.
pub const SETTINGS_CHANGED: &str = "settings-changed";

#[tauri::command]
pub async fn get_settings(app: State<'_, AppState>) -> Result<Settings, IpcError> {
    let store = app.store.clone();
    Ok(spawn_blocking(move || store.load_settings()).await?)
}

/// Saves the settings and sends them to every window; an open Vault follows
/// the new minutes at once.
#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), IpcError> {
    let state = app.state::<AppState>();
    let store = state.store.clone();
    let saved = Settings {
        version: SETTINGS_VERSION,
        ..settings.clone()
    };
    spawn_blocking(move || store.save_settings(settings)).await??;
    if let Some(vault) = state.vault.existing() {
        vault.set_policy(saved.security.policy()?);
    }
    // The file is saved; a window that misses the news is no reason to
    // report a failed save.
    let _ = app.emit(SETTINGS_CHANGED, saved);
    Ok(())
}

#[tauri::command]
pub async fn get_ui_state(
    app: State<'_, AppState>,
    scope: String,
    slot: u8,
) -> Result<UiState, IpcError> {
    let store = app.store.clone();
    Ok(spawn_blocking(move || store.load_ui_state(&scope, slot)).await?)
}

#[tauri::command]
pub async fn save_ui_state(
    app: State<'_, AppState>,
    scope: String,
    slot: u8,
    state: UiState,
) -> Result<(), IpcError> {
    let store = app.store.clone();
    Ok(spawn_blocking(move || store.save_ui_state(&scope, slot, state)).await??)
}
