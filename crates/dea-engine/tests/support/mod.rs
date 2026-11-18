//! Fakes for the Vault service: an authenticator, a prompter and a clock
//! that tests control.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use dea_engine::{Clock, PasswordPrompt, Prompter, VaultService, VaultStatus};
use dea_vault::{
    AuthError, AuthMethod, Authenticator, FileBackend, KdfParams, MemoryBackend, Policy,
    VaultBackend,
};
use secrecy::SecretString;

/// Answers authentication with scripted results (success when the script
/// runs out), and records every reason it was shown.
pub struct FakeAuthenticator {
    pub method: AuthMethod,
    pub results: Mutex<VecDeque<Result<(), AuthError>>>,
    pub reasons: Arc<Mutex<Vec<String>>>,
    /// When set, every call waits here until the gate opens.
    pub hold: Option<Arc<Gate>>,
}

impl FakeAuthenticator {
    pub fn native(results: Vec<Result<(), AuthError>>) -> (Self, Arc<Mutex<Vec<String>>>) {
        let reasons = Arc::new(Mutex::new(Vec::new()));
        let auth = FakeAuthenticator {
            method: AuthMethod::DeviceOwner,
            results: Mutex::new(results.into()),
            reasons: Arc::clone(&reasons),
            hold: None,
        };
        (auth, reasons)
    }

    pub fn linux() -> Self {
        FakeAuthenticator {
            method: AuthMethod::VaultPassword,
            results: Mutex::new(VecDeque::new()),
            reasons: Arc::new(Mutex::new(Vec::new())),
            hold: None,
        }
    }
}

impl Authenticator for FakeAuthenticator {
    fn method(&self) -> AuthMethod {
        self.method
    }

    fn authenticate(&self, reason: &str) -> Result<(), AuthError> {
        if self.method == AuthMethod::VaultPassword {
            return Err(AuthError::Unavailable);
        }
        self.reasons.lock().unwrap().push(reason.to_string());
        if let Some(hold) = &self.hold {
            hold.wait();
        }
        self.results.lock().unwrap().pop_front().unwrap_or(Ok(()))
    }
}

/// Holds callers until opened, then lets every one through.
#[derive(Default)]
pub struct Gate {
    open: Mutex<bool>,
    opened: Condvar,
}

impl Gate {
    pub fn wait(&self) {
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.opened.wait(open).unwrap();
        }
    }

    pub fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.opened.notify_all();
    }
}

/// Answers password prompts from a script (`None` = the person cancels)
/// and records every prompt.
#[derive(Default)]
pub struct FakePrompter {
    pub answers: Mutex<VecDeque<Option<&'static str>>>,
    pub prompts: Mutex<Vec<(&'static str, PasswordPrompt)>>,
}

impl FakePrompter {
    pub fn answering(answers: &[Option<&'static str>]) -> Arc<Self> {
        Arc::new(FakePrompter {
            answers: Mutex::new(answers.iter().copied().collect()),
            prompts: Mutex::new(Vec::new()),
        })
    }

    fn answer(&self, kind: &'static str, prompt: &PasswordPrompt) -> Option<SecretString> {
        self.prompts.lock().unwrap().push((kind, prompt.clone()));
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .flatten()
            .map(SecretString::from)
    }
}

impl Prompter for FakePrompter {
    fn vault_password(&self, prompt: &PasswordPrompt) -> Option<SecretString> {
        self.answer("vault_password", prompt)
    }

    fn new_vault_password(&self, prompt: &PasswordPrompt) -> Option<SecretString> {
        self.answer("new_vault_password", prompt)
    }
}

/// A clock that moves only when told to.
pub struct ManualClock {
    start: (Instant, SystemTime),
    offset: Mutex<Duration>,
}

impl ManualClock {
    pub fn new() -> Arc<Self> {
        Arc::new(ManualClock {
            start: (Instant::now(), SystemTime::now()),
            offset: Mutex::new(Duration::ZERO),
        })
    }

    pub fn advance(&self, by: Duration) {
        *self.offset.lock().unwrap() += by;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.start.0 + *self.offset.lock().unwrap()
    }

    fn wall(&self) -> SystemTime {
        self.start.1 + *self.offset.lock().unwrap()
    }
}

pub type Statuses = Arc<Mutex<Vec<VaultStatus>>>;

/// A service over an in-memory store.
pub fn service(
    dir: &Path,
    auth: FakeAuthenticator,
    prompter: Arc<FakePrompter>,
    clock: Arc<ManualClock>,
) -> (Arc<VaultService>, Statuses) {
    let backend = VaultBackend::Memory(Arc::new(MemoryBackend::default()));
    service_with(dir, backend, auth, prompter, clock)
}

/// A service over the encrypted file, as on Linux without Secret Service.
pub fn file_service(
    dir: &Path,
    prompter: Arc<FakePrompter>,
    clock: Arc<ManualClock>,
) -> (Arc<VaultService>, Statuses) {
    let file = FileBackend::open(&dir.join("vault.enc"), KdfParams::insecure_for_tests());
    service_with(
        dir,
        VaultBackend::File(Arc::new(file)),
        FakeAuthenticator::linux(),
        prompter,
        clock,
    )
}

fn service_with(
    dir: &Path,
    backend: VaultBackend,
    auth: FakeAuthenticator,
    prompter: Arc<FakePrompter>,
    clock: Arc<ManualClock>,
) -> (Arc<VaultService>, Statuses) {
    let statuses: Statuses = Arc::default();
    let seen = Arc::clone(&statuses);
    let service = VaultService::new(
        dir,
        backend,
        Box::new(auth),
        prompter,
        Policy::default(),
        KdfParams::insecure_for_tests(),
        clock,
        Box::new(move |status| seen.lock().unwrap().push(status)),
    )
    .unwrap();
    (service, statuses)
}
