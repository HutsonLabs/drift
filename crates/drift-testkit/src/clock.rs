//! A manually driven clock for deterministic timer tests.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use drift_core::Clock;

/// A [`Clock`] frozen at a fixed instant until [`ManualClock::advance`] is called.
///
/// Clones share the same time, so a test can keep one handle while the code under test
/// owns another (`Arc<dyn Clock>`).
#[derive(Debug, Clone)]
pub struct ManualClock {
    start: Instant,
    now: Arc<Mutex<Instant>>,
}

impl ManualClock {
    /// A clock starting at the current real instant.
    pub fn new() -> Self {
        todo!("M0-5 green")
    }

    /// Moves time forward by `by`.
    pub fn advance(&self, by: Duration) {
        todo!("M0-5 green")
    }

    /// Time elapsed since this clock (or its first clone) was created.
    pub fn elapsed(&self) -> Duration {
        todo!("M0-5 green")
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        todo!("M0-5 green")
    }
}
