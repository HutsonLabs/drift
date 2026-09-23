//! M1-6 / M6-2 / M3-2 Red: pure presentation rules for session windows.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test fixtures

use std::sync::Arc;
use std::time::Duration;

use drift_app::present::{
    Focus, Hud, Surface, accessibility_label, band_y, chrome, cursor_shape, find_autoconnect, focus_for,
    greeter_hint, hud_frame, surface_for, window_title,
};
use drift_app::strip::{CONNECTIONS_TITLE, STRIP_HEIGHT};
use drift_app::view::{Screen, SessionView};
use drift_core::{
    ConnectMode, ConnectStage, ConnectionProfile, DesktopSize, DisconnectReason, Point, SessionState, Size,
};
use drift_macos::CursorShape;
use drift_rdp::{CursorBitmap, CursorUpdate, SessionEvent};

fn view_in(mode: ConnectMode, state: SessionState) -> SessionView {
    let mut p = ConnectionProfile::new("Homelab", "10.1.2.40", mode);
    p.linux_username = Some("drifttest".into());
    let mut v = SessionView::new(&p);
    v.apply(&SessionEvent::State(state));
    v
}

/// A view forced onto `screen` (the fields the surface rules read are set directly).
fn on_screen(screen: Screen) -> SessionView {
    let mut v = SessionView::new(&ConnectionProfile::new("Homelab", "h", ConnectMode::RemoteLogin));
    v.screen = screen;
    v
}

#[test]
fn webview_is_hidden_only_while_there_is_a_live_picture() {
    #[rustfmt::skip]
    let table = [
        (Screen::Profiles, Surface::Webview),
        (Screen::Connecting, Surface::Webview),
        (Screen::Certificate, Surface::Webview),
        (Screen::Error, Surface::Webview),
        (Screen::Live, Surface::Remote),
        // M7-3: the last frame stays on screen, dimmed under a transparent full-window overlay.
        (Screen::Reconnecting, Surface::Overlay),
        // M3-2/M7-3: a non-modal banner over the greeter picture; the RemoteView keeps focus.
        (Screen::GreeterHint, Surface::Hud(Hud::Banner)),
    ];
    for (screen, surface) in table {
        assert_eq!(surface_for(&on_screen(screen)), surface, "{screen:?}");
    }
}

#[test]
fn the_statistics_hud_floats_over_the_live_picture() {
    // M1 "Done (manual M1)": `anim.py` shows >= 55 fps in the stats overlay.
    let mut live = on_screen(Screen::Live);
    live.stats = Some(Default::default());
    assert_eq!(surface_for(&live), Surface::Remote, "off by default");
    live.show_stats = true;
    assert_eq!(surface_for(&live), Surface::Hud(Hud::Stats));
    live.stats = None;
    assert_eq!(surface_for(&live), Surface::Remote, "nothing to draw before the first sample");
    live.stats = Some(Default::default());
    // The HUD is only a HUD while a picture is live; elsewhere the full webview wins.
    let mut form = on_screen(Screen::Profiles);
    form.show_stats = true;
    assert_eq!(surface_for(&form), Surface::Webview);
    let mut retry = on_screen(Screen::Reconnecting);
    retry.show_stats = true;
    assert_eq!(surface_for(&retry), Surface::Overlay);
}

#[test]
fn a_hud_never_covers_the_whole_picture() {
    let parent = Size::new(1280.0_f64, 800.0);
    for hud in [Hud::Banner, Hud::Stats] {
        let f = hud_frame(parent, hud, false);
        assert!(f.x >= 0.0 && f.y >= 0.0, "{hud:?} {f:?}");
        assert!(f.x + f.width <= parent.width && f.y + f.height <= parent.height, "{hud:?} {f:?}");
        let covered = f.width * f.height / (parent.width * parent.height);
        assert!(covered < 0.1, "{hud:?} covers {covered:.3} of the picture");
    }
    // The banner sits at the top centre, the statistics panel in the bottom-right corner.
    let banner = hud_frame(parent, Hud::Banner, true);
    assert_eq!(banner.y, 0.0, "banner at the top of a flipped superview");
    assert!((banner.x - (parent.width - banner.width) / 2.0).abs() < 0.5, "centred: {banner:?}");
    assert!(banner.flexible.left && banner.flexible.right && banner.flexible.bottom);
    assert!(!banner.flexible.top, "the banner stays glued to the top edge");
    let stats = hud_frame(parent, Hud::Stats, true);
    assert!(stats.x + stats.width < parent.width, "inset from the right");
    assert!(stats.y > parent.height / 2.0, "bottom half: {stats:?}");
    assert!(stats.flexible.left && stats.flexible.top);
    assert!(!stats.flexible.right && !stats.flexible.bottom, "glued to the bottom-right corner");
}

#[test]
fn hud_geometry_follows_the_superviews_flippedness() {
    let parent = Size::new(1280.0_f64, 800.0);
    for hud in [Hud::Banner, Hud::Stats] {
        let flipped = hud_frame(parent, hud, true);
        let appkit = hud_frame(parent, hud, false);
        assert_eq!((flipped.width, flipped.height), (appkit.width, appkit.height), "{hud:?}");
        assert_eq!(flipped.x, appkit.x, "{hud:?}");
        // AppKit's origin is bottom-left: the same panel is mirrored vertically.
        assert!(
            (appkit.y - (parent.height - flipped.y - flipped.height)).abs() < f64::EPSILON,
            "{hud:?}: flipped {flipped:?} vs appkit {appkit:?}"
        );
    }
}

#[test]
fn a_hud_shrinks_with_a_narrow_window() {
    let narrow = Size::new(320.0_f64, 240.0);
    for hud in [Hud::Banner, Hud::Stats] {
        let f = hud_frame(narrow, hud, false);
        assert!(f.x >= 0.0 && f.y >= 0.0 && f.width > 0.0 && f.height > 0.0, "{hud:?} {f:?}");
        assert!(f.x + f.width <= narrow.width, "{hud:?} {f:?}");
        assert!(f.y + f.height <= narrow.height, "{hud:?} {f:?}");
    }
}

#[test]
fn title_is_profile_name_plus_state_glyph() {
    // UI-tabs: a window on the connect form is a Connection Manager tab, titled "Connections"
    // (the title is what AppKit lists in the Window menu).
    assert_eq!(window_title(None), CONNECTIONS_TITLE);
    let idle = SessionView::new(&ConnectionProfile::new("Homelab", "h", ConnectMode::Headless));
    assert_eq!(window_title(Some(&idle)), CONNECTIONS_TITLE, "the connect form is the Connection Manager");
    #[rustfmt::skip]
    let table = [
        (SessionState::Connecting { leg: 2, stage: ConnectStage::Tls }, "◌ Homelab"),
        (SessionState::AwaitingGreeterLogin, "◐ Homelab"),
        (SessionState::Connected { desktop: DesktopSize::new(1280, 800), scale: 100 }, "● Homelab"),
        (SessionState::Reconnecting { attempt: 1, next_in: Duration::from_secs(1), reason: DisconnectReason::Network }, "↻ Homelab"),
        (SessionState::Disconnected { reason: DisconnectReason::UserClosed }, "○ Homelab"),
        (SessionState::Failed { reason: DisconnectReason::AuthFailed }, "⚠ Homelab"),
    ];
    for (state, want) in table {
        let v = view_in(ConnectMode::Headless, state.clone());
        assert_eq!(window_title(Some(&v)), want, "{state:?}");
    }
}

/// UI-tabs: the title bar is hidden, so the greeter hint no longer lives in the window
/// subtitle; the same text is the floating banner's and the session tab's tooltip.
#[test]
fn the_greeter_hint_names_the_linux_user() {
    assert_eq!(greeter_hint(None), None);
    let live = view_in(
        ConnectMode::RemoteLogin,
        SessionState::Connected { desktop: DesktopSize::new(1280, 800), scale: 100 },
    );
    assert_eq!(greeter_hint(Some(&live)), None);
    let greeter = view_in(ConnectMode::RemoteLogin, SessionState::AwaitingGreeterLogin);
    assert_eq!(greeter_hint(Some(&greeter)).as_deref(), Some("Log in as “drifttest” to start your session"));
    let mut resuming = live.clone();
    resuming.apply(&SessionEvent::State(SessionState::AwaitingGreeterLogin));
    assert!(resuming.resuming);
    assert_eq!(
        greeter_hint(Some(&resuming)).as_deref(),
        Some("Session is still running — log in as “drifttest” to resume")
    );
    let mut anon = ConnectionProfile::new("Homelab", "h", ConnectMode::RemoteLogin);
    anon.linux_username = None;
    let mut v = SessionView::new(&anon);
    v.apply(&SessionEvent::State(SessionState::Connecting { leg: 1, stage: ConnectStage::Tcp }));
    v.apply(&SessionEvent::State(SessionState::AwaitingGreeterLogin));
    assert_eq!(greeter_hint(Some(&v)).as_deref(), Some("Log in to start your session"));
}

/// M9-4: VoiceOver names the picture after the connection and the desktop it shows.
#[test]
fn the_picture_is_labelled_for_voiceover() {
    assert_eq!(accessibility_label(None), "Remote desktop");
    let live = view_in(
        ConnectMode::Headless,
        SessionState::Connected { desktop: DesktopSize::new(2560, 1600), scale: 200 },
    );
    assert_eq!(accessibility_label(Some(&live)), "Homelab — remote desktop, 2560 by 1600 pixels");
    let greeter = view_in(ConnectMode::RemoteLogin, SessionState::AwaitingGreeterLogin);
    assert_eq!(accessibility_label(Some(&greeter)), "Homelab — remote desktop");
}

#[test]
fn cursor_updates_map_to_shapes() {
    assert_eq!(cursor_shape(&CursorUpdate::Hidden), Some((CursorShape::Hidden, 100)));
    assert_eq!(cursor_shape(&CursorUpdate::Default), Some((CursorShape::Default, 100)));
    assert_eq!(cursor_shape(&CursorUpdate::Position(Point::new(3, 4))), None);
    let bgra: Arc<[u8]> = vec![1u8; 86 * 86 * 4].into();
    let b =
        CursorBitmap { size: Size::new(86, 86), hotspot: Point::new(5, 6), bgra: bgra.clone(), scale: 200 };
    let Some((CursorShape::Image(img), scale)) = cursor_shape(&CursorUpdate::Bitmap(b)) else {
        panic!("bitmap → image");
    };
    assert_eq!(scale, 200);
    assert_eq!(img.size, Size::new(86, 86));
    assert_eq!(img.hotspot, Point::new(5, 6));
    assert_eq!(img.bgra, bgra);
    assert_eq!(img.size_points(scale), Size::new(43.0, 43.0));
}

#[test]
fn autoconnect_finds_the_named_profile() {
    let a = ConnectionProfile::new("Homelab", "h", ConnectMode::Headless);
    let b = ConnectionProfile::new("homelab login", "h", ConnectMode::RemoteLogin);
    let c = ConnectionProfile::new("HOMELAB LOGIN", "h", ConnectMode::RemoteLogin);
    let all = [a.clone(), b.clone(), c.clone()];
    assert_eq!(find_autoconnect(&all, "Homelab").map(|p| p.id), Some(a.id));
    assert_eq!(find_autoconnect(&all, "homelab").map(|p| p.id), Some(a.id), "unique case-insensitive");
    assert_eq!(find_autoconnect(&all, "HOMELAB LOGIN").map(|p| p.id), Some(c.id), "exact wins");
    assert_eq!(find_autoconnect(&all, "Homelab Login"), None, "ambiguous");
    assert_eq!(find_autoconnect(&all, "  Homelab ").map(|p| p.id), Some(a.id), "trimmed");
    assert_eq!(find_autoconnect(&all, "nope"), None);
    assert_eq!(find_autoconnect(&all, ""), None);
}

/// UI-tabs: the HTML tab strip is the title bar row; the picture and the page start below it.
#[test]
fn the_tab_strip_sits_above_the_picture_and_the_page() {
    let c = chrome(false);
    assert!((c.strip - STRIP_HEIGHT).abs() < f64::EPSILON, "{c:?}");
    assert!((c.content_top - STRIP_HEIGHT).abs() < f64::EPSILON, "{c:?}");
}

/// UI-tabs board 4: in full screen the strip is hidden and the remote desktop fills the screen.
#[test]
fn full_screen_hides_the_strip_and_the_picture_fills_the_screen() {
    let c = chrome(true);
    assert_eq!((c.strip, c.content_top), (0.0, 0.0));
}

#[test]
fn bands_are_measured_from_the_top_edge_in_either_coordinate_space() {
    // A 46-point strip at the top of an 800-point view.
    assert!((band_y(800.0, 0.0, 46.0, true) - 0.0).abs() < f64::EPSILON);
    assert!((band_y(800.0, 0.0, 46.0, false) - 754.0).abs() < f64::EPSILON);
    // The content below it.
    assert!((band_y(800.0, 46.0, 754.0, true) - 46.0).abs() < f64::EPSILON);
    assert!((band_y(800.0, 46.0, 754.0, false) - 0.0).abs() < f64::EPSILON);
}

/// Clicking the strip must not leave the keyboard in the strip: focus goes back to whatever
/// the tab's surface says owns it.
#[test]
fn keyboard_focus_follows_the_surface() {
    assert_eq!(focus_for(Surface::Remote), Focus::Remote);
    assert_eq!(focus_for(Surface::Hud(Hud::Banner)), Focus::Remote);
    assert_eq!(focus_for(Surface::Hud(Hud::Stats)), Focus::Remote);
    assert_eq!(focus_for(Surface::Webview), Focus::Page);
    assert_eq!(focus_for(Surface::Overlay), Focus::Page);
}
