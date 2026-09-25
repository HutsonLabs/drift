//! IPC commands exposed to the webview.
//!
//! Commands are thin: profile logic lives in [`crate::profiles::ProfileService`] and
//! `drift-core`; session intents are forwarded to the `SessionManager` (M6-1). Intents whose
//! backend is not wired yet return [`CommandError::NotImplemented`] so the UI can already be
//! built and tested against the final signatures.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use drift_core::{
    CertFingerprint, ConnectMode, ConnectionProfile, DisconnectReason, ErrorExplanation, ProfileIssue,
    explain_disconnect,
};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::connections::{Connections, WindowIdentity};
use crate::manager::SessionManager;
use crate::profiles::{CommandError, ProfileEntry, ProfileService, SecretsUpdate};
use crate::strip::TabStrip;

/// Shared state managed by Tauri.
#[derive(Debug)]
pub struct AppState {
    /// Saved profiles and their passwords.
    pub profiles: Arc<ProfileService>,
    /// Live sessions, one per window (tab).
    pub sessions: SessionManager,
    next_window: AtomicU64,
    closing: Mutex<HashSet<String>>,
    quitting: AtomicBool,
}

impl AppState {
    /// The state for a running app.
    pub fn new(profiles: Arc<ProfileService>, sessions: SessionManager) -> Self {
        Self {
            profiles,
            sessions,
            next_window: AtomicU64::new(0),
            closing: Mutex::new(HashSet::new()),
            quitting: AtomicBool::new(false),
        }
    }

    /// The number of the next session window.
    pub(crate) fn next_window(&self) -> u64 {
        self.next_window.fetch_add(1, Ordering::Relaxed)
    }

    fn closing(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
        self.closing.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Marks `window` as closing; `false` if it already was (the close is under way).
    pub(crate) fn begin_closing(&self, window: &str) -> bool {
        self.closing().insert(window.to_owned())
    }

    /// Whether `window` is being closed by Drift (its `CloseRequested` may proceed).
    pub(crate) fn is_closing(&self, window: &str) -> bool {
        self.closing().contains(window)
    }

    /// The window is gone.
    pub(crate) fn finish_closing(&self, window: &str) {
        self.closing().remove(window);
    }

    /// Starts the quit sequence; `false` if it is already running.
    pub(crate) fn begin_quit(&self) -> bool {
        !self.quitting.swap(true, Ordering::SeqCst)
    }
}

/// Static information about the running app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Product name.
    pub name: String,
    /// Crate version.
    pub version: String,
}

/// Returns the app name and version.
#[tauri::command]
#[specta::specta]
pub fn app_info() -> AppInfo {
    AppInfo { name: "Drift".into(), version: env!("CARGO_PKG_VERSION").into() }
}

/// Lists saved profiles (sorted by name) with which passwords are stored.
#[tauri::command]
#[specta::specta]
pub fn list_profiles(state: State<'_, AppState>) -> Result<Vec<ProfileEntry>, CommandError> {
    state.profiles.list()
}

/// A new, unsaved profile for `mode` with a fresh id and default settings.
#[tauri::command]
#[specta::specta]
pub fn new_profile(mode: ConnectMode) -> ConnectionProfile {
    ConnectionProfile::new("", "", mode)
}

/// Validates a profile without saving it; an empty list means valid.
#[tauri::command]
#[specta::specta]
pub fn validate_profile(profile: ConnectionProfile) -> Vec<ProfileIssue> {
    drift_core::profile::validate(&profile).err().unwrap_or_default()
}

/// Saves (inserts or updates) a profile and applies password changes.
#[tauri::command]
#[specta::specta]
pub fn save_profile(
    state: State<'_, AppState>,
    profile: ConnectionProfile,
    secrets: SecretsUpdate,
) -> Result<ProfileEntry, CommandError> {
    state.profiles.save(profile, secrets)
}

/// Deletes a profile and its stored passwords.
#[tauri::command]
#[specta::specta]
pub fn delete_profile(state: State<'_, AppState>, id: Uuid) -> Result<(), CommandError> {
    state.profiles.delete(id)
}

/// Clears a profile's pinned certificate (after a legitimate certificate change).
#[tauri::command]
#[specta::specta]
pub fn forget_certificate(state: State<'_, AppState>, id: Uuid) -> Result<ProfileEntry, CommandError> {
    state.profiles.set_pin(id, None)?;
    state.profiles.get(id)
}

/// Explains a disconnect reason for a mode (title, message, next steps, actions).
#[tauri::command]
#[specta::specta]
pub fn explain(reason: DisconnectReason, mode: ConnectMode) -> ErrorExplanation {
    explain_disconnect(&reason, mode)
}

/// Opens System Settings › Privacy & Security › Local Network (errno 65 screen).
#[tauri::command]
#[specta::specta]
pub fn open_local_network_settings() -> Result<(), CommandError> {
    crate::platform::open_url(drift_core::messages::LOCAL_NETWORK_SETTINGS_URL)
}

/// Connects this window's session to the saved profile `profile_id`.
#[tauri::command]
#[specta::specta]
pub fn connect(app: tauri::AppHandle, window: tauri::Window, profile_id: Uuid) -> Result<(), CommandError> {
    crate::windows::connect_profile(&app, window.label(), profile_id)
}

/// Trusts the prompted certificate (`pin` = remember it for this profile).
#[tauri::command]
#[specta::specta]
pub fn accept_certificate(
    state: State<'_, AppState>,
    window: tauri::Window,
    fingerprint: CertFingerprint,
    pin: bool,
) -> Result<(), CommandError> {
    state.sessions.accept_certificate(window.label(), fingerprint, pin)
}

/// Rejects the prompted certificate (the session fails with `CertMismatch`).
#[tauri::command]
#[specta::specta]
pub fn reject_certificate(state: State<'_, AppState>, window: tauri::Window) -> Result<(), CommandError> {
    state.sessions.reject_certificate(window.label())
}

/// Skips the backoff delay and reconnects now ("Now" button, error screen "Reconnect").
#[tauri::command]
#[specta::specta]
pub fn reconnect_now(app: tauri::AppHandle, window: tauri::Window) -> Result<(), CommandError> {
    crate::windows::reconnect(&app, window.label())
}

/// Stops reconnecting; the session stays disconnected ("Cancel" button).
#[tauri::command]
#[specta::specta]
pub fn cancel_reconnect(state: State<'_, AppState>, window: tauri::Window) -> Result<(), CommandError> {
    state.sessions.send(window.label(), drift_rdp::SessionCommand::Cancel)
}

/// Ends this window's session gracefully and returns it to the connect form.
#[tauri::command]
#[specta::specta]
pub fn disconnect(state: State<'_, AppState>, window: tauri::Window) -> Result<(), CommandError> {
    state.sessions.disconnect(window.label())
}

/// Closes this window's session gracefully and then the tab.
#[tauri::command]
#[specta::specta]
pub fn close_session(app: tauri::AppHandle, window: tauri::Window) -> Result<(), CommandError> {
    crate::windows::close_tab(&app, window.label());
    Ok(())
}

// ---- Connections gallery and session windows (UI-windows) ------------------------------------

fn not_yet(what: &str) -> CommandError {
    CommandError::NotImplemented { what: what.to_owned() }
}

/// Copies a profile and its stored passwords under a new id, named "<name> copy", without the
/// certificate pin.
#[tauri::command]
#[specta::specta]
pub fn duplicate_profile(state: State<'_, AppState>, id: Uuid) -> Result<ProfileEntry, CommandError> {
    let _ = (state, id);
    Err(not_yet("duplicate_profile"))
}

/// The profiles that have a session window (pulled by the Connections page on load).
#[tauri::command]
#[specta::specta]
pub fn connections(app: tauri::AppHandle) -> Result<Connections, CommandError> {
    let _ = app;
    Err(not_yet("connections"))
}

/// Brings `profile_id`'s session window forward; `NotFound` if it has none.
#[tauri::command]
#[specta::specta]
pub fn show_window(app: tauri::AppHandle, profile_id: Uuid) -> Result<(), CommandError> {
    let _ = (app, profile_id);
    Err(not_yet("show_window"))
}

/// Closes `profile_id`'s session window without asking (the card's Disconnect).
#[tauri::command]
#[specta::specta]
pub fn disconnect_profile(app: tauri::AppHandle, profile_id: Uuid) -> Result<(), CommandError> {
    let _ = (app, profile_id);
    Err(not_yet("disconnect_profile"))
}

/// Shows the Connections window and makes it key; with `edit`, opens that profile's edit sheet.
#[tauri::command]
#[specta::specta]
pub fn show_connections(app: tauri::AppHandle, edit: Option<Uuid>) -> Result<(), CommandError> {
    let _ = (app, edit);
    Err(not_yet("show_connections"))
}

/// Shows the Connections window with the New Connection sheet.
#[tauri::command]
#[specta::specta]
pub fn new_connection(app: tauri::AppHandle) -> Result<(), CommandError> {
    let _ = app;
    Err(not_yet("new_connection"))
}

/// The calling session window's identity (pulled by its title bar on load).
#[tauri::command]
#[specta::specta]
pub fn window_identity(app: tauri::AppHandle, window: tauri::Window) -> Result<WindowIdentity, CommandError> {
    let _ = (app, window);
    Err(not_yet("window_identity"))
}

/// Turns the calling session window's statistics HUD on or off (the title bar's gauge).
#[tauri::command]
#[specta::specta]
pub fn toggle_stats(app: tauri::AppHandle, window: tauri::Window) -> Result<(), CommandError> {
    let _ = (app, window);
    Err(not_yet("toggle_stats"))
}

// ---- the tab strip (UI-tabs) -------------------------------------------------------------------

/// The tab strip of the calling window: its group's tabs in order, its own tab active.
#[tauri::command]
#[specta::specta]
pub fn tab_strip(
    app: tauri::AppHandle,
    window: tauri::Window,
    webview: tauri::Webview,
) -> Result<TabStrip, CommandError> {
    let label = window.label().to_owned();
    tracing::debug!(window = %label, webview = %webview.label(), "tab strip requested");
    let handle = app.clone();
    crate::windows::on_main(&app, move |_| crate::windows::tab_strip(&handle, &label))?
        .ok_or(CommandError::Platform { message: "this window is not a session tab".into() })
}

/// Selects tab `tab` (a click on it in the strip).
#[tauri::command]
#[specta::specta]
pub fn select_tab(app: tauri::AppHandle, tab: String) -> Result<(), CommandError> {
    crate::windows::select_tab(&app, &tab)
}

/// Closes tab `tab` (its × in the strip): the session closes gracefully, then the window.
#[tauri::command]
#[specta::specta]
pub fn close_tab(app: tauri::AppHandle, tab: String) -> Result<(), CommandError> {
    let known = {
        let (handle, tab) = (app.clone(), tab.clone());
        crate::windows::on_main(&app, move |_| crate::windows::tab_count(&handle, &tab).is_some())?
    };
    if !known {
        return Err(CommandError::Platform { message: format!("there is no tab {tab}") });
    }
    crate::windows::close_tab(&app, &tab);
    Ok(())
}

/// Opens a new Connection Manager tab (the strip's +, like File ▸ New Tab).
#[tauri::command]
#[specta::specta]
pub fn new_tab(app: tauri::AppHandle) -> Result<(), CommandError> {
    crate::windows::open_tab(&app);
    Ok(())
}

/// Gives the keyboard back to the calling window's page or live picture (the strip never keeps
/// it).
#[tauri::command]
#[specta::specta]
pub fn focus_content(app: tauri::AppHandle, window: tauri::Window) -> Result<(), CommandError> {
    crate::windows::focus_content(&app, window.label())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_reports_version() {
        let info = app_info();
        assert_eq!(info.name, "Drift");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn new_profile_uses_mode_defaults() {
        let a = new_profile(ConnectMode::RemoteLogin);
        let b = new_profile(ConnectMode::RemoteLogin);
        assert_ne!(a.id, b.id);
        assert_eq!(a.port, 3389);
        assert_eq!(a.mode, ConnectMode::RemoteLogin);
        assert!(a.name.is_empty() && a.host.is_empty());
    }

    #[test]
    fn validate_profile_lists_issues() {
        let issues = validate_profile(new_profile(ConnectMode::Headless));
        let fields: Vec<_> = issues.iter().map(|i| i.field).collect();
        use drift_core::ProfileField as F;
        assert_eq!(fields, [F::Name, F::Host, F::RdpUsername]);
        let mut ok = new_profile(ConnectMode::Headless);
        ok.name = "n".into();
        ok.host = "h".into();
        ok.rdp_username = "u".into();
        assert!(validate_profile(ok).is_empty());
    }

    #[test]
    fn explain_forwards_to_core() {
        let e = explain(DisconnectReason::LocalNetworkDenied, ConnectMode::Headless);
        assert!(e.next_steps[0].contains("Local Network"));
    }

    #[test]
    fn no_session_error_is_user_readable() {
        assert_eq!(CommandError::NoSession.to_string(), "this tab has no active session");
    }
}
