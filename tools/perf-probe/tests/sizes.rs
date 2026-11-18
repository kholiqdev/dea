use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use flate2::Compression;
use flate2::write::GzEncoder;
use perf_probe::{installer_sizes_mb, startup_js_gzip_kb};

fn gzip_len(bytes: &[u8]) -> usize {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(bytes).unwrap();
    enc.finish().unwrap().len()
}

fn sized(path: &Path, bytes: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    File::create(path).unwrap().set_len(bytes).unwrap();
}

#[test]
fn startup_js_counts_only_files_referenced_by_index_html() {
    let dist = tempfile::tempdir().unwrap();
    let assets = dist.path().join("assets");
    fs::create_dir(&assets).unwrap();
    let index_js = "console.log('index');".repeat(200);
    let vendor_js = "export const v = 1;".repeat(300);
    fs::write(assets.join("index-a.js"), &index_js).unwrap();
    fs::write(assets.join("vendor-b.js"), &vendor_js).unwrap();
    fs::write(assets.join("requests-c.js"), "x".repeat(50_000)).unwrap();
    fs::write(assets.join("index.css"), "body{}".repeat(1000)).unwrap();
    fs::write(
        dist.path().join("index.html"),
        r#"<!doctype html>
<html>
  <head>
    <script type="module" crossorigin src="/assets/index-a.js"></script>
    <link rel="modulepreload" crossorigin href="/assets/vendor-b.js">
    <link rel="stylesheet" crossorigin href="/assets/index.css">
  </head>
  <body><div id="app"></div></body>
</html>
"#,
    )
    .unwrap();

    let expected = (gzip_len(index_js.as_bytes()) + gzip_len(vendor_js.as_bytes())) as f64 / 1024.0;
    let kb = startup_js_gzip_kb(dist.path()).unwrap();
    assert!((kb - expected).abs() < 1e-9, "{kb} vs {expected}");
}

#[test]
fn modulepreload_with_href_first_is_counted() {
    let dist = tempfile::tempdir().unwrap();
    fs::create_dir(dist.path().join("assets")).unwrap();
    fs::write(dist.path().join("assets/a.js"), "a".repeat(10)).unwrap();
    fs::write(
        dist.path().join("index.html"),
        "<link href='/assets/a.js' rel='modulepreload'>",
    )
    .unwrap();
    assert!(startup_js_gzip_kb(dist.path()).unwrap() > 0.0);
}

#[test]
fn missing_referenced_file_is_an_error() {
    let dist = tempfile::tempdir().unwrap();
    fs::write(
        dist.path().join("index.html"),
        r#"<script type="module" src="/assets/gone.js"></script>"#,
    )
    .unwrap();
    assert!(startup_js_gzip_kb(dist.path()).is_err());
}

#[test]
fn installer_sizes_are_grouped_by_extension() {
    let bundle = tempfile::tempdir().unwrap();
    let mib = 1024 * 1024;
    sized(&bundle.path().join("dmg/Dea_0.1.0_aarch64.dmg"), 18 * mib);
    sized(
        &bundle.path().join("appimage/Dea_0.1.0_amd64.AppImage"),
        90 * mib,
    );
    sized(&bundle.path().join("nsis/Dea_0.1.0_x64-setup.exe"), 5 * mib);
    sized(&bundle.path().join("msi/Dea_0.1.0_x64_en-US.msi"), 6 * mib);
    sized(
        &bundle
            .path()
            .join("macos/Dea.app/Contents/MacOS/dea-desktop"),
        40 * mib,
    );
    let sizes = installer_sizes_mb(bundle.path()).unwrap();
    assert_eq!(sizes.len(), 4, "{sizes:?}");
    assert_eq!(sizes["dmg"], 18.0);
    assert_eq!(sizes["appimage"], 90.0);
    assert_eq!(sizes["exe"], 5.0);
    assert_eq!(sizes["msi"], 6.0);
}

#[test]
fn largest_installer_of_a_kind_counts() {
    let bundle = tempfile::tempdir().unwrap();
    let mib = 1024 * 1024;
    sized(&bundle.path().join("deb/a.deb"), 3 * mib);
    sized(&bundle.path().join("deb/b.deb"), 7 * mib);
    assert_eq!(installer_sizes_mb(bundle.path()).unwrap()["deb"], 7.0);
}
