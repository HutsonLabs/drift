//! Pure presentation rules for a session window (tasks **M1-6**, **M6-2**, **M3-2**).
//!
//! The humble window glue (`crate::windows`) applies these decisions to AppKit/Tauri:
//! which surface is in front (webview or RemoteView), the tab title and subtitle, and the
//! remote pointer shape.

use std::sync::Arc;

use drift_core::{ConnectionProfile, SessionState, Size};
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
    /// The webview fills the window and there is nothing to see behind it (form, certificate
    /// prompt, progress, error). The RemoteView is hidden, so the page's glass panels sit on
    /// the window's native vibrancy.
    Webview,
    /// The RemoteView with the live picture; the webview is hidden and the view is first
    /// responder.
    Remote,
    /// The webview fills the window but paints only a dimming wash and a card, so the last
    /// frame stays visible underneath (M7-3). It takes keyboard focus: the only thing the user
    /// can do is answer the overlay.
    Overlay,
    /// A small panel of the webview floats over a **live** picture. The webview is shrunk to
    /// [`hud_frame`] so every event outside that rectangle still reaches the RemoteView, which
    /// keeps first responder.
    Hud(Hud),
}

/// Which panel a [`Surface::Hud`] draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hud {
    /// The greeter-wait hint, centred under the title bar (M3-2, M7-3).
    Banner,
    /// The statistics readouts in the bottom-right corner (M1 "Done (manual M1)", M9-1).
    Stats,
}

/// Which HUD (if any) belongs over the picture of `view`.
pub fn hud_for(view: &SessionView) -> Option<Hud> {
    match view.screen {
        Screen::GreeterHint => Some(Hud::Banner),
        Screen::Live if view.show_stats && view.stats.is_some() => Some(Hud::Stats),
        _ => None,
    }
}

/// The surface for a view.
///
/// The webview is shown whenever there is no live picture (plan §2 decision 6). Where the plan
/// asks for something drawn *over* the picture — the reconnect overlay's dimmed last frame, the
/// greeter hint, the statistics line — the webview stays in front of the RemoteView and is
/// transparent instead (see `docs/adr/M7-3-overlays-over-the-live-picture.md`).
pub fn surface_for(view: &SessionView) -> Surface {
    if let Some(hud) = hud_for(view) {
        return Surface::Hud(hud);
    }
    match view.screen {
        Screen::Live | Screen::GreeterHint => Surface::Remote,
        Screen::Reconnecting => Surface::Overlay,
        Screen::Profiles | Screen::Connecting | Screen::Certificate | Screen::Error => Surface::Webview,
    }
}

/// Edges a [`HudFrame`] may move away from when the window resizes (the other two pin it to the
/// window edge). These are screen directions; `crate::windows` maps them to an AppKit
/// autoresizing mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flexible {
    /// The space on the left may grow.
    pub left: bool,
    /// The space on the right may grow.
    pub right: bool,
    /// The space above may grow.
    pub top: bool,
    /// The space below may grow.
    pub bottom: bool,
}

/// Where a HUD panel sits inside the web view's superview, in that view's own coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HudFrame {
    /// Distance from the left edge.
    pub x: f64,
    /// Distance from the coordinate-space origin (top edge when flipped, bottom edge otherwise).
    pub y: f64,
    /// Width in points.
    pub width: f64,
    /// Height in points.
    pub height: f64,
    /// How the panel follows a window resize.
    pub flexible: Flexible,
}

/// Margin between a HUD panel and the window edges, in points.
const HUD_INSET: f64 = 16.0;
/// Widest the greeter banner gets.
const BANNER_WIDTH: f64 = 560.0;
/// Height of the greeter banner (two lines plus its top margin).
const BANNER_HEIGHT: f64 = 76.0;
/// Width of the statistics panel (four labelled readouts).
const STATS_WIDTH: f64 = 300.0;
/// Height of the statistics panel (value over unit).
const STATS_HEIGHT: f64 = 56.0;

/// The rectangle a HUD panel occupies over a `parent`-sized picture.
///
/// `flipped` is the superview's `isFlipped`: AppKit's default coordinate space has its origin in
/// the bottom-left, so the same panel is mirrored vertically there. The panel is always fully
/// inside `parent` and never covers more than a corner of it, so the user keeps clicking and
/// typing into the remote desktop everywhere else.
pub fn hud_frame(parent: Size<f64>, hud: Hud, flipped: bool) -> HudFrame {
    let (width, height, flexible) = match hud {
        Hud::Banner => (
            BANNER_WIDTH.min(parent.width - 2.0 * HUD_INSET).max(1.0),
            BANNER_HEIGHT.min(parent.height).max(1.0),
            Flexible { left: true, right: true, top: false, bottom: true },
        ),
        Hud::Stats => (
            STATS_WIDTH.min(parent.width).max(1.0),
            STATS_HEIGHT.min(parent.height).max(1.0),
            Flexible { left: true, right: false, top: true, bottom: false },
        ),
    };
    let (x, top) = match hud {
        Hud::Banner => ((parent.width - width) / 2.0, 0.0),
        Hud::Stats => (parent.width - width - HUD_INSET, parent.height - height - HUD_INSET),
    };
    let x = x.clamp(0.0, (parent.width - width).max(0.0));
    let top = top.clamp(0.0, (parent.height - height).max(0.0));
    let y = if flipped { top } else { parent.height - top - height };
    HudFrame { x, y, width, height, flexible }
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
/// GNOME login screen is live, otherwise empty. The same text is in the [`Hud::Banner`] over the
/// picture; the subtitle also survives the user scrolling the banner out of mind.
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

/// What VoiceOver announces for the live picture (task M9-4).
///
/// The `RemoteView` has an image role and this label; naming the connection and the desktop
/// size is the only way a VoiceOver user can tell two session tabs apart, because the picture
/// itself is pixels from another computer.
pub fn accessibility_label(view: Option<&SessionView>) -> String {
    let Some(view) = view else { return drift_macos::view::DEFAULT_ACCESSIBILITY_LABEL.to_owned() };
    match view.state {
        SessionState::Connected { desktop, .. } => {
            format!("{} — remote desktop, {} by {} pixels", view.profile_name, desktop.width, desktop.height)
        }
        _ => format!("{} — remote desktop", view.profile_name),
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
