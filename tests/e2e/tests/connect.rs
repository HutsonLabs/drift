//! Real-host e2e tests for M1-1 (headless connect) and M3-1 (Remote Login redirect loop).
//! Run through `cargo xtask e2e` (SSH forwards + credentials); never in `cargo xtask ci`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use drift_core::{ConnectMode, ConnectStage, DesktopSize, SessionState};
use drift_e2e::{E2eSession, forwarded_port, init_logging, require, var};
use drift_rdp::{SessionCommand, SessionEvent};
use drift_testkit::e2e::Script;

fn port(var_name: &str, remote: u16) -> u16 {
    var(var_name).and_then(|p| p.parse().ok()).or_else(|| forwarded_port(remote)).unwrap_or(10_000 + remote)
}

/// M1-1 Done: Headless connect to drifttest2 (:3392) reaches `Connected{1280x800}`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_headless_connects() {
    init_logging();
    let (user, pass) = (require("DRIFT_E2E_HL_USER"), require("DRIFT_E2E_HL_PASS"));
    let mut s = E2eSession::start(ConnectMode::Headless, port("DRIFT_E2E_HL_PORT", 3392), &user, &pass);

    let connected = s
        .wait_for("Connected", Duration::from_secs(30), |e| {
            matches!(e, SessionEvent::State(SessionState::Connected { .. }))
        })
        .await;
    assert_eq!(
        connected,
        SessionEvent::State(SessionState::Connected {
            desktop: DesktopSize { width: 1280, height: 800 },
            scale: 100
        })
    );
    // The session stays up (the server does not drop a client that advertises the GFX pipeline).
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(
        s.close().await,
        Some(SessionState::Disconnected { reason: drift_core::DisconnectReason::UserClosed })
    );
}

/// M3-1: Remote Login: leg 1 NLA → redirect → greeter (RDSTLS) → scripted GDM login as
/// drifttest → redirect → user session.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_remote_login() {
    init_logging();
    let (user, pass) = (require("DRIFT_E2E_SYS_USER"), require("DRIFT_E2E_SYS_PASS"));
    let login_pass = require("DRIFT_E2E_LOGIN_PASS");
    let mut s = E2eSession::start(ConnectMode::RemoteLogin, port("DRIFT_E2E_PORT_3389", 3389), &user, &pass);

    s.wait_for("greeter", Duration::from_secs(45), |e| {
        *e == SessionEvent::State(SessionState::AwaitingGreeterLogin)
    })
    .await;
    let states = s.states();
    assert!(states.contains(&SessionState::Connecting { leg: 1, stage: ConnectStage::Nla }), "{states:?}");
    assert!(states.contains(&SessionState::Connecting { leg: 2, stage: ConnectStage::Rdstls }), "{states:?}");

    let script = match var("DRIFT_E2E_GREETER_SCRIPT") {
        Some(src) => Script::parse(&src, &[("@PW", login_pass.as_str())]).expect("DRIFT_E2E_GREETER_SCRIPT"),
        None => Script::gdm_login(&login_pass),
    };
    let handle = s.handle.clone();
    let typing = tokio::spawn(async move {
        script
            .run(|ev| {
                let _ = handle.send(SessionCommand::Input(ev));
            })
            .await;
    });

    s.wait_for("user session", Duration::from_secs(90), |e| {
        matches!(e, SessionEvent::State(SessionState::Connected { .. }))
    })
    .await;
    typing.abort();
    let states = s.states();
    assert!(states.contains(&SessionState::Connecting { leg: 3, stage: ConnectStage::Rdstls }), "{states:?}");
    tokio::time::sleep(Duration::from_secs(2)).await;
    s.close().await;
}
