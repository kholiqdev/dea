use std::io::Write;

/// Prints the ready line for the performance probe once the first frame is
/// drawn. Does nothing unless `DEA_PERF_PROBE=1`.
#[tauri::command]
fn perf_ready() {
    if std::env::var_os("DEA_PERF_PROBE").is_some_and(|v| v == "1") {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "DEA_READY");
        let _ = out.flush();
    }
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![perf_ready])
        .run(tauri::generate_context!())
        .expect("error while running Dea");
}
