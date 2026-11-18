//! Copying a secret value, and clearing it again after 30 s if the person
//! has not copied something else meanwhile (F1-FON-08 #9).

use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub const CLIPBOARD_CLEAR_AFTER: Duration = Duration::from_secs(30);

pub trait Clipboard: Send + Sync {
    fn text(&self) -> Option<String>;
    /// Sets `text`, marked where the OS allows so clipboard history and
    /// sync skip it.
    fn set_secret_text(&self, text: &str) -> Result<(), String>;
    fn clear(&self);
}

/// The OS clipboard.
pub struct SystemClipboard;

impl Clipboard for SystemClipboard {
    fn text(&self) -> Option<String> {
        arboard::Clipboard::new().ok()?.get_text().ok()
    }

    fn set_secret_text(&self, text: &str) -> Result<(), String> {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        let set = clipboard.set();
        #[cfg(target_os = "macos")]
        let set = arboard::SetExtApple::exclude_from_history(set);
        #[cfg(windows)]
        let set = arboard::SetExtWindows::exclude_from_history(set);
        #[cfg(target_os = "linux")]
        let set = arboard::SetExtLinux::exclude_from_history(set);
        set.text(text).map_err(|e| e.to_string())
    }

    fn clear(&self) {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.clear();
        }
    }
}

fn digest(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

/// Puts `value` on the clipboard and clears it after `clear_after` if it is
/// still there. Only a hash of the value is kept while waiting.
pub(crate) fn copy_and_clear_later(
    clipboard: Arc<dyn Clipboard>,
    value: &str,
    clear_after: Duration,
) -> Result<(), String> {
    clipboard.set_secret_text(value)?;
    let copied = digest(value);
    std::thread::Builder::new()
        .name("dea-clipboard-clear".into())
        .spawn(move || {
            std::thread::sleep(clear_after);
            let current = clipboard.text().map(Zeroizing::new);
            if current.is_some_and(|text| digest(&text) == copied) {
                clipboard.clear();
            }
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}
