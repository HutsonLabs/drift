//! Reconnect trigger merging (task **M7-2**, pure part).
//!
//! The platform layer (`drift-macos`: `NWPathMonitor`, `NSWorkspace.didWakeNotification`) turns
//! OS notifications into [`Trigger`]s; [`TriggerMerger`] turns that noisy stream into at most one
//! [`TriggerAction`] per real change:
//!
//! | Input | State | Action |
//! |---|---|---|
//! | `NetworkOffline` | online | `PauseReconnect` (the actor sends `NetworkReachable(false)`) |
//! | `NetworkOffline` | offline | nothing (duplicate) |
//! | `NetworkOnline` | offline | `RetryNow` (unless a retry fired within the debounce window) |
//! | `NetworkOnline` | online | nothing (duplicate path update) |
//! | `Wake` | online | `RetryNow` (unless a retry fired within the debounce window) |
//! | `Wake` | offline | nothing: the retry happens when the network comes back |
//!
//! Typical wake sequences (`Wake` then `NetworkOnline` a few hundred ms later, or the reverse)
//! therefore produce a single `RetryNow`.

use std::time::{Duration, Instant};

/// A platform event that may affect reconnection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Trigger {
    /// The network path became unsatisfied (no route).
    NetworkOffline,
    /// The network path became satisfied.
    NetworkOnline,
    /// The Mac woke from sleep.
    Wake,
}

/// What the session layer should do in response to a trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriggerAction {
    /// Stop backoff timers and wait for the network (`SessionCommand::NetworkReachable(false)`).
    PauseReconnect,
    /// Reconnect immediately, skipping any backoff (`NetworkReachable(true)` + `ReconnectNow`).
    RetryNow,
}

/// Merges and debounces [`Trigger`]s (see the module docs for the table).
#[derive(Debug, Clone)]
pub struct TriggerMerger {
    debounce: Duration,
    online: bool,
    last_retry: Option<Instant>,
}

impl TriggerMerger {
    /// Default window within which a second `RetryNow` is suppressed.
    pub const DEFAULT_DEBOUNCE: Duration = Duration::from_secs(2);

    /// Creates a merger with the default debounce window. `online` is the initial path state.
    pub fn new(online: bool) -> Self {
        Self::with_debounce(online, Self::DEFAULT_DEBOUNCE)
    }

    /// Creates a merger with a custom debounce window.
    pub fn with_debounce(online: bool, debounce: Duration) -> Self {
        Self { debounce, online, last_retry: None }
    }

    /// Whether the network is currently considered reachable.
    pub const fn is_online(&self) -> bool {
        self.online
    }

    /// Feeds one trigger observed at `now`; returns the action to take, if any.
    pub fn on_trigger(&mut self, trigger: Trigger, now: Instant) -> Option<TriggerAction> {
        todo!("Red: not implemented yet")
    }

    fn retry(&mut self, now: Instant) -> Option<TriggerAction> {
        if self.last_retry.is_some_and(|t| now.saturating_duration_since(t) < self.debounce) {
            return None;
        }
        self.last_retry = Some(now);
        Some(TriggerAction::RetryNow)
    }
}
