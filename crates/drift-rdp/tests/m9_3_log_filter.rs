//! M9-3: the credential cap wins over `RUST_LOG`.
//!
//! `drift_rdp::logging` is data — a list of `EnvFilter` directives — and the whole guarantee
//! rests on one `tracing-subscriber` behaviour: a directive added **after** the ones parsed
//! from `RUST_LOG` replaces the earlier directive for the same target. This test proves that
//! instead of trusting it, for the two shapes the app and the e2e harness build
//! (`RUST_LOG=trace` and an explicit `RUST_LOG=sspi=trace`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use tracing_subscriber::EnvFilter;

/// Shared log buffer.
type Captured = Arc<Mutex<Vec<u8>>>;

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

/// Logs one event and one span field on a capped target and on Drift's own, and returns the
/// output of the subscriber stack the app and the e2e harness build for `RUST_LOG=<rust_log>`.
fn capture(rust_log: &str) -> String {
    use tracing_subscriber::layer::SubscriberExt as _;

    let buffer: Captured = Arc::default();
    let credentials =
        tracing_subscriber::filter::filter_fn(|m| drift_rdp::logging::allows(m.target(), *m.level()));
    let subscriber = tracing_subscriber::registry().with(EnvFilter::new(rust_log)).with(credentials).with(
        tracing_subscriber::fmt::layer().with_ansi(false).with_writer(CaptureWriter(Arc::clone(&buffer))),
    );

    tracing::subscriber::with_default(subscriber, || {
        // What `sspi` does: an instrumented span whose fields carry the credential buffers,
        // and events inside it.
        let span = tracing::debug_span!(
            target: "sspi::credssp::ts_request",
            "write_ts_credentials",
            credentials = "SECRET-CREDENTIAL"
        );
        let _entered = span.enter();
        tracing::trace!(target: "sspi::ntlm", password = "SECRET-CREDENTIAL", "return=Ok(..)");
        tracing::debug!(target: "sspi", "acquire_credentials_handle");
        tracing::warn!(target: "sspi", "NTLM failed");
        // Drift's own tracing must be unaffected.
        tracing::trace!(target: "drift_rdp::actor", "leg 2 activated");
    });

    let bytes = buffer.lock().unwrap_or_else(PoisonError::into_inner).clone();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
fn rust_log_trace_still_hides_the_credential_targets() {
    let out = capture("trace");
    assert!(!out.contains("SECRET-CREDENTIAL"), "credentials survived RUST_LOG=trace:\n{out}");
    assert!(!out.contains("write_ts_credentials"), "the instrumented span is off too:\n{out}");
    assert!(out.contains("leg 2 activated"), "Drift's own trace output is kept:\n{out}");
    assert!(out.contains("NTLM failed"), "warnings from the capped target are kept:\n{out}");
}

#[test]
fn an_explicit_rust_log_directive_cannot_re_enable_them() {
    for rust_log in ["sspi=trace", "info,sspi=debug", "sspi::credssp=trace,sspi=trace"] {
        let out = capture(rust_log);
        assert!(!out.contains("SECRET-CREDENTIAL"), "RUST_LOG={rust_log} re-enabled them:\n{out}");
    }
}
