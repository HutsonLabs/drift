//! M2-4 Red test (loopback half): a scripted session's exact fast-path sequence, with no
//! allow-listed Command combo ever reaching the server (plan §6 M2-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::{Harness, PASS, USER, is_connected, profile, wait_log};
use drift_core::{ConnectMode, KeyboardPrefs};
use drift_input::keymap::{KeyboardType, kvk};
use drift_input::modifiers::ModifierFlags;
use drift_input::{KeyDown, Keyboard, MenuShortcut, menu_shortcut};
use drift_rdp::SessionCommand;
use drift_testkit::{Channels, FakeServer, LegScript, TestCert};
use ironrdp_pdu::input::fast_path::{FastPathInputEvent, KeyboardFlags};

const WAIT: Duration = Duration::from_secs(20);

fn key(scancode: u8, extended: bool, down: bool) -> FastPathInputEvent {
    let mut flags = KeyboardFlags::empty();
    flags.set(KeyboardFlags::EXTENDED, extended);
    flags.set(KeyboardFlags::RELEASE, !down);
    FastPathInputEvent::KeyboardEvent(flags, scancode)
}

/// Types the M2-4 script on a `Keyboard` exactly as `RemoteView` would, returning the input
/// events to send and the menu shortcuts the menu bar claimed.
fn scripted_session() -> (Vec<drift_core::InputEvent>, Vec<MenuShortcut>) {
    let cmd = ModifierFlags(ModifierFlags::COMMAND | ModifierFlags::DEVICE_LEFT_COMMAND);
    let ctrl = ModifierFlags(ModifierFlags::CONTROL | ModifierFlags::DEVICE_LEFT_CONTROL);
    let none = ModifierFlags(0);
    let mut kb = Keyboard::new(KeyboardPrefs::default(), KeyboardType::Ansi);
    let mut events = Vec::new();
    let mut claimed = Vec::new();

    // Cmd+N: allow-listed, goes to the menu (new connection); the remote never sees it.
    events.extend(kb.flags_changed(kvk::COMMAND, cmd));
    let shortcut = menu_shortcut(kvk::ANSI_N, "n", cmd).expect("Cmd+N is allow-listed");
    claimed.push(shortcut);
    kb.menu_shortcut_taken();
    events.extend(kb.flags_changed(kvk::COMMAND, none));

    // Ctrl+C: not a Command combo, goes to the remote as scancodes.
    events.extend(kb.flags_changed(kvk::CONTROL, ctrl));
    assert_eq!(menu_shortcut(kvk::ANSI_C, "c", ctrl), None);
    match kb.key_down(kvk::ANSI_C, ctrl, false, false) {
        KeyDown::Send(e) => events.extend(e),
        KeyDown::InterpretText => panic!("a Control chord is never text"),
    }
    events.extend(kb.key_up(kvk::ANSI_C));
    events.extend(kb.flags_changed(kvk::CONTROL, none));

    // Cmd+C: not allow-listed, claimed by the view and sent as Super+C.
    events.extend(kb.flags_changed(kvk::COMMAND, cmd));
    assert_eq!(menu_shortcut(kvk::ANSI_C, "c", cmd), None);
    events.extend(kb.key_down_scancode(kvk::ANSI_C, cmd));
    events.extend(kb.key_up(kvk::ANSI_C));
    events.extend(kb.flags_changed(kvk::COMMAND, none));

    // Cmd+W: allow-listed (close window).
    events.extend(kb.flags_changed(kvk::COMMAND, cmd));
    claimed.push(menu_shortcut(kvk::ANSI_W, "w", cmd).expect("Cmd+W is allow-listed"));
    kb.menu_shortcut_taken();
    events.extend(kb.flags_changed(kvk::COMMAND, none));
    (events, claimed)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scripted_session_produces_the_exact_fast_path_sequence() {
    let (events, claimed) = scripted_session();
    assert_eq!(claimed, vec![MenuShortcut::NewConnection, MenuShortcut::CloseWindow]);

    let cert = TestCert::generate("127.0.0.1");
    let server =
        FakeServer::start(vec![LegScript::nla(cert.clone(), USER, PASS).with_channels(Channels::all())])
            .await
            .unwrap();
    let mut h = Harness::start(profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())), PASS);
    h.wait_for("Connected", WAIT, is_connected).await;
    for event in events {
        h.handle.send(SessionCommand::Input(event)).unwrap();
    }

    let expected = vec![
        key(0x1D, false, true),  // Control down
        key(0x2E, false, true),  // C down
        key(0x2E, false, false), // C up
        key(0x1D, false, false), // Control up
        key(0x5B, true, true),   // deferred Command flushed as left Super
        key(0x2E, false, true),
        key(0x2E, false, false),
        key(0x5B, true, false),
    ];
    let log =
        wait_log(&server, "the full sequence", WAIT, |l| l.legs[0].fast_path_events.len() >= expected.len())
            .await;
    assert_eq!(log.legs[0].fast_path_events, expected);
    assert!(
        !log.legs[0]
            .fast_path_events
            .iter()
            .any(|e| matches!(e, FastPathInputEvent::KeyboardEvent(_, 0x14 | 0x11))),
        "allow-listed Cmd+T / Cmd+W never reach the server"
    );
    h.close().await;
}
