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
        f.debug_struct("SessionManager")
            .field("cap", &self.shared.cap)
            .field("windows", &self.windows())
            .finish_non_exhaustive()
    }
}

impl SessionManager {
    /// A manager whose pump tasks run on `runtime`.
    pub fn new(host: Arc<dyn SessionHost>, runtime: Handle) -> Self {
        Self {
            shared: Arc::new(Shared { host, runtime, cap: CLOSE_CAP, state: Mutex::new(State::default()) }),
        }
    }

    /// Starts a session for `profile` in `window`.
    ///
    /// A live session in the same window is replaced: it is told to `Close`, its pump stops and
    /// [`SessionHost::session_ended`] runs for it *before* the new actor is spawned, so the host
    /// can reuse the window's resources.
    pub fn open(&self, window: &str, profile: ConnectionProfile) -> Result<(), CommandError> {
        let old = self.shared.lock().windows.get_mut(window).and_then(|slot| slot.live.take());
        if let Some(old) = old {
            let _ = old.handle.send(SessionCommand::Close);
            old.pump.abort();
            self.shared.host.session_ended(window);
        }
        let (handle, events) = self.shared.host.spawn(window, &profile)?;
        let mut state = self.shared.lock();
        state.next_generation += 1;
        let generation = state.next_generation;
        let pump = self.shared.runtime.spawn(pump(
            self.shared.clone(),
            window.to_owned(),
            generation,
            profile.id,
            events,
        ));
        let view = SessionView::new(&profile);
        state.windows.insert(
            window.to_owned(),
            Slot { profile, view, live: Some(Live { generation, handle, pump }), back_to_profiles: false },
        );
        Ok(())
    }

    /// Sends `cmd` to `window`'s live session.
    pub fn send(&self, window: &str, cmd: SessionCommand) -> Result<(), CommandError> {
        let state = self.shared.lock();
        let live = state.windows.get(window).and_then(|s| s.live.as_ref()).ok_or(CommandError::NoSession)?;
        live.handle.send(cmd).map_err(|_| CommandError::NoSession)
    }

    /// Answers the pending certificate prompt with "trust" (`pin` = remember it).
    pub fn accept_certificate(
        &self,
        window: &str,
        fingerprint: CertFingerprint,
        pin: bool,
    ) -> Result<(), CommandError> {
        self.answer_certificate(window, SessionCommand::AcceptCertificate { fingerprint, pin })
    }

    /// Answers the pending certificate prompt with "reject".
    pub fn reject_certificate(&self, window: &str) -> Result<(), CommandError> {
        self.answer_certificate(window, SessionCommand::RejectCertificate)
    }

    fn answer_certificate(&self, window: &str, answer: SessionCommand) -> Result<(), CommandError> {
        self.send(window, answer)?;
        // Dismiss the prompt at once; the actor's next state reports the outcome.
        let view = {
            let mut state = self.shared.lock();
            let slot = state.windows.get_mut(window).ok_or(CommandError::NoSession)?;
            if slot.view.certificate.is_none() {
                return Ok(());
            }
            slot.view.clear_certificate();
            slot.view.clone()
        };
        self.shared.host.view_changed(window, &view);
        Ok(())
    }

    /// Turns `window`'s statistics HUD on or off (Session ▸ Show Statistics).
    ///
    /// The choice is per tab and survives session events; it only decides whether the sample in
    /// the view is drawn over the picture (plan M1 "Done (manual M1)", M9-1).
    pub fn toggle_stats(&self, window: &str) -> Result<(), CommandError> {
        let view = {
            let mut state = self.shared.lock();
            let slot = state.windows.get_mut(window).ok_or(CommandError::NoSession)?;
            slot.view.show_stats = !slot.view.show_stats;
            slot.view.clone()
        };
        self.shared.host.view_changed(window, &view);
        Ok(())
    }

    /// "Reconnect now": skip the backoff of a live session, or ask the caller to reopen.
    pub fn reconnect_now(&self, window: &str) -> Result<Reconnect, CommandError> {
        let state = self.shared.lock();
        let slot = state.windows.get(window).ok_or(CommandError::NoSession)?;
        match &slot.live {
            Some(live) => {
                live.handle.send(SessionCommand::ReconnectNow).map_err(|_| CommandError::NoSession)?;
                Ok(Reconnect::Sent)
            }
            None => Ok(Reconnect::Reopen(slot.profile.id)),
        }
    }

    /// Ends `window`'s session gracefully and returns the window to the profiles screen (the tab
    /// becomes a Connection Manager again).
    ///
    /// If the actor has already ended (a failed tab showing its error), the window is reset at
    /// once (UI-tabs board 5: "Closing it, or Disconnect, turns it back into a Connection
    /// Manager").
    pub fn disconnect(&self, window: &str) -> Result<(), CommandError> {
        let reset = {
            let mut state = self.shared.lock();
            let slot = state.windows.get_mut(window).ok_or(CommandError::NoSession)?;
            match slot.live.as_ref() {
                Some(live) => {
                    live.handle.send(SessionCommand::Close).map_err(|_| CommandError::NoSession)?;
                    slot.back_to_profiles = true;
                    None
                }
                None => {
                    slot.view = SessionView::new(&slot.profile);
                    Some(slot.view.clone())
                }
            }
        };
        if let Some(view) = reset {
            self.shared.host.view_changed(window, &view);
        }
        Ok(())
    }

    /// The window (other than `except`) with a live session for `profile`, if any: activating
    /// a connection that is already open switches to its tab instead of opening a second
    /// session (UI-tabs board 1). The first such window by label when there are several.
    pub fn live_window_for(&self, profile: Uuid, except: &str) -> Option<String> {
        let state = self.shared.lock();
        let mut windows: Vec<&String> = state
            .windows
            .iter()
            .filter(|(window, slot)| {
                window.as_str() != except && slot.live.is_some() && slot.profile.id == profile
            })
            .map(|(window, _)| window)
            .collect();
        windows.sort();
        windows.first().map(|w| (*w).clone())
    }

    /// Profiles with a live session in some window (sorted, no duplicates).
    pub fn live_profiles(&self) -> Vec<Uuid> {
        let mut ids: Vec<Uuid> =
            self.shared.lock().windows.values().filter(|s| s.live.is_some()).map(|s| s.profile.id).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Closes `window`'s session (if any) and forgets the window. Resolves when the actor has
    /// exited or the cap expired; `true` means it exited gracefully (or there was none).
    ///
    /// The window is forgotten immediately (before the future is polled), so no later view or
    /// session event reaches it.
    pub fn close(&self, window: &str) -> impl Future<Output = bool> + Send + 'static {
        let live = self.shared.lock().windows.remove(window).and_then(|slot| slot.live);
        if let Some(live) = &live {
            let _ = live.handle.send(SessionCommand::Close);
        }
        let shared = self.shared.clone();
        let window = window.to_owned();
        async move {
            let Some(live) = live else { return true };
            let deadline = tokio::time::Instant::now() + shared.cap;
            let graceful = finish(live, deadline).await;
            shared.host.session_ended(&window);
            graceful
        }
    }

    /// Closes every session (quit): all actors are told to `Close` at once and awaited until a
    /// common deadline ([`CLOSE_CAP`] from now); the rest are abandoned. Every window is
    /// forgotten immediately.
    pub fn shutdown(&self) -> impl Future<Output = ShutdownReport> + Send + 'static {
        let mut lives: Vec<(String, Live)> = self
            .shared
            .lock()
            .windows
            .drain()
            .filter_map(|(window, slot)| slot.live.map(|live| (window, live)))
            .collect();
        lives.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, live) in &lives {
            let _ = live.handle.send(SessionCommand::Close);
        }
        let shared = self.shared.clone();
        async move {
            let deadline = tokio::time::Instant::now() + shared.cap;
            let mut report = ShutdownReport::default();
            for (window, live) in lives {
                if finish(live, deadline).await {
                    report.graceful += 1;
                } else {
                    report.abandoned += 1;
                }
                shared.host.session_ended(&window);
            }
            report
        }
    }

    /// The current view of `window`, if the window has had a session.
    pub fn view(&self, window: &str) -> Option<SessionView> {
        self.shared.lock().windows.get(window).map(|s| s.view.clone())
    }

    /// The profile id of `window`'s (current or last) session.
    pub fn profile_id(&self, window: &str) -> Option<Uuid> {
        self.shared.lock().windows.get(window).map(|s| s.profile.id)
    }

    /// Whether `window` has a live session actor.
    pub fn is_live(&self, window: &str) -> bool {
        self.shared.lock().windows.get(window).is_some_and(|s| s.live.is_some())
    }

    /// Every window with a live session actor, as `(window, clipboard preference, is waiting
    /// out a reconnect backoff, actor generation)`. The app's platform services fan out over
    /// this (M5-3, M7-2); the generation changes when a window's actor is replaced.
    pub fn live_session_states(&self) -> Vec<(String, drift_core::ClipboardPrefs, bool, u64)> {
        self.shared
            .lock()
            .windows
            .iter()
            .filter_map(|(window, slot)| {
                let live = slot.live.as_ref()?;
                let reconnecting = matches!(slot.view.state, drift_core::SessionState::Reconnecting { .. });
                Some((window.clone(), slot.profile.clipboard, reconnecting, live.generation))
            })
            .collect()
    }

    /// Number of live session actors.
    pub fn live_sessions(&self) -> usize {
        self.shared.lock().windows.values().filter(|s| s.live.is_some()).count()
    }

    /// Windows the manager knows (sorted).
    pub fn windows(&self) -> Vec<String> {
        let mut windows: Vec<String> = self.shared.lock().windows.keys().cloned().collect();
        windows.sort();
        windows
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Drops the manager's handle (the caller has already sent `Close`) and waits until `deadline`
/// for the pump — that is, for the actor to exit and its event stream to end. Aborts it after.
async fn finish(live: Live, deadline: tokio::time::Instant) -> bool {
    let Live { handle, mut pump, .. } = live;
    drop(handle);
    if tokio::time::timeout_at(deadline, &mut pump).await.is_ok() {
        true
    } else {
        pump.abort();
        false
    }
}

/// One session's event pump: folds events into its window's view and forwards them to the host
/// for that window only. Events of a session that no longer owns its window (closed or
/// replaced) are drained and dropped, so the pump ends exactly when the actor's stream ends.
async fn pump(
    shared: Arc<Shared>,
    window: String,
    generation: u64,
    profile_id: Uuid,
    mut events: SessionEvents,
) {
    while let Some(event) = events.recv().await {
        let view = {
            let mut state = shared.lock();
            match state.windows.get_mut(&window) {
                Some(slot) if slot.live.as_ref().is_some_and(|l| l.generation == generation) => {
                    slot.view.apply(&event).then(|| slot.view.clone())
                }
                _ => continue,
            }
        };
        match &event {
            SessionEvent::State(_) | SessionEvent::CertificatePrompt { .. } => {}
            SessionEvent::CertificatePinned(fingerprint) => {
                shared.host.certificate_pinned(profile_id, *fingerprint);
            }
            other => shared.host.session_event(&window, other),
        }
        if let Some(view) = view {
            shared.host.view_changed(&window, &view);
        }
    }
    // The actor exited by itself (failure, remote logoff, or after `disconnect`).
    let ended = {
        let mut state = shared.lock();
        match state.windows.get_mut(&window) {
            Some(slot) if slot.live.as_ref().is_some_and(|l| l.generation == generation) => {
                slot.live = None;
                if std::mem::take(&mut slot.back_to_profiles) {
                    slot.view = SessionView::new(&slot.profile);
                    Some(Some(slot.view.clone()))
                } else {
                    Some(None)
                }
            }
            _ => None,
        }
    };
    if let Some(reset) = ended {
        shared.host.session_ended(&window);
        if let Some(view) = reset {
            shared.host.view_changed(&window, &view);
        }
    }
}
