//! Windows Hello through `UserConsentVerifier`, or the account password
//! through the Windows credential dialog when Hello is not set up.
#![allow(unsafe_code)] // Win32 and WinRT calls.

use windows::Security::Credentials::UI::{
    UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::IUserConsentVerifierInterop;
use windows::core::{HSTRING, factory};
use windows_future::IAsyncOperation;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, HANDLE};
use windows_sys::Win32::Security::Credentials::{
    CREDUI_INFOW, CREDUIWIN_ENUMERATE_CURRENT_USER, CredUIPromptForWindowsCredentialsW,
    CredUnPackAuthenticationBufferW,
};
use windows_sys::Win32::Security::{
    LOGON32_LOGON_INTERACTIVE, LOGON32_PROVIDER_DEFAULT, LogonUserW,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use zeroize::Zeroize;

use super::{AuthError, AuthMethod, Authenticator, WindowHandle};

pub(super) struct WindowsAuthenticator {
    pub window: Option<WindowHandle>,
}

impl WindowsAuthenticator {
    fn hwnd(&self) -> isize {
        match self.window {
            Some(WindowHandle(h)) => h,
            // SAFETY: no arguments; returns null when there is no window.
            None => (unsafe { GetForegroundWindow() }) as isize,
        }
    }
}

fn hello_available() -> bool {
    UserConsentVerifier::CheckAvailabilityAsync()
        .and_then(|op| op.join())
        .is_ok_and(|a| a == UserConsentVerifierAvailability::Available)
}

impl Authenticator for WindowsAuthenticator {
    fn method(&self) -> AuthMethod {
        if hello_available() {
            AuthMethod::WindowsHello
        } else {
            AuthMethod::WindowsPassword
        }
    }

    fn authenticate(&self, reason: &str) -> Result<(), AuthError> {
        if hello_available() {
            hello(self.hwnd(), reason)
        } else {
            password(self.hwnd(), reason)
        }
    }
}

fn hello(hwnd: isize, reason: &str) -> Result<(), AuthError> {
    let other = |e: windows::core::Error| AuthError::Other(format!("Windows Hello: {e}"));
    let interop = factory::<UserConsentVerifier, IUserConsentVerifierInterop>().map_err(other)?;
    // SAFETY: a window handle (or null) and a valid string; the result type
    // is the one the interface documents.
    let op: IAsyncOperation<UserConsentVerificationResult> = unsafe {
        interop.RequestVerificationForWindowAsync(HWND(hwnd as *mut _), &HSTRING::from(reason))
    }
    .map_err(other)?;
    match op.join().map_err(other)? {
        UserConsentVerificationResult::Verified => Ok(()),
        UserConsentVerificationResult::Canceled => Err(AuthError::Cancelled),
        UserConsentVerificationResult::RetriesExhausted => Err(AuthError::Failed),
        UserConsentVerificationResult::DeviceNotPresent
        | UserConsentVerificationResult::NotConfiguredForUser
        | UserConsentVerificationResult::DisabledByPolicy => Err(AuthError::Unavailable),
        r => Err(AuthError::Other(format!("Windows Hello result {}", r.0))),
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Asks for the current user's Windows password and checks it with
/// `LogonUserW`. Every buffer holding the password is wiped.
fn password(hwnd: isize, reason: &str) -> Result<(), AuthError> {
    let caption = wide("Dea");
    let message = wide(reason);
    let info = CREDUI_INFOW {
        cbSize: size_of::<CREDUI_INFOW>() as u32,
        hwndParent: hwnd as _,
        pszMessageText: message.as_ptr(),
        pszCaptionText: caption.as_ptr(),
        hbmBanner: std::ptr::null_mut(),
    };
    let mut package = 0u32;
    let mut buffer: *mut core::ffi::c_void = std::ptr::null_mut();
    let mut size = 0u32;
    // SAFETY: valid pointers to locals; the output buffer is freed below.
    let rc = unsafe {
        CredUIPromptForWindowsCredentialsW(
            &info,
            0,
            &mut package,
            std::ptr::null(),
            0,
            &mut buffer,
            &mut size,
            std::ptr::null_mut(),
            CREDUIWIN_ENUMERATE_CURRENT_USER,
        )
    };
    if rc == ERROR_CANCELLED {
        return Err(AuthError::Cancelled);
    }
    if rc != 0 {
        return Err(AuthError::Other(format!("credential dialog error {rc}")));
    }

    let mut user = vec![0u16; 514];
    let mut domain = vec![0u16; 338];
    let mut pass = vec![0u16; 514];
    let (mut user_len, mut domain_len, mut pass_len) =
        (user.len() as u32, domain.len() as u32, pass.len() as u32);
    // SAFETY: the buffer comes from the dialog above; output buffers are
    // sized by their length arguments.
    let unpacked = unsafe {
        CredUnPackAuthenticationBufferW(
            0,
            buffer,
            size,
            user.as_mut_ptr(),
            &mut user_len,
            domain.as_mut_ptr(),
            &mut domain_len,
            pass.as_mut_ptr(),
            &mut pass_len,
        )
    };
    // SAFETY: the dialog's buffer is ours to wipe and free.
    unsafe {
        std::slice::from_raw_parts_mut(buffer.cast::<u8>(), size as usize).zeroize();
        CoTaskMemFree(buffer);
    }
    let result = if unpacked == 0 {
        Err(AuthError::Other(
            "the credential dialog's answer could not be read".into(),
        ))
    } else {
        // "DOMAIN\\user" from the dialog: LogonUserW takes the two apart.
        let name_end = user.iter().position(|c| *c == 0).unwrap_or(user.len());
        let name = &user[..name_end];
        let (domain_part, user_part) = match name.iter().position(|c| *c == u16::from(b'\\')) {
            Some(i) => (name[..i].to_vec(), name[i + 1..].to_vec()),
            None => (Vec::new(), name.to_vec()),
        };
        let user_z: Vec<u16> = user_part.into_iter().chain(Some(0)).collect();
        let domain_z: Vec<u16> = domain_part.into_iter().chain(Some(0)).collect();
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: null-terminated strings and an output handle closed below.
        let ok = unsafe {
            LogonUserW(
                user_z.as_ptr(),
                domain_z.as_ptr(),
                pass.as_ptr(),
                LOGON32_LOGON_INTERACTIVE,
                LOGON32_PROVIDER_DEFAULT,
                &mut token,
            )
        };
        if ok != 0 {
            // SAFETY: a valid token from LogonUserW.
            unsafe { CloseHandle(token) };
            Ok(())
        } else {
            Err(AuthError::Failed)
        }
    };
    pass.zeroize();
    user.zeroize();
    domain.zeroize();
    result
}
