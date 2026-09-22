//! Connection establishment for one leg (task **M1-1**): TCP, X.224, TLS with TOFU / pinning
//! on the SHA-256 of the leaf certificate, NLA (CredSSP) or RDSTLS, then capability exchange
//! and finalization.
//!
//! The pure decisions live here as small functions with table tests:
//! [`check_certificate`], [`build_config`], [`client_name_from_hostname`],
//! [`classify_io_error`] and [`classify_connector_error`]. [`connect_leg`] is the thin async
//! driver around IronRDP's `ClientConnector`; every timer goes through the injected
//! [`Clock`] (a `ManualClock` in tests), and the TCP dial goes through a [`Dialer`] so tests
//! can inject errors such as `EHOSTUNREACH`.

use std::future::Future;
use std::io;
use std::time::Duration;

use drift_core::{CertFingerprint, Clock, ConnectMode, ConnectStage, DesktopSize, DisconnectReason};
use ironrdp_connector::{ClientConnector, ConnectionResult, ConnectorError};
use tokio::net::TcpStream;
use zeroize::Zeroizing;

use crate::rdstls::OneTimeCredentials;
use crate::session::CertificateRole;

/// The TLS-upgraded transport of a connected leg.
pub type UpgradedFramed = ironrdp_tokio::TokioFramed<tokio_rustls::client::TlsStream<TcpStream>>;

/// Desktop size requested when the view geometry is not known yet (plan §1.4 captures).
pub const DEFAULT_DESKTOP: DesktopSize = DesktopSize { width: 1280, height: 800 };

/// Maximum RDP client name length in characters (GCC `clientName`, 16 UTF-16 units with NUL).
pub const MAX_CLIENT_NAME_CHARS: usize = 15;

/// How a leg authenticates.
pub enum LegAuth {
    /// NLA (CredSSP/NTLM) with long-lived credentials: Headless, Desktop Sharing, Remote Login leg 1.
    Nla {
        /// RDP user name.
        username: String,
        /// RDP password.
        password: Zeroizing<String>,
    },
    /// RDSTLS with the one-time credentials of a Server Redirection PDU (Remote Login legs ≥ 2).
    Rdstls(OneTimeCredentials),
}

impl std::fmt::Debug for LegAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nla { username, .. } => {
                f.debug_struct("Nla").field("username", username).field("password", &"<redacted>").finish()
            }
            Self::Rdstls(c) => f.debug_tuple("Rdstls").field(c).finish(),
        }
    }
}

/// What the leaf certificate of a leg must be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CertExpectation {
    /// The profile's TOFU pin (SHA-256 of the leaf DER).
    Pinned(CertFingerprint),
    /// No pin yet: an unknown certificate is shown to the user (TOFU).
    TrustOnFirstUse,
    /// A Server Redirection target: the leaf DER must equal the certificate from the
    /// redirection PDU's target-certificate container (plan §1.3, M3-1).
    Exact(Vec<u8>),
}

/// Outcome of [`check_certificate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertVerdict {
    /// The certificate is the expected one.
    Trusted,
    /// The certificate differs from the pin / redirect target: abort with `CertMismatch`.
    Mismatch,
    /// Unknown certificate: ask the user.
    Unknown(CertFingerprint),
}

/// Checks a leaf certificate against the expectation. Pure.
pub fn check_certificate(expectation: &CertExpectation, leaf_der: &[u8]) -> CertVerdict {
    let _ = (expectation, leaf_der);
    CertVerdict::Mismatch
}

/// Everything needed to connect one leg.
#[derive(Debug)]
pub struct LegRequest {
    /// Connection leg (1 = initial, ≥ 2 after a redirect).
    pub leg: u8,
    /// Profile mode.
    pub mode: ConnectMode,
    /// Host to dial.
    pub host: String,
    /// Port to dial.
    pub port: u16,
    /// TLS server name / host shown in prompts (e.g. `10.1.2.40` behind an SSH forward).
    pub tls_server_name: String,
    /// Authentication.
    pub auth: LegAuth,
    /// Certificate expectation.
    pub expectation: CertExpectation,
    /// Which certificate a prompt is about.
    pub role: CertificateRole,
    /// Routing token from the redirection PDU (`Cookie: msts=<n>`, without CRLF).
    pub routing_token: Option<String>,
    /// RDP client name (the Mac's host name).
    pub client_name: String,
    /// Requested desktop size.
    pub desktop: DesktopSize,
    /// `DesktopScaleFactor` in percent.
    pub scale: u32,
}

/// The user's answer to a certificate prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertDecision {
    /// Trust it; `pin = true` persists it as the profile pin.
    Accept {
        /// Persist the pin.
        pin: bool,
    },
    /// Reject (ends in `CertMismatch`).
    Reject,
    /// The session was closed while prompting (ends in `UserClosed`).
    Closed,
}

/// Receives progress and answers certificate prompts while a leg connects.
pub trait ConnectObserver: Send {
    /// A new stage of the leg started.
    fn stage(&mut self, stage: ConnectStage);

    /// An unknown certificate needs a decision. Connection timers are paused meanwhile.
    fn decide_certificate(
        &mut self,
        host: &str,
        port: u16,
        fingerprint: CertFingerprint,
        role: CertificateRole,
    ) -> impl Future<Output = CertDecision> + Send;
}

/// Opens the TCP connection of a leg (injectable for tests).
pub trait Dialer: Send + Sync {
    /// Connects to `host:port`.
    fn dial(&self, host: &str, port: u16) -> impl Future<Output = io::Result<TcpStream>> + Send;
}

/// The real dialer (`tokio::net::TcpStream::connect`, `TCP_NODELAY`).
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioDialer;

impl Dialer for TokioDialer {
    async fn dial(&self, host: &str, port: u16) -> io::Result<TcpStream> {
        let stream = TcpStream::connect((host, port)).await?;
        stream.set_nodelay(true)?;
        Ok(stream)
    }
}

/// A connected, activated leg.
pub struct ConnectedLeg {
    /// The TLS transport, ready for the active stage.
    pub framed: UpgradedFramed,
    /// IronRDP's connection result (channels, share id, desktop size).
    pub result: ConnectionResult,
    /// The server's leaf certificate DER.
    pub leaf_der: Vec<u8>,
}

impl std::fmt::Debug for ConnectedLeg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectedLeg")
            .field("desktop_size", &self.result.desktop_size)
            .field("leaf", &CertFingerprint::of_der(&self.leaf_der).to_string())
            .finish_non_exhaustive()
    }
}

/// Builds IronRDP's connector configuration for a leg. Pure.
///
/// Fixed by plan §1.3/M1-1: TLS on, NLA for [`LegAuth::Nla`] (RDSTLS legs turn NLA off,
/// set `autologon` and send an empty password), `support_dyn_vc_gfx_protocol = true`,
/// platform `MACINTOSH`, client name as given.
pub fn build_config(req: &LegRequest) -> ironrdp_connector::Config {
    let _ = req;
    todo!("M1-1")
}

/// Derives an RDP client name from a host name: the first DNS label, `[A-Za-z0-9-]` only,
/// at most [`MAX_CLIENT_NAME_CHARS`] characters, `"drift"` when nothing is left. Pure.
pub fn client_name_from_hostname(hostname: &str) -> String {
    let _ = hostname;
    String::new()
}

/// The Mac's host name as an RDP client name.
pub fn local_client_name() -> String {
    client_name_from_hostname("")
}

/// Maps an I/O error at `stage` to a disconnect reason. Pure.
///
/// `EHOSTUNREACH` (errno 65) is how macOS Local Network Privacy denies a connection
/// (plan §1.8) → [`DisconnectReason::LocalNetworkDenied`].
pub fn classify_io_error(error: &io::Error, stage: ConnectStage) -> DisconnectReason {
    let _ = (error, stage);
    DisconnectReason::ProtocolError("unclassified".into())
}

/// Maps an IronRDP connector error at `stage` to a disconnect reason. Pure.
pub fn classify_connector_error(error: &ConnectorError, stage: ConnectStage) -> DisconnectReason {
    let _ = (error, stage);
    DisconnectReason::ProtocolError("unclassified".into())
}

/// Connects one leg: TCP → X.224 → TLS (certificate check, prompt if unknown) →
/// NLA or RDSTLS → capability exchange → finalization.
///
/// `attach` adds static channels (DRDYNVC with GFX/DISP, CLIPRDR) to the connector.
/// `timeout` bounds each network phase on `clock`; it is paused while the user decides on a
/// certificate.
pub async fn connect_leg(
    req: LegRequest,
    dialer: &impl Dialer,
    clock: &dyn Clock,
    timeout: Duration,
    observer: &mut impl ConnectObserver,
    attach: impl FnOnce(ClientConnector) -> ClientConnector + Send,
) -> Result<ConnectedLeg, DisconnectReason> {
    let _ = (req, dialer, clock, timeout, observer, attach);
    Err(DisconnectReason::ProtocolError("connect_leg: not implemented".into()))
}
