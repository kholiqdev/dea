//! The one Vault of the app, for the project of the calling window.
//! Commands that can show a dialog or reach the keychain run off the main
//! thread, and send any Vault password prompt to the calling window.

use dea_engine::projects::ProjectInfo;
use dea_engine::vault::{
    CLIPBOARD_CLEAR_AFTER, SecretInput, SecretKey, SecretRow, UnlockMethod, UseError, VaultService,
    is_production, save_secret, unlock_method,
};
use dea_project::{Project, load_project};
use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::commands::project::{info_of, writable};
use crate::error::IpcError;
use crate::state::AppState;

/// What the unlock dialog says Dea wants to do.
const UNLOCK_REASON: &str = "unlock the Vault";

/// Whether the Vault is locked, and how this device unlocks it.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    pub locked: bool,
    /// Decides the unlock button's label.
    pub method: UnlockMethod,
}

/// Runs `f` off the main thread with the Vault, opening it the first time.
/// Any Vault password prompt `f` causes goes to `window`.
async fn with_vault<T, F>(app: AppHandle, window: &WebviewWindow, f: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&VaultService) -> Result<T, IpcError> + Send + 'static,
{
    let window = window.label().to_string();
    spawn_blocking(move || {
        let state = app.state::<AppState>();
        let vault = state.vault.get_or_open(&app)?;
        state.vault.prompter().asking(&window, || f(&vault))
    })
    .await?
}

/// Opens the production window before a value of a production environment
/// is used, asking for authentication if it is closed.
fn authorize_value(
    vault: &VaultService,
    project: &Project,
    verb: &str,
    environment: &str,
    name: &str,
) -> Result<(), UseError> {
    if is_production(project, environment) {
        vault.authorize_production(&format!("{verb} a value in {environment}"), &[name])?;
    }
    Ok(())
}

/// Does not open the Vault: before it is first used it counts as locked.
#[tauri::command]
pub async fn vault_info(app: AppHandle) -> Result<VaultInfo, IpcError> {
    let locked = app
        .state::<AppState>()
        .vault
        .existing()
        .is_none_or(|v| v.status().locked);
    // On Windows this asks WinRT whether Hello is set up, which blocks.
    let method = spawn_blocking(unlock_method).await?;
    Ok(VaultInfo { locked, method })
}

/// The secrets of the window's project in every environment, without
/// values.
#[tauri::command]
pub async fn vault_secrets(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Vec<SecretRow>, IpcError> {
    let session = writable(&app, window.label())?;
    with_vault(app, &window, move |vault| {
        let project = load_project(&session.root)?.project;
        Ok(vault.rows(&project))
    })
    .await
}

/// Unlocks with the OS dialog, or on Linux the Vault password.
#[tauri::command]
pub async fn vault_unlock(app: AppHandle, window: WebviewWindow) -> Result<(), IpcError> {
    with_vault(app, &window, |vault| Ok(vault.unlock(UNLOCK_REASON)?)).await
}

/// Locks every window now. Does nothing before the Vault is first used.
#[tauri::command]
pub fn vault_lock(app: AppHandle) {
    if let Some(vault) = app.state::<AppState>().vault.existing() {
        vault.lock();
    }
}

/// The person did something in a window: the idle timer starts again.
#[tauri::command]
pub fn vault_activity(app: AppHandle) {
    if let Some(vault) = app.state::<AppState>().vault.existing() {
        vault.record_activity();
    }
}

/// Declares the secret in `vault.yaml` and stores its value; returns the
/// project as it is now.
#[tauri::command]
pub async fn vault_save_secret(
    app: AppHandle,
    window: WebviewWindow,
    input: SecretInput,
) -> Result<ProjectInfo, IpcError> {
    let session = writable(&app, window.label())?;
    with_vault(app, &window, move |vault| {
        let project = load_project(&session.root)?.project;
        save_secret(vault, &project, input)?;
        info_of(&session)
    })
    .await
}

/// Deletes the value on this device; `vault.yaml` keeps the declaration.
#[tauri::command]
pub async fn vault_delete_value(
    app: AppHandle,
    window: WebviewWindow,
    environment: String,
    name: String,
) -> Result<(), IpcError> {
    let session = writable(&app, window.label())?;
    with_vault(app, &window, move |vault| {
        let key = SecretKey {
            scope: session.scope,
            environment,
            name,
        };
        Ok(vault.delete_secret(&key)?)
    })
    .await
}

/// The value, for the Vault screen to show. One of the two places a value
/// crosses IPC.
#[tauri::command]
pub async fn vault_reveal(
    app: AppHandle,
    window: WebviewWindow,
    environment: String,
    name: String,
) -> Result<String, IpcError> {
    let session = writable(&app, window.label())?;
    with_vault(app, &window, move |vault| {
        let project = load_project(&session.root)?.project;
        authorize_value(vault, &project, "reveal", &environment, &name)?;
        let value = vault
            .secret(&project, &environment, &name)?
            .ok_or(UseError::Missing)?;
        Ok(value.value_for_use().to_string())
    })
    .await
}

/// Copies the value; the clipboard is cleared after 30 s unless something
/// else was copied.
#[tauri::command]
pub async fn vault_copy(
    app: AppHandle,
    window: WebviewWindow,
    environment: String,
    name: String,
) -> Result<(), IpcError> {
    let session = writable(&app, window.label())?;
    let clipboard = app.state::<AppState>().vault.clipboard();
    with_vault(app, &window, move |vault| {
        let project = load_project(&session.root)?.project;
        authorize_value(vault, &project, "copy", &environment, &name)?;
        vault.copy_secret(
            &project,
            &environment,
            &name,
            clipboard,
            CLIPBOARD_CLEAR_AFTER,
        )?;
        Ok(())
    })
    .await
}

/// Opens the production window for an action that uses no secret, such as
/// `turn off production for <env>`.
#[tauri::command]
pub async fn vault_authorize(
    app: AppHandle,
    window: WebviewWindow,
    action: String,
) -> Result<(), IpcError> {
    with_vault(app, &window, move |vault| {
        Ok(vault.authorize_production(&action, &[])?)
    })
    .await
}

/// The window's answer to prompt `id`; `None` when the person cancelled.
#[tauri::command]
pub fn vault_answer_prompt(app: AppHandle, id: u64, password: Option<String>) {
    app.state::<AppState>()
        .vault
        .prompter()
        .answer(id, password);
}
