//! IPC commands exposed to the webview.
//!
//! Commands are thin: profile logic lives in [`crate::profiles::ProfileService`] and
//! `drift-core`; session intents are forwarded to the `SessionManager` (M6-1). Intents whose
//! backend is not wired yet return [`CommandError::NotImplemented`] so the UI can already be
//! built and tested against the final signatures.

use drift_core::{
    CertFingerprint, ConnectMode, ConnectionProfile, DisconnectReason, ErrorExplanation, ProfileIssue,
    explain_disconnect,
};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::profiles::{CommandError, ProfileEntry, ProfileService, SecretsUpdate};

/// Shared state managed by Tauri.
#[derive(Debug)]
pub struct AppState {
    /// Saved profiles and their passwords.
    pub profiles: ProfileService,
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

fn session_manager_pending(what: &str) -> Result<(), CommandError> {
    Err(CommandError::NotImplemented { what: format!("{what} (SessionManager, task M6-1)") })
}

/// Connects this window's session to the saved profile `profile_id`.
#[tauri::command]
#[specta::specta]
pub fn connect(window: tauri::Window, profile_id: Uuid) -> Result<(), CommandError> {
    let _ = (window, profile_id);
    session_manager_pending("Connecting")
}

/// Trusts the prompted certificate (`pin` = remember it for this profile).
#[tauri::command]
#[specta::specta]
pub fn accept_certificate(
    window: tauri::Window,
    fingerprint: CertFingerprint,
    pin: bool,
) -> Result<(), CommandError> {
    let _ = (window, fingerprint, pin);
    session_manager_pending("Accepting certificates")
}

/// Rejects the prompted certificate (the session fails with `CertMismatch`).
#[tauri::command]
#[specta::specta]
pub fn reject_certificate(window: tauri::Window) -> Result<(), CommandError> {
    let _ = window;
    session_manager_pending("Rejecting certificates")
}

/// Skips the backoff delay and reconnects now ("Now" button, error screen "Reconnect").
#[tauri::command]
#[specta::specta]
pub fn reconnect_now(window: tauri::Window) -> Result<(), CommandError> {
    let _ = window;
    session_manager_pending("Reconnecting")
}

/// Stops reconnecting; the session stays disconnected ("Cancel" button).
#[tauri::command]
#[specta::specta]
pub fn cancel_reconnect(window: tauri::Window) -> Result<(), CommandError> {
    let _ = window;
    session_manager_pending("Cancelling reconnection")
}

/// Closes this window's session gracefully (and the tab).
#[tauri::command]
#[specta::specta]
pub fn close_session(window: tauri::Window) -> Result<(), CommandError> {
    let _ = window;
    session_manager_pending("Closing sessions")
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
    fn pending_session_intents_say_so() {
        let e = session_manager_pending("Connecting").unwrap_err();
        assert!(matches!(&e, CommandError::NotImplemented { what } if what.contains("M6-1")));
        assert!(e.to_string().ends_with("is not available yet"));
    }
}
