//! What the unit tests share: windows that pass their prompts on to the
//! test, a state that shows prompts in them, a confirmation to ask, and a
//! response to keep.

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Duration;

use bytes::Bytes;
use dea_engine::vault::VaultService;
use dea_engine::{ResponseStore, SendConfirmation, Sent, UseError, VaultProvider};
use dea_http::{HttpVersion, ResponseBody, Timing};

use crate::state::AppState;
use crate::vault::{Prompt, PromptWindows, SendConfirmEvent, UiPrompter, VaultPromptEvent};

/// How long a test waits for a prompt or an answer before it fails.
pub const WAIT: Duration = Duration::from_secs(5);

/// The Vault password prompts the windows were shown.
pub type Shown = Receiver<(String, VaultPromptEvent)>;
/// The send confirmations the windows were shown.
pub type Confirms = Receiver<(String, SendConfirmEvent)>;
/// The ids of the send confirmations withdrawn from the windows.
pub type Withdrawn = Receiver<(String, u64)>;

/// Windows that pass each prompt on to the test. Only `open` windows exist.
pub struct RecordingWindows {
    vault: Sender<(String, VaultPromptEvent)>,
    sends: Sender<(String, SendConfirmEvent)>,
    withdrawn: Sender<(String, u64)>,
    open: Vec<&'static str>,
    focused: Option<&'static str>,
}

impl RecordingWindows {
    /// The `open` windows, `focused` having focus, what they are shown,
    /// and the confirmations withdrawn from them.
    pub fn new(
        open: &[&'static str],
        focused: Option<&'static str>,
    ) -> (Self, Shown, Confirms, Withdrawn) {
        let (vault, shown) = channel();
        let (sends, confirms) = channel();
        let (withdrawn, withdrawals) = channel();
        let windows = Self {
            vault,
            sends,
            withdrawn,
            open: open.to_vec(),
            focused,
        };
        (windows, shown, confirms, withdrawals)
    }
}

impl PromptWindows for RecordingWindows {
    fn focused(&self) -> Option<String> {
        self.focused.map(str::to_string)
    }

    fn show(&self, window: &str, prompt: Prompt) -> bool {
        let label = window.to_string();
        self.open.contains(&window)
            && match prompt {
                Prompt::Vault(event) => self.vault.send((label, event)).is_ok(),
                Prompt::Send(event) => self.sends.send((label, event)).is_ok(),
            }
    }

    fn withdraw(&self, window: &str, id: u64) {
        if self.open.contains(&window) {
            let _ = self.withdrawn.send((window.to_string(), id));
        }
    }
}

/// No test sends a request: opening the Vault fails the test.
struct NeverOpened;

impl VaultProvider for NeverOpened {
    fn open(&self) -> Result<Arc<VaultService>, UseError> {
        panic!("the Vault was opened")
    }
}

/// State kept in `app_data` that shows its prompts in `windows`.
pub fn state_with(app_data: &Path, windows: impl PromptWindows + 'static) -> Arc<AppState> {
    let state = AppState::new(
        app_data.to_path_buf(),
        UiPrompter::new(windows),
        NeverOpened,
    );
    Arc::new(state)
}

/// A `POST` to production that uses secrets.
pub fn confirmation() -> SendConfirmation {
    SendConfirmation {
        method: "POST".into(),
        url: "https://api.test/v1/x".into(),
        environment: "production".into(),
        can_skip: false,
        secrets: true,
    }
}

/// Keeps a `200` JSON response with `body` in `store` under `key`.
pub fn kept(store: &ResponseStore, key: &str, body: &str) {
    store.insert(
        key,
        Sent {
            response: dea_http::Response {
                version: HttpVersion::Http11,
                status: 200,
                reason: "OK".into(),
                headers: vec![("content-type".into(), "application/json".into())],
                body: ResponseBody::Memory(Bytes::from(body.to_string())),
                truncated: false,
                content_encoding: None,
                timing: Timing::default(),
                url: "http://example.test/".into(),
                redirects: Vec::new(),
            },
            url: "http://example.test/".into(),
            cookies_not_saved: None,
        },
    );
}

/// Runs `f` on another thread; what it returns comes back on the receiver.
pub fn on_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Receiver<T> {
    let (done, result) = channel();
    thread::spawn(move || {
        let _ = done.send(f());
    });
    result
}
