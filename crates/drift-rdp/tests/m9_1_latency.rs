//! M9-1 Red: the session actor measures the two latencies the bench reports.
//!
//! * **decode + present**: from the moment a graphics payload comes off the wire until the
//!   frame sink reports it presented (the `presented` callback that also sends the ack).
//! * **input-to-wire**: from the moment an [`InputEvent`] reaches the actor until its
//!   fast-path frame has been written to the socket.
//!
//! Both are reported in [`drift_rdp::SessionStats`], which is what
//! `cargo xtask e2e --bench` records against the real host.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::{Harness, PASS, USER, is_connected, profile, wait_log};
use drift_core::{ConnectMode, InputEvent};
use drift_rdp::{SessionCommand, SessionEvent, SessionStats};
use drift_testkit::{Channels, FakeServer, LegScript, ServerAction, TestCert};

const WAIT: Duration = Duration::from_secs(20);

fn samples(events: &[SessionEvent]) -> Vec<SessionStats> {
    events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::Stats(s) => Some(*s),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn presented_frames_report_a_decode_and_present_p95() {
    let cert = TestCert::generate("127.0.0.1");
    let mut leg = LegScript::nla(cert.clone(), USER, PASS).with_channels(Channels::all());
    for _ in 0..8 {
        leg =
            leg.then(ServerAction::Wait(Duration::from_millis(120))).then(ServerAction::GfxReset(1280, 800));
    }
    let server = FakeServer::start(vec![leg]).await.unwrap();
    let mut h = Harness::start(profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())), PASS);
    h.wait_for("Connected", WAIT, is_connected).await;
    // Frames must actually have been presented before the statistic means anything.
    wait_log(&server, "frame acks", WAIT, |l| l.legs[0].gfx_frame_acks.len() >= 4).await;

    let stats = samples(&h.drain_for(Duration::from_secs(3)).await);
    assert!(!stats.is_empty(), "the actor samples statistics");
    assert!(
        stats.iter().any(|s| s.frame_latency_p95_ms > 0.0),
        "decode + present p95 is measured, not hard-coded to zero: {stats:?}"
    );
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn input_events_report_an_input_to_wire_p99() {
    let cert = TestCert::generate("127.0.0.1");
    let server =
        FakeServer::start(vec![LegScript::nla(cert.clone(), USER, PASS).with_channels(Channels::all())])
            .await
            .unwrap();
    let mut h = Harness::start(profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())), PASS);
    h.wait_for("Connected", WAIT, is_connected).await;

    // A pointer path, as a user would draw it, spread over more than one statistics window.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut x = 0_u16;
    let mut seen: Vec<SessionStats> = Vec::new();
    while tokio::time::Instant::now() < deadline {
        x = (x + 7) % 1280;
        h.handle.send(SessionCommand::Input(InputEvent::MouseMove { x, y: 400 })).unwrap();
        seen.extend(samples(&h.drain_for(Duration::from_millis(20)).await));
    }
    assert!(!seen.is_empty(), "the actor samples statistics");
    assert!(
        seen.iter().any(|s| s.input_to_wire_p99_ms > 0.0),
        "input-to-wire p99 is measured while input flows: {seen:?}"
    );
    assert!(
        seen.iter().all(|s| s.input_to_wire_p99_ms < 1000.0),
        "the measurement is a per-event latency, not the window: {seen:?}"
    );
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_idle_session_reports_no_input_latency() {
    let cert = TestCert::generate("127.0.0.1");
    let server =
        FakeServer::start(vec![LegScript::nla(cert.clone(), USER, PASS).with_channels(Channels::all())])
            .await
            .unwrap();
    let mut h = Harness::start(profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())), PASS);
    h.wait_for("Connected", WAIT, is_connected).await;
    let stats = samples(&h.drain_for(Duration::from_millis(2500)).await);
    assert!(!stats.is_empty(), "the actor samples statistics");
    assert!(stats.iter().all(|s| s.input_to_wire_p99_ms == 0.0), "no input, no input latency: {stats:?}");
    h.close().await;
}
