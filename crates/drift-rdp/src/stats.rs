//! Session statistics (fps, bit rate) sampled about once per second. Pure.

use std::time::{Duration, Instant};

use crate::session::SessionStats;

/// How often [`StatsMeter::sample`] produces a [`SessionStats`].
pub const STATS_PERIOD: Duration = Duration::from_secs(1);

/// Counts presented frames and received bytes between samples.
#[derive(Debug, Clone)]
pub struct StatsMeter {
    since: Instant,
    frames: u64,
    bytes: u64,
}

impl StatsMeter {
    /// A meter whose first window starts at `now`.
    pub fn new(now: Instant) -> Self {
        Self { since: now, frames: 0, bytes: 0 }
    }

    /// Starts a new window at `now`, discarding counts (e.g. after a reconnect or unhide).
    pub fn restart(&mut self, now: Instant) {
        *self = Self::new(now);
    }

    /// `n` frames were presented.
    pub fn frames(&mut self, n: u64) {
        self.frames = self.frames.saturating_add(n);
    }

    /// `n` payload bytes were received.
    pub fn bytes(&mut self, n: u64) {
        self.bytes = self.bytes.saturating_add(n);
    }

    /// When the current window ends.
    pub fn deadline(&self) -> Instant {
        self.since + STATS_PERIOD
    }

    /// The stats of the finished window, once [`STATS_PERIOD`] has elapsed since it started;
    /// `unacked_frames` is reported as given.
    pub fn sample(&mut self, now: Instant, unacked_frames: u32) -> Option<SessionStats> {
        let elapsed = now.saturating_duration_since(self.since);
        if elapsed < STATS_PERIOD {
            return None;
        }
        let secs = elapsed.as_secs_f64();
        #[expect(clippy::cast_possible_truncation, reason = "fps and bit rate are display values")]
        let stats = SessionStats {
            fps: (self.frames as f64 / secs) as f32,
            bitrate_bps: (self.bytes as f64 * 8.0 / secs) as u64,
            frame_latency_p95_ms: 0.0,
            unacked_frames,
        };
        self.restart(now);
        Some(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_once_per_period() {
        let t0 = Instant::now();
        let mut m = StatsMeter::new(t0);
        m.frames(60);
        m.bytes(150_000);
        assert_eq!(m.sample(t0 + Duration::from_millis(999), 0), None, "window not over");
        let stats = m.sample(t0 + STATS_PERIOD, 3).expect("a sample after one second");
        assert!((stats.fps - 60.0).abs() < 0.1, "{stats:?}");
        assert_eq!(stats.bitrate_bps, 1_200_000);
        assert_eq!(stats.unacked_frames, 3);
        assert_eq!(m.sample(t0 + STATS_PERIOD, 0), None, "the window restarted");
        assert_eq!(m.deadline(), t0 + STATS_PERIOD * 2);
    }

    #[test]
    fn restart_discards_counts() {
        let t0 = Instant::now();
        let mut m = StatsMeter::new(t0);
        m.frames(10);
        m.restart(t0 + Duration::from_millis(500));
        let stats = m.sample(t0 + Duration::from_millis(1500), 0).expect("a sample");
        assert_eq!(stats.fps, 0.0);
    }
}
