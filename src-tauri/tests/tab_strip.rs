//! UI-tabs Red: the tab strip model (docs/design/mockup-glass.html boards 0–5).
//!
//! Every window is one tab of two kinds: a **Connection Manager** ("Connections", neutral icon)
//! or a **Session** (mode glyph, profile name, status dot). The strip is drawn in HTML from this
//! pure model; Rust pushes it to every window's strip webview.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test fixtures

use std::time::Duration;

use drift_app::strip::{
    CONNECTIONS_TITLE, STRIP_HEIGHT, TabItem, TabKind, TabStatus, TabStrip, strip_label, tab_item,
};
use drift_app::view::{Screen, SessionView};
use drift_core::{
    CertFingerprint, ConnectMode, ConnectStage, ConnectionProfile, DesktopSize, DisconnectReason,
    SessionState,
};
use drift_rdp::{CertificateRole, SessionEvent};
use uuid::Uuid;

fn view(mode: ConnectMode, name: &str, states: &[SessionState]) -> SessionView {
    let mut p = ConnectionProfile::new(name, "gnome.local", mode);
    p.linux_username = Some("hutson".into());
    let mut v = SessionView::new(&p);
    for s in states {
        v.apply(&SessionEvent::State(s.clone()));
    }
    v
}

fn connecting() -> SessionState {
    SessionState::Connecting { leg: 1, stage: ConnectStage::Tls }
}

fn connected() -> SessionState {
    SessionState::Connected { desktop: DesktopSize::new(1280, 800), scale: 100 }
}

#[test]
fn the_strip_is_one_46_point_title_bar_row() {
    assert!((STRIP_HEIGHT - 46.0).abs() < f64::EPSILON);
    assert_eq!(CONNECTIONS_TITLE, "Connections");
}

#[test]
fn a_window_without_a_session_is_a_connection_manager() {
    let id = Uuid::new_v4();
    for item in [
        tab_item("session-0", None, None),
        tab_item("session-0", Some(&view(ConnectMode::Headless, "Homelab", &[])), Some(id)),
    ] {
        assert_eq!(item.id, "session-0");
        assert_eq!(item.kind, TabKind::Manager);
        assert_eq!(item.title, "Connections");
        assert_eq!(item.status, TabStatus::Idle);
        assert_eq!(item.mode, None, "the manager has its own neutral icon");
        assert_eq!(item.profile_id, None, "a manager tab is not connected to anything");
        assert_eq!(item.hint, None);
    }
}

#[test]
fn a_connecting_tab_shows_a_spinner_and_connecting_to_name() {
    let id = Uuid::new_v4();
    let v = view(ConnectMode::RemoteLogin, "Homelab", &[connecting()]);
    let item = tab_item("session-1", Some(&v), Some(id));
    assert_eq!(item.kind, TabKind::Session);
    assert_eq!(item.title, "Connecting to Homelab…");
    assert_eq!(item.status, TabStatus::Connecting);
    assert_eq!(item.mode, Some(ConnectMode::RemoteLogin));
    assert_eq!(item.profile_id, Some(id));
}

#[test]
fn the_certificate_prompt_keeps_the_spinner_with_the_plain_name() {
    let mut v = view(ConnectMode::RemoteLogin, "Homelab", &[connecting()]);
    v.apply(&SessionEvent::CertificatePrompt {
        host: "gnome.local".into(),
        port: 3389,
        fingerprint: CertFingerprint([7; 32]),
        role: CertificateRole::Server,
    });
    assert_eq!(v.screen, Screen::Certificate);
    let item = tab_item("session-1", Some(&v), None);
    assert_eq!((item.title.as_str(), item.status), ("Homelab", TabStatus::Connecting));
}

#[test]
fn session_tabs_carry_a_status_dot() {
    #[rustfmt::skip]
    let table: [(Vec<SessionState>, TabStatus); 5] = [
        (vec![connecting(), connected()], TabStatus::Live),
        (vec![connecting(), SessionState::AwaitingGreeterLogin], TabStatus::Live),
        (vec![connecting(), connected(), SessionState::Reconnecting {
            attempt: 2, next_in: Duration::from_secs(3), reason: DisconnectReason::Network,
        }], TabStatus::Reconnecting),
        (vec![connecting(), SessionState::Failed { reason: DisconnectReason::LocalNetworkDenied }], TabStatus::Failed),
        (vec![connecting(), SessionState::Disconnected { reason: DisconnectReason::UserClosed }], TabStatus::Failed),
    ];
    for (states, want) in table {
        let v = view(ConnectMode::Headless, "Studio Workstation", &states);
        let item = tab_item("session-2", Some(&v), None);
        assert_eq!(item.kind, TabKind::Session, "{states:?}");
        assert_eq!(item.title, "Studio Workstation", "{states:?}");
        assert_eq!(item.status, want, "{states:?}");
        assert_eq!(item.mode, Some(ConnectMode::Headless));
    }
}

#[test]
fn the_greeter_hint_is_the_tabs_tooltip() {
    let v = view(ConnectMode::RemoteLogin, "Homelab", &[connecting(), SessionState::AwaitingGreeterLogin]);
    let item = tab_item("session-1", Some(&v), None);
    assert_eq!(item.hint.as_deref(), Some("Log in as “hutson” to start your session"));
    let live = view(ConnectMode::RemoteLogin, "Homelab", &[connecting(), connected()]);
    assert_eq!(tab_item("session-1", Some(&live), None).hint, None);
}

#[test]
fn blank_or_control_character_names_are_cleaned() {
    let v = view(ConnectMode::Headless, " \t", &[connecting(), connected()]);
    assert_eq!(tab_item("session-1", Some(&v), None).title, "Untitled");
    let v = view(ConnectMode::Headless, "Lab\nBox", &[connecting(), connected()]);
    assert_eq!(tab_item("session-1", Some(&v), None).title, "Lab Box");
}

#[test]
fn the_strip_keeps_the_tab_group_order_and_marks_its_own_window_active() {
    let a = Uuid::from_u128(2);
    let b = Uuid::from_u128(1);
    let tabs: Vec<TabItem> = ["session-3", "session-0", "session-5"]
        .iter()
        .map(|id| tab_item(id, None, None))
        .collect();
    let strip = TabStrip::new(tabs, "session-0", vec![a, b, a]);
    let ids: Vec<&str> = strip.tabs.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["session-3", "session-0", "session-5"], "leading to trailing, as AppKit orders them");
    assert_eq!(strip.active, "session-0");
    assert_eq!(strip.live_profiles, [b, a], "sorted and de-duplicated");
}

#[test]
fn each_window_has_its_own_strip_webview_label() {
    assert_eq!(strip_label("session-3"), "session-3-strip");
    // Covered by the `session-*` capability like its window.
    assert!(strip_label("session-3").starts_with("session-"));
}

#[test]
fn the_model_serialises_in_kebab_case_for_the_webview() {
    let v = view(ConnectMode::DesktopSharing, "Kitchen", &[connecting(), connected()]);
    let json = serde_json::to_value(tab_item("session-4", Some(&v), None)).unwrap();
    assert_eq!(json["kind"], "session");
    assert_eq!(json["status"], "live");
    assert_eq!(json["mode"], "desktop-sharing");
    let json = serde_json::to_value(tab_item("session-4", None, None)).unwrap();
    assert_eq!(json["kind"], "manager");
    assert_eq!(json["status"], "idle");
}
