//! Server Redirection loop (task **M3-1**): leg sequencing, routing token, target certificate
//! and loop protection. Pure; the session actor drives it.
//!
//! Remote Login (plan §1.3): leg 1 authenticates with NLA and receives a Server Redirection
//! PDU about 2.4 s after activation; leg 2 reconnects to the **same host and port** (or
//! `TargetNetAddress` when present) with the routing token and `SSL|RDSTLS` and shows the GDM
//! greeter; after the greeter login a second redirection (same token, new one-time
//! credentials) leads to leg 3, the user session.
//!
//! | Event | Remote Login | Headless / Desktop Sharing |
//! |---|---|---|
//! | leg 1 active | stay `Connecting{1}` (redirect expected) | `Connected` |
//! | leg 2 active | `AwaitingGreeterLogin` | `Connected` |
//! | leg ≥ 3 active | `Connected` | `Connected` |
//! | redirect #5 | `RedirectLoop` | `RedirectLoop` |

use drift_core::state::MAX_REDIRECTS;
use drift_core::{ConnectMode, DesktopSize, DisconnectReason, SessionState};
use ironrdp_pdu::rdp::server_redirection::ServerRedirectionPdu;
use zeroize::Zeroize as _;

use crate::rdstls::OneTimeCredentials;

/// Where and how to connect the next leg after a redirection.
#[derive(Debug)]
pub struct NextLeg {
    /// The new leg number (≥ 2).
    pub leg: u8,
    /// Host to dial.
    pub host: String,
    /// Port to dial.
    pub port: u16,
    /// X.224 routing token, `Cookie: msts=<n>` without the CRLF.
    pub routing_token: Option<String>,
    /// One-time RDSTLS credentials.
    pub credentials: OneTimeCredentials,
    /// DER of the target certificate from the container; the next leaf must equal it.
    pub target_certificate: Option<Vec<u8>>,
}

/// Redirect bookkeeping for one connection attempt.
#[derive(Debug, Clone)]
pub struct RedirectLoop {
    mode: ConnectMode,
    host: String,
    port: u16,
    leg: u8,
}

impl RedirectLoop {
    /// Starts at leg 1 for `host:port`.
    pub fn new(mode: ConnectMode, host: impl Into<String>, port: u16) -> Self {
        Self { mode, host: host.into(), port, leg: 1 }
    }

    /// The current leg (1-based).
    pub fn leg(&self) -> u8 {
        self.leg
    }

    /// The state to enter when the current leg is activated, if any (see module docs).
    pub fn on_activated(&self, desktop: DesktopSize, scale: u32) -> Option<SessionState> {
        match (self.mode, self.leg) {
            (ConnectMode::RemoteLogin, 1) => None,
            (ConnectMode::RemoteLogin, 2) => Some(SessionState::AwaitingGreeterLogin),
            _ => Some(SessionState::Connected { desktop, scale }),
        }
    }

    /// Handles a Server Redirection PDU: validates it, advances the leg and returns the next
    /// leg's parameters. The PDU's secret fields are zeroized and cleared.
    ///
    /// Errors: [`DisconnectReason::RedirectLoop`] beyond [`MAX_REDIRECTS`];
    /// [`DisconnectReason::ProtocolError`] when credentials are missing or the target
    /// certificate container is malformed.
    pub fn on_redirect(&mut self, pdu: &mut ServerRedirectionPdu) -> Result<NextLeg, DisconnectReason> {
        let result = self.next_leg(pdu);
        // The one-time secrets now live only in `NextLeg::credentials` (zeroized on drop).
        pdu.password.zeroize();
        pdu.username.zeroize();
        pdu.redirection_guid.zeroize();
        pdu.domain.zeroize();
        result
    }

    fn next_leg(&mut self, pdu: &ServerRedirectionPdu) -> Result<NextLeg, DisconnectReason> {
        let redirects_so_far = self.leg - 1;
        if redirects_so_far >= MAX_REDIRECTS {
            return Err(DisconnectReason::RedirectLoop);
        }
        let credentials = OneTimeCredentials::from_redirection(pdu).ok_or_else(|| {
            DisconnectReason::ProtocolError("Server Redirection PDU without one-time credentials".into())
        })?;
        let target_certificate = pdu
            .decode_target_certificate()
            .map_err(|e| {
                DisconnectReason::ProtocolError(format!("malformed redirect target certificate: {e}"))
            })?
            .and_then(|c| c.der_certificate().map(<[u8]>::to_vec));
        if let Some(address) = pdu.target_net_address.as_deref().map(|a| a.trim_end_matches('\0'))
            && !address.is_empty()
        {
            self.host = address.to_owned();
        }
        self.leg += 1;
        Ok(NextLeg {
            leg: self.leg,
            host: self.host.clone(),
            port: self.port,
            routing_token: pdu.load_balance_info.as_deref().and_then(routing_token),
            credentials,
            target_certificate,
        })
    }
}

/// The routing token of a `LoadBalanceInfo` blob: ASCII `Cookie: msts=<n>\r\n` → `Cookie: msts=<n>`.
///
/// `None` for an empty or non-printable-ASCII blob (IronRDP sends the token verbatim plus CRLF).
pub fn routing_token(load_balance_info: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(load_balance_info).ok()?;
    let token = text.trim_end_matches(['\r', '\n', '\0']);
    (!token.is_empty() && token.bytes().all(|b| b.is_ascii_graphic() || b == b' ')).then(|| token.to_owned())
}

/// Verifies the leaf certificate of a redirected leg against the target certificate.
pub fn verify_target_certificate(expected_der: &[u8], leaf_der: &[u8]) -> Result<(), DisconnectReason> {
    if expected_der == leaf_der { Ok(()) } else { Err(DisconnectReason::CertMismatch) }
}
