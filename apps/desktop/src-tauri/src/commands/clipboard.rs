//! Putting text the window shows on the clipboard.

use dea_engine::Clipboard;
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Manager};

use crate::error::{IpcError, IpcErrorKind};
use crate::state::AppState;

fn copy(clipboard: &dyn Clipboard, text: &str) -> Result<(), IpcError> {
    clipboard.set_text(text).map_err(|message| IpcError {
        kind: IpcErrorKind::Internal,
        message,
    })
}

/// Puts `text` on the clipboard as it is. For text the window already
/// shows, so never a secret value.
#[tauri::command]
pub async fn copy_text(app: AppHandle, text: String) -> Result<(), IpcError> {
    spawn_blocking(move || {
        let state = app.state::<AppState>();
        copy(&*state.vault.clipboard(), &text)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct Fake {
        content: Mutex<Option<String>>,
        fail: bool,
    }

    impl Clipboard for Fake {
        fn text(&self) -> Option<String> {
            self.content.lock().unwrap().clone()
        }

        fn set_text(&self, text: &str) -> Result<(), String> {
            if self.fail {
                return Err("no clipboard".into());
            }
            *self.content.lock().unwrap() = Some(text.to_string());
            Ok(())
        }

        fn set_secret_text(&self, _text: &str) -> Result<(), String> {
            unreachable!("copy_text never marks its text secret")
        }

        fn clear(&self) {}
    }

    #[test]
    fn copy_text_puts_the_text_on_the_clipboard() {
        let clipboard = Fake::default();
        copy(&clipboard, "https://api.test/x?t=••••••••").unwrap();
        assert_eq!(
            clipboard.text().as_deref(),
            Some("https://api.test/x?t=••••••••")
        );
    }

    #[test]
    fn copy_text_reports_a_clipboard_that_fails() {
        let clipboard = Fake {
            fail: true,
            ..Fake::default()
        };
        let error = copy(&clipboard, "x").unwrap_err();
        assert_eq!(error.message, "no clipboard");
    }
}
