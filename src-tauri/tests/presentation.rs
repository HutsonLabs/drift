//! M1-6 / M6-2 / M3-2 Red: pure presentation rules for session windows.

use std::sync::Arc;
use std::time::Duration;

use drift_app::present::{
    NEW_SESSION_TITLE, Surface, cursor_shape, find_autoconnect, surface_for, window_subtitle, window_title,
};
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

#[test]
fn webview_is_hidden_only_while_there_is_a_live_picture() {
    #[rustfmt::skip]
    let table = [
        (Screen::Profiles, Surface::Webview),
        (Screen::Connecting, Surface::Webview),
        (Screen::Certificate, Surface::Webview),
        (Screen::Reconnecting, Surface::Webview),
        (Screen::Error, Surface::Webview),
        (Screen::Live, Surface::Remote),
        // The GDM greeter is a live picture the user must type into; the hint is the subtitle.
        (Screen::GreeterHint, Surface::Remote),
    ];
    for (screen, surface) in table {
        assert_eq!(surface_for(screen), surface, "{screen:?}");
    }
}

#[test]
fn title_is_profile_name_plus_state_glyph() {
    assert_eq!(window_title(None), NEW_SESSION_TITLE);
    let idle = SessionView::new(&ConnectionProfile::new("Homelab", "h", ConnectMode::Headless));
    assert_eq!(window_title(Some(&idle)), NEW_SESSION_TITLE, "the connect form is a new session");
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

#[test]
fn subtitle_carries_the_greeter_hint() {
    assert_eq!(window_subtitle(None), "");
    let live = view_in(
        ConnectMode::RemoteLogin,
        SessionState::Connected { desktop: DesktopSize::new(1280, 800), scale: 100 },
    );
    assert_eq!(window_subtitle(Some(&live)), "");
    let greeter = view_in(ConnectMode::RemoteLogin, SessionState::AwaitingGreeterLogin);
    assert_eq!(window_subtitle(Some(&greeter)), "Log in as “drifttest” to start your session");
    let mut resuming = live.clone();
    resuming.apply(&SessionEvent::State(SessionState::AwaitingGreeterLogin));
    assert!(resuming.resuming);
    assert_eq!(
        window_subtitle(Some(&resuming)),
        "Session is still running — log in as “drifttest” to resume"
    );
    let mut anon = ConnectionProfile::new("Homelab", "h", ConnectMode::RemoteLogin);
    anon.linux_username = None;
    let mut v = SessionView::new(&anon);
    v.apply(&SessionEvent::State(SessionState::Connecting { leg: 1, stage: ConnectStage::Tcp }));
    v.apply(&SessionEvent::State(SessionState::AwaitingGreeterLogin));
    assert_eq!(window_subtitle(Some(&v)), "Log in to start your session");
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
