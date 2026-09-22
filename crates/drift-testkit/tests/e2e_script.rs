//! `drift_testkit::e2e::Script` (the probe2 step runner port) and `FakeServer` smoke tests.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use drift_core::{InputEvent, MouseButton};
use drift_testkit::e2e::{GREETER_TEST_USER_TILE, Script, Step};
use drift_testkit::{FakeServer, LegScript, TestCert, redirection_pdu};

#[test]
fn parses_probe2_steps() {
    let s = Script::parse(
        "wait:1.5|click:640,427|text:@PW|key:Enter|ctrl:c|super|scroll:-12|move:1,2",
        &[("@PW", "Fake9")],
    )
    .unwrap();
    assert_eq!(
        s.steps(),
        &[
            Step::Wait(Duration::from_millis(1500)),
            Step::Click { x: 640, y: 427 },
            Step::Text("Fake9".into()),
            Step::Key { scancode: 0x1C, extended: false },
            Step::Chord(vec![(0x1D, false), (0x2E, false)]),
            Step::Chord(vec![(0x5B, true)]),
            Step::Scroll(-12),
            Step::Move { x: 1, y: 2 },
        ]
    );
}

#[test]
fn rejects_bad_steps_without_echoing_secrets() {
    for bad in ["wait:x", "click:1", "key:F13", "ctrl:q", "nope", "wait:-1"] {
        let err = Script::parse(bad, &[]).unwrap_err();
        assert_eq!(err.0, bad);
    }
}

#[test]
fn debug_never_shows_typed_text() {
    let s = Script::parse("text:@PW", &[("@PW", "Secret9-Fake")]).unwrap();
    assert!(!format!("{s:?}").contains("Secret9"));
}

#[test]
fn steps_expand_to_input_events() {
    assert_eq!(
        Step::Click { x: 5, y: 6 }.input_events(),
        vec![
            InputEvent::MouseMove { x: 5, y: 6 },
            InputEvent::MouseButton { button: MouseButton::Left, down: true, x: 5, y: 6 },
            InputEvent::MouseButton { button: MouseButton::Left, down: false, x: 5, y: 6 },
        ]
    );
    // Surrogate pairs go out as two UTF-16 units, each pressed and released.
    let ev = Step::Text("a😀".into()).input_events();
    assert_eq!(ev.len(), 6);
    assert_eq!(ev[0], InputEvent::Unicode { ch: u16::from(b'a'), down: true });
    assert_eq!(ev[2], InputEvent::Unicode { ch: 0xD83D, down: true });
    assert_eq!(ev[4], InputEvent::Unicode { ch: 0xDE00, down: true });
    assert_eq!(
        Step::Chord(vec![(0x1D, false), (0x38, false)]).input_events(),
        vec![
            InputEvent::Key { scancode: 0x1D, extended: false, down: true },
            InputEvent::Key { scancode: 0x38, extended: false, down: true },
            InputEvent::Key { scancode: 0x38, extended: false, down: false },
            InputEvent::Key { scancode: 0x1D, extended: false, down: false },
        ]
    );
    assert!(Step::Wait(Duration::from_secs(1)).input_events().is_empty());
}

#[test]
fn gdm_login_clicks_the_tile_types_and_presses_enter() {
    let s = Script::gdm_login("Fake9");
    let (x, y) = GREETER_TEST_USER_TILE;
    assert!(s.steps().contains(&Step::Click { x, y }));
    assert!(s.steps().contains(&Step::Text("Fake9".into())));
    assert_eq!(s.steps().last(), Some(&Step::Key { scancode: 0x1C, extended: false }));
}

#[tokio::test(start_paused = true)]
async fn run_sends_events_in_order() {
    let s = Script::parse("key:Tab|wait:10|key:Esc", &[]).unwrap();
    let mut sent = Vec::new();
    s.run(|e| sent.push(e)).await;
    assert_eq!(sent.len(), 4);
    assert_eq!(sent[0], InputEvent::Key { scancode: 0x0F, extended: false, down: true });
    assert_eq!(sent[3], InputEvent::Key { scancode: 0x01, extended: false, down: false });
}

#[test]
fn redirection_pdu_has_the_grd_shape() {
    let cert = TestCert::generate("127.0.0.1");
    let pdu = redirection_pdu(123, cert.der(), 1);
    assert_eq!(pdu.redirection_flags.bits(), 0x1C016);
    assert_eq!(pdu.load_balance_info.as_deref(), Some(&b"Cookie: msts=123\r\n"[..]));
    assert_eq!(pdu.username.as_ref().map(|u| u.chars().count()), Some(16));
    assert_eq!(pdu.password.as_ref().map(Vec::len), Some(34));
    assert_eq!(pdu.redirection_guid.as_ref().map(Vec::len), Some(50));
    let container = pdu.decode_target_certificate().unwrap().unwrap();
    assert_eq!(container.der_certificate(), Some(cert.der()));
    assert_ne!(redirection_pdu(123, cert.der(), 2).password, pdu.password);
}

#[tokio::test]
async fn fake_server_records_unscripted_connections() {
    let server = FakeServer::start(vec![]).await.unwrap();
    let _ = tokio::net::TcpStream::connect(server.addr()).await.unwrap();
    for _ in 0..50 {
        if !server.log().legs.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(server.log().legs.len(), 1);
    let stall = FakeServer::start(vec![LegScript::stall(TestCert::generate("127.0.0.1"))]).await.unwrap();
    assert_eq!(stall.addr().ip().to_string(), "127.0.0.1");
}
