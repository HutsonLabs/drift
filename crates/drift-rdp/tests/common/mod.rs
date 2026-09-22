//! Shared loopback harness: a session actor pointed at a `FakeServer`.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use drift_core::{CertFingerprint, Clock, ConnectMode, ConnectionProfile, SessionState, SystemClock};
use drift_rdp::{SessionCommand, SessionEvent, SessionEvents, SessionHandle, SessionOptions, SessionSecrets};
use drift_testkit::{PresentMode, RecordingFrameSink};

/// Fake credentials (never real ones).
pub const USER: &str = "drift-fake-user";
/// Fake password.
pub const PASS: &str = "Fake9-password";

/// A profile for `127.0.0.1:<port>`.
pub fn profile(mode: ConnectMode, port: u16, pin: Option<CertFingerprint>) -> ConnectionProfile {
    let mut p = ConnectionProfile::new("fake", "127.0.0.1", mode);
    p.port = port;
    p.rdp_username = USER.into();
    p.cert_pin = pin;
    p
}

/// A running session actor and everything it emitted so far.
pub struct Harness {
    pub handle: SessionHandle,
    events: SessionEvents,
    pub seen: Vec<SessionEvent>,
}

impl Harness {
    /// Spawns a session with the real clock.
    pub fn start(profile: ConnectionProfile, password: &str) -> Self {
        Self::start_with_clock(profile, password, Arc::new(SystemClock))
    }

    /// Spawns a session with `clock`.
    pub fn start_with_clock(profile: ConnectionProfile, password: &str, clock: Arc<dyn Clock>) -> Self {
        let (sink, _log) = RecordingFrameSink::new(PresentMode::Immediate);
        let options =
            SessionOptions { connect_timeout: Duration::from_secs(15), ..SessionOptions::default() };
        let (handle, events) =
            drift_rdp::spawn_session(profile, SessionSecrets::new(password), Box::new(sink), clock, options);
        Self { handle, events, seen: Vec::new() }
    }

    /// Waits (real time) for an event matching `pred`; returns it, or panics with the history.
    pub async fn wait_for(
        &mut self,
        what: &str,
        within: Duration,
        pred: impl Fn(&SessionEvent) -> bool,
    ) -> SessionEvent {
        let deadline = tokio::time::Instant::now() + within;
        loop {
            match tokio::time::timeout_at(deadline, self.events.recv()).await {
                Ok(Some(ev)) => {
                    self.seen.push(ev.clone());
                    if pred(&ev) {
                        return ev;
                    }
                }
                Ok(None) => panic!("actor exited before {what}; saw {:#?}", self.seen),
                Err(_) => panic!("timed out waiting for {what}; saw {:#?}", self.seen),
            }
        }
    }

    /// Waits for a terminal state (`Disconnected` / `Failed`).
    pub async fn wait_terminal(&mut self, within: Duration) -> SessionState {
        match self
            .wait_for("terminal state", within, |e| {
                matches!(
                    e,
                    SessionEvent::State(SessionState::Disconnected { .. } | SessionState::Failed { .. })
                )
            })
            .await
        {
            SessionEvent::State(s) => s,
            _ => unreachable!(),
        }
    }

    /// Collects events for `period` (real time) without expecting anything.
    pub async fn drain_for(&mut self, period: Duration) -> Vec<SessionEvent> {
        let deadline = tokio::time::Instant::now() + period;
        let mut got = Vec::new();
        while let Ok(Some(ev)) = tokio::time::timeout_at(deadline, self.events.recv()).await {
            self.seen.push(ev.clone());
            got.push(ev);
        }
        got
    }

    /// Every `State` emitted so far.
    pub fn states(&self) -> Vec<SessionState> {
        self.seen
            .iter()
            .filter_map(|e| match e {
                SessionEvent::State(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }

    /// Sends `Close` and waits for the actor to finish.
    pub async fn close(mut self) {
        let _ = self.handle.send(SessionCommand::Close);
        let _ = tokio::time::timeout(Duration::from_secs(5), async {
            while self.events.recv().await.is_some() {}
        })
        .await;
    }
}

/// `true` for `State(Connected{..})`.
pub fn is_connected(e: &SessionEvent) -> bool {
    matches!(e, SessionEvent::State(SessionState::Connected { .. }))
}
