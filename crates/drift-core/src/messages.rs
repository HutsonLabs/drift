//! User-facing explanations for disconnects, with next steps (tasks M1-6, M9-4).
//!
//! Kept in Rust (not in the webview) so the mode-specific wording is table-tested; the UI only
//! renders an [`ErrorExplanation`] and wires its [`ErrorAction`]s to commands.

use serde::{Deserialize, Serialize};

use crate::profile::ConnectMode;
use crate::state::DisconnectReason;

/// `x-apple.systempreferences` URL of System Settings › Privacy & Security › Local Network.
pub const LOCAL_NETWORK_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork";

/// Human path to the Local Network privacy pane, as shown in the UI.
pub const LOCAL_NETWORK_SETTINGS_PATH: &str = "System Settings › Privacy & Security › Local Network";

/// A button offered on an error screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum ErrorAction {
    /// Connect again.
    Reconnect,
    /// Open the profile form.
    EditProfile,
    /// Open System Settings › Privacy & Security › Local Network.
    OpenLocalNetworkSettings,
    /// Close the tab.
    Close,
}

/// What went wrong, in words, and what the user can do about it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ErrorExplanation {
    /// Short headline.
    pub title: String,
    /// One or two sentences describing the problem.
    pub message: String,
    /// Concrete next steps, in order.
    pub next_steps: Vec<String>,
    /// Buttons to offer; the first is the default.
    pub actions: Vec<ErrorAction>,
}

fn explanation(
    title: &str,
    message: impl Into<String>,
    steps: &[&str],
    actions: &[ErrorAction],
) -> ErrorExplanation {
    ErrorExplanation {
        title: title.into(),
        message: message.into(),
        next_steps: steps.iter().map(|s| (*s).to_owned()).collect(),
        actions: actions.to_vec(),
    }
}

/// Where the RDP credentials of each mode are configured on the host.
fn credential_hint(mode: ConnectMode) -> &'static str {
    match mode {
        ConnectMode::RemoteLogin => {
            "Use the system RDP credentials: on the host run “sudo grdctl --system status --show-credentials”."
        }
        ConnectMode::Headless => {
            "Use the credentials set with “grdctl --headless rdp set-credentials” for that user on the host."
        }
        ConnectMode::DesktopSharing => {
            "Desktop Sharing credentials must be set in GNOME Settings on the host."
        }
    }
}

/// The service that must be running on the host for each mode.
fn service_hint(mode: ConnectMode) -> &'static str {
    match mode {
        ConnectMode::RemoteLogin => {
            "Check that Remote Login is enabled on the host: “sudo grdctl --system status”."
        }
        ConnectMode::Headless => "Headless session not running — see host/drift-host-setup.sh.",
        ConnectMode::DesktopSharing => {
            "Check that Desktop Sharing is on in GNOME Settings › System › Remote Desktop and that someone is logged in on the host."
        }
    }
}

/// Where to read the certificate fingerprint on the host, per mode.
fn fingerprint_hint(mode: ConnectMode) -> &'static str {
    match mode {
        ConnectMode::RemoteLogin => "Compare it with “sudo grdctl --system status” on the host.",
        ConnectMode::Headless => "Compare it with “grdctl --headless status” on the host.",
        ConnectMode::DesktopSharing => "Compare it with “grdctl status” on the host.",
    }
}

/// Explains why a session with `mode` ended with `reason` (M9-4 texts).
pub fn explain_disconnect(reason: &DisconnectReason, mode: ConnectMode) -> ErrorExplanation {
    todo!("Red: not implemented yet")
}
