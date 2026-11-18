//! LocalAuthentication with the device owner policy: Touch ID, with the
//! account password as the fallback the system offers itself.
#![allow(unsafe_code)] // Objective-C calls into LocalAuthentication.

use std::sync::mpsc;

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{NSError, NSString};
use objc2_local_authentication::{LAContext, LAError, LAPolicy};

use super::{AuthError, AuthMethod, Authenticator};

pub(super) struct MacAuthenticator;

impl Authenticator for MacAuthenticator {
    fn method(&self) -> AuthMethod {
        AuthMethod::DeviceOwner
    }

    fn authenticate(&self, reason: &str) -> Result<(), AuthError> {
        let (tx, rx) = mpsc::channel();
        let reply = RcBlock::new(move |ok: Bool, error: *mut NSError| {
            // SAFETY: LocalAuthentication passes either null or a valid
            // NSError that lives for the duration of this call.
            let code = unsafe { error.as_ref() }.map(NSError::code);
            let _ = tx.send((ok.as_bool(), code));
        });
        // The context must outlive the dialog: a context that is released
        // is invalidated, which would end the evaluation at once.
        // SAFETY: creating a context has no preconditions.
        let context = unsafe { LAContext::new() };
        // SAFETY: a valid policy and string, and a block that only sends on
        // a channel, so it may run on any queue.
        unsafe {
            context.evaluatePolicy_localizedReason_reply(
                LAPolicy::DeviceOwnerAuthentication,
                &NSString::from_str(reason),
                &reply,
            );
        }
        let answer = rx.recv();
        drop(context);
        match answer {
            Ok((true, _)) => Ok(()),
            Ok((false, code)) => Err(map_error(code)),
            Err(_) => Err(AuthError::Other(
                "LocalAuthentication gave no answer".into(),
            )),
        }
    }
}

fn map_error(code: Option<isize>) -> AuthError {
    let Some(code) = code else {
        return AuthError::Failed;
    };
    match LAError(code) {
        LAError::UserCancel | LAError::SystemCancel | LAError::AppCancel => AuthError::Cancelled,
        LAError::AuthenticationFailed | LAError::BiometryLockout => AuthError::Failed,
        LAError::PasscodeNotSet | LAError::NotInteractive => AuthError::Unavailable,
        other => AuthError::Other(format!("LocalAuthentication error {}", other.0)),
    }
}
