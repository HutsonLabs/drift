//! Time source abstraction so timers (debounce, backoff, timeouts) are testable.

use std::time::Instant;

/// A monotonic clock. Production code uses [`SystemClock`];
/// tests use `drift_testkit::ManualClock`.
pub trait Clock: Send + Sync {
    /// The current instant.
    fn now(&self) -> Instant;
}

/// The real monotonic clock (`Instant::now`).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_is_monotonic() {
        let c = SystemClock;
        let a = c.now();
        assert!(c.now() >= a);
    }
}
