//! The Connections window's model and the session windows' identity (task **UI-windows**,
//! `docs/adr/UI-windows-gallery.md`).
//!
//! Types in this module are the IPC contract between Rust and the Connections page
//! (`ConnectionsChanged`, `ThumbnailUpdated`, `ConnectionsIntentRequested`) and the session
//! windows' title-bar webview (`WindowIdentityChanged`).

use drift_core::ConnectMode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Where a profile's connection stands (gallery pills, Dock menu, Window menu, identity capsule).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectionStatus {
    /// No session window.
    Idle,
    /// Connecting, or waiting for a certificate decision (spinner).
    Connecting,
    /// The picture (or the GNOME login screen) is live (green).
    Live,
    /// Waiting out a reconnect backoff (amber).
    Reconnecting,
    /// Failed or disconnected with an explanation (red).
    Failed,
}

/// One profile that has a session window (the gallery's "Open" section).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct OpenConnection {
    /// The profile.
    pub profile_id: Uuid,
    /// Its window's label (`session-<n>`).
    pub window: String,
    /// Where the connection stands.
    pub status: ConnectionStatus,
    /// Uptime in seconds at emit time while live (the UI ticks locally).
    pub live_secs: Option<u32>,
    /// Seconds until the next attempt while reconnecting.
    pub reconnect_in_secs: Option<u32>,
    /// The reconnect attempt while reconnecting.
    pub attempt: Option<u32>,
}

/// Everything the Connections window needs besides the profile list.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
pub struct Connections {
    /// Profiles with a session window, in window opening order.
    pub open: Vec<OpenConnection>,
}

/// A session window's title bar: the identity capsule and the buttons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct WindowIdentity {
    /// The window's profile.
    pub profile_id: Uuid,
    /// Display name.
    pub name: String,
    /// Host as saved in the profile.
    pub host: String,
    /// Connection mode (picks the glyph).
    pub mode: ConnectMode,
    /// Dot or spinner.
    pub status: ConnectionStatus,
    /// Greeter hint (tooltip) while the GNOME login screen waits.
    pub hint: Option<String>,
    /// The statistics HUD is on (gauge pressed).
    pub show_stats: bool,
}

/// A live session's preview for its gallery card, as a `data:image/png;base64,…` URL.
/// `image: None` drops the preview (the session ended). Never logged, never written to disk.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Thumbnail {
    /// The profile whose card shows it.
    pub profile_id: Uuid,
    /// The PNG as a data URL, or `None`.
    pub image: Option<String>,
}

impl std::fmt::Debug for Thumbnail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Thumbnail")
            .field("profile_id", &self.profile_id)
            .field("image", &self.image.as_ref().map(|i| format!("<{} bytes>", i.len())))
            .finish()
    }
}

/// What the Connections page should open when Rust brings it forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ConnectionsIntent {
    /// The New Connection sheet (Cmd+N, Dock ▸ New Connection…).
    New,
    /// The edit sheet of a profile (Cmd+E, the error sheet's Edit Connection…).
    Edit {
        /// The profile to edit.
        profile_id: Uuid,
    },
}

/// Emitted to the `connections` page after any status change, window open or close.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct ConnectionsChanged(pub Connections);

/// Emitted to the `connections` page only, while it is visible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct ThumbnailUpdated(pub Thumbnail);

/// Emitted to the `connections` page to open a sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct ConnectionsIntentRequested(pub ConnectionsIntent);

/// Emitted to a session window's `<label>-titlebar` webview (identical pushes are skipped).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct WindowIdentityChanged(pub WindowIdentity);
