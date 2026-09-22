//! RDSTLS client authentication ([MS-RDPBCGR] 5.4.5.3).
//!
//! When the server selects `PROTOCOL_RDSTLS` in the X.224 Connection Confirm, the client performs
//! the TLS handshake and then, still before the MCS Connect Initial, exchanges three RDSTLS PDUs
//! over the TLS channel ([MS-RDPBCGR] 2.2.17):
//!
//! 1. the server sends its capabilities (`RDSTLS_TYPE_CAPABILITIES`, 8 bytes);
//! 2. the client sends an authentication request carrying the redirection GUID and the one-time
//!    credentials received in a Server Redirection PDU (`RDSTLS_TYPE_AUTHREQ`);
//! 3. the server answers with an authentication response holding a result code
//!    (`RDSTLS_TYPE_AUTHRSP`, 10 bytes).
//!
//! [`ClientConnector`](crate::ClientConnector) drives this exchange itself through
//! [`ClientConnectorState::RdstlsCapabilities`](crate::ClientConnectorState::RdstlsCapabilities)
//! and [`ClientConnectorState::RdstlsAuthResponse`](crate::ClientConnectorState::RdstlsAuthResponse)
//! once [`ClientConnector::with_rdstls_credentials`](crate::ClientConnector::with_rdstls_credentials)
//! has been set.

use core::fmt;

use bitflags::bitflags;
use ironrdp_core::{
    Decode, DecodeResult, Encode, EncodeResult, ReadCursor, WriteCursor, cast_length, ensure_fixed_part_size,
    ensure_size, invalid_field_err,
};
use ironrdp_pdu::PduHint;
use ironrdp_pdu::rdp::server_redirection::ServerRedirectionPdu;

const RDSTLS_VERSION_1: u16 = 0x0001;

const RDSTLS_TYPE_CAPABILITIES: u16 = 0x0001;
const RDSTLS_TYPE_AUTHREQ: u16 = 0x0002;
const RDSTLS_TYPE_AUTHRSP: u16 = 0x0004;

const RDSTLS_DATA_CAPABILITIES: u16 = 0x0001;
const RDSTLS_DATA_PASSWORD_CREDS: u16 = 0x0001;
const RDSTLS_DATA_RESULT_CODE: u16 = 0x0001;

const HEADER_SIZE: usize = 2 /* version */ + 2 /* pduType */ + 2 /* dataType */;

bitflags! {
    /// `supportedVersions` of the RDSTLS capabilities PDU.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct RdstlsVersions: u16 {
        /// `RDSTLS_VERSION_1`.
        const V1 = 0x0001;
        /// `RDSTLS_VERSION_2`.
        const V2 = 0x0002;

        const _ = !0;
    }
}

/// RDSTLS Capabilities PDU, sent by the server ([MS-RDPBCGR] 2.2.17.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RdstlsCapabilities {
    /// Versions of RDSTLS the server supports.
    pub supported_versions: RdstlsVersions,
}

impl RdstlsCapabilities {
    const NAME: &'static str = "RdstlsCapabilities";

    /// Size of the PDU on the wire.
    pub const FIXED_PART_SIZE: usize = HEADER_SIZE + 2 /* supportedVersions */;
}

impl Encode for RdstlsCapabilities {
    fn encode(&self, dst: &mut WriteCursor<'_>) -> EncodeResult<()> {
        ensure_fixed_part_size!(in: dst);

        dst.write_u16(RDSTLS_VERSION_1);
        dst.write_u16(RDSTLS_TYPE_CAPABILITIES);
        dst.write_u16(RDSTLS_DATA_CAPABILITIES);
        dst.write_u16(self.supported_versions.bits());

        Ok(())
    }

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn size(&self) -> usize {
        Self::FIXED_PART_SIZE
    }
}

impl<'de> Decode<'de> for RdstlsCapabilities {
    fn decode(src: &mut ReadCursor<'de>) -> DecodeResult<Self> {
        ensure_fixed_part_size!(in: src);

        let _version = src.read_u16();
        if src.read_u16() != RDSTLS_TYPE_CAPABILITIES {
            return Err(invalid_field_err!("pduType", "not RDSTLS_TYPE_CAPABILITIES", in: src));
        }
        if src.read_u16() != RDSTLS_DATA_CAPABILITIES {
            return Err(invalid_field_err!("dataType", "not RDSTLS_DATA_CAPABILITIES", in: src));
        }
        let supported_versions = RdstlsVersions::from_bits_retain(src.read_u16());

        Ok(Self { supported_versions })
    }
}

/// One-time credentials for RDSTLS password authentication.
///
/// They come from a Server Redirection PDU (see [`Self::from_server_redirection`]). They are
/// single-use: never persist them. The password is redacted from the `Debug` output, and every
/// field is zeroized on drop.
#[derive(Clone, PartialEq, Eq)]
pub struct RdstlsCredentials {
    /// `RedirectionGuid` of the Server Redirection PDU, verbatim.
    pub redirection_guid: Vec<u8>,
    /// User name of the Server Redirection PDU.
    pub username: String,
    /// Domain of the Server Redirection PDU (empty when absent).
    pub domain: String,
    /// `Password` of the Server Redirection PDU: an opaque blob, forwarded verbatim.
    pub password: Vec<u8>,
}

impl RdstlsCredentials {
    /// Extracts the RDSTLS credentials of a Server Redirection PDU.
    ///
    /// Returns `None` unless the PDU carries a redirection GUID, a user name and a password.
    pub fn from_server_redirection(redirection: &ServerRedirectionPdu) -> Option<Self> {
        Some(Self {
            redirection_guid: redirection.redirection_guid.clone()?,
            username: redirection.username.clone()?,
            domain: redirection.domain.clone().unwrap_or_default(),
            password: redirection.password.clone()?,
        })
    }
}

impl Drop for RdstlsCredentials {
    /// One-time credentials are secret: wipe them from memory when they are dropped.
    fn drop(&mut self) {
        use zeroize::Zeroize as _;

        self.redirection_guid.zeroize();
        self.username.zeroize();
        self.domain.zeroize();
        self.password.zeroize();
    }
}

impl fmt::Debug for RdstlsCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RdstlsCredentials")
            .field("redirection_guid_len", &self.redirection_guid.len())
            .field("username", &self.username)
            .field("domain", &self.domain)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// RDSTLS Authentication Request PDU with Password Credentials, sent by the client
/// ([MS-RDPBCGR] 2.2.17.2).
///
/// Strings are encoded as null-terminated UTF-16LE with a `u16` byte length; an empty domain is
/// therefore `02 00 00 00`.
#[derive(Clone, PartialEq, Eq)]
pub struct RdstlsAuthRequest<'a> {
    credentials: &'a RdstlsCredentials,
}

impl<'a> From<&'a RdstlsCredentials> for RdstlsAuthRequest<'a> {
    fn from(credentials: &'a RdstlsCredentials) -> Self {
        Self { credentials }
    }
}

impl fmt::Debug for RdstlsAuthRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RdstlsAuthRequest")
            .field("credentials", self.credentials)
            .finish()
    }
}

impl RdstlsAuthRequest<'_> {
    const NAME: &'static str = "RdstlsAuthRequest";

    fn utf16_size(s: &str) -> usize {
        (s.encode_utf16().count() + 1/* null terminator */) * 2
    }
}

impl Encode for RdstlsAuthRequest<'_> {
    fn encode(&self, dst: &mut WriteCursor<'_>) -> EncodeResult<()> {
        ensure_size!(in: dst, size: self.size());

        let RdstlsCredentials {
            redirection_guid,
            username,
            domain,
            password,
        } = self.credentials;

        dst.write_u16(RDSTLS_VERSION_1);
        dst.write_u16(RDSTLS_TYPE_AUTHREQ);
        dst.write_u16(RDSTLS_DATA_PASSWORD_CREDS);

        dst.write_u16(cast_length!("redirectionGuidLength", redirection_guid.len(), in: dst)?);
        dst.write_slice(redirection_guid);

        for s in [username, domain] {
            dst.write_u16(cast_length!("stringLength", Self::utf16_size(s), in: dst)?);
            for unit in s.encode_utf16() {
                dst.write_u16(unit);
            }
            dst.write_u16(0);
        }

        dst.write_u16(cast_length!("passwordLength", password.len(), in: dst)?);
        dst.write_slice(password);

        Ok(())
    }

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn size(&self) -> usize {
        HEADER_SIZE
            + 2 /* redirectionGuidLength */ + self.credentials.redirection_guid.len()
            + 2 /* userNameLength */ + Self::utf16_size(&self.credentials.username)
            + 2 /* domainLength */ + Self::utf16_size(&self.credentials.domain)
            + 2 /* passwordLength */ + self.credentials.password.len()
    }
}

/// `resultCode` of an RDSTLS Authentication Response ([MS-RDPBCGR] 2.2.17.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RdstlsResultCode(pub u32);

impl RdstlsResultCode {
    /// `RDSTLS_RESULT_SUCCESS`.
    pub const SUCCESS: Self = Self(0x0000_0000);
    /// `RDSTLS_RESULT_ACCESS_DENIED`.
    pub const ACCESS_DENIED: Self = Self(0x0000_0005);
    /// `RDSTLS_RESULT_LOGON_FAILURE`: wrong or already used credentials.
    pub const LOGON_FAILURE: Self = Self(0x0000_052E);
    /// `RDSTLS_RESULT_INVALID_LOGON_HOURS`.
    pub const INVALID_LOGON_HOURS: Self = Self(0x0000_0530);
    /// `RDSTLS_RESULT_PASSWORD_EXPIRED`.
    pub const PASSWORD_EXPIRED: Self = Self(0x0000_0532);
    /// `RDSTLS_RESULT_ACCOUNT_DISABLED`.
    pub const ACCOUNT_DISABLED: Self = Self(0x0000_0533);
    /// `RDSTLS_RESULT_PASSWORD_MUST_CHANGE`.
    pub const PASSWORD_MUST_CHANGE: Self = Self(0x0000_0773);
    /// `RDSTLS_RESULT_ACCOUNT_LOCKED_OUT`.
    pub const ACCOUNT_LOCKED_OUT: Self = Self(0x0000_0775);

    /// Whether the server accepted the credentials.
    pub fn is_success(self) -> bool {
        self == Self::SUCCESS
    }

    /// A human-readable description of the result code.
    pub fn description(self) -> &'static str {
        match self {
            Self::SUCCESS => "success",
            Self::ACCESS_DENIED => "access denied",
            Self::LOGON_FAILURE => "logon failure",
            Self::INVALID_LOGON_HOURS => "invalid logon hours",
            Self::PASSWORD_EXPIRED => "password expired",
            Self::ACCOUNT_DISABLED => "account disabled",
            Self::PASSWORD_MUST_CHANGE => "password must change",
            Self::ACCOUNT_LOCKED_OUT => "account locked out",
            _ => "unknown result code",
        }
    }
}

impl fmt::Display for RdstlsResultCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (0x{:08X})", self.description(), self.0)
    }
}

/// RDSTLS Authentication Response PDU, sent by the server ([MS-RDPBCGR] 2.2.17.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RdstlsAuthResponse {
    /// Outcome of the authentication.
    pub result_code: RdstlsResultCode,
}

impl RdstlsAuthResponse {
    const NAME: &'static str = "RdstlsAuthResponse";

    /// Size of the PDU on the wire.
    pub const FIXED_PART_SIZE: usize = HEADER_SIZE + 4 /* resultCode */;
}

impl Encode for RdstlsAuthResponse {
    fn encode(&self, dst: &mut WriteCursor<'_>) -> EncodeResult<()> {
        ensure_fixed_part_size!(in: dst);

        dst.write_u16(RDSTLS_VERSION_1);
        dst.write_u16(RDSTLS_TYPE_AUTHRSP);
        dst.write_u16(RDSTLS_DATA_RESULT_CODE);
        dst.write_u32(self.result_code.0);

        Ok(())
    }

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn size(&self) -> usize {
        Self::FIXED_PART_SIZE
    }
}

impl<'de> Decode<'de> for RdstlsAuthResponse {
    fn decode(src: &mut ReadCursor<'de>) -> DecodeResult<Self> {
        ensure_fixed_part_size!(in: src);

        let _version = src.read_u16();
        if src.read_u16() != RDSTLS_TYPE_AUTHRSP {
            return Err(invalid_field_err!("pduType", "not RDSTLS_TYPE_AUTHRSP", in: src));
        }
        if src.read_u16() != RDSTLS_DATA_RESULT_CODE {
            return Err(invalid_field_err!("dataType", "not RDSTLS_DATA_RESULT_CODE", in: src));
        }
        let result_code = RdstlsResultCode(src.read_u32());

        Ok(Self { result_code })
    }
}

/// [`PduHint`] for the fixed-size RDSTLS PDUs sent by the server.
#[derive(Debug, Clone, Copy)]
pub struct RdstlsHint(usize);

/// Hint for [`RdstlsCapabilities`].
pub const RDSTLS_CAPABILITIES_HINT: RdstlsHint = RdstlsHint(RdstlsCapabilities::FIXED_PART_SIZE);

/// Hint for [`RdstlsAuthResponse`].
pub const RDSTLS_AUTH_RESPONSE_HINT: RdstlsHint = RdstlsHint(RdstlsAuthResponse::FIXED_PART_SIZE);

impl PduHint for RdstlsHint {
    fn find_size(&self, _bytes: &[u8]) -> DecodeResult<Option<(bool, usize)>> {
        Ok(Some((true, self.0)))
    }
}
