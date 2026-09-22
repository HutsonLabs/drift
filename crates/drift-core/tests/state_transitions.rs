//! M0-5 Red: `SessionState` transition table (invalid transitions return `Err`).

use std::time::Duration;

use drift_core::state::MAX_LEG;
use drift_core::{ConnectStage, DisconnectReason, SessionState, Size};

use ConnectStage::*;
use SessionState::*;

fn conn(leg: u8, stage: ConnectStage) -> SessionState {
    Connecting { leg, stage }
}
fn connected(w: u32, h: u32) -> SessionState {
    Connected { desktop: Size::new(w, h), scale: 100 }
}
fn reconnecting(attempt: u32) -> SessionState {
    Reconnecting { attempt, next_in: Duration::from_millis(500), reason: DisconnectReason::Network }
}
fn disconnected(r: DisconnectReason) -> SessionState {
    Disconnected { reason: r }
}
fn failed(r: DisconnectReason) -> SessionState {
    Failed { reason: r }
}

#[test]
fn valid_transitions_are_accepted() {
    let valid: Vec<(SessionState, SessionState)> = vec![
        // start
        (Idle, conn(1, Tcp)),
        (Idle, disconnected(DisconnectReason::UserClosed)),
        // leg progress
        (conn(1, Tcp), conn(1, Tls)),
        (conn(1, Tls), conn(1, Nla)),
        (conn(1, Nla), conn(1, Activation)),
        (conn(1, Activation), connected(1280, 800)),
        // Remote Login: redirect to leg 2 (RDSTLS), greeter, leg 3, desktop
        (conn(1, Activation), conn(2, Tcp)),
        (conn(2, Tcp), conn(2, Rdstls)),
        (conn(2, Activation), AwaitingGreeterLogin),
        (AwaitingGreeterLogin, conn(3, Tcp)),
        (conn(3, Activation), connected(1280, 800)),
        // resize, and redirect while connected
        (connected(1280, 800), connected(2560, 1600)),
        (connected(1280, 800), conn(2, Tcp)),
        // drops
        (conn(1, Tls), reconnecting(1)),
        (AwaitingGreeterLogin, reconnecting(1)),
        (connected(1280, 800), reconnecting(1)),
        (connected(1280, 800), disconnected(DisconnectReason::UserClosed)),
        (conn(1, Nla), failed(DisconnectReason::AuthFailed)),
        (conn(2, Rdstls), failed(DisconnectReason::RdstlsFailed(0x52E))),
        (AwaitingGreeterLogin, disconnected(DisconnectReason::UserClosed)),
        // backoff
        (reconnecting(1), reconnecting(1)),
        (reconnecting(1), reconnecting(2)),
        (reconnecting(3), conn(1, Tcp)),
        (reconnecting(3), disconnected(DisconnectReason::UserClosed)),
        (reconnecting(20), failed(DisconnectReason::Network)),
        // manual retry
        (disconnected(DisconnectReason::UserClosed), conn(1, Tcp)),
        (failed(DisconnectReason::AuthFailed), conn(1, Tcp)),
        (disconnected(DisconnectReason::Network), Idle),
        (failed(DisconnectReason::CertMismatch), Idle),
        // highest leg allowed
        (conn(MAX_LEG - 1, Activation), conn(MAX_LEG, Tcp)),
    ];
    for (from, to) in valid {
        assert_eq!(from.transition(to.clone()), Ok(to.clone()), "{from:?} -> {to:?} should be valid");
    }
}

#[test]
fn invalid_transitions_are_rejected() {
    let invalid: Vec<(SessionState, SessionState)> = vec![
        (Idle, connected(1280, 800)),
        (Idle, AwaitingGreeterLogin),
        (Idle, conn(2, Tcp)),
        (Idle, conn(0, Tcp)),
        (Idle, reconnecting(1)),
        (Idle, Idle),
        // legs must start at 1, stay in range, advance by at most one
        (conn(1, Tcp), conn(3, Tcp)),
        (conn(2, Tcp), conn(1, Tcp)),
        (conn(MAX_LEG, Activation), conn(MAX_LEG + 1, Tcp)),
        // greeter only after a redirect
        (conn(1, Activation), AwaitingGreeterLogin),
        (AwaitingGreeterLogin, connected(1280, 800)),
        (AwaitingGreeterLogin, conn(1, Tcp)),
        (AwaitingGreeterLogin, AwaitingGreeterLogin),
        (connected(1280, 800), conn(1, Tcp)),
        (connected(1280, 800), AwaitingGreeterLogin),
        (connected(1280, 800), Idle),
        // reconnecting needs a retryable reason and attempt ≥ 1, and never goes backwards
        (
            connected(1280, 800),
            Reconnecting { attempt: 1, next_in: Duration::ZERO, reason: DisconnectReason::AuthFailed },
        ),
        (connected(1280, 800), reconnecting(0)),
        (reconnecting(2), reconnecting(1)),
        (reconnecting(1), conn(2, Tcp)),
        (reconnecting(1), connected(1280, 800)),
        (reconnecting(1), Idle),
        // terminal states only restart at leg 1 or go idle
        (disconnected(DisconnectReason::UserClosed), connected(1280, 800)),
        (disconnected(DisconnectReason::UserClosed), reconnecting(1)),
        (failed(DisconnectReason::AuthFailed), conn(2, Tcp)),
        (failed(DisconnectReason::AuthFailed), failed(DisconnectReason::AuthFailed)),
        (disconnected(DisconnectReason::Network), disconnected(DisconnectReason::Network)),
    ];
    for (from, to) in invalid {
        let err = from.transition(to.clone()).expect_err(&format!("{from:?} -> {to:?} should be invalid"));
        assert_eq!((err.from, err.to), (from, to));
    }
}

#[test]
fn is_active_classifies_states() {
    assert!(conn(1, Tcp).is_active());
    assert!(AwaitingGreeterLogin.is_active());
    assert!(connected(1, 1).is_active());
    assert!(!Idle.is_active());
    assert!(!reconnecting(1).is_active());
    assert!(!failed(DisconnectReason::Timeout).is_active());
}
