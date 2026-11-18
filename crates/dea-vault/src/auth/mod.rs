//! Proof that the person at the computer owns the account: Touch ID or the
//! macOS password, Windows Hello or the Windows password. Linux has no such
//! dialog; there `dea-engine` asks for the Vault password instead (K19).

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

/// How this device authenticates the person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    /// macOS: Touch ID, or the account password when Touch ID is missing.
    DeviceOwner,
    WindowsHello,
    /// Windows without Windows Hello set up: the account password.
    WindowsPassword,
    /// Linux: no native dialog. The Vault password is asked for in Dea.
    VaultPassword,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// The person closed the dialog. Not counted as a failed attempt.
    Cancelled,
    /// Wrong fingerprint, face, PIN or password.
    Failed,
    /// This device cannot authenticate this way.
    Unavailable,
    Other(String),
}

/// A native window, so the dialog opens in front of it. On Windows the
/// `HWND`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowHandle(pub isize);

pub trait Authenticator: Send + Sync {
    /// Checks what is available. Never shows a dialog.
    fn method(&self) -> AuthMethod;
    /// Shows the OS dialog with `reason` and waits for the person.
    fn authenticate(&self, reason: &str) -> Result<(), AuthError>;
}

/// The authenticator of this OS. `window` is where Windows shows its dialog;
/// other systems ignore it.
pub fn native_authenticator(window: Option<WindowHandle>) -> Box<dyn Authenticator> {
    #[cfg(target_os = "macos")]
    let inner: Box<dyn Authenticator> = {
        let _ = window;
        Box::new(macos::MacAuthenticator)
    };
    #[cfg(windows)]
    let inner: Box<dyn Authenticator> = Box::new(windows::WindowsAuthenticator { window });
    #[cfg(not(any(target_os = "macos", windows)))]
    let inner: Box<dyn Authenticator> = {
        let _ = window;
        Box::new(NoNativeDialog)
    };
    Box::new(RequireReason(inner))
}

/// Refuses an empty reason before any OS call: the dialog must say what it
/// is for (F1-FON-10 #3), and macOS raises an exception without one.
struct RequireReason(Box<dyn Authenticator>);

impl Authenticator for RequireReason {
    fn method(&self) -> AuthMethod {
        self.0.method()
    }

    fn authenticate(&self, reason: &str) -> Result<(), AuthError> {
        if reason.trim().is_empty() {
            return Err(AuthError::Other("the dialog needs a reason".into()));
        }
        self.0.authenticate(reason)
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
struct NoNativeDialog;

#[cfg(not(any(target_os = "macos", windows)))]
impl Authenticator for NoNativeDialog {
    fn method(&self) -> AuthMethod {
        AuthMethod::VaultPassword
    }

    fn authenticate(&self, _reason: &str) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_os_reports_a_method_without_prompting() {
        let auth = native_authenticator(None);
        #[cfg(target_os = "linux")]
        assert_eq!(auth.method(), AuthMethod::VaultPassword);
        #[cfg(target_os = "macos")]
        assert_eq!(auth.method(), AuthMethod::DeviceOwner);
        #[cfg(windows)]
        assert!(matches!(
            auth.method(),
            AuthMethod::WindowsHello | AuthMethod::WindowsPassword
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_native_authentication_is_unavailable() {
        assert_eq!(
            native_authenticator(None).authenticate("Unlock the Vault"),
            Err(AuthError::Unavailable)
        );
    }

    #[test]
    fn an_empty_reason_is_refused_before_any_dialog() {
        // macOS raises an exception for an empty reason, which would crash.
        assert!(matches!(
            native_authenticator(None).authenticate("  "),
            Err(AuthError::Other(_))
        ));
    }
}
