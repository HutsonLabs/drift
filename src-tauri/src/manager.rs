//! `SessionManager`: maps session windows (tabs) to session actors (task **M6-1**).
//!
//! Each Drift window (a native tab) owns at most one live session actor at a time. The
//! manager
//!
//! * starts actors through a [`SessionHost`] (production: `crate::host::TauriHost`, which
//!   creates the tab's render thread and calls `drift_rdp::spawn_session`; tests: a fake),
//! * runs one *pump* task per session that folds the actor's [`SessionEvent`]s into the
//!   window's [`SessionView`] and forwards them to that window only,
//! * forwards intents (certificate answers, reconnect, cancel, input) to the right actor,
//! * closes sessions gracefully — `SessionCommand::Close` then wait for the actor to exit —
//!   with a hard cap ([`CLOSE_CAP`]) after which the pump is aborted, and
//! * shuts every session down on quit within the same cap.
//!
//! All decisions here are plain Rust over channels, so the whole lifecycle is tested with fake
//! actors (`tests/session_manager.rs`) without AppKit or Tauri.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use drift_core::{CertFingerprint, ConnectionProfile};
use drift_rdp::{SessionCommand, SessionEvent, SessionEvents, SessionHandle};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::profiles::CommandError;
use crate::view::SessionView;

/// Longest a graceful close (one tab) or shutdown (quit, all tabs) may take before the
/// remaining actors are abandoned (plan M6-1: "shuts down gracefully on quit (2 s cap)").
pub const CLOSE_CAP: Duration = Duration::from_secs(2);

/// The side effects the manager needs from the application (humble object in production).
///
/// Methods are called from Tokio tasks (never with the manager's lock held); implementations
/// must not block on the main thread.
pub trait SessionHost: Send + Sync + 'static {
    /// Starts a session actor for `profile` in `window` (production: creates the tab's render
    /// thread, fetches the secrets and calls `drift_rdp::spawn_session`).
    fn spawn(
        &self,
        window: &str,
        profile: &ConnectionProfile,
    ) -> Result<(SessionHandle, SessionEvents), CommandError>;

    /// `window`'s view changed: emit it to that window's webview, update the tab title and
    /// the webview/RemoteView visibility.
    fn view_changed(&self, window: &str, view: &SessionView);

    /// A non-view event for `window` (cursor, capabilities, clipboard, stats).
    fn session_event(&self, window: &str, event: &SessionEvent);

    /// The user accepted a certificate with "remember": persist it as the profile's pin.
    fn certificate_pinned(&self, profile: Uuid, fingerprint: CertFingerprint);

    /// The session in `window` has ended (actor exited, closed or abandoned). Called exactly
    /// once per started session; release the tab's render thread and handle clones here.
    fn session_ended(&self, window: &str);
}

/// What [`SessionManager::reconnect_now`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconnect {
    /// The live actor was told to skip its backoff delay.
    Sent,
    /// The window's session has ended; the caller should [`SessionManager::open`] this
    /// profile again (after reloading it, so pins are current).
    Reopen(Uuid),
}

/// Result of [`SessionManager::shutdown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShutdownReport {
    /// Sessions whose actor exited within the cap.
    pub graceful: usize,
    /// Sessions abandoned at the cap.
    pub abandoned: usize,
}

/// Maps windows to sessions. Cheap to clone (shared state).
#[derive(Clone)]
pub struct SessionManager {
    shared: Arc<Shared>,
}

struct Shared {
    host: Arc<dyn SessionHost>,
    runtime: Handle,
    cap: Duration,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    windows: HashMap<String, Slot>,
    next_generation: u64,
}

/// One window's session record (kept after the actor ends so the window can reconnect).
struct Slot {
    profile: ConnectionProfile,
    view: SessionView,
    live: Option<Live>,
    /// Set by [`SessionManager::disconnect`]: when the actor ends, show the profiles screen.
    back_to_profiles: bool,
}

struct Live {
    generation: u64,
    handle: SessionHandle,
    pump: JoinHandle<()>,
}

impl std::fmt::Debug for SessionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionManager").field("cap", &self.shared.cap).finish_non_exhaustive()
    }
}

impl SessionManager {
    /// A manager whose pump tasks run on `runtime`.
    pub fn new(host: Arc<dyn SessionHost>, runtime: Handle) -> Self {
        let _ = (host, runtime);
        todo!("M6-1")
    }

    /// Replaces the close/shutdown cap (tests).
    #[must_use]
    pub fn with_close_cap(self, cap: Duration) -> Self {
        let _ = cap;
        todo!("M6-1")
    }

    /// Starts a session for `profile` in `window`, replacing (closing) any live one.
    pub fn open(&self, window: &str, profile: ConnectionProfile) -> Result<(), CommandError> {
        let _ = (window, profile);
        todo!("M6-1")
    }

    /// Sends `cmd` to `window`'s live session.
    pub fn send(&self, window: &str, cmd: SessionCommand) -> Result<(), CommandError> {
        let _ = (window, cmd);
        todo!("M6-1")
    }

    /// Answers the pending certificate prompt with "trust" (`pin` = remember).
    pub fn accept_certificate(
        &self,
        window: &str,
        fingerprint: CertFingerprint,
        pin: bool,
    ) -> Result<(), CommandError> {
        let _ = (window, fingerprint, pin);
        todo!("M6-1")
    }

    /// Answers the pending certificate prompt with "reject".
    pub fn reject_certificate(&self, window: &str) -> Result<(), CommandError> {
        let _ = window;
        todo!("M6-1")
    }

    /// "Reconnect now": skip the backoff of a live session, or ask the caller to reopen.
    pub fn reconnect_now(&self, window: &str) -> Result<Reconnect, CommandError> {
        let _ = window;
        todo!("M6-1")
    }

    /// Ends `window`'s session gracefully and returns the window to the profiles screen.
    pub fn disconnect(&self, window: &str) -> Result<(), CommandError> {
        let _ = window;
        todo!("M6-1")
    }

    /// Closes `window`'s session (if any) and forgets the window. Resolves when the actor has
    /// exited or the cap expired; returns `true` if it exited gracefully (or there was none).
    pub fn close(&self, window: &str) -> impl Future<Output = bool> + Send + 'static {
        let _ = window;
        async { todo!("M6-1") }
    }

    /// Closes every session (quit). Resolves within the cap.
    pub fn shutdown(&self) -> impl Future<Output = ShutdownReport> + Send + 'static {
        async { todo!("M6-1") }
    }

    /// The current view of `window`, if the window has had a session.
    pub fn view(&self, window: &str) -> Option<SessionView> {
        let _ = window;
        todo!("M6-1")
    }

    /// The profile id of `window`'s (current or last) session.
    pub fn profile_id(&self, window: &str) -> Option<Uuid> {
        let _ = window;
        todo!("M6-1")
    }

    /// Number of live session actors.
    pub fn live_sessions(&self) -> usize {
        todo!("M6-1")
    }

    /// Windows the manager knows (sorted).
    pub fn windows(&self) -> Vec<String> {
        todo!("M6-1")
    }

    #[allow(dead_code)]
    fn lock(&self) -> MutexGuard<'_, State> {
        self.shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
