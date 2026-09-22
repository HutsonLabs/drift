//! Reconnect backoff policy (task **M7-1**).
//!
//! [`ReconnectPolicy`] is pure: the session actor feeds it disconnects, successful connects and
//! network reachability, and it answers with a [`ReconnectDecision`]. Timing comes from the
//! caller (`Instant`s from a [`crate::Clock`]), randomness from a seeded [`SplitMix64`], so every
//! decision is reproducible in tests.
//!
//! Policy (plan §6 M7-1):
//! - Exponential backoff with **full jitter**: attempt `n` (1-based) waits a uniformly random
//!   delay in `[0, min(cap, base × 2^(n-1))]`; base 500 ms, cap 30 s.
//! - Only retryable [`DisconnectReason`]s are retried ([`DisconnectReason::is_retryable`]).
//! - While the network is unreachable no attempt is made or counted
//!   ([`ReconnectDecision::WaitForNetwork`]); the attempt budget only applies to attempts made
//!   while the network is reachable. The budget is user-visible and defaults to 20
//!   (`None` = unlimited).
//! - The attempt count resets once a connection has been stable for 60 s.

use std::time::{Duration, Instant};

use crate::state::DisconnectReason;

/// Tunables for [`ReconnectPolicy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconnectConfig {
    /// Backoff ceiling of the first attempt.
    pub base: Duration,
    /// Upper bound of any backoff ceiling.
    pub cap: Duration,
    /// Maximum attempts made while the network is reachable (`None` = unlimited).
    pub max_attempts: Option<u32>,
    /// A connection that lasts this long resets the attempt count.
    pub stable_after: Duration,
}

impl ReconnectConfig {
    /// Default attempt budget shown to the user.
    pub const DEFAULT_MAX_ATTEMPTS: u32 = 20;
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            base: Duration::from_millis(500),
            cap: Duration::from_secs(30),
            max_attempts: Some(Self::DEFAULT_MAX_ATTEMPTS),
            stable_after: Duration::from_secs(60),
        }
    }
}

/// Why the policy stopped reconnecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GiveUpReason {
    /// The disconnect reason is not retryable (auth, certificate, protocol, user, …).
    NotRetryable,
    /// The attempt budget ([`ReconnectConfig::max_attempts`]) is spent.
    AttemptsExhausted,
}

/// What the actor should do after a disconnect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReconnectDecision {
    /// Wait `delay`, then make reconnect attempt number `attempt` (1-based).
    Retry {
        /// 1-based attempt number (shown to the user).
        attempt: u32,
        /// Backoff delay before the attempt.
        delay: Duration,
    },
    /// The network is down: make no attempt until it is reachable again (then call
    /// [`ReconnectPolicy::retry_now`]); `attempt` is the number the next attempt will get.
    WaitForNetwork {
        /// The attempt number that will be used once the network is back.
        attempt: u32,
    },
    /// Stop; the session ends in `Failed`/`Disconnected`.
    GiveUp(GiveUpReason),
}

/// SplitMix64 PRNG (Steele, Lea, Flood 2014): tiny, seedable and good enough for jitter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Creates a generator from a seed.
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform integer in `0..=max` (Lemire's multiply-shift; bias below 2^-32 for delays under 49 days).
    pub fn below_or_eq(&mut self, max: u64) -> u64 {
        let span = u128::from(max) + 1;
        ((u128::from(self.next_u64()) * span) >> 64) as u64
    }
}

/// Exponential full-jitter reconnect policy (see the module docs).
#[derive(Debug, Clone)]
pub struct ReconnectPolicy {
    config: ReconnectConfig,
    rng: SplitMix64,
    attempts: u32,
    connected_since: Option<Instant>,
    network_reachable: bool,
}

impl ReconnectPolicy {
    /// Creates a policy. Use a random seed in the app and a fixed one in tests.
    pub fn new(config: ReconnectConfig, seed: u64) -> Self {
        Self {
            config,
            rng: SplitMix64::new(seed),
            attempts: 0,
            connected_since: None,
            network_reachable: true,
        }
    }

    /// The configuration.
    pub const fn config(&self) -> &ReconnectConfig {
        &self.config
    }

    /// Attempts made since the last reset.
    pub const fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Whether the network is currently considered reachable.
    pub const fn network_reachable(&self) -> bool {
        self.network_reachable
    }

    /// Backoff ceiling for 1-based `attempt`: `min(cap, base × 2^(attempt-1))`.
    pub fn ceiling(&self, attempt: u32) -> Duration {
        let exp = attempt.saturating_sub(1).min(63);
        let factor = 1u32.checked_shl(exp).unwrap_or(u32::MAX);
        self.config.base.checked_mul(factor).map_or(self.config.cap, |d| d.min(self.config.cap))
    }

    /// A connection (any leg reaching `Connected`/`AwaitingGreeterLogin`) was established at `now`.
    pub fn on_connected(&mut self, now: Instant) {
        self.connected_since = Some(now);
    }

    /// The session dropped (or a reconnect attempt failed) at `now` with `reason`.
    pub fn on_disconnect(&mut self, reason: &DisconnectReason, now: Instant) -> ReconnectDecision {
        if let Some(since) = self.connected_since.take()
            && now.saturating_duration_since(since) >= self.config.stable_after
        {
            self.attempts = 0;
        }
        if !reason.is_retryable() {
            return ReconnectDecision::GiveUp(GiveUpReason::NotRetryable);
        }
        self.next_decision(true)
    }

    /// Network reachability changed (from the trigger merger, M7-2). While unreachable,
    /// disconnects yield [`ReconnectDecision::WaitForNetwork`] and no attempt is counted.
    pub fn set_network_reachable(&mut self, reachable: bool) {
        self.network_reachable = reachable;
    }

    /// Skip the backoff: the user pressed "Now", the network came back or the Mac woke up.
    ///
    /// Consumes an attempt like a normal retry but with zero delay; still honours the network
    /// state and the attempt budget.
    pub fn retry_now(&mut self) -> ReconnectDecision {
        self.next_decision(false)
    }

    /// Forgets the attempt history (user pressed Cancel, or edited the profile).
    pub fn reset(&mut self) {
        self.attempts = 0;
        self.connected_since = None;
    }

    fn next_decision(&mut self, jitter: bool) -> ReconnectDecision {
        if !self.network_reachable {
            return ReconnectDecision::WaitForNetwork { attempt: self.attempts.saturating_add(1) };
        }
        if self.config.max_attempts.is_some_and(|max| self.attempts >= max) {
            return ReconnectDecision::GiveUp(GiveUpReason::AttemptsExhausted);
        }
        self.attempts = self.attempts.saturating_add(1);
        let delay = if jitter {
            let ceiling_ms = u64::try_from(self.ceiling(self.attempts).as_millis()).unwrap_or(u64::MAX);
            Duration::from_millis(self.rng.below_or_eq(ceiling_ms))
        } else {
            Duration::ZERO
        };
        ReconnectDecision::Retry { attempt: self.attempts, delay }
    }
}
