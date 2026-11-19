use std::io::Write;

use tauri::Manager;
use tauri::webview::PageLoadEvent;

mod commands;
mod error;
mod state;

use state::AppState;

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
        .setup(|app| {
            let dir = state::app_data_dir(app.handle())?;
            app.manage(AppState::new(&dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            perf_ready,
            perf_mark,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_ui_state,
            commands::settings::save_ui_state,
        ])
        .on_page_load(move |_webview, payload| {
            if probe && payload.event() == PageLoadEvent::Finished {
                probe_print("DEA_MARK page loaded");
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Dea");
}
