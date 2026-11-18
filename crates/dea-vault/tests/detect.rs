//! Which backend this machine gets. Runs only when DEA_EXPECT_BACKEND says
//! what to expect: `keychain` or `encrypted-file`.

use dea_vault::{BackendKind, KdfParams, open_native};

#[test]
fn detected_backend_matches_the_environment() {
    let Some(expected) = std::env::var_os("DEA_EXPECT_BACKEND") else {
        return;
    };
    let expected = match expected.to_str() {
        Some("keychain") => BackendKind::Keychain,
        Some("encrypted-file") => BackendKind::EncryptedFile,
        other => panic!("unknown DEA_EXPECT_BACKEND {other:?}"),
    };
    let dir = tempfile::tempdir().unwrap();
    let backend = open_native(
        "dev.dea.desktop.test-detect",
        dir.path(),
        KdfParams::insecure_for_tests(),
    )
    .unwrap();
    assert_eq!(backend.kind(), expected);
}
