//! M9-3 Red: the TOFU pin covers the redirect target, not only the system daemon.
//!
//! Plan M9-3: "TOFU pins cover both the system daemon and the redirect target". Leg 1's
//! certificate is checked against the profile pin (M1-1) and the redirected legs are normally
//! checked against the **target certificate container** of the redirection PDU (M3-1, which is
//! stricter: byte equality with the DER the server announced).
//!
//! The gap this test closes is the case where the server sends **no** target certificate:
//! `redirFlags` need not contain `LB_TARGET_CERTIFICATE`, and a man in the middle can simply
//! drop it. The redirected leg must then fall back to the pin Drift already trusts for this
//! host — the profile pin, or the one accepted for this session — and never to "trust
//! anything", because the one-time credentials are sent right after the handshake.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::{Harness, PASS, USER, profile};
use drift_core::{ConnectMode, DisconnectReason, SessionState};
use drift_rdp::{CertificateRole, SessionCommand, SessionEvent};
use drift_testkit::{FakeServer, LegScript, ServerAction, TestCert, redirection_pdu};
use ironrdp_pdu::rdp::server_redirection::{ServerRedirectionFlags, ServerRedirectionPdu};

const WAIT: Duration = Duration::from_secs(20);

/// A redirection PDU with the target certificate container removed, as a server that does not
/// set `LB_TARGET_CERTIFICATE` sends it.
fn redirect_without_target_certificate(token: u32, seed: u8) -> ServerRedirectionPdu {
    let cert = TestCert::generate("unused");
    let mut pdu = redirection_pdu(token, cert.der(), seed);
    pdu.target_certificate = None;
    pdu.redirection_flags.remove(ServerRedirectionFlags::TARGET_CERTIFICATE);
    pdu
}

fn redirect_after(ms: u64, pdu: ServerRedirectionPdu) -> Vec<ServerAction> {
    vec![ServerAction::Wait(Duration::from_millis(ms)), ServerAction::Redirect(Box::new(pdu))]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_redirect_without_a_target_certificate_still_honours_the_profile_pin() {
    let cert = TestCert::generate("127.0.0.1");
    let imposter = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![
        LegScript {
            actions: redirect_after(50, redirect_without_target_certificate(9, 1)),
            ..LegScript::nla(cert.clone(), USER, PASS)
        },
        LegScript::rdstls(imposter, 0),
    ])
    .await
    .unwrap();

    let mut h =
        Harness::start(profile(ConnectMode::RemoteLogin, server.port(), Some(cert.fingerprint())), PASS);
    assert_eq!(h.wait_terminal(WAIT).await, SessionState::Failed { reason: DisconnectReason::CertMismatch });
    let log = server.log();
    assert_eq!(log.legs.len(), 2);
    assert_eq!(log.legs[1].rdstls_request, None, "one-time credentials never reach an unpinned host");
    assert!(
        !h.seen.iter().any(|e| matches!(e, SessionEvent::CertificatePrompt { .. })),
        "a pinned profile is never asked again: {:?}",
        h.seen
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_pin_accepted_for_this_session_also_covers_the_redirect_target() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![
        LegScript {
            actions: redirect_after(50, redirect_without_target_certificate(11, 2)),
            ..LegScript::nla(cert.clone(), USER, PASS)
        },
        LegScript::rdstls(cert.clone(), 0),
    ])
    .await
    .unwrap();

    // No profile pin: the first leg is trust-on-first-use and the user accepts it.
    let mut h = Harness::start(profile(ConnectMode::RemoteLogin, server.port(), None), PASS);
    let prompt =
        h.wait_for("CertificatePrompt", WAIT, |e| matches!(e, SessionEvent::CertificatePrompt { .. })).await;
    let SessionEvent::CertificatePrompt { fingerprint, role, .. } = prompt else { unreachable!() };
    assert_eq!((fingerprint, role), (cert.fingerprint(), CertificateRole::Server));
    h.handle.send(SessionCommand::AcceptCertificate { fingerprint, pin: true }).unwrap();

    // The redirected leg presents the same certificate: it is already trusted, so the user is
    // not asked a second time and the session reaches the greeter.
    h.wait_for("AwaitingGreeterLogin", WAIT, |e| {
        *e == SessionEvent::State(SessionState::AwaitingGreeterLogin)
    })
    .await;
    let prompts = h.seen.iter().filter(|e| matches!(e, SessionEvent::CertificatePrompt { .. })).count();
    assert_eq!(prompts, 1, "the accepted pin covers the redirect target too: {:?}", h.seen);
    assert_eq!(server.log().legs.len(), 2);
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_redirect_target_that_changes_its_certificate_fails_closed() {
    let cert = TestCert::generate("127.0.0.1");
    let other = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![
        LegScript {
            actions: redirect_after(50, redirect_without_target_certificate(13, 3)),
            ..LegScript::nla(cert.clone(), USER, PASS)
        },
        LegScript::rdstls(other, 0),
    ])
    .await
    .unwrap();

    // Session-level trust only: the user accepts leg 1's certificate without pinning it.
    let mut h = Harness::start(profile(ConnectMode::RemoteLogin, server.port(), None), PASS);
    let prompt =
        h.wait_for("leg 1 prompt", WAIT, |e| matches!(e, SessionEvent::CertificatePrompt { .. })).await;
    let SessionEvent::CertificatePrompt { fingerprint, .. } = prompt else { unreachable!() };
    h.handle.send(SessionCommand::AcceptCertificate { fingerprint, pin: false }).unwrap();

    // Leg 2 announces no target certificate and presents a different one. The redirect is to
    // the same host and port, so a changed certificate is not a new host to decide about: it
    // is the certificate the user just approved being swapped, mid-connection, by whoever
    // controls the redirect. Drift fails closed instead of asking — asking would train users
    // to click through exactly the attack the pin exists to stop.
    assert_eq!(h.wait_terminal(WAIT).await, SessionState::Failed { reason: DisconnectReason::CertMismatch });
    let log = server.log();
    assert_eq!(log.legs.len(), 2);
    assert_eq!(log.legs[1].rdstls_request, None, "one-time credentials are never sent");
    let prompts = h.seen.iter().filter(|e| matches!(e, SessionEvent::CertificatePrompt { .. })).count();
    assert_eq!(prompts, 1, "only leg 1 is ever prompted for: {:?}", h.seen);
}
