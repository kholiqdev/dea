//! Settings and UI state. File work runs off the main thread.

use dea_engine::store::{Settings, UiState};
use tauri::State;
use tauri::async_runtime::spawn_blocking;

use crate::error::IpcError;
use crate::state::AppState;

#[tauri::command]
pub async fn get_settings(app: State<'_, AppState>) -> Result<Settings, IpcError> {
    let store = app.store.clone();
    Ok(spawn_blocking(move || store.settings()).await?)
}

#[tauri::command]
pub async fn save_settings(app: State<'_, AppState>, settings: Settings) -> Result<(), IpcError> {
    let store = app.store.clone();
    Ok(spawn_blocking(move || store.save_settings(&settings)).await??)
}

#[tauri::command]
pub async fn get_ui_state(app: State<'_, AppState>, key: String) -> Result<UiState, IpcError> {
    let store = app.store.clone();
    Ok(spawn_blocking(move || store.ui_state(&key)).await?)
}

#[tauri::command]
pub async fn save_ui_state(
    app: State<'_, AppState>,
    key: String,
    state: UiState,
) -> Result<(), IpcError> {
    let store = app.store.clone();
    Ok(spawn_blocking(move || store.save_ui_state(&key, &state)).await??)
}
