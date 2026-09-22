//! M5-2 Red tests: CLIPRDR synchronisation (plan §6 M5-2, §1.6).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{Harness, PASS, USER, is_connected, options, profile, wait_log};
use drift_clipboard::{ClipboardContents, ClipboardItem};
use drift_core::ConnectMode;
use drift_rdp::{SessionCommand, SessionEvent, SessionSecrets};
use drift_testkit::fixtures::{self, names};
use drift_testkit::{
    Channels, FakeServer, LegScript, ManualClock, ServerAction, ServerClipFormat, TestCert,
};

const WAIT: Duration = Duration::from_secs(20);
/// `CF_UNICODETEXT`.
const CF_UNICODETEXT: u32 = 13;
/// `CF_DIB`.
const CF_DIB: u32 = 8;
/// The id Drift registers for `"image/png"` outbound (drift-clipboard `LOCAL_PNG_FORMAT_ID`).
const LOCAL_PNG: u32 = 0xC0F0;

fn leg(cert: &TestCert) -> LegScript {
    LegScript::nla(cert.clone(), USER, PASS).with_channels(Channels::all())
}

fn remote_clip(e: &SessionEvent) -> Option<ClipboardContents> {
    match e {
        SessionEvent::ClipboardRemote(c) => Some(c.clone()),
        _ => None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_initial_format_list_request_is_answered_so_cliprdr_becomes_ready() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![leg(&cert)]).await.unwrap();
    let mut h = Harness::start(profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())), PASS);
    h.wait_for("Connected", WAIT, is_connected).await;
    let log = wait_log(&server, "CLIPRDR ready", WAIT, |l| l.legs[0].clipboard_ready).await;
    assert_eq!(
        log.legs[0].client_format_lists.first().map(Vec::as_slice),
        Some([].as_slice()),
        "an empty list still answers the initial request (plan §1.6)"
    );
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remote_text_copy_is_fetched_eagerly() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![
        leg(&cert)
            .then(ServerAction::Wait(Duration::from_millis(300)))
            .then(ServerAction::ClipboardCopy(vec![ServerClipFormat::unicode_text("copy-me-42")])),
    ])
    .await
    .unwrap();
    let mut h = Harness::start(profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())), PASS);
    h.handle.send(SessionCommand::Focus(true)).unwrap();
    h.wait_for("Connected", WAIT, is_connected).await;
    let event = h
        .wait_for("ClipboardRemote", WAIT, |e| matches!(e, SessionEvent::ClipboardRemote(_)))
        .await;
    assert_eq!(remote_clip(&event).and_then(|c| c.text().map(str::to_owned)), Some("copy-me-42".to_owned()));
    assert_eq!(server.log().legs[0].client_data_requests, vec![CF_UNICODETEXT], "eager fetch, once");
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remote_png_copy_is_fetched_unchanged() {
    let png = fixtures::read(names::CLIP_REMOTE_PNG);
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![
        leg(&cert)
            .then(ServerAction::Wait(Duration::from_millis(300)))
            .then(ServerAction::ClipboardCopy(vec![ServerClipFormat::png(png.clone())])),
    ])
    .await
    .unwrap();
    let mut h = Harness::start(profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())), PASS);
    h.handle.send(SessionCommand::Focus(true)).unwrap();
    h.wait_for("Connected", WAIT, is_connected).await;
    let event = h
        .wait_for("ClipboardRemote", WAIT, |e| matches!(e, SessionEvent::ClipboardRemote(_)))
        .await;
    assert_eq!(remote_clip(&event).map(|c| c.items), Some(vec![ClipboardItem::Png(png)]));
    assert_eq!(server.log().legs[0].client_data_requests, vec![0xD011]);
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_local_copy_is_advertised_and_served() {
    let png = fixtures::read(names::CLIP_REMOTE_PNG);
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![
        leg(&cert)
            .then(ServerAction::Wait(Duration::from_millis(700)))
            .then(ServerAction::ClipboardPaste(CF_UNICODETEXT))
            .then(ServerAction::Wait(Duration::from_millis(300)))
            .then(ServerAction::ClipboardPaste(LOCAL_PNG)),
    ])
    .await
    .unwrap();
    let mut h = Harness::start(profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())), PASS);
    h.handle.send(SessionCommand::Focus(true)).unwrap();
    h.wait_for("Connected", WAIT, is_connected).await;
    h.handle
        .send(SessionCommand::ClipboardLocalChanged(ClipboardContents {
            items: vec![ClipboardItem::Text("Grüße ✓".into()), ClipboardItem::Png(png.clone())],
        }))
        .unwrap();

    let log = wait_log(&server, "local format list", WAIT, |l| l.legs[0].client_format_lists.len() >= 2).await;
    assert_eq!(
        log.legs[0].client_format_lists.last().unwrap(),
        &vec![
            (CF_UNICODETEXT, None),
            (LOCAL_PNG, Some("image/png".to_owned())),
            (CF_DIB, None),
        ],
        "plan §1.6: CF_UNICODETEXT + image/png (+ CF_DIB for compatibility)"
    );

    let log = wait_log(&server, "served data", WAIT, |l| l.legs[0].client_data_responses.len() >= 2).await;
    let responses = &log.legs[0].client_data_responses;
    let mut expected_text: Vec<u8> = "Grüße ✓".encode_utf16().flat_map(u16::to_le_bytes).collect();
    expected_text.extend_from_slice(&[0, 0]);
    assert_eq!(responses[0].as_ref(), Some(&expected_text), "UTF-16LE with a NUL");
    assert_eq!(responses[1].as_ref(), Some(&png), "the PNG unchanged");
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_the_focused_session_syncs() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![
        leg(&cert)
            .then(ServerAction::Wait(Duration::from_millis(300)))
            .then(ServerAction::ClipboardCopy(vec![ServerClipFormat::unicode_text("background")])),
    ])
    .await
    .unwrap();
    let mut h = Harness::start(profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())), PASS);
    h.wait_for("Connected", WAIT, is_connected).await;
    let events = h.drain_for(Duration::from_millis(900)).await;
    assert!(
        !events.iter().any(|e| matches!(e, SessionEvent::ClipboardRemote(_))),
        "an unfocused tab ignores remote copies: {events:?}"
    );
    assert!(server.log().legs[0].client_data_requests.is_empty());

    // Regaining focus advertises the local clipboard that changed meanwhile.
    h.handle
        .send(SessionCommand::ClipboardLocalChanged(ClipboardContents {
            items: vec![ClipboardItem::Text("local".into())],
        }))
        .unwrap();
    h.handle.send(SessionCommand::Focus(true)).unwrap();
    let log = wait_log(&server, "advertised on focus", WAIT, |l| {
        l.legs[0].client_format_lists.iter().any(|f| f.iter().any(|(id, _)| *id == CF_UNICODETEXT))
    })
    .await;
    assert!(log.legs[0].clipboard_ready);
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn writing_the_fetched_contents_back_does_not_echo_to_the_server() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![
        leg(&cert)
            .then(ServerAction::Wait(Duration::from_millis(300)))
            .then(ServerAction::ClipboardCopy(vec![ServerClipFormat::unicode_text("no-loops")])),
    ])
    .await
    .unwrap();
    let mut h = Harness::start(profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())), PASS);
    h.handle.send(SessionCommand::Focus(true)).unwrap();
    h.wait_for("Connected", WAIT, is_connected).await;
    let event = h
        .wait_for("ClipboardRemote", WAIT, |e| matches!(e, SessionEvent::ClipboardRemote(_)))
        .await;
    let lists_before = server.log().legs[0].client_format_lists.len();

    // The app wrote the pasteboard; its watcher reports the change straight back.
    h.handle.send(SessionCommand::ClipboardLocalChanged(remote_clip(&event).unwrap())).unwrap();
    let _ = h.drain_for(Duration::from_millis(600)).await;
    assert_eq!(
        server.log().legs[0].client_format_lists.len(),
        lists_before,
        "our own write is not advertised back (no copy loop)"
    );
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_newer_local_change_wins_over_an_in_flight_remote_fetch() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![
        leg(&cert)
            .then(ServerAction::Wait(Duration::from_millis(300)))
            .then(ServerAction::HoldClipboardResponses(true))
            .then(ServerAction::ClipboardCopy(vec![ServerClipFormat::unicode_text("stale-remote")]))
            .then(ServerAction::Wait(Duration::from_millis(1500)))
            .then(ServerAction::HoldClipboardResponses(false)),
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
    h.handle.send(SessionCommand::Focus(true)).unwrap();
    h.wait_for("Connected", WAIT, is_connected).await;
    wait_log(&server, "fetch in flight", WAIT, |l| !l.legs[0].client_data_requests.is_empty()).await;

    // A local copy happens while the remote data is still on the wire: newest wins.
    clock.advance(Duration::from_millis(10));
    h.handle
        .send(SessionCommand::ClipboardLocalChanged(ClipboardContents {
            items: vec![ClipboardItem::Text("fresh-local".into())],
        }))
        .unwrap();
    let events = h.drain_for(Duration::from_millis(2500)).await;
    assert!(
        !events.iter().any(|e| matches!(e, SessionEvent::ClipboardRemote(_))),
        "the superseded remote fetch is dropped: {events:?}"
    );
    let log = server.log();
    assert!(
        log.legs[0].client_format_lists.last().unwrap().iter().any(|(id, _)| *id == CF_UNICODETEXT),
        "the local clipboard was advertised instead: {:?}",
        log.legs[0].client_format_lists
    );
    h.close().await;
}
