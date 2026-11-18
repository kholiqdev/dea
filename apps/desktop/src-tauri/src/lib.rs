use std::io::Write;

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
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![perf_ready, perf_mark])
        .run(tauri::generate_context!())
        .expect("error while running Dea");
}
