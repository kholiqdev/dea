//! The real operating system keychain. Runs only with DEA_KEYCHAIN_TESTS=1,
//! so a developer's `cargo test` never writes to their keychain.

use dea_vault::{Backend, KeychainBackend};

fn enabled() -> bool {
    std::env::var_os("DEA_KEYCHAIN_TESTS").is_some_and(|v| v == "1")
}

/// A backend on a service of its own, whose entries are deleted on drop,
/// even when an assertion fails.
struct Scratch {
    backend: KeychainBackend,
    accounts: Vec<String>,
}

impl Scratch {
    fn new() -> Self {
        let service = format!("dev.dea.desktop.test-{}", uuid::Uuid::new_v4().simple());
        Scratch {
            backend: KeychainBackend::new(&service).expect("keychain available"),
            accounts: Vec::new(),
        }
    }

    fn account(&mut self, name: &str) -> String {
        self.accounts.push(name.to_string());
        name.to_string()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for account in &self.accounts {
            let _ = self.backend.delete(account);
        }
    }
}

/// A value shaped like an RSA-4096 private key in PEM form.
fn pem(len: usize) -> String {
    let mut out = String::from("-----BEGIN OPENSSH PRIVATE KEY-----\n");
    let mut i = 0usize;
    while out.len() < len - 34 {
        out.push(char::from(b'A' + (i % 26) as u8));
        i += 1;
        if i.is_multiple_of(70) {
            out.push('\n');
        }
    }
    out.push_str("\n-----END OPENSSH PRIVATE KEY-----");
    out
}

#[test]
fn secret_round_trips_in_the_os_keychain() {
    if !enabled() {
        return;
    }
    let mut s = Scratch::new();
    let account = s.account("p/staging/access_token");
    assert!(s.backend.get(&account).unwrap().is_none());
    s.backend.set(&account, "abc123").unwrap();
    assert_eq!(s.backend.get(&account).unwrap().unwrap().as_str(), "abc123");
    s.backend.set(&account, "changed").unwrap();
    assert_eq!(
        s.backend.get(&account).unwrap().unwrap().as_str(),
        "changed"
    );
    s.backend.delete(&account).unwrap();
    assert!(s.backend.get(&account).unwrap().is_none());
    s.backend.delete(&account).unwrap();
}

#[test]
fn large_secret_round_trips_in_chunks() {
    if !enabled() {
        return;
    }
    let mut s = Scratch::new();
    let account = s.account("p/production/deploy_key");
    s.account("p/production/deploy_key#2");
    s.account("p/production/deploy_key#3");
    let key = pem(4096);
    s.backend.set(&account, &key).unwrap();
    assert_eq!(s.backend.get(&account).unwrap().unwrap().as_str(), key);
}

#[test]
fn shrinking_a_secret_removes_old_chunks() {
    if !enabled() {
        return;
    }
    let mut s = Scratch::new();
    let account = s.account("p/production/deploy_key");
    let second = s.account("p/production/deploy_key#2");
    s.account("p/production/deploy_key#3");
    s.backend.set(&account, &pem(4096)).unwrap();
    assert!(s.backend.get(&second).unwrap().is_some());
    s.backend.set(&account, "short now").unwrap();
    assert_eq!(
        s.backend.get(&account).unwrap().unwrap().as_str(),
        "short now"
    );
    assert!(s.backend.get(&second).unwrap().is_none());
    s.backend.delete(&account).unwrap();
}

#[test]
fn unicode_environment_names_work() {
    if !enabled() {
        return;
    }
    let mut s = Scratch::new();
    let account = s.account("p/Produksi 🚀/token");
    s.backend.set(&account, "nilai rahasia ✓").unwrap();
    assert_eq!(
        s.backend.get(&account).unwrap().unwrap().as_str(),
        "nilai rahasia ✓"
    );
}
