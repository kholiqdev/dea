use std::time::{Instant, SystemTime};

/// The time, replaceable in tests.
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
    fn wall(&self) -> SystemTime;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn wall(&self) -> SystemTime {
        SystemTime::now()
    }
}
