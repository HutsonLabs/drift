//! Pure presentation rules for a session window (tasks **M1-6**, **M6-2**, **M3-2**).
//!
//! The humble window glue (`crate::windows`) applies these decisions to AppKit/Tauri:
//! which surface is in front (webview or RemoteView), the tab title and subtitle, and the
//! remote pointer shape.

use drift_core::ConnectionProfile;
use drift_macos::CursorShape;
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
pub fn surface_for(screen: Screen) -> Surface {
    let _ = screen;
    todo!("M1-6")
}

/// The window (= tab) title: profile name plus state glyph, or [`NEW_SESSION_TITLE`].
pub fn window_title(view: Option<&SessionView>) -> String {
    let _ = view;
    todo!("M6-2")
}

/// The window subtitle (shown under the title in the title bar): the greeter hint while the
/// GNOME login screen is live, otherwise empty.
pub fn window_subtitle(view: Option<&SessionView>) -> String {
    let _ = view;
    todo!("M3-2")
}

/// Converts an actor pointer update into a RemoteView cursor and its desktop scale (percent).
/// Position updates do not change the shape (`None`).
pub fn cursor_shape(update: &CursorUpdate) -> Option<(CursorShape, u32)> {
    let _ = update;
    todo!("M1-6")
}

/// The profile `DRIFT_AUTOCONNECT` names: an exact name match, else a unique case-insensitive
/// match (surrounding whitespace ignored).
pub fn find_autoconnect<'a>(profiles: &'a [ConnectionProfile], name: &str) -> Option<&'a ConnectionProfile> {
    let _ = (profiles, name);
    todo!("M1-6")
}
