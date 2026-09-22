//! M7-3 Red tests: mode-specific resume after a drop (plan §6 M7-3).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{Harness, PASS, USER, is_connected, options, profile, wait_log};
use drift_core::{ConnectMode, DesktopSize, DisconnectReason, InputEvent, MouseButton, SessionState};
use drift_rdp::greeter::TYPE_DELAY;
use drift_rdp::{SessionCommand, SessionEvent, SessionSecrets};
use drift_testkit::{
    Channels, FakeServer, LegScript, ManualClock, ServerAction, TestCert, redirection_pdu,
};
use ironrdp_pdu::input::fast_path::{FastPathInputEvent, KeyboardFlags, SynchronizeFlags};
use zeroize::Zeroizing;

const WAIT: Duration = Duration::from_secs(20);
const DESK: DesktopSize = DesktopSize { width: 1280, height: 800 };
/// A fake Linux password; never a real one.
const LINUX_PASS: &str = "Fake9-linux";

fn dropped_leg(cert: &TestCert) -> LegScript {
    LegScript::nla(cert.clone(), USER, PASS)
        .with_channels(Channels::all())
        .then(ServerAction::Wait(Duration::from_millis(200)))
        .then(ServerAction::Close)
}

/// The retryable reason a dropped TLS transport produces.
fn dropped_reason(state: &SessionState) -> DisconnectReason {
    match state {
        SessionState::Reconnecting { reason, .. } => reason.clone(),
        other => panic!("expected Reconnecting, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_killed_socket_reconnects_with_backoff_and_resyncs_input() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![
        dropped_leg(&cert),
        LegScript::nla(cert.clone(), USER, PASS).with_channels(Channels::all()),
    ])
    .await
    .unwrap();
    let clock = ManualClock::new();
    let mut h = Harness::start_full(
        profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())),
        SessionSecrets::new(PASS),
        Arc::new(clock.clone()),
        options(),
    );
    h.wait_for("Connected", WAIT, is_connected).await;
    // A key the user is holding when the transport dies.
    h.handle.send(SessionCommand::Input(InputEvent::Key { scancode: 0x1D, extended: false, down: true })).unwrap();
    h.handle.send(SessionCommand::Input(InputEvent::SyncToggles { caps: true, num: true })).unwrap();

    let reconnecting = h
        .wait_for("Reconnecting", WAIT, |e| matches!(e, SessionEvent::State(SessionState::Reconnecting { .. })))
        .await;
    let SessionEvent::State(state) = &reconnecting else { unreachable!() };
    let reason = dropped_reason(state);
    assert!(reason.is_retryable(), "{reason:?}");
    assert_eq!(
        *state,
        SessionState::Reconnecting { attempt: 1, next_in: match state {
            SessionState::Reconnecting { next_in, .. } => *next_in,
            _ => unreachable!(),
        }, reason: reason.clone() }
    );
    assert!(
        matches!(state, SessionState::Reconnecting { next_in, .. } if *next_in <= Duration::from_millis(500)),
        "attempt 1 waits at most the 500 ms base (full jitter): {state:?}"
    );
    // No attempt is made until the backoff elapses on the injected clock.
    let _ = h.drain_for(Duration::from_millis(300)).await;
    assert_eq!(server.log().legs.len(), 1, "no attempt before the backoff elapses");

    clock.advance(Duration::from_secs(1));
    h.wait_for("reconnected", WAIT, is_connected).await;
    let states = h.states();
    let after: Vec<SessionState> = states
        .iter()
        .skip_while(|s| !matches!(s, SessionState::Reconnecting { .. }))
        .cloned()
        .collect();
    assert!(
        matches!(after.as_slice(), [SessionState::Reconnecting { .. }, rest @ ..]
            if rest.iter().any(|s| matches!(s, SessionState::Connecting { leg: 1, .. }))
                && rest.last() == Some(&SessionState::Connected { desktop: DESK, scale: 100 })),
        "{states:?}"
    );

    // Plan M7-3: after any reconnect the client releases keys and re-syncs the lock keys.
    let log = wait_log(&server, "resync input", WAIT, |l| l.legs.len() > 1 && !l.legs[1].fast_path_events.is_empty())
        .await;
    let events = &log.legs[1].fast_path_events;
    assert_eq!(
        events.first(),
        Some(&FastPathInputEvent::KeyboardEvent(KeyboardFlags::RELEASE, 0x1D)),
        "ReleaseAll first: {events:?}"
    );
    assert!(
        events.contains(&FastPathInputEvent::SyncEvent(
            SynchronizeFlags::CAPS_LOCK | SynchronizeFlags::NUM_LOCK
        )),
        "SyncToggles after the reconnect: {events:?}"
    );
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_auth_failure_during_reconnect_fails_without_looping() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![
        dropped_leg(&cert),
        LegScript::nla(cert.clone(), USER, "Fake9-rotated-password"),
        LegScript::nla(cert.clone(), USER, PASS),
    ])
    .await
    .unwrap();
    let clock = ManualClock::new();
    let mut h = Harness::start_full(
        profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())),
        SessionSecrets::new(PASS),
        Arc::new(clock.clone()),
        options(),
    );
    h.wait_for("Connected", WAIT, is_connected).await;
    h.wait_for("Reconnecting", WAIT, |e| matches!(e, SessionEvent::State(SessionState::Reconnecting { .. })))
        .await;
    clock.advance(Duration::from_secs(1));
    assert_eq!(h.wait_terminal(WAIT).await, SessionState::Failed { reason: DisconnectReason::AuthFailed });
    clock.advance(Duration::from_secs(60));
    let _ = h.drain_for(Duration::from_millis(400)).await;
    assert_eq!(server.log().legs.len(), 2, "no further attempts after a non-retryable failure");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_stops_the_reconnect_timers() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![dropped_leg(&cert), LegScript::nla(cert.clone(), USER, PASS)])
        .await
        .unwrap();
    let clock = ManualClock::new();
    let mut h = Harness::start_full(
        profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())),
        SessionSecrets::new(PASS),
        Arc::new(clock.clone()),
        options(),
    );
    h.wait_for("Connected", WAIT, is_connected).await;
    let reconnecting = h
        .wait_for("Reconnecting", WAIT, |e| matches!(e, SessionEvent::State(SessionState::Reconnecting { .. })))
        .await;
    let SessionEvent::State(state) = &reconnecting else { unreachable!() };
    let reason = dropped_reason(state);

    h.handle.send(SessionCommand::Cancel).unwrap();
    assert_eq!(h.wait_terminal(WAIT).await, SessionState::Disconnected { reason });
    clock.advance(Duration::from_secs(120));
    let _ = h.drain_for(Duration::from_millis(400)).await;
    assert_eq!(server.log().legs.len(), 1, "cancel stopped the timers");

    // The user can still reconnect by hand.
    h.handle.send(SessionCommand::ReconnectNow).unwrap();
    h.wait_for("reconnected", WAIT, is_connected).await;
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unreachable_network_pauses_and_resumes_reconnection() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![dropped_leg(&cert), LegScript::nla(cert.clone(), USER, PASS)])
        .await
        .unwrap();
    let clock = ManualClock::new();
    let mut h = Harness::start_full(
        profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())),
        SessionSecrets::new(PASS),
        Arc::new(clock.clone()),
        options(),
    );
    h.wait_for("Connected", WAIT, is_connected).await;
    h.handle.send(SessionCommand::NetworkReachable(false)).unwrap();
    h.wait_for("Reconnecting", WAIT, |e| matches!(e, SessionEvent::State(SessionState::Reconnecting { .. })))
        .await;
    clock.advance(Duration::from_secs(120));
    let _ = h.drain_for(Duration::from_millis(400)).await;
    assert_eq!(server.log().legs.len(), 1, "no attempt while the network is down");

    h.handle.send(SessionCommand::NetworkReachable(true)).unwrap();
    h.wait_for("reconnected", WAIT, is_connected).await;
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_login_reconnects_to_the_greeter_and_types_the_stored_password() {
    let cert = TestCert::generate("127.0.0.1");
    let redirect = |seed: u8| {
        [
            ServerAction::Wait(Duration::from_millis(50)),
            ServerAction::Redirect(Box::new(redirection_pdu(4242, cert.der(), seed))),
        ]
    };
    let server = FakeServer::start(vec![
        // First connection: leg 1 (NLA) → greeter → user session, then the transport dies.
        LegScript { actions: redirect(1).into(), ..LegScript::nla(cert.clone(), USER, PASS) },
        LegScript { actions: redirect(2).into(), ..LegScript::rdstls(cert.clone(), 0) },
        LegScript::rdstls(cert.clone(), 0)
            .with_channels(Channels::all())
            .then(ServerAction::Wait(Duration::from_millis(200)))
            .then(ServerAction::Close),
        // Reconnect: the chain stops at the greeter (no second redirect).
        LegScript { actions: redirect(3).into(), ..LegScript::nla(cert.clone(), USER, PASS) },
        LegScript::rdstls(cert.clone(), 0).with_channels(Channels::all()),
    ])
    .await
    .unwrap();
    let clock = ManualClock::new();
    let mut secrets = SessionSecrets::new(PASS);
    secrets.linux_password = Some(Zeroizing::new(LINUX_PASS.into()));
    let mut h = Harness::start_full(
        profile(ConnectMode::RemoteLogin, server.port(), Some(cert.fingerprint())),
        secrets,
        Arc::new(clock.clone()),
        options(),
    );
    h.wait_for("Connected", WAIT, is_connected).await;
    h.wait_for("Reconnecting", WAIT, |e| matches!(e, SessionEvent::State(SessionState::Reconnecting { .. })))
        .await;
    clock.advance(Duration::from_secs(1));
    h.wait_for("greeter after reconnect", WAIT, |e| {
        *e == SessionEvent::State(SessionState::AwaitingGreeterLogin)
    })
    .await;
    let _ = h.drain_for(Duration::from_millis(200)).await;
    assert_eq!(server.log().legs.len(), 5, "Remote Login always passes through the greeter (plan §8)");

    // Nothing is typed before the user picks their tile.
    clock.advance(TYPE_DELAY * 4);
    let _ = h.drain_for(Duration::from_millis(200)).await;
    assert!(server.log().legs[4].fast_path_events.iter().all(|e| !matches!(
        e,
        FastPathInputEvent::UnicodeKeyboardEvent(..)
    )));

    let clicks_before = server.log().legs[4].fast_path_events.len();
    for ev in [
        InputEvent::MouseButton { button: MouseButton::Left, down: true, x: 640, y: 427 },
        InputEvent::MouseButton { button: MouseButton::Left, down: false, x: 640, y: 427 },
    ] {
        h.handle.send(SessionCommand::Input(ev)).unwrap();
    }
    // The typist arms on the click, so the clock may only move once the click has landed.
    wait_log(&server, "the click", WAIT, |l| l.legs[4].fast_path_events.len() > clicks_before + 1).await;
    clock.advance(TYPE_DELAY);
    let log = wait_log(&server, "typed password", WAIT, |l| {
        l.legs[4].fast_path_events.iter().any(|e| matches!(e, FastPathInputEvent::UnicodeKeyboardEvent(..)))
    })
    .await;
    let typed: Vec<u16> = log.legs[4]
        .fast_path_events
        .iter()
        .filter_map(|e| match e {
            FastPathInputEvent::UnicodeKeyboardEvent(flags, ch) if !flags.contains(KeyboardFlags::RELEASE) => {
                Some(*ch)
            }
            _ => None,
        })
        .collect();
    assert_eq!(typed, LINUX_PASS.encode_utf16().collect::<Vec<_>>(), "the stored password, as Unicode events");
    assert!(
        log.legs[4]
            .fast_path_events
            .contains(&FastPathInputEvent::KeyboardEvent(KeyboardFlags::empty(), 0x1C)),
        "followed by Enter"
    );
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_the_opt_in_nothing_is_typed_at_the_greeter() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![
        LegScript {
            actions: vec![
                ServerAction::Wait(Duration::from_millis(50)),
                ServerAction::Redirect(Box::new(redirection_pdu(1, cert.der(), 1))),
            ],
            ..LegScript::nla(cert.clone(), USER, PASS)
        },
        LegScript::rdstls(cert.clone(), 0).with_channels(Channels::all()),
    ])
    .await
    .unwrap();
    let clock = ManualClock::new();
    let mut h = Harness::start_full(
        profile(ConnectMode::RemoteLogin, server.port(), Some(cert.fingerprint())),
        SessionSecrets::new(PASS),
        Arc::new(clock.clone()),
        options(),
    );
    h.wait_for("greeter", WAIT, |e| *e == SessionEvent::State(SessionState::AwaitingGreeterLogin)).await;
    for ev in [
        InputEvent::MouseButton { button: MouseButton::Left, down: true, x: 640, y: 427 },
        InputEvent::MouseButton { button: MouseButton::Left, down: false, x: 640, y: 427 },
    ] {
        h.handle.send(SessionCommand::Input(ev)).unwrap();
    }
    clock.advance(TYPE_DELAY * 4);
    let _ = h.drain_for(Duration::from_millis(400)).await;
    assert!(
        server.log().legs[1]
            .fast_path_events
            .iter()
            .all(|e| !matches!(e, FastPathInputEvent::UnicodeKeyboardEvent(..))),
        "no stored password, nothing typed"
    );
    h.close().await;
}
