//! logind on the system bus (sleep, `loginctl lock-session`) and the
//! screensaver on the session bus (GNOME, KDE and others). A bus that is
//! not there is skipped.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use zbus::blocking::{Connection, MessageIterator};

use super::{Emit, OsEvent, Stop};
use crate::VaultError;

const SYSTEM_RULES: [&str; 2] = [
    "type='signal',interface='org.freedesktop.login1.Manager',member='PrepareForSleep'",
    "type='signal',interface='org.freedesktop.login1.Session',member='Lock'",
];
const SESSION_RULES: [&str; 2] = [
    "type='signal',interface='org.freedesktop.ScreenSaver',member='ActiveChanged'",
    "type='signal',interface='org.gnome.ScreenSaver',member='ActiveChanged'",
];

/// Which D-Bus signal means what. `active` is the boolean argument, where
/// the signal has one.
pub(super) fn classify(interface: &str, member: &str, active: bool) -> Option<OsEvent> {
    match (interface, member) {
        ("org.freedesktop.login1.Manager", "PrepareForSleep") if active => Some(OsEvent::Sleeping),
        ("org.freedesktop.login1.Session", "Lock") => Some(OsEvent::ScreenLocked),
        ("org.freedesktop.ScreenSaver" | "org.gnome.ScreenSaver", "ActiveChanged") if active => {
            Some(OsEvent::ScreenLocked)
        }
        _ => None,
    }
}

pub(super) fn start(emit: Emit, stopped: Arc<AtomicBool>) -> Result<Stop, VaultError> {
    let buses = [
        (Connection::system(), &SYSTEM_RULES),
        (Connection::session(), &SESSION_RULES),
    ];
    for (bus, rules) in buses {
        let Ok(bus) = bus else { continue };
        for rule in rules.iter() {
            // Subscribe now, so no signal sent after this returns is missed.
            let Ok(messages) = MessageIterator::for_match_rule(*rule, &bus, None) else {
                continue;
            };
            let emit = Arc::clone(&emit);
            let stopped = Arc::clone(&stopped);
            // Blocks until the next message; ends at the first message
            // after watching stopped.
            std::thread::Builder::new()
                .name("dea-vault-os-events".into())
                .spawn(move || {
                    for message in messages {
                        if stopped.load(Ordering::SeqCst) {
                            break;
                        }
                        let Ok(message) = message else { continue };
                        let header = message.header();
                        let (Some(interface), Some(member)) = (header.interface(), header.member())
                        else {
                            continue;
                        };
                        let active = message.body().deserialize::<bool>().unwrap_or(false);
                        if let Some(event) = classify(interface.as_str(), member.as_str(), active) {
                            emit(event);
                        }
                    }
                })
                .map_err(|e| VaultError::Backend(format!("cannot watch D-Bus: {e}")))?;
        }
    }
    Ok(Box::new(|| {}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screensaver_active_true_locks_false_does_not() {
        for iface in ["org.freedesktop.ScreenSaver", "org.gnome.ScreenSaver"] {
            assert_eq!(
                classify(iface, "ActiveChanged", true),
                Some(OsEvent::ScreenLocked)
            );
            assert_eq!(classify(iface, "ActiveChanged", false), None);
        }
    }

    #[test]
    fn logind_sleep_and_session_lock_are_recognised() {
        assert_eq!(
            classify("org.freedesktop.login1.Manager", "PrepareForSleep", true),
            Some(OsEvent::Sleeping)
        );
        // `false` is sent on resume.
        assert_eq!(
            classify("org.freedesktop.login1.Manager", "PrepareForSleep", false),
            None
        );
        assert_eq!(
            classify("org.freedesktop.login1.Session", "Lock", false),
            Some(OsEvent::ScreenLocked)
        );
        assert_eq!(
            classify("org.freedesktop.login1.Session", "Unlock", false),
            None
        );
    }
}
