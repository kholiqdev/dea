//! How long each phase of a request took (decision 8). Phases are taken
//! from consecutive instants, so they add up to the total exactly.

use std::time::{Duration, Instant};

/// Timing of the final hop. `waiting` runs from the end of TLS, or from
/// taking a reused connection out of the pool, until the response head
/// arrives, so it includes writing the request. On a reused connection
/// `dns`, `connect` and `tls` are zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Timing {
    pub dns: Duration,
    pub connect: Duration,
    pub tls: Duration,
    pub waiting: Duration,
    pub download: Duration,
    pub total: Duration,
    pub reused: bool,
}

/// The instants up to a usable connection: t0 (start), t1 (address
/// resolved), t2 (TCP connected) and t3 (TLS done).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Marks {
    pub(crate) start: Instant,
    pub(crate) resolved: Instant,
    pub(crate) connected: Instant,
    pub(crate) secured: Instant,
    pub(crate) reused: bool,
}

impl Marks {
    /// A connection taken from the pool at `at`.
    pub(crate) fn reused(at: Instant) -> Self {
        Marks {
            start: at,
            resolved: at,
            connected: at,
            secured: at,
            reused: true,
        }
    }

    /// The timing once the response head arrived (t4) and the body was
    /// read (t5).
    pub(crate) fn finish(&self, head: Instant, done: Instant) -> Timing {
        Timing {
            dns: self.resolved - self.start,
            connect: self.connected - self.resolved,
            tls: self.secured - self.connected,
            waiting: head - self.secured,
            download: done - head,
            total: done - self.start,
            reused: self.reused,
        }
    }
}
