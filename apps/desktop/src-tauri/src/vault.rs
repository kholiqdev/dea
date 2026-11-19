//! The one Vault of the app, and the Vault password dialog of each window.
//! The Vault is opened by the first command that needs it, so starting Dea
//! never touches the keychain, and it is shared by every window and
//! project.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread::{self, ThreadId};
use std::time::Duration;

use dea_engine::vault::{
    PasswordPrompt, PromptProblem, Prompter, SecretString, SystemClipboard, VaultService,
    app_vault_has_values, open_app_vault,
};
use dea_project::Project;
use serde::Serialize;
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::error::IpcError;
use crate::state::AppState;

/// The event every window gets when the Vault locks or unlocks, with a
/// `VaultStatus`.
pub const VAULT_STATUS: &str = "vault-status";
/// The event that asks one window for the Vault password, with a
/// [`VaultPromptEvent`].
pub const VAULT_PROMPT: &str = "vault-prompt";
/// Where a prompt goes when no window asked for it and none has focus.
const MAIN_WINDOW: &str = "main";

fn guard<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The one Vault of the app, opened locked the first time it is needed.
pub struct AppVault {
    app_data: PathBuf,
    prompter: Arc<UiPrompter>,
    /// Lives as long as the app: on Linux this instance serves what was
    /// copied until the clipboard is cleared.
    clipboard: Arc<SystemClipboard>,
    vault: OnceLock<Arc<VaultService>>,
    /// Held while the Vault is being opened, so it is opened once.
    opening: Mutex<()>,
}

impl AppVault {
    /// A Vault in `<app_data>/vault` that asks for passwords with
    /// `prompter`. Nothing is opened yet.
    pub fn new(app_data: PathBuf, prompter: UiPrompter) -> Self {
        Self {
            app_data,
            prompter: Arc::new(prompter),
            clipboard: Arc::new(SystemClipboard::default()),
            vault: OnceLock::new(),
            opening: Mutex::new(()),
        }
    }

    /// The Vault, opened locked the first time with the security settings.
    /// Each status change goes to every window as `vault-status`. Opening
    /// reaches the keychain, so call this off the main thread.
    pub fn get_or_open(&self, app: &AppHandle) -> Result<Arc<VaultService>, IpcError> {
        if let Some(vault) = self.vault.get() {
            return Ok(Arc::clone(vault));
        }
        let _opening = guard(&self.opening);
        if let Some(vault) = self.vault.get() {
            return Ok(Arc::clone(vault));
        }
        let policy = app
            .state::<AppState>()
            .store
            .load_settings()
            .security
            .policy()?;
        let windows = app.clone();
        let opened = open_app_vault(
            &self.app_data,
            Arc::clone(&self.prompter) as Arc<dyn Prompter>,
            policy,
            Box::new(move |status| {
                let _ = windows.emit(VAULT_STATUS, status);
            }),
        )?;
        Ok(Arc::clone(self.vault.get_or_init(|| opened)))
    }

    /// The Vault, if a command has opened it.
    pub fn existing(&self) -> Option<Arc<VaultService>> {
        self.vault.get().cloned()
    }

    /// Whether `environment` of `project` has values on this device: asked
    /// of the open Vault or, before the Vault is first used, read from its
    /// index on disk, so the keychain is not reached. A damaged index counts
    /// as having values.
    pub fn has_values(&self, project: &Project, environment: &str) -> bool {
        match self.existing() {
            Some(vault) => vault.has_values(project, environment),
            None => app_vault_has_values(&self.app_data, project, environment),
        }
    }

    /// Where Vault password prompts go and their answers come back.
    pub fn prompter(&self) -> &UiPrompter {
        &self.prompter
    }

    /// The clipboard `Copy` uses.
    pub fn clipboard(&self) -> Arc<SystemClipboard> {
        Arc::clone(&self.clipboard)
    }
}

/// The windows a Vault password prompt can be shown in: the app's own, or
/// a recorder in tests.
pub trait PromptWindows: Send + Sync {
    /// The label of the window that has focus, if any.
    fn focused(&self) -> Option<String>;
    /// Shows `prompt` in `window`. False when the window is gone, so no
    /// answer can come.
    fn show(&self, window: &str, prompt: VaultPromptEvent) -> bool;
}

impl<R: Runtime> PromptWindows for AppHandle<R> {
    fn focused(&self) -> Option<String> {
        self.webview_windows()
            .into_iter()
            .find(|(_, window)| window.is_focused().unwrap_or(false))
            .map(|(label, _)| label)
    }

    fn show(&self, window: &str, prompt: VaultPromptEvent) -> bool {
        self.get_webview_window(window).is_some()
            && self.emit_to(window, VAULT_PROMPT, prompt).is_ok()
    }
}

/// A Vault password dialog for one window to show and then answer with
/// `vault_answer_prompt`. Never holds a password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultPromptEvent {
    /// Identifies the answer.
    pub id: u64,
    pub kind: VaultPromptKind,
    /// What waits on the answer, such as `unlock the Vault`.
    pub reason: String,
    /// Why the previous answer was not accepted.
    pub problem: Option<VaultPromptProblem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultPromptKind {
    /// The Vault password.
    Password,
    /// A new Vault password, the first time one is needed.
    NewPassword,
}

/// [`PromptProblem`] as the dialog reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum VaultPromptProblem {
    WrongPassword,
    /// A new password with fewer than `min` characters.
    TooShort {
        min: usize,
    },
    /// Too many wrong answers: the dialog can only be closed for `seconds`.
    RetryAfter {
        seconds: u64,
    },
}

impl From<&PromptProblem> for VaultPromptProblem {
    fn from(problem: &PromptProblem) -> Self {
        match problem {
            PromptProblem::WrongPassword => Self::WrongPassword,
            PromptProblem::TooShort { min } => Self::TooShort { min: *min },
            PromptProblem::RetryAfter(wait) => Self::RetryAfter {
                seconds: whole_seconds(*wait),
            },
        }
    }
}

/// Rounded up, so the dialog never offers a try too early.
fn whole_seconds(wait: Duration) -> u64 {
    (wait.as_secs() + u64::from(wait.subsec_nanos() > 0)).max(1)
}

/// Asks for the Vault password in a window of Dea (Linux). Each prompt goes
/// as `vault-prompt` to the window that asked, and the asking thread waits
/// until that window answers or closes.
pub struct UiPrompter {
    windows: Box<dyn PromptWindows>,
    next_id: AtomicU64,
    /// The window each thread asks for while it runs inside `asking`.
    requesters: Mutex<HashMap<ThreadId, String>>,
    /// Prompts waiting for an answer, by id.
    waiting: Mutex<HashMap<u64, Waiting>>,
}

struct Waiting {
    window: String,
    /// What was sent, to send again to a page that missed it.
    event: VaultPromptEvent,
    answer: Sender<Option<SecretString>>,
}

impl UiPrompter {
    /// Shows prompts in `windows`.
    pub fn new(windows: impl PromptWindows + 'static) -> Self {
        Self {
            windows: Box::new(windows),
            next_id: AtomicU64::new(1),
            requesters: Mutex::new(HashMap::new()),
            waiting: Mutex::new(HashMap::new()),
        }
    }

    /// Runs `f`, sending every prompt it causes on this thread to `window`.
    pub fn asking<T>(&self, window: &str, f: impl FnOnce() -> T) -> T {
        let thread = thread::current().id();
        let previous = guard(&self.requesters).insert(thread, window.to_string());
        let _restore = Restore {
            requesters: &self.requesters,
            thread,
            previous,
        };
        f()
    }

    /// Answers prompt `id`; `None` cancels it. An unknown id, such as a
    /// prompt already cancelled, is ignored.
    pub fn answer(&self, id: u64, password: Option<String>) {
        let password = password.map(SecretString::from);
        if let Some(waiting) = guard(&self.waiting).remove(&id) {
            let _ = waiting.answer.send(password);
        }
    }

    /// Cancels every prompt of `window`, which has closed.
    pub fn cancel_window(&self, window: &str) {
        // A dropped sender wakes its prompt with no answer.
        guard(&self.waiting).retain(|_, waiting| waiting.window != window);
    }

    /// A page of `window` is loading. A reload drops the dialog and its
    /// listener without closing the window, so its prompts are cancelled
    /// when the new page starts.
    pub fn page_load(&self, window: &str, event: PageLoadEvent) {
        if event == PageLoadEvent::Started {
            self.cancel_window(window);
        }
    }

    /// Sends `window` every prompt still waiting for it again. A page that
    /// starts listening calls this, as a prompt sent while it loaded is
    /// lost and would keep its asker waiting. A window that is gone has its
    /// prompts cancelled.
    pub fn resend(&self, window: &str) {
        let events: Vec<VaultPromptEvent> = guard(&self.waiting)
            .values()
            .filter(|waiting| waiting.window == window)
            .map(|waiting| waiting.event.clone())
            .collect();
        for event in events {
            let id = event.id;
            if !self.windows.show(window, event) {
                guard(&self.waiting).remove(&id);
            }
        }
    }

    fn ask(&self, kind: VaultPromptKind, prompt: &PasswordPrompt) -> Option<SecretString> {
        let requester = guard(&self.requesters)
            .get(&thread::current().id())
            .cloned();
        let window = requester
            .or_else(|| self.windows.focused())
            .unwrap_or_else(|| MAIN_WINDOW.to_string());
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (answer, answered) = mpsc::channel();
        let event = VaultPromptEvent {
            id,
            kind,
            reason: prompt.reason.clone(),
            problem: prompt.problem.as_ref().map(VaultPromptProblem::from),
        };
        guard(&self.waiting).insert(
            id,
            Waiting {
                window: window.clone(),
                event: event.clone(),
                answer,
            },
        );
        if !self.windows.show(&window, event) {
            guard(&self.waiting).remove(&id);
            return None;
        }
        answered.recv().ok().flatten()
    }
}

impl Prompter for UiPrompter {
    fn vault_password(&self, prompt: &PasswordPrompt) -> Option<SecretString> {
        self.ask(VaultPromptKind::Password, prompt)
    }

    fn new_vault_password(&self, prompt: &PasswordPrompt) -> Option<SecretString> {
        self.ask(VaultPromptKind::NewPassword, prompt)
    }
}

/// Puts back the window a thread asked for before `asking`, even when `f`
/// panics: blocking threads are reused.
struct Restore<'a> {
    requesters: &'a Mutex<HashMap<ThreadId, String>>,
    thread: ThreadId,
    previous: Option<String>,
}

impl Drop for Restore<'_> {
    fn drop(&mut self) {
        let mut requesters = guard(self.requesters);
        match self.previous.take() {
            Some(window) => requesters.insert(self.thread, window),
            None => requesters.remove(&self.thread),
        };
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{Receiver, channel};

    use serde_json::json;

    use super::*;

    const WAIT: Duration = Duration::from_secs(5);

    /// Windows that pass each prompt on to the test. Only `open` windows
    /// exist.
    struct Recorder {
        shown: Mutex<Sender<(String, VaultPromptEvent)>>,
        open: Vec<&'static str>,
        focused: Option<&'static str>,
    }

    impl PromptWindows for Recorder {
        fn focused(&self) -> Option<String> {
            self.focused.map(str::to_string)
        }

        fn show(&self, window: &str, prompt: VaultPromptEvent) -> bool {
            self.open.contains(&window)
                && guard(&self.shown)
                    .send((window.to_string(), prompt))
                    .is_ok()
        }
    }

    type Shown = Receiver<(String, VaultPromptEvent)>;

    fn windows_with(
        open: &[&'static str],
        focused: Option<&'static str>,
    ) -> (Arc<UiPrompter>, Shown) {
        let (shown, receiver) = channel();
        let windows = Recorder {
            shown: Mutex::new(shown),
            open: open.to_vec(),
            focused,
        };
        (Arc::new(UiPrompter::new(windows)), receiver)
    }

    fn prompt(problem: Option<PromptProblem>) -> PasswordPrompt {
        PasswordPrompt {
            reason: "unlock the Vault".into(),
            problem,
        }
    }

    /// A prompt running on another thread.
    struct Asked(Receiver<bool>);

    impl Asked {
        /// Whether the prompt returned a password. Fails instead of hanging
        /// when it never returns.
        fn got_password(&self) -> bool {
            self.0.recv_timeout(WAIT).expect("the prompt returned")
        }
    }

    fn on_thread(ask: impl FnOnce() -> Option<SecretString> + Send + 'static) -> Asked {
        let (done, asked) = channel();
        thread::spawn(move || {
            let _ = done.send(ask().is_some());
        });
        Asked(asked)
    }

    /// Asks for the Vault password on another thread, for `window`.
    fn ask_from(
        prompter: &Arc<UiPrompter>,
        window: &'static str,
        problem: Option<PromptProblem>,
    ) -> Asked {
        let prompter = Arc::clone(prompter);
        on_thread(move || prompter.asking(window, || prompter.vault_password(&prompt(problem))))
    }

    #[test]
    fn the_prompt_goes_to_the_window_that_asked() {
        let (prompter, shown) = windows_with(&["main", "project-1"], Some("main"));
        let asked = ask_from(&prompter, "project-1", None);

        let (window, event) = shown.recv_timeout(WAIT).unwrap();
        assert_eq!(window, "project-1");
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            json!({ "id": event.id, "kind": "password", "reason": "unlock the Vault", "problem": null })
        );
        // An answer for another prompt changes nothing.
        prompter.answer(event.id + 1, None);
        prompter.answer(event.id, Some("correct horse".into()));
        assert!(asked.got_password());
        assert!(shown.try_recv().is_err(), "only one prompt was sent");
    }

    #[test]
    fn a_prompt_nobody_asked_for_goes_to_the_focused_window() {
        let (prompter, shown) = windows_with(&["main", "project-2"], Some("project-2"));
        let asker = Arc::clone(&prompter);
        let asked = on_thread(move || asker.new_vault_password(&prompt(None)));
        let (window, event) = shown.recv_timeout(WAIT).unwrap();
        assert_eq!(window, "project-2");
        assert_eq!(event.kind, VaultPromptKind::NewPassword);
        prompter.answer(event.id, Some("correct horse".into()));
        assert!(asked.got_password());

        // No window has focus: the first window gets it.
        let (prompter, shown) = windows_with(&["main"], None);
        let asker = Arc::clone(&prompter);
        let asked = on_thread(move || asker.vault_password(&prompt(None)));
        let (window, event) = shown.recv_timeout(WAIT).unwrap();
        assert_eq!(window, "main");
        prompter.answer(event.id, None);
        assert!(!asked.got_password());
    }

    #[test]
    fn a_closed_window_cancels_its_prompt() {
        let (prompter, shown) = windows_with(&["main", "project-1"], None);
        let in_main = ask_from(&prompter, "main", None);
        let (_, main_prompt) = shown.recv_timeout(WAIT).unwrap();
        let in_project = ask_from(&prompter, "project-1", None);
        let (window, _) = shown.recv_timeout(WAIT).unwrap();
        assert_eq!(window, "project-1");

        prompter.cancel_window("project-1");
        assert!(!in_project.got_password());

        // The prompt of the other window is still open.
        prompter.answer(main_prompt.id, Some("correct horse".into()));
        assert!(in_main.got_password());

        // A window that is already gone cannot answer, so nothing waits.
        assert!(!ask_from(&prompter, "project-9", None).got_password());
    }

    #[test]
    fn a_reloaded_window_cancels_its_prompt() {
        let (prompter, shown) = windows_with(&["project-1"], None);

        // A page that finished loading keeps its dialog.
        let first = ask_from(&prompter, "project-1", None);
        let (_, event) = shown.recv_timeout(WAIT).unwrap();
        prompter.page_load("project-1", PageLoadEvent::Finished);
        prompter.answer(event.id, Some("correct horse".into()));
        assert!(first.got_password());

        // A reload drops the dialog and its listener without closing the
        // window, so the prompt is cancelled when the new page starts.
        let second = ask_from(&prompter, "project-1", None);
        shown.recv_timeout(WAIT).unwrap();
        prompter.page_load("project-1", PageLoadEvent::Started);
        assert!(!second.got_password());
    }

    #[test]
    fn a_new_page_gets_the_prompts_still_waiting_for_its_window() {
        let (prompter, shown) = windows_with(&["main", "project-1"], None);
        let in_project = ask_from(&prompter, "project-1", None);
        let (_, first) = shown.recv_timeout(WAIT).unwrap();
        let in_main = ask_from(&prompter, "main", None);
        let (window, main_prompt) = shown.recv_timeout(WAIT).unwrap();
        assert_eq!(window, "main");

        // The page of project-1 started listening after its prompt was
        // sent, so it asks again: only its own prompt comes, unchanged.
        prompter.resend("project-1");
        let (window, again) = shown.recv_timeout(WAIT).unwrap();
        assert_eq!(window, "project-1");
        assert_eq!(again, first);
        assert!(shown.try_recv().is_err(), "only project-1's prompt");

        // A window with nothing waiting gets nothing.
        prompter.resend("project-9");
        assert!(shown.try_recv().is_err());

        prompter.answer(first.id, Some("correct horse".into()));
        assert!(in_project.got_password());
        // An answered prompt is not sent again.
        prompter.resend("project-1");
        assert!(shown.try_recv().is_err());
        prompter.answer(main_prompt.id, None);
        assert!(!in_main.got_password());
    }

    #[test]
    fn retry_after_reaches_the_window_in_seconds() {
        let (prompter, shown) = windows_with(&["project-1"], None);
        let wait = Duration::from_millis(29_200);
        let asked = ask_from(
            &prompter,
            "project-1",
            Some(PromptProblem::RetryAfter(wait)),
        );

        let (window, event) = shown.recv_timeout(WAIT).unwrap();
        assert_eq!(window, "project-1");
        // Rounded up, so the dialog never offers a try too early.
        assert_eq!(
            serde_json::to_value(event.problem).unwrap(),
            json!({ "kind": "retryAfter", "seconds": 30 })
        );
        prompter.answer(event.id, None);
        assert!(!asked.got_password());
    }

    #[test]
    fn every_prompt_problem_has_a_kind() {
        let shape = |problem: PromptProblem| {
            serde_json::to_value(VaultPromptProblem::from(&problem)).unwrap()
        };
        assert_eq!(
            shape(PromptProblem::WrongPassword),
            json!({ "kind": "wrongPassword" })
        );
        assert_eq!(
            shape(PromptProblem::TooShort { min: 8 }),
            json!({ "kind": "tooShort", "min": 8 })
        );
        assert_eq!(
            shape(PromptProblem::RetryAfter(Duration::from_secs(30))),
            json!({ "kind": "retryAfter", "seconds": 30 })
        );
    }
}
