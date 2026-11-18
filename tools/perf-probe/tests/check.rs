use std::collections::BTreeMap;
use std::path::Path;

use perf_probe::{Budgets, Measurement, Violation, check, load_budgets, median};

fn budgets() -> Budgets {
    load_budgets(&Path::new(env!("CARGO_MANIFEST_DIR")).join("budgets.toml")).unwrap()
}

fn measurement(
    os: &str,
    start: f64,
    idle: f64,
    js: f64,
    installers: &[(&str, f64)],
) -> Measurement {
    Measurement {
        os: os.into(),
        start_ms: start,
        idle_mb: idle,
        startup_js_gzip_kb: js,
        installers_mb: installers
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect::<BTreeMap<_, _>>(),
    }
}

#[test]
fn budgets_file_matches_the_phase_1_targets() {
    let b = budgets();
    // CI budgets: macOS runners are virtual machines about twice as slow
    // to start WebKit; 1.0 s is checked by hand on reference hardware.
    assert_eq!(b.start_ms["macos"], 2000.0);
    assert_eq!(b.start_ms["windows"], 1000.0);
    assert_eq!(b.start_ms["linux"], 1000.0);
    assert_eq!(b.startup_js_gzip_kb, 250.0);
    assert_eq!(b.regression_pct, 10.0);
    assert_eq!(b.idle_mb["macos"], 150.0);
    assert_eq!(b.idle_mb["windows"], 200.0);
    assert_eq!(b.idle_mb["linux"], 200.0);
    for kind in ["dmg", "exe", "msi", "deb", "rpm"] {
        assert_eq!(b.installer_mb[kind], 30.0, "{kind}");
    }
    assert_eq!(b.installer_mb["appimage"], 120.0);
}

#[test]
fn within_budget_has_no_violations() {
    let m = measurement("macos", 600.0, 120.0, 180.0, &[("dmg", 18.0)]);
    assert!(check(&m, &budgets(), None).is_empty());
}

#[test]
fn over_budget_is_reported_per_metric() {
    let m = measurement("macos", 600.0, 160.0, 180.0, &[]);
    assert_eq!(
        check(&m, &budgets(), None),
        [Violation::OverBudget {
            metric: "idle_mb".into(),
            value: 160.0,
            limit: 150.0
        }]
    );
    let m = measurement("windows", 600.0, 160.0, 180.0, &[]);
    assert!(check(&m, &budgets(), None).is_empty());
}

#[test]
fn every_metric_has_a_budget() {
    let m = measurement(
        "linux",
        1200.0,
        210.0,
        260.0,
        &[("deb", 31.0), ("appimage", 119.0)],
    );
    let metrics: Vec<String> = check(&m, &budgets(), None)
        .into_iter()
        .map(|v| match v {
            Violation::OverBudget { metric, .. } => metric,
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        metrics,
        [
            "start_ms",
            "idle_mb",
            "startup_js_gzip_kb",
            "installer_mb.deb"
        ]
    );
}

#[test]
fn regression_over_ten_percent_fails() {
    let baseline = measurement("macos", 500.0, 100.0, 100.0, &[("dmg", 10.0)]);
    let m = measurement("macos", 600.0, 100.0, 100.0, &[("dmg", 10.0)]);
    assert_eq!(
        check(&m, &budgets(), Some(&baseline)),
        [Violation::Regression {
            metric: "start_ms".into(),
            value: 600.0,
            baseline: 500.0,
            pct: 20.0
        }]
    );
    let m = measurement("macos", 540.0, 100.0, 100.0, &[("dmg", 10.0)]);
    assert!(check(&m, &budgets(), Some(&baseline)).is_empty());
}

#[test]
fn installer_regression_is_checked_per_kind() {
    let baseline = measurement("macos", 500.0, 100.0, 100.0, &[("dmg", 10.0)]);
    let m = measurement("macos", 500.0, 100.0, 100.0, &[("dmg", 12.0)]);
    let v = check(&m, &budgets(), Some(&baseline));
    assert!(
        matches!(&v[..], [Violation::Regression { metric, .. }] if metric == "installer_mb.dmg"),
        "{v:?}"
    );
}

#[test]
fn missing_baseline_skips_regression_check() {
    let m = measurement("macos", 900.0, 140.0, 240.0, &[("dmg", 29.0)]);
    assert!(check(&m, &budgets(), None).is_empty());
}

#[test]
fn median_of_five_runs() {
    assert_eq!(median(&[900.0, 300.0, 500.0, 400.0, 1200.0]), 500.0);
    assert_eq!(median(&[1.0, 4.0, 2.0, 3.0]), 2.5);
}

#[test]
fn measurement_round_trips_as_json() {
    let m = measurement("linux", 700.0, 150.0, 120.5, &[("deb", 12.0)]);
    let json = serde_json::to_string(&m).unwrap();
    assert!(json.contains("\"startup_js_gzip_kb\":120.5"), "{json}");
    assert_eq!(serde_json::from_str::<Measurement>(&json).unwrap(), m);
}

#[test]
fn start_budget_depends_on_the_os() {
    let m = measurement("macos", 1500.0, 100.0, 100.0, &[]);
    assert!(check(&m, &budgets(), None).is_empty());
    let m = measurement("windows", 1500.0, 100.0, 100.0, &[]);
    assert_eq!(
        check(&m, &budgets(), None),
        [Violation::OverBudget {
            metric: "start_ms".into(),
            value: 1500.0,
            limit: 1000.0
        }]
    );
}
