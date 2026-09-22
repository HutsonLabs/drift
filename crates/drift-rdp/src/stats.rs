//! Session statistics (fps, bit rate) sampled about once per second. Pure.

use std::time::{Duration, Instant};

use crate::session::SessionStats;

/// How often [`StatsMeter::sample`] produces a [`SessionStats`].
pub const STATS_PERIOD: Duration = Duration::from_secs(1);

/// Counts presented frames and received bytes between samples.
#[derive(Debug, Clone)]
pub struct StatsMeter {
    since: Instant,
}

impl StatsMeter {
    /// A meter whose first window starts at `now`.
    pub fn new(now: Instant) -> Self {
        Self { since: now }
    }

    /// Starts a new window at `now`, discarding counts (e.g. after a reconnect or unhide).
    pub fn restart(&mut self, now: Instant) {
        todo!("{now:?} {:?}", self.since)
    }

    /// `n` frames were presented.
    pub fn frames(&mut self, n: u64) {
        todo!("{n}")
    }

    /// `n` payload bytes were received.
    pub fn bytes(&mut self, n: u64) {
        todo!("{n}")
    }

    /// The stats of the finished window, once [`STATS_PERIOD`] has elapsed since it started;
    /// `unacked_frames` is reported as given.
    pub fn sample(&mut self, now: Instant, unacked_frames: u32) -> Option<SessionStats> {
        todo!("{now:?} {unacked_frames}")
    }
}
