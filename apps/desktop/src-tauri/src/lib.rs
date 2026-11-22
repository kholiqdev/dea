use std::io::Write;

use tauri::Manager;
use tauri::webview::PageLoadEvent;

mod commands;
mod error;
mod session;
mod state;
#[cfg(test)]
mod testing;
mod vault;

use state::AppState;
use vault::{UiPrompter, VaultOnDemand};

fn probe_enabled() -> bool {
    std::env::var_os("DEA_PERF_PROBE").is_some_and(|v| v == "1")
}

fn probe_print(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// Prints the ready line for the performance probe once the first frame is
/// drawn. Does nothing unless `DEA_PERF_PROBE=1`.
#[tauri::command]
fn perf_ready() {
    if probe_enabled() {
        probe_print("DEA_READY");
    }
}

/// Marks an earlier step of start-up for the performance probe, such as the
/// UI being mounted. Does nothing unless `DEA_PERF_PROBE=1`.
#[tauri::command]
fn perf_mark(name: &str) {
    if probe_enabled() {
        probe_print(&format!("DEA_MARK {name}"));
    }
}

pub fn run() {
    let probe = probe_enabled();
    if probe {
        probe_print("DEA_MARK process started");
    }
    let mut builder = tauri::Builder::default();
    if state::e2e() {
        // Lets the UI register the test-only tab type (debug builds only).
        builder = builder.plugin(
            tauri::plugin::Builder::<tauri::Wry>::new("dea-e2e")
                .js_init_script("window.__DEA_E2E__ = true;".to_string())
                .build(),
        );
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = state::app_data_dir(app.handle())?;
            let prompter = UiPrompter::new(app.handle().clone());
            let vault = VaultOnDemand(app.handle().clone());
            app.manage(AppState::new(dir, prompter, vault));
            commands::project::reopen_last(app.handle(), "main");
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                commands::project::window_closed(window.app_handle(), window.label());
            }
        })
        .invoke_handler(tauri::generate_handler![
            perf_ready,
            perf_mark,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_ui_state,
            commands::settings::save_ui_state,
            commands::project::window_project,
            commands::project::open_project,
            commands::project::create_project,
            commands::project::open_window,
            commands::project::close_project,
            commands::project::pick_folder,
            commands::project::recent_projects,
            commands::project::remove_recent,
            commands::project::save_environment,
            commands::project::delete_environment,
            commands::project::duplicate_environment,
            commands::project::environment_references,
            commands::project::set_active_environment,
            commands::project::clear_local_data,
            commands::requests::request_tree,
            commands::requests::read_request,
            commands::requests::save_request,
            commands::requests::read_folder,
            commands::requests::save_folder,
            commands::requests::create_request,
            commands::requests::create_folder,
            commands::requests::rename_request_item,
            commands::requests::duplicate_request_item,
            commands::requests::move_request_item,
            commands::requests::delete_request_item,
            commands::requests::restore_request_item,
            commands::requests::inspect_request,
            commands::requests::pick_request_file,
            commands::requests::describe_file_ref,
            commands::responses::response_text,
            commands::responses::response_bytes,
            commands::responses::response_rows,
            commands::responses::find_in_response,
            commands::responses::filter_response,
            commands::responses::copy_response,
            commands::clipboard::copy_text,
            commands::responses::save_response,
            commands::responses::release_response,
            commands::responses::list_cookies,
            commands::responses::save_cookie,
            commands::responses::delete_cookie,
            commands::responses::delete_domain_cookies,
            commands::send::send_request,
            commands::send::cancel_send,
            commands::send::answer_send_confirmation,
            commands::send::resend_send_confirmations,
            commands::vault::vault_info,
            commands::vault::vault_secrets,
            commands::vault::vault_unlock,
            commands::vault::vault_lock,
            commands::vault::vault_activity,
            commands::vault::vault_save_secret,
            commands::vault::vault_delete_value,
            commands::vault::vault_reveal,
            commands::vault::vault_copy,
            commands::vault::vault_authorize,
            commands::vault::vault_answer_prompt,
            commands::vault::vault_resend_prompts,
        ])
        .on_page_load(move |webview, payload| {
            // Before setup the state is not there yet, and nothing can wait.
            if let Some(state) = webview.try_state::<AppState>() {
                state.page_load(webview.label(), payload.event());
            }
            if probe && payload.event() == PageLoadEvent::Finished {
                probe_print("DEA_MARK page loaded");
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Dea");
}
