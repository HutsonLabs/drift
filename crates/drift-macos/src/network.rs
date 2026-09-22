//! Reconnect triggers from the OS (task **M7-2**, platform part).
//!
//! * [`PathMonitor`] wraps Network.framework's `nw_path_monitor` (the path status of the default
//!   route) and reports [`PathStatus`] changes on a private dispatch queue.
//! * [`WakeObserver`] observes `NSWorkspaceDidWakeNotification`.
//! * [`TriggerFeed`] turns both into [`Trigger`]s and runs them through the pure
//!   [`TriggerMerger`] (drift-core), so the app receives at most one [`TriggerAction`] per real
//!   change: offline pauses reconnecting, online and wake retry immediately, duplicates are
//!   debounced.
//! * [`ReconnectTriggers`] owns all three for the app's lifetime.

use std::sync::Arc;

use drift_core::{Clock, Trigger, TriggerAction};

/// `nw_path_status_t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathStatus {
    /// `nw_path_status_invalid`: not yet determined.
    Invalid,
    /// `nw_path_status_satisfied`: usable route.
    Satisfied,
    /// `nw_path_status_unsatisfied`: no route.
    Unsatisfied,
    /// `nw_path_status_satisfiable`: a route may appear if a connection is attempted
    /// (VPN on demand, cellular); treated as online so the reconnect attempt brings it up.
    Satisfiable,
}

impl PathStatus {
    /// Converts the raw `nw_path_status_t` value (unknown values are `Invalid`).
    pub const fn from_raw(raw: i32) -> Self {
        let _ = raw;
        Self::Invalid
    }

    /// The reconnect trigger for this status (`None` for `Invalid`).
    pub const fn trigger(self) -> Option<Trigger> {
        None
    }
}

/// Merges platform triggers and forwards the resulting actions.
pub struct TriggerFeed {
    _sink: Box<dyn Fn(TriggerAction) + Send + Sync>,
}

impl std::fmt::Debug for TriggerFeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TriggerFeed").finish_non_exhaustive()
    }
}

impl TriggerFeed {
    /// A feed with the default debounce. `online` is the initial reachability.
    pub fn new(
        online: bool,
        clock: Arc<dyn Clock>,
        sink: impl Fn(TriggerAction) + Send + Sync + 'static,
    ) -> Self {
        let _ = (online, clock);
        Self { _sink: Box::new(sink) }
    }

    /// Feeds one trigger.
    pub fn trigger(&self, trigger: Trigger) {
        let _ = trigger;
    }

    /// Feeds a path status update.
    pub fn path_status(&self, status: PathStatus) {
        let _ = status;
    }

    /// Whether the network is currently considered reachable.
    pub fn is_online(&self) -> bool {
        false
    }
}

/// A running `nw_path_monitor`. Cancelled on drop.
#[derive(Debug)]
pub struct PathMonitor {}

impl PathMonitor {
    /// Starts monitoring; `handler` is called on a private serial queue with the initial status
    /// and every change.
    pub fn start(handler: impl Fn(PathStatus) + Send + Sync + 'static) -> Self {
        let _ = handler;
        Self {}
    }
}

/// An `NSWorkspaceDidWakeNotification` observer. Removed on drop.
#[derive(Debug)]
pub struct WakeObserver {}

impl WakeObserver {
    /// Calls `handler` (on the posting thread, the main thread for real wakes) after every wake.
    pub fn start(handler: impl Fn() + Send + Sync + 'static) -> Self {
        let _ = handler;
        Self {}
    }
}

/// Path monitor + wake observer feeding one [`TriggerFeed`].
#[derive(Debug)]
pub struct ReconnectTriggers {
    _feed: Arc<TriggerFeed>,
}

impl ReconnectTriggers {
    /// Starts both observers; `sink` receives the merged actions (from the monitor's queue or
    /// the main thread) and should forward them to every session (`NetworkReachable`,
    /// `ReconnectNow`).
    pub fn start(clock: Arc<dyn Clock>, sink: impl Fn(TriggerAction) + Send + Sync + 'static) -> Self {
        Self { _feed: Arc::new(TriggerFeed::new(true, clock, sink)) }
    }

    /// The shared feed (e.g. to inject a trigger).
    pub fn feed(&self) -> &Arc<TriggerFeed> {
        &self._feed
    }
}
