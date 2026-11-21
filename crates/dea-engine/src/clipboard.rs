//! Copying a secret value, and clearing it again after 30 s if the person
//! has not copied something else meanwhile (F1-FON-08 #9).

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub const CLIPBOARD_CLEAR_AFTER: Duration = Duration::from_secs(30);

pub trait Clipboard: Send + Sync {
    fn text(&self) -> Option<String>;
    /// Sets `text` as it is: not marked secret, and never cleared by Dea.
    fn set_text(&self, text: &str) -> Result<(), String>;
    /// Sets `text`, marked where the OS allows so clipboard history and
    /// sync skip it.
    fn set_secret_text(&self, text: &str) -> Result<(), String>;
    fn clear(&self);
}

/// The OS clipboard. One instance lives as long as this value: on Linux
/// the copied text is served by that instance, and would be gone once it
/// was dropped where no clipboard manager takes it over.
#[derive(Default)]
pub struct SystemClipboard {
    inner: Mutex<Option<arboard::Clipboard>>,
}

impl SystemClipboard {
    fn with<T>(
        &self,
        f: impl FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error>,
    ) -> Result<T, String> {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if inner.is_none() {
            *inner = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
        }
        let clipboard = inner.as_mut().ok_or("no clipboard")?;
        f(clipboard).map_err(|e| e.to_string())
    }
}

impl Clipboard for SystemClipboard {
    fn text(&self) -> Option<String> {
        self.with(arboard::Clipboard::get_text).ok()
    }

    fn set_text(&self, text: &str) -> Result<(), String> {
        self.with(|clipboard| clipboard.set_text(text))
    }

    fn set_secret_text(&self, text: &str) -> Result<(), String> {
        self.with(|clipboard| {
            let set = clipboard.set();
            #[cfg(target_os = "macos")]
            let set = arboard::SetExtApple::exclude_from_history(set);
            #[cfg(windows)]
            let set = arboard::SetExtWindows::exclude_from_history(set);
            #[cfg(target_os = "linux")]
            let set = arboard::SetExtLinux::exclude_from_history(set);
            set.text(text)
        })
    }

    fn clear(&self) {
        let _ = self.with(arboard::Clipboard::clear);
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
