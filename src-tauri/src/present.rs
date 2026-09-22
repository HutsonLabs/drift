//! Pure presentation rules for a session window (tasks **M1-6**, **M6-2**, **M3-2**).
//!
//! The humble window glue (`crate::windows`) applies these decisions to AppKit/Tauri:
//! which surface is in front (webview or RemoteView), the tab title and subtitle, and the
//! remote pointer shape.

use std::sync::Arc;

use drift_core::ConnectionProfile;
use drift_macos::CursorShape;
use drift_macos::cursor::CursorImage;
use drift_macos::tabs::tab_title;
use drift_rdp::CursorUpdate;

use crate::view::{Screen, SessionView};

/// Title of a window without a session (the connect form).
pub const NEW_SESSION_TITLE: &str = "New Session";

/// Which view is in front of a session window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// The webview (form, certificate prompt, progress, reconnect overlay, error).
    Webview,
    /// The RemoteView with the live picture; the webview is hidden and the view is first
    /// responder.
    Remote,
}

/// The surface for a screen.
///
/// The webview is shown whenever there is no live picture (plan §2 decision 6). The GDM
/// greeter *is* a live picture — the user types the Linux password into it — so the
/// greeter-wait hint is not a webview screen; it goes into the window subtitle
/// ([`window_subtitle`]).
pub fn surface_for(screen: Screen) -> Surface {
    match screen {
        Screen::Live | Screen::GreeterHint => Surface::Remote,
        Screen::Profiles
        | Screen::Connecting
        | Screen::Certificate
        | Screen::Reconnecting
        | Screen::Error => Surface::Webview,
    }
}

/// The window (= tab) title: profile name plus state glyph, or [`NEW_SESSION_TITLE`] when the
/// window shows the connect form.
pub fn window_title(view: Option<&SessionView>) -> String {
    match view {
        Some(view) if view.screen != Screen::Profiles => tab_title(&view.profile_name, &view.state),
        _ => NEW_SESSION_TITLE.to_owned(),
    }
}

/// The window subtitle (shown next to the title in the title bar): the greeter hint while the
/// GNOME login screen is live, otherwise empty.
pub fn window_subtitle(view: Option<&SessionView>) -> String {
    let Some(view) = view else { return String::new() };
    if view.screen != Screen::GreeterHint {
        return String::new();
    }
    match (&view.linux_username, view.resuming) {
        (Some(user), true) => format!("Session is still running — log in as “{user}” to resume"),
        (Some(user), false) => format!("Log in as “{user}” to start your session"),
        (None, true) => "Session is still running — log in to resume".to_owned(),
        (None, false) => "Log in to start your session".to_owned(),
    }
}

/// Converts an actor pointer update into a RemoteView cursor and its desktop scale (percent).
/// Position updates do not change the shape (`None`); the Mac pointer is never warped.
pub fn cursor_shape(update: &CursorUpdate) -> Option<(CursorShape, u32)> {
    match update {
        CursorUpdate::Hidden => Some((CursorShape::Hidden, 100)),
        CursorUpdate::Default => Some((CursorShape::Default, 100)),
        CursorUpdate::Bitmap(bitmap) => Some((
            CursorShape::Image(Arc::new(CursorImage {
                size: bitmap.size,
                hotspot: bitmap.hotspot,
                bgra: bitmap.bgra.clone(),
            })),
            bitmap.scale,
        )),
        CursorUpdate::Position(_) => None,
    }
}

/// The profile `DRIFT_AUTOCONNECT` names: an exact name match, else a unique case-insensitive
/// match (surrounding whitespace ignored).
pub fn find_autoconnect<'a>(profiles: &'a [ConnectionProfile], name: &str) -> Option<&'a ConnectionProfile> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    if let Some(exact) = profiles.iter().find(|p| p.name == name) {
        return Some(exact);
    }
    let mut matches = profiles.iter().filter(|p| p.name.eq_ignore_ascii_case(name));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}
