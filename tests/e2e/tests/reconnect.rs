//! M7-3 e2e: auto-reconnect against the real GNOME host, with the transport cut by killing
//! the test's own SSH forward. Run through `cargo xtask e2e`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use drift_core::{ConnectMode, SessionState};
use drift_e2e::{E2eSession, host, init_logging, require, var};
use drift_rdp::SessionCommand;
use drift_testkit::e2e::Script;

const CONNECT: Duration = Duration::from_secs(40);
const RECONNECT: Duration = Duration::from_secs(90);
const DESKTOP: Duration = Duration::from_secs(120);

/// Headless (`drifttest2`): a reconnect lands straight back in the same session, no greeter.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_reconnect_headless() {
    init_logging();
    let (rdp_user, pass) = (require("DRIFT_E2E_HL_USER"), require("DRIFT_E2E_HL_PASS"));
    let user = drift_e2e::headless_session_user();
    host::ensure_unlocked_session(&user);
    let mut forward = host::SshForward::open(3392);
    let mut s = E2eSession::start(ConnectMode::Headless, forward.port(), &rdp_user, &pass);
    s.wait_state("Connected", CONNECT, |st| matches!(st, SessionState::Connected { .. })).await;
    let before = host::session_id(&user).expect("the headless session");

    forward.kill();
    s.wait_state("Reconnecting", RECONNECT, |st| matches!(st, SessionState::Reconnecting { .. })).await;
    // The backoff keeps trying; restoring the forward lets the next attempt through.
    forward.start();
    s.send(SessionCommand::ReconnectNow);
    s.wait_state("the resumed session", RECONNECT, |st| matches!(st, SessionState::Connected { .. })).await;

    let states = s.states();
    assert!(
        !states.contains(&SessionState::AwaitingGreeterLogin),
        "Headless resumes without a greeter: {states:?}"
    );
    assert_eq!(host::session_id(&user).as_deref(), Some(before.as_str()), "the same session");
    s.close().await;
}

/// Remote Login: a reconnect always passes through the GDM greeter and the login lands in the
/// same session (plan §8).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_reconnect_remote_login() {
    init_logging();
    let (sys_user, sys_pass) = (require("DRIFT_E2E_SYS_USER"), require("DRIFT_E2E_SYS_PASS"));
    let login_user = require("DRIFT_E2E_LOGIN_USER");
    let login_pass = require("DRIFT_E2E_LOGIN_PASS");
    let script = || match var("DRIFT_E2E_GREETER_SCRIPT") {
        Some(src) => Script::parse(&src, &[("@PW", login_pass.as_str())]).expect("DRIFT_E2E_GREETER_SCRIPT"),
        None => Script::gdm_login(&login_pass),
    };

    let mut forward = host::SshForward::open(3389);
    let mut s = E2eSession::start(ConnectMode::RemoteLogin, forward.port(), &sys_user, &sys_pass);
    s.wait_state("the greeter", RECONNECT, |st| *st == SessionState::AwaitingGreeterLogin).await;
    type_script(&mut s, script());
    s.wait_state("the user session", DESKTOP, |st| matches!(st, SessionState::Connected { .. })).await;
    let before = host::session_id(&login_user).expect("a session for the test user");

    forward.kill();
    s.wait_state("Reconnecting", RECONNECT, |st| matches!(st, SessionState::Reconnecting { .. })).await;
    forward.start();
    s.send(SessionCommand::ReconnectNow);
    s.wait_state("the greeter again", RECONNECT, |st| *st == SessionState::AwaitingGreeterLogin).await;

    type_script(&mut s, script());
    s.wait_state("the resumed session", DESKTOP, |st| matches!(st, SessionState::Connected { .. })).await;
    assert_eq!(
        host::session_id(&login_user).as_deref(),
        Some(before.as_str()),
        "after the greeter login the same session resumed"
    );
    s.close().await;
}

/// Runs a scripted login in the background (typing must not block the event loop).
fn type_script(s: &mut E2eSession, script: Script) {
    let handle = s.handle.clone();
    tokio::spawn(async move {
        script
            .run(|ev| {
                let _ = handle.send(SessionCommand::Input(ev));
            })
            .await;
    });
}
