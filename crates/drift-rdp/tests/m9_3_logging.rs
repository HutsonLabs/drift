//! M9-3 Red: credentials never appear in the session's own logs.
//!
//! Plan M9-3: "Credentials never appear in logs (a redaction test on e2e logs)". The e2e
//! harness redacts what it prints (`drift-e2e`'s `Redactor`), but a redactor only helps where
//! the operator remembered to install one: `RUST_LOG=trace cargo tauri dev` has none. So the
//! rule this test enforces is stricter — **the actor never writes a credential in the first
//! place**, at any level.
//!
//! It runs a full Remote Login connection (NLA leg 1, then two RDSTLS legs with one-time
//! credentials from a Server Redirection PDU) against the `FakeServer` with a `TRACE`
//! subscriber installed process-wide, and searches everything that was logged for the NLA
//! password, the one-time user name, the password blob and the redirection GUID — as text and
//! as the `Debug` renderings a stray `?field` would produce.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::io;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use common::{Harness, USER, is_connected, profile};
use drift_core::{ConnectMode, SessionState};
use drift_rdp::SessionEvent;
use drift_testkit::{FakeServer, LegScript, ServerAction, TestCert, redirection_pdu};

const WAIT: Duration = Duration::from_secs(20);
/// The NLA password of this test; it must never be logged.
const NLA_PASSWORD: &str = "Fake9-m93-logging-canary";

/// Everything the process logged, shared with the subscriber.
type Captured = Arc<Mutex<Vec<u8>>>;

/// A `MakeWriter` that appends to a shared buffer.
#[derive(Clone)]
struct CaptureWriter(Captured);

impl io::Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CaptureWriter {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Installs a `TRACE` subscriber for the whole process (this binary has one test), with the
/// credential cap the app and the e2e harness install (`drift_rdp::logging`).
///
/// `RUST_LOG=trace` is the worst case an operator can ask for, so that is what the test runs:
/// everything is on except the targets Drift refuses to let log credentials.
fn capture_logs() -> Captured {
    let buffer: Captured = Arc::default();
    let writer = CaptureWriter(Arc::clone(&buffer));
    let mut filter = tracing_subscriber::EnvFilter::new("trace");
    for directive in drift_rdp::logging::credential_safe_directives() {
        filter = filter.add_directive(directive.parse().expect("a valid directive"));
    }
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_target(true)
        .with_writer(writer)
        .init();
    buffer
}

/// Every rendering a leaked `secret` could have in a log line.
///
/// Text is the obvious one, but the RDP stack hands credentials to `sspi`, which logs its
/// buffers as decimal byte lists and as hex — and always in UTF-16LE, the encoding of the
/// wire. Byte lists are matched without their brackets so a secret nested in a longer list
/// still counts.
fn renderings(secret: &[u8]) -> Vec<(String, String)> {
    let utf16: Vec<u8> = String::from_utf8_lossy(secret)
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let list = |bytes: &[u8]| bytes.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
    let hex_lower = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let hex_upper = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
    let mut out = vec![
        ("text".into(), String::from_utf8_lossy(secret).into_owned()),
        ("byte list".into(), list(secret)),
        ("lower-case hex".into(), hex_lower(secret)),
        ("upper-case hex".into(), hex_upper(secret)),
        ("UTF-16LE byte list".into(), list(&utf16)),
        ("UTF-16LE lower-case hex".into(), hex_lower(&utf16)),
        ("UTF-16LE upper-case hex".into(), hex_upper(&utf16)),
    ];
    out.retain(|(_, text)| text.len() >= 8);
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_full_remote_login_logs_no_credential() {
    let logs = capture_logs();
    let cert = TestCert::generate("127.0.0.1");
    let pdu1 = redirection_pdu(424_242, cert.der(), 1);
    let pdu2 = redirection_pdu(424_242, cert.der(), 2);
    let redirect = |pdu: &ironrdp_pdu::rdp::server_redirection::ServerRedirectionPdu| {
        vec![
            ServerAction::Wait(Duration::from_millis(50)),
            ServerAction::Redirect(Box::new(pdu.clone())),
        ]
    };
    let server = FakeServer::start(vec![
        LegScript { actions: redirect(&pdu1), ..LegScript::nla(cert.clone(), USER, NLA_PASSWORD) },
        LegScript { actions: redirect(&pdu2), ..LegScript::rdstls(cert.clone(), 0) },
        LegScript::rdstls(cert.clone(), 0),
    ])
    .await
    .unwrap();

    let mut h = Harness::start(
        profile(ConnectMode::RemoteLogin, server.port(), Some(cert.fingerprint())),
        NLA_PASSWORD,
    );
    h.wait_for("AwaitingGreeterLogin", WAIT, |e| {
        *e == SessionEvent::State(SessionState::AwaitingGreeterLogin)
    })
    .await;
    h.wait_for("Connected", WAIT, is_connected).await;
    h.close().await;

    let text = String::from_utf8_lossy(&logs.lock().unwrap_or_else(PoisonError::into_inner)).into_owned();
    assert!(text.contains("following Server Redirection"), "the actor did log its progress:\n{text}");

    let mut secrets: Vec<(String, Vec<u8>)> =
        vec![("NLA password".into(), NLA_PASSWORD.as_bytes().to_vec())];
    for (i, pdu) in [&pdu1, &pdu2].into_iter().enumerate() {
        let leg = i + 2;
        secrets.push((format!("leg {leg} one-time user name"), pdu.username.clone().unwrap().into_bytes()));
        secrets.push((format!("leg {leg} one-time password"), pdu.password.clone().unwrap()));
        secrets.push((format!("leg {leg} redirection GUID"), pdu.redirection_guid.clone().unwrap()));
    }
    for (what, secret) in secrets {
        for (how, rendering) in renderings(&secret) {
            assert!(
                !text.contains(&rendering),
                "the {what} appears in the logs as {how}:\n{text}"
            );
        }
    }
}
