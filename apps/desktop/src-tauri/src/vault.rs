//! The one Vault of the app, and the dialogs each window is asked through:
//! the Vault password and the production send confirmation. The Vault is
//! opened by the first command that needs it, so starting Dea never touches
//! the keychain, and it is shared by every window and project.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread::{self, ThreadId};
use std::time::Duration;

use dea_engine::requests::CancellationToken;
use dea_engine::vault::{
    PasswordPrompt, PromptProblem, Prompter, SecretString, SystemClipboard, UseError, VaultError,
    VaultService, app_vault_has_values, open_app_vault,
};
use dea_engine::{SendAnswer, SendConfirmation, VaultProvider};
use dea_project::Project;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::error::IpcError;
use crate::state::AppState;

/// The event every window gets when the Vault locks or unlocks, with a
/// `VaultStatus`.
pub const VAULT_STATUS: &str = "vault-status";
/// The event that asks one window for the Vault password, with a
/// [`VaultPromptEvent`].
pub const VAULT_PROMPT: &str = "vault-prompt";
/// The event that asks one window whether to send a request to
/// production, with a [`SendConfirmEvent`].
pub const SEND_CONFIRM: &str = "send-confirm";
/// The event that tells one window a send confirmation was answered
/// without it, so its dialog closes, with a [`SendConfirmWithdrawn`].
pub const SEND_CONFIRM_WITHDRAWN: &str = "send-confirm-withdrawn";
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
        Ok(self.open(app)?)
    }

    /// [`AppVault::get_or_open`], failing with the Vault's own error.
    fn open(&self, app: &AppHandle) -> Result<Arc<VaultService>, VaultError> {
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

    /// Where Vault password prompts and send confirmations go and their
    /// answers come back.
    pub fn prompter(&self) -> &UiPrompter {
        &self.prompter
    }

    /// [`AppVault::prompter`], for the request service to ask with too.
    pub fn shared_prompter(&self) -> Arc<dyn Prompter> {
        Arc::clone(&self.prompter) as Arc<dyn Prompter>
    }

    /// The clipboard `Copy` uses.
    pub fn clipboard(&self) -> Arc<SystemClipboard> {
        Arc::clone(&self.clipboard)
    }
}

/// Gives the request service the app's Vault, opened the first time a send
/// needs it, so starting Dea never reaches the keychain.
pub struct VaultOnDemand(pub AppHandle);

impl VaultProvider for VaultOnDemand {
    fn open(&self) -> Result<Arc<VaultService>, UseError> {
        Ok(self.0.state::<AppState>().vault.open(&self.0)?)
    }
}

/// The windows a prompt can be shown in: the app's own, or a recorder in
/// tests.
pub trait PromptWindows: Send + Sync {
    /// The label of the window that has focus, if any.
    fn focused(&self) -> Option<String>;
    /// Shows `prompt` in `window`. False when the window is gone, so no
    /// answer can come.
    fn show(&self, window: &str, prompt: Prompt) -> bool;
    /// Tells `window` that send confirmation `id` no longer waits for it.
    fn withdraw(&self, window: &str, id: u64);
}

impl<R: Runtime> PromptWindows for AppHandle<R> {
    fn focused(&self) -> Option<String> {
        self.webview_windows()
            .into_iter()
            .find(|(_, window)| window.is_focused().unwrap_or(false))
            .map(|(label, _)| label)
    }

    fn show(&self, window: &str, prompt: Prompt) -> bool {
        self.get_webview_window(window).is_some()
            && self.emit_to(window, prompt.event(), prompt).is_ok()
    }

    fn withdraw(&self, window: &str, id: u64) {
        // A window that is gone has no dialog to close.
        let _ = self.emit_to(window, SEND_CONFIRM_WITHDRAWN, SendConfirmWithdrawn { id });
    }
}

/// No window can show a Vault password prompt.
#[cfg(test)]
pub struct NoWindows;

#[cfg(test)]
impl PromptWindows for NoWindows {
    fn focused(&self) -> Option<String> {
        None
    }

    fn show(&self, _window: &str, _prompt: Prompt) -> bool {
        false
    }

    fn withdraw(&self, _window: &str, _id: u64) {}
}

/// A dialog for one window to show and then answer by its id. Its payload
/// is the event inside.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Prompt {
    /// The Vault password, as `vault-prompt`.
    Vault(VaultPromptEvent),
    /// Whether to send to production, as `send-confirm`.
    Send(SendConfirmEvent),
}

impl Prompt {
    /// The name of the event that carries it.
    pub fn event(&self) -> &'static str {
        match self {
            Self::Vault(_) => VAULT_PROMPT,
            Self::Send(_) => SEND_CONFIRM,
        }
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

/// A production send confirmation for one window to show and then answer
/// with `answer_send_confirmation`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SendConfirmEvent {
    /// Identifies the answer.
    pub id: u64,
    #[serde(flatten)]
    pub confirmation: SendConfirmation,
}

/// A send confirmation that was cancelled, or answered by Dea, before its
/// window answered: the window closes its dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SendConfirmWithdrawn {
    pub id: u64,
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

/// Asks in a window of Dea for the Vault password (Linux) and whether to
/// send a request to production. Each prompt goes to the window that asked,
/// as `vault-prompt` or `send-confirm`, and the asking thread waits until
/// that window answers or closes, or the send that asked is cancelled.
pub struct UiPrompter {
    windows: Box<dyn PromptWindows>,
    next_id: AtomicU64,
    /// Who each thread asks for while it runs inside `asking` or `sending`.
    requesters: Mutex<HashMap<ThreadId, Requester>>,
    /// Prompts waiting for an answer, by id.
    waiting: Mutex<HashMap<u64, Waiting>>,
}

/// The window a prompt goes to, and the send that asks, if one does.
#[derive(Clone)]
struct Requester {
    window: String,
    send: Option<CancellationToken>,
}

struct Waiting {
    window: String,
    /// Cancels the prompt, with [`UiPrompter::withdraw_cancelled`].
    send: Option<CancellationToken>,
    /// What was sent, to send again to a page that missed it.
    prompt: Prompt,
    answer: Answer,
}

/// Where the answer to a prompt goes. Dropping it wakes the asker with no
/// answer.
enum Answer {
    Password(Sender<Option<SecretString>>),
    Send(Sender<SendAnswer>),
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
        self.asking_for(
            Requester {
                window: window.to_string(),
                send: None,
            },
            f,
        )
    }

    /// Runs `f` for a send of `window` that `send` cancels: every prompt it
    /// causes on this thread goes to `window`, and is cancelled with the
    /// send by [`UiPrompter::withdraw_cancelled`].
    pub fn sending<T>(&self, window: &str, send: &CancellationToken, f: impl FnOnce() -> T) -> T {
        self.asking_for(
            Requester {
                window: window.to_string(),
                send: Some(send.clone()),
            },
            f,
        )
    }

    fn asking_for<T>(&self, requester: Requester, f: impl FnOnce() -> T) -> T {
        let thread = thread::current().id();
        let previous = guard(&self.requesters).insert(thread, requester);
        let _restore = Restore {
            requesters: &self.requesters,
            thread,
            previous,
        };
        f()
    }

    /// Answers Vault password prompt `id`; `None` cancels it. An unknown
    /// id, such as a prompt already cancelled, is ignored.
    pub fn answer(&self, id: u64, password: Option<String>) {
        match self.take(id) {
            Some(Waiting {
                answer: Answer::Password(answer),
                ..
            }) => {
                let _ = answer.send(password.map(SecretString::from));
            }
            // A password does not answer a confirmation: it is cancelled.
            Some(waiting) => self.dismiss([(id, waiting)]),
            None => {}
        }
    }

    /// Answers send confirmation `id`. An unknown id, such as a
    /// confirmation already cancelled, is ignored.
    pub fn answer_send(&self, id: u64, answer: SendAnswer) {
        match self.take(id) {
            Some(Waiting {
                answer: Answer::Send(sender),
                ..
            }) => {
                let _ = sender.send(answer);
            }
            // This does not answer a Vault password prompt: it is cancelled.
            Some(waiting) => self.dismiss([(id, waiting)]),
            None => {}
        }
    }

    /// Stops prompt `id` waiting.
    fn take(&self, id: u64) -> Option<Waiting> {
        guard(&self.waiting).remove(&id)
    }

    /// Cancels every prompt whose send was cancelled.
    pub fn withdraw_cancelled(&self) {
        let gone: Vec<(u64, Waiting)> = guard(&self.waiting)
            .extract_if(|_, waiting| {
                waiting
                    .send
                    .as_ref()
                    .is_some_and(CancellationToken::is_cancelled)
            })
            .collect();
        self.dismiss(gone);
    }

    /// Cancels every prompt of `window`, which has closed or started a new
    /// page: a reload drops the dialog and its listener without closing the
    /// window.
    pub fn cancel_window(&self, window: &str) {
        let gone: Vec<(u64, Waiting)> = guard(&self.waiting)
            .extract_if(|_, waiting| waiting.window == window)
            .collect();
        self.dismiss(gone);
    }

    /// Ends prompts that stopped waiting without an answer from their
    /// window: each asker wakes with no answer, and each confirmation is
    /// withdrawn from its window, so its dialog closes.
    fn dismiss(&self, gone: impl IntoIterator<Item = (u64, Waiting)>) {
        for (id, waiting) in gone {
            // Dropping the sender wakes the asker.
            drop(waiting.answer);
            if let Prompt::Send(_) = waiting.prompt {
                self.windows.withdraw(&waiting.window, id);
            }
        }
    }

    /// Sends `window` every Vault password prompt still waiting for it
    /// again. A page that starts listening calls this, as a prompt sent
    /// while it loaded is lost and would keep its asker waiting. A window
    /// that is gone has its prompts cancelled.
    pub fn resend(&self, window: &str) {
        self.resend_where(window, |prompt| matches!(prompt, Prompt::Vault(_)));
    }

    /// [`UiPrompter::resend`] for the send confirmations of `window`.
    pub fn resend_confirmations(&self, window: &str) {
        self.resend_where(window, |prompt| matches!(prompt, Prompt::Send(_)));
    }

    fn resend_where(&self, window: &str, kind: impl Fn(&Prompt) -> bool) {
        let prompts: Vec<(u64, Prompt)> = guard(&self.waiting)
            .iter()
            .filter(|(_, waiting)| waiting.window == window && kind(&waiting.prompt))
            .map(|(id, waiting)| (*id, waiting.prompt.clone()))
            .collect();
        for (id, prompt) in prompts {
            if !self.show(window, id, prompt) {
                self.dismiss(self.take(id).map(|waiting| (id, waiting)));
            }
        }
    }

    /// Shows prompt `id` in `window`. A confirmation that stopped waiting
    /// meanwhile is withdrawn once more after it: its withdrawal may have
    /// reached the window first.
    fn show(&self, window: &str, id: u64, prompt: Prompt) -> bool {
        let confirmation = matches!(prompt, Prompt::Send(_));
        if !self.windows.show(window, prompt) {
            return false;
        }
        if confirmation && !guard(&self.waiting).contains_key(&id) {
            self.windows.withdraw(window, id);
        }
        true
    }

    /// Shows the prompt `prompt` makes from its id in the window this
    /// thread asks for, and waits for the answer that `answer` carries.
    /// `None` when no answer comes: the window is gone, closes or reloads,
    /// or the send that asks was cancelled.
    fn ask<A>(
        &self,
        prompt: impl FnOnce(u64) -> Prompt,
        answer: impl FnOnce(Sender<A>) -> Answer,
    ) -> Option<A> {
        let requester = guard(&self.requesters)
            .get(&thread::current().id())
            .cloned();
        let Requester { window, send } = requester.unwrap_or_else(|| Requester {
            window: self
                .windows
                .focused()
                .unwrap_or_else(|| MAIN_WINDOW.to_string()),
            send: None,
        });
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, answered) = mpsc::channel();
        let prompt = prompt(id);
        guard(&self.waiting).insert(
            id,
            Waiting {
                window: window.clone(),
                send: send.clone(),
                prompt: prompt.clone(),
                answer: answer(sender),
            },
        );
        // Checked once it waits: a send cancelled later is withdrawn.
        let cancelled = send.as_ref().is_some_and(CancellationToken::is_cancelled);
        if cancelled || !self.show(&window, id, prompt) {
            // A resend may have shown it meanwhile.
            self.dismiss(self.take(id).map(|waiting| (id, waiting)));
            return None;
        }
        answered.recv().ok()
    }

    fn ask_password(&self, kind: VaultPromptKind, prompt: &PasswordPrompt) -> Option<SecretString> {
        let event = |id| {
            Prompt::Vault(VaultPromptEvent {
                id,
                kind,
                reason: prompt.reason.clone(),
                problem: prompt.problem.as_ref().map(VaultPromptProblem::from),
            })
        };
        self.ask(event, Answer::Password).flatten()
    }
}

impl Prompter for UiPrompter {
    fn vault_password(&self, prompt: &PasswordPrompt) -> Option<SecretString> {
        self.ask_password(VaultPromptKind::Password, prompt)
    }

    fn new_vault_password(&self, prompt: &PasswordPrompt) -> Option<SecretString> {
        self.ask_password(VaultPromptKind::NewPassword, prompt)
    }

    /// Nothing is sent without an answer: a window that closes or reloads,
    /// or a send that is cancelled, answers [`SendAnswer::Cancel`].
    fn confirm_send(&self, confirmation: &SendConfirmation) -> SendAnswer {
        let event = |id| {
            Prompt::Send(SendConfirmEvent {
                id,
                confirmation: confirmation.clone(),
            })
        };
        self.ask(event, Answer::Send).unwrap_or(SendAnswer::Cancel)
    }
}

/// Puts back who a thread asked for before `asking`, even when `f` panics:
/// blocking threads are reused.
struct Restore<'a> {
    requesters: &'a Mutex<HashMap<ThreadId, Requester>>,
    thread: ThreadId,
    previous: Option<Requester>,
}

impl Drop for Restore<'_> {
    fn drop(&mut self) {
        let mut requesters = guard(self.requesters);
        match self.previous.take() {
            Some(requester) => requesters.insert(self.thread, requester),
            None => requesters.remove(&self.thread),
        };
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{Receiver, channel};

    use serde_json::json;

    use super::*;
    use crate::testing::{Confirms, RecordingWindows, Shown, WAIT, Withdrawn, confirmation};

    /// A prompter for the `open` windows, `focused` having focus, the
    /// Vault prompts and confirmations they are shown, and the
    /// confirmations withdrawn from them.
    fn recorded(
        open: &[&'static str],
        focused: Option<&'static str>,
    ) -> (Arc<UiPrompter>, Shown, Confirms, Withdrawn) {
        let (windows, shown, confirms, withdrawn) = RecordingWindows::new(open, focused);
        (
            Arc::new(UiPrompter::new(windows)),
            shown,
            confirms,
            withdrawn,
        )
    }

    fn windows_with(
        open: &[&'static str],
        focused: Option<&'static str>,
    ) -> (Arc<UiPrompter>, Shown) {
        let (prompter, shown, _, _) = recorded(open, focused);
        (prompter, shown)
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

    /// Asks on another thread whether to send, for send `send` of
    /// `window`.
    fn confirm_from(
        prompter: &Arc<UiPrompter>,
        window: &'static str,
        send: &CancellationToken,
    ) -> Receiver<SendAnswer> {
        let (prompter, send) = (Arc::clone(prompter), send.clone());
        crate::testing::on_thread(move || {
            prompter.sending(window, &send, || prompter.confirm_send(&confirmation()))
        })
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

    #[test]
    fn send_confirmation_goes_to_the_window_that_sent() {
        let (prompter, shown, confirms, withdrawn) = recorded(&["main", "project-1"], Some("main"));
        let asked = confirm_from(&prompter, "project-1", &CancellationToken::new());

        let (window, event) = confirms.recv_timeout(WAIT).unwrap();
        assert_eq!(window, "project-1");
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            json!({
                "id": event.id,
                "method": "POST",
                "url": "https://api.test/v1/x",
                "environment": "production",
                "canSkip": false,
                "secrets": true,
            })
        );
        // An answer for another prompt changes nothing.
        prompter.answer_send(event.id + 1, SendAnswer::Send);
        prompter.answer_send(event.id, SendAnswer::SendAndStopAsking);
        assert_eq!(
            asked.recv_timeout(WAIT).unwrap(),
            SendAnswer::SendAndStopAsking
        );
        assert!(
            confirms.try_recv().is_err(),
            "only one confirmation was sent"
        );
        assert!(shown.try_recv().is_err(), "no Vault prompt was sent");
        assert!(
            withdrawn.try_recv().is_err(),
            "an answered confirmation is not withdrawn"
        );
    }

    #[test]
    fn a_withdrawn_confirmation_is_withdrawn_from_the_window_that_asked() {
        let (prompter, _shown, confirms, withdrawn) =
            recorded(&["main", "project-1"], Some("main"));
        let send = CancellationToken::new();
        let asked = confirm_from(&prompter, "project-1", &send);
        let (_, event) = confirms.recv_timeout(WAIT).unwrap();

        send.cancel();
        prompter.withdraw_cancelled();
        assert_eq!(asked.recv_timeout(WAIT).unwrap(), SendAnswer::Cancel);
        assert_eq!(
            withdrawn.recv_timeout(WAIT).unwrap(),
            ("project-1".to_string(), event.id)
        );
        assert!(withdrawn.try_recv().is_err(), "no other window is told");
        assert_eq!(
            serde_json::to_value(SendConfirmWithdrawn { id: event.id }).unwrap(),
            json!({ "id": event.id })
        );
    }

    #[test]
    fn a_confirmation_withdrawn_while_shown_is_withdrawn_after_it() {
        let (prompter, _shown, confirms, withdrawn) = recorded(&["main"], None);
        // Confirmation 99 no longer waits: its withdrawal may have reached
        // the window before the confirmation did, so it comes again after.
        let event = SendConfirmEvent {
            id: 99,
            confirmation: confirmation(),
        };
        assert!(prompter.show("main", 99, Prompt::Send(event.clone())));
        assert_eq!(
            confirms.recv_timeout(WAIT).unwrap(),
            ("main".to_string(), event)
        );
        assert_eq!(
            withdrawn.recv_timeout(WAIT).unwrap(),
            ("main".to_string(), 99)
        );
    }

    #[test]
    fn a_confirmation_without_an_answer_is_a_cancel() {
        let (prompter, _shown, confirms, withdrawn) = recorded(&["main", "project-1"], None);
        let send = CancellationToken::new();
        let withdrawn_from = |window: &str, id: u64| {
            assert_eq!(
                withdrawn.recv_timeout(WAIT).unwrap(),
                (window.to_string(), id)
            );
        };

        let closed = confirm_from(&prompter, "project-1", &send);
        let (_, event) = confirms.recv_timeout(WAIT).unwrap();
        prompter.cancel_window("project-1");
        assert_eq!(closed.recv_timeout(WAIT).unwrap(), SendAnswer::Cancel);
        withdrawn_from("project-1", event.id);

        // A password does not answer whether to send.
        let mixed_up = confirm_from(&prompter, "main", &send);
        let (_, event) = confirms.recv_timeout(WAIT).unwrap();
        prompter.answer(event.id, Some("correct horse".into()));
        assert_eq!(mixed_up.recv_timeout(WAIT).unwrap(), SendAnswer::Cancel);
        withdrawn_from("main", event.id);

        // A window that is already gone cannot answer, so nothing waits.
        let gone = confirm_from(&prompter, "project-9", &send);
        assert_eq!(gone.recv_timeout(WAIT).unwrap(), SendAnswer::Cancel);
        assert!(withdrawn.try_recv().is_err(), "a gone window is not told");
    }

    #[test]
    fn a_cancelled_send_withdraws_only_its_own_prompts() {
        let (prompter, shown, confirms, withdrawn) = recorded(&["main"], None);
        let (first, second) = (CancellationToken::new(), CancellationToken::new());
        let first_asked = confirm_from(&prompter, "main", &first);
        let (_, first_event) = confirms.recv_timeout(WAIT).unwrap();
        let second_asked = confirm_from(&prompter, "main", &second);
        let (_, second_event) = confirms.recv_timeout(WAIT).unwrap();
        // The Vault password a send asks for (Linux) belongs to it too.
        let password = {
            let (prompter, first) = (Arc::clone(&prompter), first.clone());
            on_thread(move || {
                prompter.sending("main", &first, || prompter.vault_password(&prompt(None)))
            })
        };
        shown.recv_timeout(WAIT).unwrap();

        // Withdrawing before cancelling changes nothing.
        prompter.withdraw_cancelled();
        first.cancel();
        prompter.withdraw_cancelled();
        assert_eq!(first_asked.recv_timeout(WAIT).unwrap(), SendAnswer::Cancel);
        assert!(!password.got_password());
        assert_eq!(
            withdrawn.recv_timeout(WAIT).unwrap(),
            ("main".to_string(), first_event.id)
        );
        assert!(withdrawn.try_recv().is_err(), "only its confirmation");

        // The other send still waits for its answer.
        prompter.answer_send(second_event.id, SendAnswer::Send);
        assert_eq!(second_asked.recv_timeout(WAIT).unwrap(), SendAnswer::Send);

        // A send cancelled before it asks is never asked.
        let late = confirm_from(&prompter, "main", &first);
        assert_eq!(late.recv_timeout(WAIT).unwrap(), SendAnswer::Cancel);
        assert!(confirms.try_recv().is_err(), "nothing was shown");
        // A resend may have shown it meanwhile, so it is withdrawn anyway.
        assert_eq!(withdrawn.recv_timeout(WAIT).unwrap().0, "main");
    }

    #[test]
    fn prompts_and_confirmations_are_sent_again_apart() {
        let (prompter, shown, confirms, _) = recorded(&["project-1"], None);
        let confirmed = confirm_from(&prompter, "project-1", &CancellationToken::new());
        let (_, confirmation) = confirms.recv_timeout(WAIT).unwrap();
        let asked = ask_from(&prompter, "project-1", None);
        let (_, vault_prompt) = shown.recv_timeout(WAIT).unwrap();

        // Each listener asks for what it missed once it listens.
        prompter.resend_confirmations("project-1");
        assert_eq!(
            confirms.recv_timeout(WAIT).unwrap(),
            ("project-1".to_string(), confirmation.clone())
        );
        assert!(
            shown.try_recv().is_err(),
            "the Vault prompt is not sent again"
        );
        prompter.resend("project-1");
        assert_eq!(
            shown.recv_timeout(WAIT).unwrap(),
            ("project-1".to_string(), vault_prompt.clone())
        );
        assert!(
            confirms.try_recv().is_err(),
            "the confirmation is not sent again"
        );

        prompter.answer_send(confirmation.id, SendAnswer::Send);
        assert_eq!(confirmed.recv_timeout(WAIT).unwrap(), SendAnswer::Send);
        prompter.answer(vault_prompt.id, None);
        assert!(!asked.got_password());
    }
}
