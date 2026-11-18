//! A real screen lock notification reaches the watcher within one second.
//!
//! Runs only with DEA_OS_EVENT_TESTS=1: the notification is sent through
//! the system, so other apps on the machine (a password manager, say) may
//! lock too. Has its own `main` (`harness = false`), so on macOS the main
//! run loop can run while waiting, wherever the system delivers.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use dea_vault::{OsEvent, watch_os_events};

fn main() {
    if std::env::var_os("DEA_OS_EVENT_TESTS").is_none_or(|v| v != "1") {
        println!("os_events: skipped, set DEA_OS_EVENT_TESTS=1 to run");
        return;
    }
    a_screen_lock_reaches_the_callback_within_one_second();
    #[cfg(target_os = "macos")]
    sleep_reaches_the_callback();
}

fn a_screen_lock_reaches_the_callback_within_one_second() {
    let (tx, rx) = mpsc::channel();
    let _watch = watch_os_events(move |e| {
        let _ = tx.send(e);
    })
    .expect("watching starts");
    // Give the watcher time to subscribe before the event is sent.
    pump(Duration::from_millis(500));

    platform::send_screen_lock();
    let sent = Instant::now();
    loop {
        pump(Duration::from_millis(50));
        match rx.try_recv() {
            Ok(OsEvent::ScreenLocked) => break,
            Ok(other) => panic!("unexpected event {other:?}"),
            Err(_) if sent.elapsed() > Duration::from_secs(1) => {
                panic!("no ScreenLocked within 1 s")
            }
            Err(_) => {}
        }
    }
    println!("os_events: a_screen_lock_reaches_the_callback_within_one_second ... ok");
}

#[cfg(target_os = "macos")]
fn sleep_reaches_the_callback() {
    let (tx, rx) = mpsc::channel();
    let _watch = watch_os_events(move |e| {
        let _ = tx.send(e);
    })
    .expect("watching starts");
    pump(Duration::from_millis(500));
    platform::send_sleep();
    let sent = Instant::now();
    while sent.elapsed() < Duration::from_secs(1) {
        pump(Duration::from_millis(50));
        if let Ok(event) = rx.try_recv() {
            assert_eq!(event, OsEvent::Sleeping);
            println!("os_events: sleep_reaches_the_callback ... ok");
            return;
        }
    }
    panic!("no Sleeping within 1 s");
}

/// Lets the main run loop deliver notifications on macOS; elsewhere waits.
fn pump(for_how_long: Duration) {
    #[cfg(target_os = "macos")]
    platform::run_main_loop(for_how_long);
    #[cfg(not(target_os = "macos"))]
    std::thread::sleep(for_how_long);
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod platform {
    use std::time::Duration;

    use objc2_app_kit::{NSWorkspace, NSWorkspaceWillSleepNotification};
    use objc2_foundation::{
        NSDate, NSDefaultRunLoopMode, NSDistributedNotificationCenter, NSRunLoop, NSString,
    };

    pub fn send_screen_lock() {
        let name = NSString::from_str("com.apple.screenIsLocked");
        // SAFETY: a valid name, no object and no user info.
        unsafe {
            NSDistributedNotificationCenter::defaultCenter()
                .postNotificationName_object_userInfo_deliverImmediately(&name, None, None, true);
        }
    }

    pub fn send_sleep() {
        // SAFETY: AppKit's own notification name, posted in this process only.
        unsafe {
            NSWorkspace::sharedWorkspace()
                .notificationCenter()
                .postNotificationName_object(NSWorkspaceWillSleepNotification, None);
        }
    }

    pub fn run_main_loop(for_how_long: Duration) {
        let until = NSDate::dateWithTimeIntervalSinceNow(for_how_long.as_secs_f64());
        // SAFETY: the default mode constant from Foundation.
        let mode = unsafe { NSDefaultRunLoopMode };
        if !NSRunLoop::mainRunLoop().runMode_beforeDate(mode, &until) {
            std::thread::sleep(for_how_long);
        }
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod platform {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowExW, HWND_MESSAGE, PostMessageW, WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK,
    };

    pub fn send_screen_lock() {
        let class: Vec<u16> = dea_vault::WINDOW_CLASS
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: a null-terminated class name; posting to our own window.
        unsafe {
            let hwnd = FindWindowExW(
                HWND_MESSAGE,
                std::ptr::null_mut(),
                class.as_ptr(),
                std::ptr::null(),
            );
            assert!(!hwnd.is_null(), "the watcher window exists");
            PostMessageW(hwnd, WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK as usize, 0);
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    pub fn send_screen_lock() {
        let bus = zbus::blocking::Connection::session().expect("a session bus");
        bus.emit_signal(
            None::<&str>,
            "/org/freedesktop/ScreenSaver",
            "org.freedesktop.ScreenSaver",
            "ActiveChanged",
            &(true,),
        )
        .expect("signal sent");
    }
}
