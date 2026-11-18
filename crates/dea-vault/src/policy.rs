use std::ops::RangeInclusive;
use std::time::Duration;

use crate::VaultError;

/// Minutes of idle time before the Vault locks (F1-FON-09 #1).
pub const AUTO_LOCK_MINUTES: RangeInclusive<u16> = 1..=240;
pub const DEFAULT_AUTO_LOCK_MINUTES: u16 = 15;
/// Minutes production secrets stay usable after authentication
/// (F1-FON-10 #2).
pub const PRODUCTION_WINDOW_MINUTES: RangeInclusive<u16> = 1..=60;
pub const DEFAULT_PRODUCTION_WINDOW_MINUTES: u16 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoLock {
    After(Duration),
    Never,
}

/// The person's lock settings. There is deliberately no way to turn the
/// production check off (F1-FON-10 #5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub auto_lock: AutoLock,
    pub production_window: Duration,
}

fn minutes(m: u16) -> Duration {
    Duration::from_secs(u64::from(m) * 60)
}

impl Policy {
    /// `auto_lock_minutes` of `None` means never lock on idle.
    pub fn new(
        auto_lock_minutes: Option<u16>,
        production_window_minutes: u16,
    ) -> Result<Policy, VaultError> {
        let auto_lock = match auto_lock_minutes {
            None => AutoLock::Never,
            Some(m) if AUTO_LOCK_MINUTES.contains(&m) => AutoLock::After(minutes(m)),
            Some(m) => {
                return Err(VaultError::InvalidSetting(format!(
                    "auto lock must be 1 to 240 minutes or never, not {m}"
                )));
            }
        };
        if !PRODUCTION_WINDOW_MINUTES.contains(&production_window_minutes) {
            return Err(VaultError::InvalidSetting(format!(
                "the production window must be 1 to 60 minutes, not {production_window_minutes}"
            )));
        }
        Ok(Policy {
            auto_lock,
            production_window: minutes(production_window_minutes),
        })
    }
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            auto_lock: AutoLock::After(minutes(DEFAULT_AUTO_LOCK_MINUTES)),
            production_window: minutes(DEFAULT_PRODUCTION_WINDOW_MINUTES),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_fifteen_and_ten_minutes() {
        let policy = Policy::default();
        assert_eq!(
            policy.auto_lock,
            AutoLock::After(Duration::from_secs(15 * 60))
        );
        assert_eq!(policy.production_window, Duration::from_secs(10 * 60));
    }

    #[test]
    fn settings_outside_their_ranges_are_refused() {
        for (auto, window) in [
            (Some(0), 10),
            (Some(241), 10),
            (Some(15), 0),
            (Some(15), 61),
        ] {
            assert!(
                matches!(
                    Policy::new(auto, window),
                    Err(VaultError::InvalidSetting(_))
                ),
                "{auto:?} {window}"
            );
        }
        for (auto, window) in [(Some(1), 1), (Some(240), 60), (None, 10)] {
            assert!(Policy::new(auto, window).is_ok(), "{auto:?} {window}");
        }
        assert_eq!(Policy::new(None, 10).unwrap().auto_lock, AutoLock::Never);
    }
}
