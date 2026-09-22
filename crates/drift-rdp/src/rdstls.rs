//! RDSTLS client glue around the IronRDP fork's `rdstls` module (task **M3-1**).
//!
//! The one-time credentials of a Server Redirection PDU (plan §1.3) are single-use: they live
//! only in memory, in [`OneTimeCredentials`], whose fields are zeroized on drop. They are
//! handed to IronRDP's connector as `RdstlsCredentials` right before the leg connects (the
//! fork zeroizes that copy on drop too) and are never persisted or logged.

use ironrdp_connector::rdstls::RdstlsCredentials;
use ironrdp_pdu::rdp::server_redirection::ServerRedirectionPdu;
use zeroize::Zeroizing;

/// One-time RDSTLS credentials from a Server Redirection PDU. `Debug` is redacted.
pub struct OneTimeCredentials {
    redirection_guid: Zeroizing<Vec<u8>>,
    username: Zeroizing<String>,
    domain: Zeroizing<String>,
    password: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for OneTimeCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OneTimeCredentials")
            .field("guid_len", &self.redirection_guid.len())
            .field("username", &"<redacted>")
            .field("password_len", &self.password.len())
            .finish_non_exhaustive()
    }
}

impl OneTimeCredentials {
    /// Extracts the credentials from a redirection PDU. `None` unless the PDU carries a GUID,
    /// a user name and a password.
    pub fn from_redirection(pdu: &ServerRedirectionPdu) -> Option<Self> {
        Some(Self {
            redirection_guid: Zeroizing::new(pdu.redirection_guid.clone()?),
            username: Zeroizing::new(pdu.username.clone()?),
            domain: Zeroizing::new(pdu.domain.clone().unwrap_or_default()),
            password: Zeroizing::new(pdu.password.clone()?),
        })
    }

    /// The one-time user name (also used as the Client Info user name with `INFO_AUTOLOGON`).
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Builds the connector's credential copy, consuming (and zeroizing) `self`.
    pub fn into_connector(self) -> RdstlsCredentials {
        RdstlsCredentials {
            redirection_guid: self.redirection_guid.to_vec(),
            username: self.username.to_string(),
            domain: self.domain.to_string(),
            password: self.password.to_vec(),
        }
    }
}

/// The RDSTLS result code of a failed connection attempt, if that is what failed.
pub fn rdstls_failure(error: &ironrdp_connector::ConnectorError) -> Option<u32> {
    match error.kind() {
        ironrdp_connector::ConnectorErrorKind::RdstlsAuthFailed(code) => Some(code.0),
        _ => None,
    }
}
