//! M3-4 Red test (loopback half): closing a tab at the GDM greeter disconnects gracefully so
//! the host is left with no orphaned greeter session (plan §6 M3-4, §1.9).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::{Harness, PASS, USER, profile, wait_log};
use drift_core::{ConnectMode, DisconnectReason, SessionState};
use drift_rdp::{SessionCommand, SessionEvent};
use drift_testkit::{Channels, FakeServer, LegScript, ServerAction, TestCert, redirection_pdu};

const WAIT: Duration = Duration::from_secs(20);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_at_the_greeter_sends_a_shutdown_request() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![
        LegScript {
            actions: vec![
                ServerAction::Wait(Duration::from_millis(50)),
                ServerAction::Redirect(Box::new(redirection_pdu(77, cert.der(), 1))),
            ],
            ..LegScript::nla(cert.clone(), USER, PASS)
        },
        LegScript::rdstls(cert.clone(), 0).with_channels(Channels::all()),
    ])
    .await
    .unwrap();
    let mut h =
        Harness::start(profile(ConnectMode::RemoteLogin, server.port(), Some(cert.fingerprint())), PASS);
    h.wait_for("greeter", WAIT, |e| *e == SessionEvent::State(SessionState::AwaitingGreeterLogin)).await;
    h.handle.send(SessionCommand::Close).unwrap();
    assert_eq!(
        h.wait_terminal(WAIT).await,
        SessionState::Disconnected { reason: DisconnectReason::UserClosed }
    );
    let log = wait_log(&server, "shutdown request", WAIT, |l| l.legs[1].shutdown_requested).await;
    assert!(log.legs[1].shutdown_requested, "the greeter leg is shut down gracefully");
}
