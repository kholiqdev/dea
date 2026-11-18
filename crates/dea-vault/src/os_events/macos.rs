//! `com.apple.screenIsLocked` from the distributed notification center, and
//! `NSWorkspaceWillSleepNotification` from the workspace.
#![allow(unsafe_code)] // Objective-C notification centers.

use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2_app_kit::{NSWorkspace, NSWorkspaceWillSleepNotification};
use objc2_foundation::{
    NSDate, NSDefaultRunLoopMode, NSDistributedNotificationCenter, NSNotification, NSRunLoop,
    NSString,
};

use super::{Emit, OsEvent, Stop};
use crate::VaultError;

const SCREEN_LOCKED: &str = "com.apple.screenIsLocked";

pub(super) fn start(emit: Emit, stopped: Arc<AtomicBool>) -> Result<Stop, VaultError> {
    let (ready, started) = mpsc::channel();
    std::thread::Builder::new()
        .name("dea-vault-os-events".into())
        .spawn(move || {
            let lock_emit = Arc::clone(&emit);
            let on_lock =
                RcBlock::new(move |_: NonNull<NSNotification>| lock_emit(OsEvent::ScreenLocked));
            let sleep_emit = Arc::clone(&emit);
            let on_sleep =
                RcBlock::new(move |_: NonNull<NSNotification>| sleep_emit(OsEvent::Sleeping));

            let distributed = NSDistributedNotificationCenter::defaultCenter();
            let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
            let name = NSString::from_str(SCREEN_LOCKED);
            // SAFETY: valid names; no object filter; a nil queue runs the
            // blocks on the delivering thread, and they only call `emit`,
            // which is Send and Sync.
            let (lock_token, sleep_token) = unsafe {
                (
                    distributed.addObserverForName_object_queue_usingBlock(
                        Some(&name),
                        None,
                        None,
                        &on_lock,
                    ),
                    workspace.addObserverForName_object_queue_usingBlock(
                        Some(NSWorkspaceWillSleepNotification),
                        None,
                        None,
                        &on_sleep,
                    ),
                )
            };
            let _ = ready.send(());

            // Distributed notifications arrive through a run loop.
            let run_loop = NSRunLoop::currentRunLoop();
            // SAFETY: Foundation's constant.
            let mode = unsafe { NSDefaultRunLoopMode };
            while !stopped.load(Ordering::SeqCst) {
                let until = NSDate::dateWithTimeIntervalSinceNow(0.5);
                if !run_loop.runMode_beforeDate(mode, &until) {
                    // No input source on this thread yet: wait instead of
                    // spinning.
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
            // SAFETY: the tokens returned above, removed from their centers.
            unsafe {
                distributed.removeObserver(lock_token.as_ref());
                workspace.removeObserver(sleep_token.as_ref());
            }
        })
        .map_err(|e| VaultError::Backend(format!("cannot watch for screen lock: {e}")))?;
    started
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| VaultError::Backend("the screen lock watcher did not start".into()))?;
    Ok(Box::new(|| {}))
}
