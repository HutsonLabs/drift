//! The tab strip model (task **UI-tabs**, `docs/design/mockup-glass.html` boards 0–5).
//!
//! Every Drift window is one tab of a native tab group, and every tab is one of two kinds:
//!
//! * a **Connection Manager** — the saved connections and the form, titled
//!   [`CONNECTIONS_TITLE`] with a neutral icon, or
//! * a **Session** — the connection's mode glyph, its name and a status dot (green live, amber
//!   reconnecting, red failed); while connecting a spinner and "Connecting to <name>…".
//!
//! AppKit's own tab bar is hidden (`drift_macos::tabs::hide_native_tab_bar`); each window
//! draws the strip in HTML in a small webview of its own, above the page and the live picture.
//! Rust pushes a [`TabStrip`] to every window whenever a tab is added, closed or changes state
//! (`crate::windows`), so this module is the whole of the strip's logic and stays pure.

use drift_core::ConnectMode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::present;
use crate::view::{Screen, SessionView};

/// Height of the tab strip — the transparent title bar row the tabs sit in — in points.
pub const STRIP_HEIGHT: f64 = 46.0;

/// Title of a Connection Manager tab (and of its window, which the Window menu lists).
pub const CONNECTIONS_TITLE: &str = "Connections";

/// The two kinds of tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum TabKind {
    /// Pick, edit and connect a saved connection.
    Manager,
    /// One session (connecting, live, reconnecting or failed).
    Session,
}

/// What a tab's status indicator shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum TabStatus {
    /// Nothing (a Connection Manager).
    Idle,
    /// A spinner instead of the glyph: connecting or waiting for a certificate decision.
    Connecting,
    /// Green dot: the picture (or the GNOME login screen) is live.
    Live,
    /// Amber dot: waiting out a reconnect backoff.
    Reconnecting,
    /// Red dot: the session failed or was disconnected with an explanation.
    Failed,
}

/// One tab of the strip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct TabItem {
    /// The tab's window label (`session-<n>`); intents from the strip name tabs by it.
    pub id: String,
    /// Connection Manager or Session.
    pub kind: TabKind,
    /// The text on the tab.
    pub title: String,
    /// The connection mode (picks the glyph); `None` for a Connection Manager.
    pub mode: Option<ConnectMode>,
    /// Spinner or dot.
    pub status: TabStatus,
    /// The session's profile; `None` for a Connection Manager.
    pub profile_id: Option<Uuid>,
    /// Tooltip: the greeter hint while the GNOME login screen waits for the user.
    pub hint: Option<String>,
}

/// Everything one window's strip draws.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct TabStrip {
    /// The tabs of the window's group, leading to trailing.
    pub tabs: Vec<TabItem>,
    /// The id of the window this strip belongs to: its own tab is the raised one.
    pub active: String,
    /// Profiles with a live session in some tab (sorted); the connection list marks them.
    pub live_profiles: Vec<Uuid>,
}

impl TabStrip {
    /// A strip of `tabs` (in group order) for window `active`.
    pub fn new(tabs: Vec<TabItem>, active: &str, mut live_profiles: Vec<Uuid>) -> Self {
        live_profiles.sort_unstable();
        live_profiles.dedup();
        Self { tabs, active: active.to_owned(), live_profiles }
    }
}

/// Emitted to a window's strip webview and page whenever its [`TabStrip`] changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct TabStripChanged(pub TabStrip);

/// Label of window `window`'s strip webview. It starts with the window's label, so the
/// `session-*` capability covers it.
pub fn strip_label(window: &str) -> String {
    format!("{window}-strip")
}

/// The tab of window `id`, whose session view (if it ever had a session) is `view` and whose
/// session profile is `profile`.
pub fn tab_item(id: &str, view: Option<&SessionView>, profile: Option<Uuid>) -> TabItem {
    let Some(view) = view.filter(|v| v.screen != Screen::Profiles) else {
        return TabItem {
            id: id.to_owned(),
            kind: TabKind::Manager,
            title: CONNECTIONS_TITLE.to_owned(),
            mode: None,
            status: TabStatus::Idle,
            profile_id: None,
            hint: None,
        };
    };
    let name = drift_macos::tabs::display_name(&view.profile_name);
    let (title, status) = match view.screen {
        Screen::Connecting => (format!("Connecting to {name}…"), TabStatus::Connecting),
        Screen::Certificate => (name, TabStatus::Connecting),
        Screen::Live | Screen::GreeterHint => (name, TabStatus::Live),
        Screen::Reconnecting => (name, TabStatus::Reconnecting),
        Screen::Error | Screen::Profiles => (name, TabStatus::Failed),
    };
    TabItem {
        id: id.to_owned(),
        kind: TabKind::Session,
        title,
        mode: Some(view.mode),
        status,
        profile_id: profile,
        hint: present::greeter_hint(Some(view)),
    }
}
