//! Locks the Vault when the screen locks or the device goes to sleep
//! (F1-FON-09 #2). Each OS reports these its own way; this module turns
//! them into one callback.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::VaultError;

#[cfg(windows)]
pub use self::windows::WINDOW_CLASS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsEvent {
    ScreenLocked,
    Sleeping,
}

pub(crate) type Emit = Arc<dyn Fn(OsEvent) + Send + Sync>;
type Stop = Box<dyn FnOnce() + Send>;

/// Watching stops when this is dropped. The callback is never called after
/// that, even if a watcher thread still sees an event.
pub struct OsEventsWatch {
    stopped: Arc<AtomicBool>,
    stop: Option<Stop>,
}

impl Drop for OsEventsWatch {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(stop) = self.stop.take() {
            stop();
        }
    }
}

/// Calls `on_event` when the screen locks or the device is about to sleep.
pub fn watch_os_events(
    on_event: impl Fn(OsEvent) + Send + Sync + 'static,
) -> Result<OsEventsWatch, VaultError> {
    let stopped = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stopped);
    let emit: Emit = Arc::new(move |event| {
        if !flag.load(Ordering::SeqCst) {
            on_event(event);
        }
    });
    let stop = start(emit, Arc::clone(&stopped))?;
    Ok(OsEventsWatch {
        stopped,
        stop: Some(stop),
    })
}

#[cfg(target_os = "macos")]
fn start(emit: Emit, stopped: Arc<AtomicBool>) -> Result<Stop, VaultError> {
    macos::start(emit, stopped)
}

#[cfg(windows)]
fn start(emit: Emit, _stopped: Arc<AtomicBool>) -> Result<Stop, VaultError> {
    windows::start(emit)
}

#[cfg(target_os = "linux")]
fn start(emit: Emit, stopped: Arc<AtomicBool>) -> Result<Stop, VaultError> {
    linux::start(emit, stopped)
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn start(_emit: Emit, _stopped: Arc<AtomicBool>) -> Result<Stop, VaultError> {
    Ok(Box::new(|| {}))
}
