//! Sending a request of a window's project, cancelling it, and the
//! production confirmation a send may ask for first. Every send of a
//! window goes through `send_request`, so the confirmation always reaches
//! the window that sent.

use std::time::Instant;

use dea_engine::requests::CancellationToken;
use dea_engine::{EventForwarder, SendAnswer, SendEvent, SendOutcome, outcome_of_error};
use dea_project::{RequestFile, load_project};
use tauri::async_runtime::spawn_blocking;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::commands::requests::{rel, session_of, tree_of};
use crate::error::IpcError;
use crate::state::AppState;

/// The send of tab `id` of `window`; its response is kept under the same
/// key.
fn send_key(window: &str, id: &str) -> String {
    format!("{window}/{id}")
}

/// Starts send `key`, ending the one it replaces and any prompt that one
/// waits on.
fn start(state: &AppState, key: &str) -> CancellationToken {
    let token = state.sends.start(key);
    state.vault.prompter().withdraw_cancelled();
    token
}

/// Cancels send `key` and any prompt it waits on. An unknown key is
/// ignored.
fn cancel(state: &AppState, key: &str) {
    state.sends.cancel(key);
    state.vault.prompter().withdraw_cancelled();
}

/// Sends `request`, the draft of tab `id`, as if it were saved at `path`,
/// to the window's active environment. Progress goes to `events`, starting
/// with `started` just before the request leaves; a production environment
/// asks this window first. The body of a response is kept for the tab.
/// Fails only when the window has no readable project.
#[tauri::command]
pub async fn send_request(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    path: String,
    request: RequestFile,
    events: Channel<SendEvent>,
) -> Result<SendOutcome, IpcError> {
    let session = session_of(&app, window.label())?;
    let label = window.label().to_string();
    let key = send_key(&label, &id);
    let state = app.state::<AppState>();
    let token = start(&state, &key);

    let prepared = {
        let (app, token) = (app.clone(), token.clone());
        spawn_blocking(move || {
            let state = app.state::<AppState>();
            let project = load_project(&session.root)?.project;
            let loaded = tree_of(&app, &label, &session)?;
            let prepared = state.vault.prompter().sending(&label, &token, || {
                state.requests.prepare(
                    &project,
                    &loaded.tree,
                    &rel(&path),
                    &request,
                    session.active_environment.as_deref(),
                )
            });
            Ok::<_, IpcError>(prepared)
        })
        .await?
    };
    let outcome = match prepared {
        Err(e) => Err(e),
        // Cancelled while a dialog was open: nothing leaves.
        Ok(_) if token.is_cancelled() => Ok(SendOutcome::Cancelled),
        Ok(Err(e)) => Ok(outcome_of_error(&e)),
        Ok(Ok(prepared)) => {
            let _ = events.send(SendEvent::Started {
                method: prepared.method.clone(),
                url: prepared.url_redacted.clone(),
            });
            let mut forwarder = EventForwarder::new();
            let mut forward = |event| {
                if let Some(event) = forwarder.forward(event, Instant::now()) {
                    let _ = events.send(event);
                }
            };
            Ok(
                match state
                    .requests
                    .send(prepared, &token, Some(&mut forward))
                    .await
                {
                    Ok(sent) => state
                        .sends
                        .if_running(&key, &token, || state.responses.insert(&key, sent))
                        .map_or(SendOutcome::Cancelled, |response| SendOutcome::Response {
                            response,
                        }),
                    Err(e) => outcome_of_error(&e),
                },
            )
        }
    };
    state.sends.finish(&key, &token);
    outcome
}

/// Cancels the send of tab `id`, and its production confirmation if that
/// is still open. An unknown id is ignored.
#[tauri::command]
pub fn cancel_send(window: WebviewWindow, id: String) -> Result<(), IpcError> {
    cancel(&window.state::<AppState>(), &send_key(window.label(), &id));
    Ok(())
}

/// The window's answer to send confirmation `id`.
#[tauri::command]
pub fn answer_send_confirmation(
    app: AppHandle,
    id: u64,
    answer: SendAnswer,
) -> Result<(), IpcError> {
    app.state::<AppState>()
        .vault
        .prompter()
        .answer_send(id, answer);
    Ok(())
}

/// Sends the calling window every send confirmation still waiting for it.
/// A page calls this once it listens for them: one sent while it loaded
/// is lost, and its send would wait for good.
#[tauri::command]
pub fn resend_send_confirmations(app: AppHandle, window: WebviewWindow) -> Result<(), IpcError> {
    app.state::<AppState>()
        .vault
        .prompter()
        .resend_confirmations(window.label());
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dea_engine::Prompter;

    use super::*;
    use crate::testing::{RecordingWindows, WAIT, confirmation, on_thread, state_with};

    #[test]
    fn cancel_for_an_unknown_id_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        let (windows, _, _) = RecordingWindows::new(&["main"], None);
        let state = state_with(dir.path(), windows);
        cancel(&state, &send_key("main", "t9"));

        let running = start(&state, &send_key("main", "t1"));
        // Neither another tab nor the same tab of another window is it.
        cancel(&state, &send_key("main", "t9"));
        cancel(&state, &send_key("project-1", "t1"));
        assert!(!running.is_cancelled());
        cancel(&state, &send_key("main", "t1"));
        assert!(running.is_cancelled());
    }

    #[test]
    fn cancelling_a_send_answers_its_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let (windows, _, confirms) = RecordingWindows::new(&["main"], None);
        let state = state_with(dir.path(), windows);
        let send_from = |id: &str| {
            let send = start(&state, &send_key("main", id));
            let state = Arc::clone(&state);
            let asked = on_thread(move || {
                let prompter = state.vault.prompter();
                prompter.sending("main", &send, || prompter.confirm_send(&confirmation()))
            });
            let (_, event) = confirms.recv_timeout(WAIT).unwrap();
            (asked, event)
        };
        let (cancelled, _) = send_from("t1");
        let (other, other_event) = send_from("t2");

        cancel(&state, &send_key("main", "t1"));
        assert_eq!(cancelled.recv_timeout(WAIT).unwrap(), SendAnswer::Cancel);

        // The send of another tab still waits for its answer.
        state
            .vault
            .prompter()
            .answer_send(other_event.id, SendAnswer::Send);
        assert_eq!(other.recv_timeout(WAIT).unwrap(), SendAnswer::Send);

        // A new send from a tab ends the one waiting there.
        let (replaced, _) = send_from("t3");
        let newer = start(&state, &send_key("main", "t3"));
        assert_eq!(replaced.recv_timeout(WAIT).unwrap(), SendAnswer::Cancel);
        assert!(!newer.is_cancelled());
    }
}
