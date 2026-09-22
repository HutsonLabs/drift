//! # drift-e2e
//!
//! Real-host tests (plan §5.3). Every test is `#[ignore]` and is run by
//! `cargo xtask e2e`, which opens `ssh -N -L 1339x:localhost:339x homelab@10.1.2.40`
//! and exports:
//!
//! | Variable | Meaning |
//! |---|---|
//! | `DRIFT_E2E_HOST` | `127.0.0.1` (the SSH forward) |
//! | `DRIFT_E2E_TLS_NAME` | `10.1.2.40` (certificate host) |
//! | `DRIFT_E2E_PORT_<remote>` | local forward port for remote port 3389..3392 |
//! | `DRIFT_E2E_SYS_USER/PASS` | Remote Login system RDP credentials |
//! | `DRIFT_E2E_LOGIN_USER/PASS` | Linux login `drifttest` |
//! | `DRIFT_E2E_HL_USER/PASS`, `DRIFT_E2E_HL_PORT` | headless daemon of `drifttest2` |
//!
//! Never print these values. [`init_logging`] installs a `tracing` subscriber whose output
//! goes through the [`Redactor`], which replaces every credential value with `<redacted>`.

use std::io::{self, Write};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use drift_core::{ConnectMode, ConnectionProfile, SessionState, SystemClock};
use drift_rdp::{SessionCommand, SessionEvent, SessionEvents, SessionHandle, SessionOptions, SessionSecrets};
use drift_testkit::{FrameLog, PresentMode, RecordingFrameSink};

/// Reads a `DRIFT_E2E_*` variable, returning `None` when unset or empty.
pub fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Reads a required `DRIFT_E2E_*` variable.
///
/// # Panics
/// With a message naming the variable (never its value) when it is missing.
pub fn require(name: &str) -> String {
    var(name).unwrap_or_else(|| panic!("{name} is not set; run the e2e suite through `cargo xtask e2e`"))
}

/// Local port that forwards to `remote_port` on the host (e.g. 3392 → 13392).
pub fn forwarded_port(remote_port: u16) -> Option<u16> {
    var(&format!("DRIFT_E2E_PORT_{remote_port}")).and_then(|p| p.parse().ok())
}

/// Names of the variables whose values are secrets (user names included: they identify accounts).
pub const SECRET_VARS: &[&str] = &[
    "DRIFT_E2E_SYS_USER",
    "DRIFT_E2E_SYS_PASS",
    "DRIFT_E2E_LOGIN_USER",
    "DRIFT_E2E_LOGIN_PASS",
    "DRIFT_E2E_HL_USER",
    "DRIFT_E2E_HL_PASS",
    "DRIFT_E2E_HL1_USER",
    "DRIFT_E2E_HL1_PASS",
    "DRIFT_E2E_SHARE_USER",
    "DRIFT_E2E_SHARE_PASS",
];

/// Replaces secret values in log text.
#[derive(Clone, Default)]
pub struct Redactor {
    secrets: Vec<String>,
}

impl std::fmt::Debug for Redactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redactor").field("secrets", &self.secrets.len()).finish()
    }
}

impl Redactor {
    /// Redacts the given values (empty values are ignored; longer values are replaced first so
    /// a secret containing another is fully removed).
    pub fn new(secrets: impl IntoIterator<Item = String>) -> Self {
        let mut secrets: Vec<String> = secrets.into_iter().filter(|s| !s.is_empty()).collect();
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
        secrets.dedup();
        Self { secrets }
    }

    /// Redacts the values of [`SECRET_VARS`] from the environment.
    pub fn from_env() -> Self {
        Self::new(SECRET_VARS.iter().filter_map(|v| var(v)))
    }

    /// `text` with every secret replaced by `<redacted>`.
    pub fn redact(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for secret in &self.secrets {
            out = out.replace(secret.as_str(), "<redacted>");
        }
        out
    }
}

/// An `io::Write` that buffers lines and writes them redacted to the inner writer.
#[derive(Debug)]
pub struct RedactingWriter<W: Write> {
    redactor: Redactor,
    inner: W,
    pending: Vec<u8>,
}

impl<W: Write> RedactingWriter<W> {
    /// Wraps `inner`.
    pub fn new(redactor: Redactor, inner: W) -> Self {
        Self { redactor, inner, pending: Vec::new() }
    }

    fn flush_pending(&mut self) -> io::Result<()> {
        if !self.pending.is_empty() {
            let text = String::from_utf8_lossy(&self.pending).into_owned();
            self.pending.clear();
            self.inner.write_all(self.redactor.redact(&text).as_bytes())?;
        }
        self.inner.flush()
    }
}

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);
        // Redact whole lines so a secret split across writes is still caught.
        if let Some(last_nl) = self.pending.iter().rposition(|b| *b == b'\n') {
            let rest = self.pending.split_off(last_nl + 1);
            let text = String::from_utf8_lossy(&self.pending).into_owned();
            self.pending = rest;
            self.inner.write_all(self.redactor.redact(&text).as_bytes())?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_pending()
    }
}

impl<W: Write> Drop for RedactingWriter<W> {
    fn drop(&mut self) {
        let _ = self.flush_pending();
    }
}

/// Installs a redacting `tracing` subscriber on stderr (once per process). `RUST_LOG` selects
/// the level (default `info`).
pub fn init_logging() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let redactor = Redactor::from_env();
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_ansi(false)
            .with_writer(move || RedactingWriter::new(redactor.clone(), io::stderr()))
            .try_init();
    });
}

/// A session against the real host, recording every event.
pub struct E2eSession {
    /// Command sender.
    pub handle: SessionHandle,
    events: SessionEvents,
    /// Everything received so far.
    pub seen: Arc<Mutex<Vec<SessionEvent>>>,
    /// Everything the GFX pipeline drew (the tab's `FrameSink` is a recording sink).
    pub frames: FrameLog,
}

impl std::fmt::Debug for E2eSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("E2eSession").finish_non_exhaustive()
    }
}

impl E2eSession {
    /// Starts a session to the SSH-forwarded `local_port` with the given mode and credentials.
    pub fn start(mode: ConnectMode, local_port: u16, user: &str, password: &str) -> Self {
        let host = var("DRIFT_E2E_HOST").unwrap_or_else(|| "127.0.0.1".into());
        let mut profile = ConnectionProfile::new("e2e", host, mode);
        profile.port = local_port;
        profile.rdp_username = user.to_owned();
        let options = SessionOptions {
            tls_server_name: var("DRIFT_E2E_TLS_NAME"),
            connect_timeout: Duration::from_secs(20),
            ..SessionOptions::default()
        };
        let (sink, frames) = RecordingFrameSink::new(PresentMode::Immediate);
        let (handle, events) = drift_rdp::spawn_session(
            profile,
            SessionSecrets::new(password),
            Box::new(sink),
            Arc::new(SystemClock),
            options,
        );
        Self { handle, events, seen: Arc::default(), frames }
    }

    /// Waits for an event matching `pred`, accepting (without pinning) any certificate prompt on
    /// the way. Panics with the state history on timeout or on a terminal state.
    pub async fn wait_for(
        &mut self,
        what: &str,
        within: Duration,
        pred: impl Fn(&SessionEvent) -> bool,
    ) -> SessionEvent {
        let deadline = tokio::time::Instant::now() + within;
        loop {
            let ev = match tokio::time::timeout_at(deadline, self.events.recv()).await {
                Ok(Some(ev)) => ev,
                Ok(None) => panic!("session ended before {what}; states {:?}", self.states()),
                Err(_) => panic!("timed out waiting for {what}; states {:?}", self.states()),
            };
            self.seen.lock().unwrap_or_else(PoisonError::into_inner).push(ev.clone());
            if pred(&ev) {
                return ev;
            }
            match &ev {
                SessionEvent::CertificatePrompt { fingerprint, .. } => {
                    eprintln!("[e2e] accepting certificate {fingerprint}");
                    let _ = self
                        .handle
                        .send(SessionCommand::AcceptCertificate { fingerprint: *fingerprint, pin: false });
                }
                SessionEvent::State(
                    s @ (SessionState::Failed { .. } | SessionState::Disconnected { .. }),
                ) => {
                    panic!("session ended ({s:?}) before {what}; states {:?}", self.states());
                }
                SessionEvent::State(s) => eprintln!("[e2e] state {s:?}"),
                _ => {}
            }
        }
    }

    /// Every `State` so far.
    pub fn states(&self) -> Vec<SessionState> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter_map(|e| match e {
                SessionEvent::State(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }

    /// Sends `Close` and waits (up to 10 s) for the final state.
    pub async fn close(mut self) -> Option<SessionState> {
        let _ = self.handle.send(SessionCommand::Close);
        let mut last = None;
        let _ = tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(ev) = self.events.recv().await {
                if let SessionEvent::State(s) = ev {
                    last = Some(s);
                }
            }
        })
        .await;
        last
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_variables_are_none() {
        assert_eq!(var("DRIFT_E2E_DEFINITELY_UNSET_VARIABLE"), None);
        assert_eq!(forwarded_port(1), None);
    }

    #[test]
    fn redactor_strips_every_secret() {
        let r = Redactor::new([
            "Fake9-pass".to_owned(),
            "fakeuser".to_owned(),
            String::new(),
            "Fake9".to_owned(),
        ]);
        assert_eq!(
            r.redact("user=fakeuser pw=Fake9-pass other=Fake9!"),
            "user=<redacted> pw=<redacted> other=<redacted>!"
        );
        assert_eq!(r.redact("nothing secret"), "nothing secret");
    }

    #[test]
    fn redacting_writer_catches_secrets_split_across_writes() {
        let r = Redactor::new(["Fake9-secret".to_owned()]);
        let mut out = Vec::new();
        {
            let mut w = RedactingWriter::new(r, &mut out);
            w.write_all(b"line one Fake9-se").unwrap();
            w.write_all(b"cret end\nsecond Fake9-secret").unwrap();
        }
        let out = String::from_utf8(out).unwrap();
        assert_eq!(out, "line one <redacted> end\nsecond <redacted>");
    }

    #[test]
    fn logging_through_the_redact_layer_never_leaks() {
        // A subscriber writing into a buffer through the same writer `init_logging` uses.
        #[derive(Clone, Default)]
        struct Buf(Arc<Mutex<Vec<u8>>>);
        impl Write for Buf {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap_or_else(PoisonError::into_inner).extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let buf = Buf::default();
        let redactor = Redactor::new(["Fake9-login-pw".to_owned(), "fake-sys-user".to_owned()]);
        let sink = buf.clone();
        let guard = tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .with_ansi(false)
                .with_writer(move || RedactingWriter::new(redactor.clone(), sink.clone()))
                .finish(),
        );
        tracing::info!(user = "fake-sys-user", "typing Fake9-login-pw into the greeter");
        drop(guard);
        let out = String::from_utf8(buf.0.lock().unwrap_or_else(PoisonError::into_inner).clone()).unwrap();
        assert!(out.contains("typing <redacted> into the greeter"), "{out}");
        assert!(!out.contains("Fake9-login-pw") && !out.contains("fake-sys-user"), "{out}");
    }
}
