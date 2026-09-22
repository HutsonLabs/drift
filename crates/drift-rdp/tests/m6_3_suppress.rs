//! M6-3 Red tests: background throttling (Suppress Output + ack suspension), plan §6 M6-3.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::{Harness, PASS, USER, is_connected, profile, wait_log};
use drift_core::ConnectMode;
use drift_rdp::SessionCommand;
use drift_testkit::{Channels, FakeServer, FrameSinkCall, LegScript, ServerAction, TestCert};

const WAIT: Duration = Duration::from_secs(20);
/// The fake server's desktop, as an inclusive full-desktop rectangle.
const FULL_RECT: [u16; 4] = [0, 0, 1279, 799];

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hiding_suppresses_output_and_showing_allows_the_full_rect() {
    let cert = TestCert::generate("127.0.0.1");
    let server =
        FakeServer::start(vec![LegScript::nla(cert.clone(), USER, PASS).with_channels(Channels::all())])
            .await
            .unwrap();
    let mut h = Harness::start(profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())), PASS);
    h.wait_for("Connected", WAIT, is_connected).await;

    h.handle.send(SessionCommand::SetVisible(false)).unwrap();
    let log = wait_log(&server, "suppress", WAIT, |l| !l.legs[0].suppress_output.is_empty()).await;
    assert_eq!(log.legs[0].suppress_output, vec![None], "allowDisplayUpdates=0 while hidden");

    h.handle.send(SessionCommand::SetVisible(true)).unwrap();
    let log = wait_log(&server, "allow", WAIT, |l| l.legs[0].suppress_output.len() == 2).await;
    assert_eq!(
        log.legs[0].suppress_output,
        vec![None, Some(FULL_RECT)],
        "on visible: allow with the full desktop rectangle (no Refresh Rect, plan §1.4)"
    );

    let calls = h.frames.calls();
    let visibility: Vec<bool> = calls
        .iter()
        .filter_map(|c| match c {
            FrameSinkCall::SetVisible { visible } => Some(*visible),
            _ => None,
        })
        .collect();
    assert_eq!(visibility, vec![false, true], "the render thread is paused and resumed: {calls:?}");
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_frame_arriving_while_hidden_suspends_acknowledgement() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![
        LegScript::nla(cert.clone(), USER, PASS)
            .with_channels(Channels::all())
            .then(ServerAction::Wait(Duration::from_millis(700)))
            .then(ServerAction::GfxReset(1280, 800))
            .then(ServerAction::Wait(Duration::from_millis(700)))
            .then(ServerAction::GfxReset(1280, 800)),
    ])
    .await
    .unwrap();
    let mut h = Harness::start(profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())), PASS);
    h.wait_for("Connected", WAIT, is_connected).await;
    // The first frame (sent when the graphics channel opened) is acknowledged normally.
    let log = wait_log(&server, "first ack", WAIT, |l| !l.legs[0].gfx_frame_acks.is_empty()).await;
    assert_eq!(log.legs[0].gfx_suspend_acks, 0);

    h.handle.send(SessionCommand::SetVisible(false)).unwrap();
    wait_log(&server, "suppress", WAIT, |l| !l.legs[0].suppress_output.is_empty()).await;
    // g-r-d would stop sending, but a frame already in flight must still suspend the acks.
    let log = wait_log(&server, "suspend ack", WAIT, |l| l.legs[0].gfx_suspend_acks == 1).await;
    let acked_while_hidden = log.legs[0].gfx_frame_acks.len();

    h.handle.send(SessionCommand::SetVisible(true)).unwrap();
    let log =
        wait_log(&server, "ack after resume", WAIT, |l| l.legs[0].gfx_frame_acks.len() > acked_while_hidden)
            .await;
    assert_eq!(log.legs[0].gfx_suspend_acks, 1, "exactly one suspend, then normal acks again");
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_hidden_before_activation_suppresses_on_connect() {
    let cert = TestCert::generate("127.0.0.1");
    let server =
        FakeServer::start(vec![LegScript::nla(cert.clone(), USER, PASS).with_channels(Channels::all())])
            .await
            .unwrap();
    let mut h = Harness::start(profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())), PASS);
    h.handle.send(SessionCommand::SetVisible(false)).unwrap();
    h.wait_for("Connected", WAIT, is_connected).await;
    let log = wait_log(&server, "suppress on connect", WAIT, |l| !l.legs[0].suppress_output.is_empty()).await;
    assert_eq!(log.legs[0].suppress_output, vec![None]);
    h.close().await;
}
