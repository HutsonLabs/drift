//! M3-3 / M3-4 e2e: Remote Login session reuse, takeover of a stale client and greeter
//! hygiene against the real GNOME host. Run through `cargo xtask e2e`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use drift_core::{ConnectMode, SessionState};
use drift_e2e::{E2eSession, host, init_logging, port, require, var};
use drift_rdp::SessionCommand;
use drift_testkit::e2e::Script;

const GREETER: Duration = Duration::from_secs(60);
const DESKTOP: Duration = Duration::from_secs(120);

/// Connects a Remote Login session through `local_port` and waits for the GDM greeter.
async fn greeter(local_port: u16) -> E2eSession {
    let (user, pass) = (require("DRIFT_E2E_SYS_USER"), require("DRIFT_E2E_SYS_PASS"));
    let mut s = E2eSession::start(ConnectMode::RemoteLogin, local_port, &user, &pass);
    s.wait_state("the GDM greeter", GREETER, |st| *st == SessionState::AwaitingGreeterLogin).await;
    s
}

/// Types the scripted GDM login and waits for the user session.
async fn log_in(s: &mut E2eSession) {
    let login_pass = require("DRIFT_E2E_LOGIN_PASS");
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
    s.wait_state("the user session", DESKTOP, |st| matches!(st, SessionState::Connected { .. })).await;
    typing.abort();
}

/// Logging in again after a disconnect lands in the same `loginctl` session (plan §1.2).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_login_reuses_session() {
    init_logging();
    let login_user = drift_e2e::login_session_user();
    let local_port = port("DRIFT_E2E_PORT_3389", 3389);

    let mut first = greeter(local_port).await;
    log_in(&mut first).await;
    let before = host::session_id(&login_user).expect("a session for the test user");
    first.close().await;
    tokio::time::sleep(Duration::from_secs(3)).await;

    let mut second = greeter(local_port).await;
    log_in(&mut second).await;
    let after = host::session_id(&login_user).expect("a session for the test user");
    assert_eq!(before, after, "GDM handed the connection to the existing session");
    second.close().await;
}

/// A second client takes the session over while the first one is frozen (its transport is
/// alive but dead in the water, plan §1.3).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_takeover_stale() {
    init_logging();
    let login_user = drift_e2e::login_session_user();

    let mut stale_forward = host::SshForward::open(3389);
    let mut stale = greeter(stale_forward.port()).await;
    log_in(&mut stale).await;
    let before = host::session_id(&login_user).expect("a session for the test user");

    // Freeze the first client's transport: the server still sees an open TCP connection.
    stale_forward.kill();

    let mut takeover = greeter(port("DRIFT_E2E_PORT_3389", 3389)).await;
    log_in(&mut takeover).await;
    let after = host::session_id(&login_user).expect("a session for the test user");
    assert_eq!(before, after, "the new client reached the same session");
    drop(stale);
    takeover.close().await;
}

/// Closing a tab at the greeter leaves no greeter session behind (plan §1.9, M3-4).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_no_greeter_leak() {
    init_logging();
    let local_port = port("DRIFT_E2E_PORT_3389", 3389);
    let before = host::greeter_count();

    for cycle in 1..=3 {
        let s = greeter(local_port).await;
        let end = s.close().await;
        eprintln!("[e2e] greeter cycle {cycle} ended as {end:?}");
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    // GDM tears greeters down asynchronously (and reaps older ones), so the count may also
    // drop; give it a moment to settle.
    for _ in 0..12 {
        if host::greeter_count() <= before {
            break;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    let after = host::greeter_count();
    assert!(
        after <= before,
        "three open/close cycles at the greeter left {} session(s) behind (before {before}, after {after})",
        after.saturating_sub(before)
    );
}
