//! Measures the numbers in the Phase 1 performance budget (F1-REL-05) and
//! compares them with the budget and with the last release's baseline.

mod memory;

pub use memory::process_memory_bytes;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use flate2::Compression;
use flate2::write::GzEncoder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sysinfo::{Pid, ProcessesToUpdate, System};

/// Line the app prints once its first frame is drawn, when `DEA_PERF_PROBE=1`.
pub const READY_LINE: &str = "DEA_READY";

const MIB: f64 = 1024.0 * 1024.0;
const INSTALLER_KINDS: [&str; 6] = ["dmg", "exe", "msi", "deb", "rpm", "appimage"];

static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<(script|link)\b[^>]*>").unwrap());
static ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)\b(src|href|rel)\s*=\s*["']([^"']*)["']"#).unwrap());

/// Limits, read from `budgets.toml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    pub start_ms: f64,
    pub startup_js_gzip_kb: f64,
    pub regression_pct: f64,
    /// Key: "macos", "windows" or "linux".
    pub idle_mb: BTreeMap<String, f64>,
    /// Key: "dmg", "exe", "msi", "deb", "rpm" or "appimage".
    pub installer_mb: BTreeMap<String, f64>,
}

/// One benchmark result. Also the format of `baselines/<os>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    pub os: String,
    pub start_ms: f64,
    pub idle_mb: f64,
    pub startup_js_gzip_kb: f64,
    pub installers_mb: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Violation {
    OverBudget {
        metric: String,
        value: f64,
        limit: f64,
    },
    Regression {
        metric: String,
        value: f64,
        baseline: f64,
        pct: f64,
    },
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Violation::OverBudget {
                metric,
                value,
                limit,
            } => write!(f, "{metric}: {value:.1} is over the budget of {limit:.1}"),
            Violation::Regression {
                metric,
                value,
                baseline,
                pct,
            } => write!(
                f,
                "{metric}: {value:.1} is {pct:.1}% worse than the baseline {baseline:.1}"
            ),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid {path}: {message}")]
    Invalid { path: PathBuf, message: String },
    #[error("cannot start {path}: {source}")]
    Spawn {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("the app exited before printing {READY_LINE}")]
    ExitedBeforeReady,
    #[error("the app did not print {READY_LINE} within {0:?}")]
    NotReady(Duration),
    #[error("cannot read process information: {0}")]
    System(String),
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> ProbeError + '_ {
    move |source| ProbeError::Io {
        path: path.to_path_buf(),
        source,
    }
}

pub fn load_budgets(path: &Path) -> Result<Budgets, ProbeError> {
    let text = fs::read_to_string(path).map_err(io_error(path))?;
    toml::from_str(&text).map_err(|e| ProbeError::Invalid {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

pub fn load_measurement(path: &Path) -> Result<Measurement, ProbeError> {
    let text = fs::read_to_string(path).map_err(io_error(path))?;
    serde_json::from_str(&text).map_err(|e| ProbeError::Invalid {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

/// Compares each number with its budget, and with the baseline when given.
/// A number with no budget for this OS or installer kind is only compared
/// with the baseline.
pub fn check(m: &Measurement, budgets: &Budgets, baseline: Option<&Measurement>) -> Vec<Violation> {
    let mut metrics: Vec<(String, f64, Option<f64>, Option<f64>)> = vec![
        (
            "start_ms".into(),
            m.start_ms,
            Some(budgets.start_ms),
            baseline.map(|b| b.start_ms),
        ),
        (
            "idle_mb".into(),
            m.idle_mb,
            budgets.idle_mb.get(&m.os).copied(),
            baseline.map(|b| b.idle_mb),
        ),
        (
            "startup_js_gzip_kb".into(),
            m.startup_js_gzip_kb,
            Some(budgets.startup_js_gzip_kb),
            baseline.map(|b| b.startup_js_gzip_kb),
        ),
    ];
    for (kind, size) in &m.installers_mb {
        metrics.push((
            format!("installer_mb.{kind}"),
            *size,
            budgets.installer_mb.get(kind).copied(),
            baseline.and_then(|b| b.installers_mb.get(kind).copied()),
        ));
    }

    let mut out = Vec::new();
    for (metric, value, limit, base) in metrics {
        if let Some(limit) = limit
            && value > limit
        {
            out.push(Violation::OverBudget {
                metric: metric.clone(),
                value,
                limit,
            });
        }
        if let Some(base) = base
            && base > 0.0
        {
            let pct = (value - base) * 100.0 / base;
            if pct > budgets.regression_pct {
                out.push(Violation::Regression {
                    metric,
                    value,
                    baseline: base,
                    pct,
                });
            }
        }
    }
    out
}

/// Middle value; the mean of the two middle values for an even count.
pub fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    }
}

/// Gzip size in KiB of the JavaScript `index.html` loads at start: scripts
/// and module preloads. Chunks loaded later with `import()` are not counted.
pub fn startup_js_gzip_kb(dist_dir: &Path) -> Result<f64, ProbeError> {
    let index = dist_dir.join("index.html");
    let html = fs::read_to_string(&index).map_err(io_error(&index))?;

    let mut files = BTreeSet::new();
    for tag in TAG.captures_iter(&html) {
        let mut attrs: BTreeMap<String, String> = BTreeMap::new();
        for a in ATTR.captures_iter(&tag[0]) {
            attrs.insert(a[1].to_ascii_lowercase(), a[2].to_string());
        }
        let url = match tag[1].to_ascii_lowercase().as_str() {
            "script" => attrs.get("src"),
            _ if attrs.get("rel").is_some_and(|r| {
                r.split_whitespace()
                    .any(|r| r.eq_ignore_ascii_case("modulepreload"))
            }) =>
            {
                attrs.get("href")
            }
            _ => None,
        };
        if let Some(url) = url
            && !url.contains("://")
        {
            files.insert(
                url.trim_start_matches("./")
                    .trim_start_matches('/')
                    .to_string(),
            );
        }
    }

    let mut total = 0usize;
    for file in files {
        let path = dist_dir.join(&file);
        let bytes = fs::read(&path).map_err(io_error(&path))?;
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(&bytes).map_err(io_error(&path))?;
        total += gz.finish().map_err(io_error(&path))?.len();
    }
    Ok(total as f64 / 1024.0)
}

/// Size in MiB of each installer under `bundle_dir`, by kind. When a kind
/// has several files, the largest counts.
pub fn installer_sizes_mb(bundle_dir: &Path) -> Result<BTreeMap<String, f64>, ProbeError> {
    let mut sizes = BTreeMap::new();
    let mut dirs = vec![bundle_dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).map_err(io_error(&dir))? {
            let entry = entry.map_err(io_error(&dir))?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(io_error(&path))?;
            if file_type.is_dir() {
                dirs.push(path);
                continue;
            }
            let Some(kind) = path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .filter(|e| INSTALLER_KINDS.contains(&e.as_str()))
            else {
                continue;
            };
            let mb = entry.metadata().map_err(io_error(&path))?.len() as f64 / MIB;
            let largest = sizes.entry(kind).or_insert(0.0_f64);
            *largest = largest.max(mb);
        }
    }
    Ok(sizes)
}

/// Starts the app, waits up to 30 s for it to draw its first frame, waits
/// `idle_wait`, then returns `(start_ms, idle_mb)` and stops the app.
pub fn measure_launch(app: &Path, idle_wait: Duration) -> Result<(f64, f64), ProbeError> {
    measure_launch_with(app, Duration::from_secs(30), idle_wait)
}

/// [`measure_launch`] with a chosen wait for the ready line.
pub fn measure_launch_with(
    app: &Path,
    ready_timeout: Duration,
    idle_wait: Duration,
) -> Result<(f64, f64), ProbeError> {
    let spawned_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let started = Instant::now();
    let mut child = Command::new(app)
        .env("DEA_PERF_PROBE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|source| ProbeError::Spawn {
            path: app.to_path_buf(),
            source,
        })?;
    let pid = Pid::from_u32(child.id());

    // Read stdout on a thread, and keep draining it after the ready line so
    // the app never blocks on a full pipe.
    let stdout = child.stdout.take().expect("stdout is piped");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut ready = false;
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if !ready && line.trim() == READY_LINE {
                ready = true;
                let _ = tx.send(true);
            }
        }
        if !ready {
            let _ = tx.send(false);
        }
    });

    let outcome = match rx.recv_timeout(ready_timeout) {
        Ok(true) => Ok(started.elapsed().as_secs_f64() * 1000.0),
        Ok(false) | Err(RecvTimeoutError::Disconnected) => Err(ProbeError::ExitedBeforeReady),
        Err(RecvTimeoutError::Timeout) => Err(ProbeError::NotReady(ready_timeout)),
    };
    let start_ms = match outcome {
        Ok(ms) => ms,
        Err(e) => {
            stop(&mut child, pid, spawned_at);
            return Err(e);
        }
    };

    std::thread::sleep(idle_wait);
    let idle = app_processes(pid, spawned_at).map(|(_, bytes)| bytes as f64 / MIB);
    stop(&mut child, pid, spawned_at);
    Ok((start_ms, idle?))
}

/// The app process, its descendants, and on macOS the WebKit processes
/// started since the launch (they run under launchd, not under the app).
/// Returns their ids and their total memory in bytes, measured as the OS
/// monitor shows it (see [`process_memory_bytes`]).
fn app_processes(root: Pid, spawned_at: u64) -> Result<(Vec<Pid>, u64), ProbeError> {
    let mut sys = System::new().map_err(|e| ProbeError::System(format!("{e:?}")))?;
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let processes = sys.processes();

    let mut members: HashSet<Pid> = HashSet::from([root]);
    loop {
        let before = members.len();
        for (pid, p) in processes {
            if p.parent().is_some_and(|parent| members.contains(&parent)) {
                members.insert(*pid);
            }
        }
        if members.len() == before {
            break;
        }
    }
    if cfg!(target_os = "macos") {
        for (pid, p) in processes {
            if p.name().to_string_lossy().starts_with("com.apple.WebKit.")
                && p.start_time() >= spawned_at
            {
                members.insert(*pid);
            }
        }
    }

    // Resident size is only a fallback for a process that cannot be read.
    let bytes = members
        .iter()
        .filter_map(|pid| processes.get(pid).map(|p| (pid, p)))
        .map(|(pid, p)| process_memory_bytes(pid.as_u32()).unwrap_or_else(|| p.memory()))
        .sum();
    Ok((members.into_iter().collect(), bytes))
}

fn stop(child: &mut std::process::Child, root: Pid, spawned_at: u64) {
    if let Ok((pids, _)) = app_processes(root, spawned_at)
        && let Ok(mut sys) = System::new()
    {
        sys.refresh_processes(ProcessesToUpdate::Some(&pids), false);
        for pid in pids.iter().filter(|p| **p != root) {
            if let Some(p) = sys.process(*pid) {
                p.kill();
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}
