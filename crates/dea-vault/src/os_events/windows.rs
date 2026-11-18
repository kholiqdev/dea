//! Session lock through `WTSRegisterSessionNotification` on a message-only
//! window, and sleep through `PowerRegisterSuspendResumeNotification`.
#![allow(unsafe_code)] // Win32 window and power notifications.

use std::sync::mpsc;
use std::time::Duration;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::{
    DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, PowerRegisterSuspendResumeNotification,
    PowerUnregisterSuspendResumeNotification,
};
use windows_sys::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DEVICE_NOTIFY_CALLBACK, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, HWND_MESSAGE, MSG, PBT_APMSUSPEND, PostMessageW,
    PostQuitMessage, RegisterClassExW, SetWindowLongPtrW, TranslateMessage, WM_CLOSE, WM_DESTROY,
    WM_POWERBROADCAST, WM_WTSSESSION_CHANGE, WNDCLASSEXW, WTS_SESSION_LOCK,
};

use super::{Emit, OsEvent, Stop};
use crate::VaultError;

/// Class of the hidden window that receives session notifications.
pub const WINDOW_CLASS: &str = "DeaVaultOsEvents";

/// Which window message or power event means what.
pub(super) fn classify(msg: u32, wparam: usize) -> Option<OsEvent> {
    match msg {
        WM_WTSSESSION_CHANGE if wparam == WTS_SESSION_LOCK as usize => Some(OsEvent::ScreenLocked),
        WM_POWERBROADCAST if wparam == PBT_APMSUSPEND as usize => Some(OsEvent::Sleeping),
        _ => None,
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

pub(super) fn start(emit: Emit) -> Result<Stop, VaultError> {
    let (ready, started) = mpsc::channel::<Result<isize, String>>();
    std::thread::Builder::new()
        .name("dea-vault-os-events".into())
        .spawn(move || {
            // SAFETY: runs the window's whole life on this thread.
            unsafe { run(emit, &ready) }
        })
        .map_err(|e| VaultError::Backend(format!("cannot watch for screen lock: {e}")))?;
    let hwnd = started
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| VaultError::Backend("the screen lock watcher did not start".into()))?
        .map_err(VaultError::Backend)?;
    Ok(Box::new(move || {
        // SAFETY: posting to the watcher's own window; ignored if it is gone.
        unsafe { PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0) };
    }))
}

/// Creates the window and runs its message loop until `WM_CLOSE`.
unsafe fn run(emit: Emit, ready: &mpsc::Sender<Result<isize, String>>) {
    let class = wide(WINDOW_CLASS);
    // SAFETY: plain Win32 calls with valid, null-terminated strings. The
    // boxed `emit` and subscription parameters outlive the window and the
    // power registration, and are freed after both are gone.
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let window_class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        // Fails harmlessly when a second watcher registers the class again.
        RegisterClassExW(&window_class);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            let _ = ready.send(Err("cannot create the session notification window".into()));
            return;
        }
        let emit = Box::into_raw(Box::new(emit));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, emit as isize);
        WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
        let mut params = Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(power_callback),
            Context: emit.cast(),
        });
        let mut registration = std::ptr::null_mut();
        PowerRegisterSuspendResumeNotification(
            DEVICE_NOTIFY_CALLBACK,
            (&raw mut *params).cast(),
            &mut registration,
        );
        let _ = ready.send(Ok(hwnd as isize));

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if !registration.is_null() {
            PowerUnregisterSuspendResumeNotification(registration as isize);
        }
        drop(params);
        drop(Box::from_raw(emit));
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: called by Windows for our window; the user data is the boxed
    // `emit` set in `run`, alive until the message loop ends.
    unsafe {
        match msg {
            WM_CLOSE => {
                WTSUnRegisterSessionNotification(hwnd);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => match classify(msg, wparam) {
                Some(event) => {
                    let emit = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Emit;
                    if let Some(emit) = emit.as_ref() {
                        emit(event);
                    }
                    0
                }
                None => DefWindowProcW(hwnd, msg, wparam, lparam),
            },
        }
    }
}

unsafe extern "system" fn power_callback(
    context: *const core::ffi::c_void,
    kind: u32,
    _setting: *const core::ffi::c_void,
) -> u32 {
    if let Some(event) = classify(WM_POWERBROADCAST, kind as usize) {
        // SAFETY: the context is the boxed `emit`, alive while registered.
        if let Some(emit) = unsafe { context.cast::<Emit>().as_ref() } {
            emit(event);
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, WM_POWERBROADCAST, WM_WTSSESSION_CHANGE,
        WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
    };

    use super::*;

    #[test]
    fn session_lock_and_suspend_are_recognised() {
        assert_eq!(
            classify(WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK as usize),
            Some(OsEvent::ScreenLocked)
        );
        assert_eq!(
            classify(WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK as usize),
            None
        );
        assert_eq!(
            classify(WM_POWERBROADCAST, PBT_APMSUSPEND as usize),
            Some(OsEvent::Sleeping)
        );
        assert_eq!(
            classify(WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC as usize),
            None
        );
    }
}
