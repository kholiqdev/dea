use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use perf_probe::{
    Measurement, check, installer_sizes_mb, load_budgets, load_measurement, measure_launch, median,
    startup_js_gzip_kb,
};

/// Measures start time, idle memory, startup JavaScript and installer sizes,
/// then fails when a number is over budget or worse than the baseline.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// The built app binary to launch.
    #[arg(long)]
    app: PathBuf,
    /// The built web assets, containing index.html.
    #[arg(long)]
    dist: PathBuf,
    /// Folder holding the installers, searched recursively.
    #[arg(long)]
    bundle_dir: PathBuf,
    /// budgets.toml
    #[arg(long)]
    budgets: PathBuf,
    /// Result of the last release on this OS.
    #[arg(long)]
    baseline: Option<PathBuf>,
    /// Launches to take the median of.
    #[arg(long, default_value_t = 5)]
    runs: usize,
    /// Launches before the measured ones, not counted. The first launches
    /// of a freshly built app include one-time work such as the OS checking
    /// a new binary; cold start is measured separately on reference hardware.
    #[arg(long, default_value_t = 0)]
    warmup: usize,
    /// Seconds to wait after the first frame before reading memory.
    #[arg(long, default_value_t = 5)]
    idle_wait_secs: u64,
    /// OS name used to pick the memory budget.
    #[arg(long, default_value = default_os())]
    os: String,
    /// Where to write the result as JSON.
    #[arg(long)]
    out: PathBuf,
}

fn default_os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    }
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(passed) if passed => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

fn run(args: Args) -> Result<bool, Box<dyn std::error::Error>> {
    let budgets = load_budgets(&args.budgets)?;
    let baseline = args.baseline.as_deref().map(load_measurement).transpose()?;

    for warmup in 1..=args.warmup {
        let (start_ms, _) = measure_launch(&args.app, Duration::ZERO)?;
        println!("warm-up {warmup}: start {start_ms:.0} ms (not counted)");
    }

    let mut starts = Vec::new();
    let mut idles = Vec::new();
    for run in 1..=args.runs.max(1) {
        let (start_ms, idle_mb) =
            measure_launch(&args.app, Duration::from_secs(args.idle_wait_secs))?;
        println!("run {run}: start {start_ms:.0} ms, idle {idle_mb:.1} MiB");
        starts.push(start_ms);
        idles.push(idle_mb);
    }

    let measurement = Measurement {
        os: args.os,
        start_ms: median(&starts),
        idle_mb: median(&idles),
        startup_js_gzip_kb: startup_js_gzip_kb(&args.dist)?,
        installers_mb: installer_sizes_mb(&args.bundle_dir)?,
    };
    if let Some(dir) = args.out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(
        &args.out,
        serde_json::to_string_pretty(&measurement)? + "\n",
    )?;

    println!("median start: {:.0} ms", measurement.start_ms);
    println!("median idle memory: {:.1} MiB", measurement.idle_mb);
    println!(
        "startup JavaScript: {:.1} KiB gzip",
        measurement.startup_js_gzip_kb
    );
    for (kind, mb) in &measurement.installers_mb {
        println!("installer {kind}: {mb:.1} MiB");
    }

    let violations = check(&measurement, &budgets, baseline.as_ref());
    if violations.is_empty() {
        println!("All numbers are within budget.");
        return Ok(true);
    }
    for v in &violations {
        println!("FAIL {v}");
    }
    Ok(false)
}
