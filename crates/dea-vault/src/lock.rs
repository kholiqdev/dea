//! When the Vault is locked, when production secrets may be used, and when
//! unlock attempts are held back. Pure rules: every time comes in as a
//! parameter, so tests can use made-up clocks.

use std::time::{Duration, Instant, SystemTime};

use crate::{AutoLock, Policy};

/// Failed unlocks in a row before attempts are held back (F1-FON-09 #7).
pub const MAX_FAILURES: u32 = 5;
/// How long attempts are held back.
pub const HOLD: Duration = Duration::from_secs(30);
/// How far the wall clock may run ahead of the monotonic clock between two
/// ticks before Dea assumes the device slept. The monotonic clock stops
/// during sleep on macOS and Linux.
pub const RESUME_GAP: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Default)]
pub struct LockState {
    unlocked: bool,
    last_activity: Option<Instant>,
    last_tick: Option<(Instant, SystemTime)>,
    production_until: Option<Instant>,
    failures: u32,
    held_until: Option<Instant>,
}

impl LockState {
    /// A locked Vault, as at start-up.
    pub fn new() -> Self {
        LockState::default()
    }

    pub fn is_unlocked(&self) -> bool {
        self.unlocked
    }

    pub fn unlock(&mut self, now: Instant, wall: SystemTime) {
        self.unlocked = true;
        self.last_activity = Some(now);
        self.last_tick = Some((now, wall));
    }

    /// Locks and closes the production window (F1-FON-10 #6).
    pub fn lock(&mut self) {
        self.unlocked = false;
        self.last_activity = None;
        self.last_tick = None;
        self.production_until = None;
    }

    pub fn record_activity(&mut self, now: Instant) {
        if self.unlocked {
            self.last_activity = Some(now);
        }
    }

    /// Locks after the idle time, or after a jump of the wall clock that
    /// means the device slept. Returns whether this tick locked.
    pub fn tick(&mut self, now: Instant, wall: SystemTime, policy: &Policy) -> bool {
        if !self.unlocked {
            return false;
        }
        if let Some((then, then_wall)) = self.last_tick {
            let monotonic = now.saturating_duration_since(then);
            let elapsed = wall.duration_since(then_wall).unwrap_or(Duration::ZERO);
            if elapsed > monotonic + RESUME_GAP {
                self.lock();
                return true;
            }
        }
        if let (AutoLock::After(limit), Some(last)) = (policy.auto_lock, self.last_activity)
            && now.saturating_duration_since(last) >= limit
        {
            self.lock();
            return true;
        }
        self.last_tick = Some((now, wall));
        false
    }

    pub fn production_open(&self, now: Instant) -> bool {
        self.unlocked && self.production_until.is_some_and(|until| now < until)
    }

    /// Production secrets may be used until the window ends. Using them does
    /// not extend it.
    pub fn open_production(&mut self, now: Instant, policy: &Policy) {
        self.production_until = Some(now + policy.production_window);
    }

    /// `Err` holds how long to wait before the next attempt.
    pub fn attempt_allowed(&self, now: Instant) -> Result<(), Duration> {
        match self.held_until {
            Some(until) if now < until => Err(until - now),
            _ => Ok(()),
        }
    }

    pub fn record_failure(&mut self, now: Instant) {
        self.failures = self.failures.saturating_add(1);
        if self.failures >= MAX_FAILURES {
            self.held_until = Some(now + HOLD);
        }
    }

    pub fn record_success(&mut self) {
        self.failures = 0;
        self.held_until = None;
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant, SystemTime};

    use super::*;
    use crate::Policy;

    const MIN: Duration = Duration::from_secs(60);

    fn unlocked() -> (LockState, Instant, SystemTime) {
        let (now, wall) = (Instant::now(), SystemTime::now());
        let mut state = LockState::new();
        assert!(!state.is_unlocked());
        state.unlock(now, wall);
        (state, now, wall)
    }

    #[test]
    fn locks_after_fifteen_idle_minutes_by_default() {
        let (mut s, t0, w0) = unlocked();
        let p = Policy::default();
        let later = 15 * MIN - Duration::from_secs(1);
        assert!(!s.tick(t0 + later, w0 + later, &p));
        assert!(s.is_unlocked());
        assert!(s.tick(t0 + 15 * MIN, w0 + 15 * MIN, &p));
        assert!(!s.is_unlocked());
        // Already locked: a later tick reports nothing new.
        assert!(!s.tick(t0 + 16 * MIN, w0 + 16 * MIN, &p));
    }

    #[test]
    fn activity_restarts_the_idle_timer() {
        let (mut s, t0, w0) = unlocked();
        let p = Policy::default();
        s.record_activity(t0 + 10 * MIN);
        assert!(!s.tick(t0 + 20 * MIN, w0 + 20 * MIN, &p));
        assert!(s.tick(t0 + 25 * MIN, w0 + 25 * MIN, &p));
    }

    #[test]
    fn never_does_not_lock_on_idle() {
        let (mut s, t0, w0) = unlocked();
        let p = Policy::new(None, 10).unwrap();
        // Ticks every 30 s for a day, as the ticker would.
        let mut t = Duration::ZERO;
        while t < 24 * 60 * MIN {
            t += Duration::from_secs(30);
            assert!(!s.tick(t0 + t, w0 + t, &p));
        }
        assert!(s.is_unlocked());
    }

    #[test]
    fn production_window_lasts_ten_minutes_from_authentication() {
        let (mut s, t0, _) = unlocked();
        let p = Policy::default();
        assert!(!s.production_open(t0));
        s.open_production(t0, &p);
        assert!(s.production_open(t0 + 10 * MIN - Duration::from_secs(1)));
        // Activity and use do not extend it.
        s.record_activity(t0 + 9 * MIN);
        assert!(!s.production_open(t0 + 10 * MIN));
    }

    #[test]
    fn locking_closes_the_production_window() {
        let (mut s, t0, w0) = unlocked();
        let p = Policy::default();
        s.open_production(t0, &p);
        s.lock();
        assert!(!s.production_open(t0 + MIN));
        s.unlock(t0 + MIN, w0 + MIN);
        assert!(!s.production_open(t0 + MIN));
    }

    #[test]
    fn five_failures_hold_the_next_attempt_for_thirty_seconds() {
        let t0 = Instant::now();
        let mut s = LockState::new();
        for _ in 0..4 {
            s.record_failure(t0);
            assert_eq!(s.attempt_allowed(t0), Ok(()));
        }
        s.record_failure(t0);
        assert_eq!(s.attempt_allowed(t0), Err(HOLD));
        assert_eq!(
            s.attempt_allowed(t0 + Duration::from_secs(29)),
            Err(Duration::from_secs(1))
        );
        let t1 = t0 + HOLD;
        assert_eq!(s.attempt_allowed(t1), Ok(()));
        // Still past the limit: the next failure holds again.
        s.record_failure(t1);
        assert_eq!(s.attempt_allowed(t1), Err(HOLD));
        // Success starts the count over.
        s.record_success();
        assert_eq!(s.attempt_allowed(t1), Ok(()));
        s.record_failure(t1);
        assert_eq!(s.attempt_allowed(t1), Ok(()));
    }

    #[test]
    fn resume_after_a_clock_gap_locks() {
        let p = Policy::default();
        // The monotonic clock stops while the laptop sleeps; the wall clock
        // does not.
        let (mut s, t0, w0) = unlocked();
        let half = Duration::from_millis(500);
        assert!(s.tick(t0 + half, w0 + 120 * MIN, &p));
        assert!(!s.is_unlocked());

        // A small wall clock correction (NTP) is not a resume.
        let (mut s, t0, w0) = unlocked();
        assert!(!s.tick(t0 + half, w0 + Duration::from_millis(1500), &p));
        assert!(s.is_unlocked());
        // A wall clock set back is not a resume either.
        assert!(!s.tick(t0 + 2 * half, w0 - 60 * MIN, &p));
        assert!(s.is_unlocked());
    }
}
