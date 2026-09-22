//! Server Redirection PDU ([MS-RDPBCGR] 2.2.13).
//!
//! A server (or connection broker) sends this PDU to tell the client to disconnect and reconnect,
//! optionally to another target, presenting a routing token (load-balance info) and, when the
//! server asks for it, one-time credentials that the client forwards over RDSTLS
//! ([MS-RDPBCGR] 5.4.5.3).

use core::fmt;

use bitflags::bitflags;
use ironrdp_core::{
    Decode, DecodeResult, Encode, EncodeResult, ReadCursor, WriteCursor, cast_length, ensure_size, invalid_field_err,
    read_padding, write_padding,
};

bitflags! {
    /// `redirFlags` of the `RDP_SERVER_REDIRECTION_PACKET` ([MS-RDPBCGR] 2.2.13.1).
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
    pub struct ServerRedirectionFlags: u32 {
        /// `LB_TARGET_NET_ADDRESS`: `TargetNetAddress` is present.
        const TARGET_NET_ADDRESS = 0x0000_0001;
        /// `LB_LOAD_BALANCE_INFO`: `LoadBalanceInfo` is present.
        const LOAD_BALANCE_INFO = 0x0000_0002;
        /// `LB_USERNAME`: `UserName` is present.
        const USERNAME = 0x0000_0004;
        /// `LB_DOMAIN`: `Domain` is present.
        const DOMAIN = 0x0000_0008;
        /// `LB_PASSWORD`: `Password` is present.
        const PASSWORD = 0x0000_0010;
        /// `LB_DONTSTOREUSERNAME`: the client must not store the user name.
        const DONT_STORE_USERNAME = 0x0000_0020;
        /// `LB_SMARTCARD_LOGON`: the user logged on with a smart card.
        const SMARTCARD_LOGON = 0x0000_0040;
        /// `LB_NOREDIRECT`: informational only, the client must not reconnect.
        const NO_REDIRECT = 0x0000_0080;
        /// `LB_TARGET_FQDN`: `TargetFQDN` is present.
        const TARGET_FQDN = 0x0000_0100;
        /// `LB_TARGET_NETBIOS_NAME`: `TargetNetBiosName` is present.
        const TARGET_NETBIOS_NAME = 0x0000_0200;
        /// `LB_TARGET_NET_ADDRESSES`: `TargetNetAddresses` is present.
        const TARGET_NET_ADDRESSES = 0x0000_0800;
        /// `LB_CLIENT_TSV_URL`: `TsvUrl` is present.
        const CLIENT_TSV_URL = 0x0000_1000;
        /// `LB_SERVER_TSV_CAPABLE`: the server supports redirection based on `TsvUrl`.
        const SERVER_TSV_CAPABLE = 0x0000_2000;
        /// `LB_PASSWORD_IS_PK_ENCRYPTED`: `Password` is an opaque blob to forward verbatim.
        const PASSWORD_IS_PK_ENCRYPTED = 0x0000_4000;
        /// `LB_REDIRECTION_GUID`: `RedirectionGuid` is present.
        const REDIRECTION_GUID = 0x0000_8000;
        /// `LB_TARGET_CERTIFICATE`: `TargetCertificate` is present.
        const TARGET_CERTIFICATE = 0x0001_0000;

        const _ = !0;
    }
}

/// Enhanced Security Server Redirection PDU body ([MS-RDPBCGR] 2.2.13.3.1).
///
/// This is what follows the Share Control Header (`pduType` = `PDUTYPE_SERVER_REDIR_PKT`, 0xA,
/// which carries no `shareId`): `pad2Octets` and an `RDP_SERVER_REDIRECTION_PACKET`
/// ([MS-RDPBCGR] 2.2.13.1). Every optional field is `Some` exactly when its bit is set in
/// [`Self::redirection_flags`].
///
/// Strings are carried on the wire as null-terminated UTF-16LE; binary fields are kept verbatim.
/// [`Self::password`] holds secret material: it is redacted from the `Debug` output and callers
/// should zeroize it after use.
#[derive(Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ServerRedirectionPdu {
    /// `SessionID`: the session identifier to reconnect to.
    pub session_id: u32,
    /// `RedirFlags`, retained verbatim (unknown bits included).
    pub redirection_flags: ServerRedirectionFlags,
    /// `TargetNetAddress` (`LB_TARGET_NET_ADDRESS`).
    pub target_net_address: Option<String>,
    /// `LoadBalanceInfo` (`LB_LOAD_BALANCE_INFO`), e.g. `Cookie: msts=<token>\r\n`.
    pub load_balance_info: Option<Vec<u8>>,
    /// `UserName` (`LB_USERNAME`).
    pub username: Option<String>,
    /// `Domain` (`LB_DOMAIN`).
    pub domain: Option<String>,
    /// `Password` (`LB_PASSWORD`), opaque; forward it verbatim.
    pub password: Option<Vec<u8>>,
    /// `TargetFQDN` (`LB_TARGET_FQDN`).
    pub target_fqdn: Option<String>,
    /// `TargetNetBiosName` (`LB_TARGET_NETBIOS_NAME`).
    pub target_netbios_name: Option<String>,
    /// `TsvUrl` (`LB_CLIENT_TSV_URL`).
    pub tsv_url: Option<Vec<u8>>,
    /// `TargetNetAddresses` (`LB_TARGET_NET_ADDRESSES`).
    pub target_net_addresses: Option<Vec<String>>,
    /// `RedirectionGuid` (`LB_REDIRECTION_GUID`), verbatim (UTF-16LE base64 in practice).
    pub redirection_guid: Option<Vec<u8>>,
    /// `TargetCertificate` (`LB_TARGET_CERTIFICATE`), verbatim wire bytes.
    ///
    /// Use [`Self::decode_target_certificate`] to parse it.
    pub target_certificate: Option<Vec<u8>>,
}

impl ServerRedirectionPdu {
    const NAME: &'static str = "ServerRedirectionPdu";

    /// `SEC_REDIRECTION_PKT`, the mandatory value of the packet's `Flags` field.
    const SEC_REDIRECTION_PKT: u16 = 0x0400;

    const PACKET_FIXED_PART_SIZE: usize = 2 /* Flags */ + 2 /* Length */ + 4 /* SessionID */ + 4 /* RedirFlags */;
    const FIXED_PART_SIZE: usize = 2 /* pad2Octets */ + Self::PACKET_FIXED_PART_SIZE;
    const PAD_SIZE: usize = 8;

    /// Parses [`Self::target_certificate`] into its certificate container.
    ///
    /// Returns `Ok(None)` when the PDU has no target certificate.
    pub fn decode_target_certificate(&self) -> DecodeResult<Option<TargetCertificateContainer>> {
        self.target_certificate
            .as_deref()
            .map(TargetCertificateContainer::decode_wire)
            .transpose()
    }

    fn fields_size(&self) -> usize {
        let string = |s: &Option<String>| s.as_deref().map_or(0, |s| 4 /* length */ + utf16_len(s));
        let blob = |b: &Option<Vec<u8>>| b.as_deref().map_or(0, |b| 4 /* length */ + b.len());

        string(&self.target_net_address)
            + blob(&self.load_balance_info)
            + string(&self.username)
            + string(&self.domain)
            + blob(&self.password)
            + string(&self.target_fqdn)
            + string(&self.target_netbios_name)
            + blob(&self.tsv_url)
            + blob(&self.redirection_guid)
            + blob(&self.target_certificate)
            + self.target_net_addresses.as_deref().map_or(0, |addresses| {
                4 /* length */ + 4 /* count */ + addresses.iter().map(|a| 4 /* length */ + utf16_len(a)).sum::<usize>()
            })
    }

    fn packet_size(&self) -> usize {
        Self::PACKET_FIXED_PART_SIZE + self.fields_size() + Self::PAD_SIZE
    }
}

impl fmt::Debug for ServerRedirectionPdu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = |b: &Option<Vec<u8>>| b.as_ref().map(Vec::len);

        f.debug_struct("ServerRedirectionPdu")
            .field("session_id", &self.session_id)
            .field("redirection_flags", &self.redirection_flags)
            .field("target_net_address", &self.target_net_address)
            .field("load_balance_info_len", &len(&self.load_balance_info))
            .field("username", &self.username)
            .field("domain", &self.domain)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("target_fqdn", &self.target_fqdn)
            .field("target_netbios_name", &self.target_netbios_name)
            .field("tsv_url_len", &len(&self.tsv_url))
            .field("target_net_addresses", &self.target_net_addresses)
            .field("redirection_guid_len", &len(&self.redirection_guid))
            .field("target_certificate_len", &len(&self.target_certificate))
            .finish()
    }
}

impl Encode for ServerRedirectionPdu {
    fn encode(&self, dst: &mut WriteCursor<'_>) -> EncodeResult<()> {
        ensure_size!(in: dst, size: self.size());

        let flags = self.redirection_flags;
        let presence = [
            (
                ServerRedirectionFlags::TARGET_NET_ADDRESS,
                self.target_net_address.is_some(),
            ),
            (
                ServerRedirectionFlags::LOAD_BALANCE_INFO,
                self.load_balance_info.is_some(),
            ),
            (ServerRedirectionFlags::USERNAME, self.username.is_some()),
            (ServerRedirectionFlags::DOMAIN, self.domain.is_some()),
            (ServerRedirectionFlags::PASSWORD, self.password.is_some()),
            (ServerRedirectionFlags::TARGET_FQDN, self.target_fqdn.is_some()),
            (
                ServerRedirectionFlags::TARGET_NETBIOS_NAME,
                self.target_netbios_name.is_some(),
            ),
            (ServerRedirectionFlags::CLIENT_TSV_URL, self.tsv_url.is_some()),
            (
                ServerRedirectionFlags::TARGET_NET_ADDRESSES,
                self.target_net_addresses.is_some(),
            ),
            (
                ServerRedirectionFlags::REDIRECTION_GUID,
                self.redirection_guid.is_some(),
            ),
            (
                ServerRedirectionFlags::TARGET_CERTIFICATE,
                self.target_certificate.is_some(),
            ),
        ];
        if presence.iter().any(|(flag, present)| flags.contains(*flag) != *present) {
            return Err(invalid_field_err!(
                "RedirFlags",
                "field presence does not match redirection flags",
                in: dst
            ));
        }

        write_padding!(dst, 2); // pad2Octets
        dst.write_u16(Self::SEC_REDIRECTION_PKT);
        dst.write_u16(cast_length!("Length", self.packet_size(), in: dst)?);
        dst.write_u32(self.session_id);
        dst.write_u32(flags.bits());

        if let Some(s) = &self.target_net_address {
            write_unicode(dst, s)?;
        }
        if let Some(b) = &self.load_balance_info {
            write_blob(dst, b)?;
        }
        if let Some(s) = &self.username {
            write_unicode(dst, s)?;
        }
        if let Some(s) = &self.domain {
            write_unicode(dst, s)?;
        }
        if let Some(b) = &self.password {
            write_blob(dst, b)?;
        }
        if let Some(s) = &self.target_fqdn {
            write_unicode(dst, s)?;
        }
        if let Some(s) = &self.target_netbios_name {
            write_unicode(dst, s)?;
        }
        if let Some(b) = &self.tsv_url {
            write_blob(dst, b)?;
        }
        if let Some(b) = &self.redirection_guid {
            write_blob(dst, b)?;
        }
        if let Some(b) = &self.target_certificate {
            write_blob(dst, b)?;
        }
        if let Some(addresses) = &self.target_net_addresses {
            let length = 4 /* count */ + addresses.iter().map(|a| 4 /* length */ + utf16_len(a)).sum::<usize>();
            dst.write_u32(cast_length!("TargetNetAddressesLength", length, in: dst)?);
            dst.write_u32(cast_length!("TargetNetAddressesCount", addresses.len(), in: dst)?);
            for address in addresses {
                write_unicode(dst, address)?;
            }
        }

        write_padding!(dst, 8); // Pad

        Ok(())
    }

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn size(&self) -> usize {
        2 /* pad2Octets */ + self.packet_size()
    }
}

impl<'de> Decode<'de> for ServerRedirectionPdu {
    fn decode(src: &mut ReadCursor<'de>) -> DecodeResult<Self> {
        ensure_size!(in: src, size: Self::FIXED_PART_SIZE);

        read_padding!(src, 2); // pad2Octets

        let flags = src.read_u16();
        if flags != Self::SEC_REDIRECTION_PKT {
            return Err(invalid_field_err!("Flags", "not SEC_REDIRECTION_PKT", in: src));
        }

        let length = usize::from(src.read_u16());
        // INVARIANT: PACKET_FIXED_PART_SIZE <= length
        if length < Self::PACKET_FIXED_PART_SIZE {
            return Err(invalid_field_err!("Length", "shorter than the fixed part", in: src));
        }
        let remaining_length = length - 4 /* Flags + Length */;
        ensure_size!(in: src, size: remaining_length);
        let mut packet = ReadCursor::new(src.read_slice(remaining_length));
        let packet = &mut packet;

        let session_id = packet.read_u32();
        let redirection_flags = ServerRedirectionFlags::from_bits_retain(packet.read_u32());
        let has = |flag| redirection_flags.contains(flag);

        let target_net_address = has(ServerRedirectionFlags::TARGET_NET_ADDRESS)
            .then(|| read_unicode(packet))
            .transpose()?;
        let load_balance_info = has(ServerRedirectionFlags::LOAD_BALANCE_INFO)
            .then(|| read_blob(packet))
            .transpose()?;
        let username = has(ServerRedirectionFlags::USERNAME)
            .then(|| read_unicode(packet))
            .transpose()?;
        let domain = has(ServerRedirectionFlags::DOMAIN)
            .then(|| read_unicode(packet))
            .transpose()?;
        let password = has(ServerRedirectionFlags::PASSWORD)
            .then(|| read_blob(packet))
            .transpose()?;
        let target_fqdn = has(ServerRedirectionFlags::TARGET_FQDN)
            .then(|| read_unicode(packet))
            .transpose()?;
        let target_netbios_name = has(ServerRedirectionFlags::TARGET_NETBIOS_NAME)
            .then(|| read_unicode(packet))
            .transpose()?;
        let tsv_url = has(ServerRedirectionFlags::CLIENT_TSV_URL)
            .then(|| read_blob(packet))
            .transpose()?;
        let redirection_guid = has(ServerRedirectionFlags::REDIRECTION_GUID)
            .then(|| read_blob(packet))
            .transpose()?;
        let target_certificate = has(ServerRedirectionFlags::TARGET_CERTIFICATE)
            .then(|| read_blob(packet))
            .transpose()?;
        let target_net_addresses = if has(ServerRedirectionFlags::TARGET_NET_ADDRESSES) {
            ensure_size!(in: packet, size: 4 /* length */);
            let length = usize::try_from(packet.read_u32())
                .map_err(|_| invalid_field_err!("TargetNetAddressesLength", "too large", in: packet))?;
            ensure_size!(in: packet, size: length);
            let mut list = ReadCursor::new(packet.read_slice(length));
            let list = &mut list;
            ensure_size!(in: list, size: 4 /* count */);
            let count = list.read_u32();
            let mut addresses = Vec::new();
            for _ in 0..count {
                addresses.push(read_unicode(list)?);
            }
            Some(addresses)
        } else {
            None
        };

        // Whatever is left in the packet is the optional 8-byte `Pad`.

        Ok(Self {
            session_id,
            redirection_flags,
            target_net_address,
            load_balance_info,
            username,
            domain,
            password,
            target_fqdn,
            target_netbios_name,
            tsv_url,
            target_net_addresses,
            redirection_guid,
            target_certificate,
        })
    }
}

/// One element of a target certificate container.
///
/// [MS-RDPBCGR] 2.2.13.1 describes `TargetCertificate` as a UTF-16LE base64 encoding of a
/// sequence of `{elementType: u32, encodingType: u32, elementSize: u32, elementData}` records.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct TargetCertificateElement {
    /// `elementType`.
    pub element_type: u32,
    /// `encodingType`.
    pub encoding: u32,
    /// `elementData`.
    pub data: Vec<u8>,
}

impl TargetCertificateElement {
    /// `ELEMENT_TYPE_CERTIFICATE`: the element holds the target server's certificate.
    pub const TYPE_CERTIFICATE: u32 = 0x20;
    /// `ENCODING_TYPE_ASN1_DER`: the element data is DER.
    pub const ENCODING_ASN1_DER: u32 = 0x01;

    const FIXED_PART_SIZE: usize = 4 /* elementType */ + 4 /* encodingType */ + 4 /* elementSize */;
}

/// Parsed `TargetCertificate` of a Server Redirection PDU.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct TargetCertificateContainer {
    /// Elements in wire order, unknown element types included.
    pub elements: Vec<TargetCertificateElement>,
}

impl TargetCertificateContainer {
    /// Decodes the wire form: UTF-16LE base64 text (optionally null-terminated, possibly split
    /// by CR/LF) of the element sequence.
    pub fn decode_wire(wire: &[u8]) -> DecodeResult<Self> {
        const CTX: &str = "TargetCertificate";

        if !wire.len().is_multiple_of(2) {
            return Err(invalid_field_err!(CTX, "TargetCertificate", "odd UTF-16 length"));
        }

        let mut text = Vec::with_capacity(wire.len() / 2);
        for unit in wire.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])) {
            match unit {
                0 => break,
                0x0D | 0x0A => {}
                _ => text.push(
                    u8::try_from(unit)
                        .map_err(|_| invalid_field_err!(CTX, "TargetCertificate", "non-ASCII base64 text"))?,
                ),
            }
        }
        let raw =
            base64_decode(&text).ok_or_else(|| invalid_field_err!(CTX, "TargetCertificate", "invalid base64 text"))?;

        let mut src = ReadCursor::new(&raw);
        let src = &mut src;
        let mut elements = Vec::new();
        while !src.is_empty() {
            ensure_size!(ctx: CTX, in: src, size: TargetCertificateElement::FIXED_PART_SIZE);
            let element_type = src.read_u32();
            let encoding = src.read_u32();
            let size =
                usize::try_from(src.read_u32()).map_err(|_| invalid_field_err!("elementSize", "too large", in: src))?;
            ensure_size!(ctx: CTX, in: src, size: size);
            elements.push(TargetCertificateElement {
                element_type,
                encoding,
                data: src.read_slice(size).to_vec(),
            });
        }

        Ok(Self { elements })
    }

    /// Encodes the wire form: UTF-16LE base64 text of the element sequence, without terminator
    /// (the layout FreeRDP-based servers emit).
    pub fn encode_wire(&self) -> EncodeResult<Vec<u8>> {
        let mut raw = Vec::new();
        for element in &self.elements {
            let size = u32::try_from(element.data.len())
                .map_err(|_| invalid_field_err!("TargetCertificate", "elementSize", "too large"))?;
            raw.extend_from_slice(&element.element_type.to_le_bytes());
            raw.extend_from_slice(&element.encoding.to_le_bytes());
            raw.extend_from_slice(&size.to_le_bytes());
            raw.extend_from_slice(&element.data);
        }

        Ok(base64_encode(&raw)
            .into_iter()
            .flat_map(|c| u16::from(c).to_le_bytes())
            .collect())
    }

    /// Returns the DER certificate of the first `ELEMENT_TYPE_CERTIFICATE` element in DER encoding.
    pub fn der_certificate(&self) -> Option<&[u8]> {
        self.elements
            .iter()
            .find(|e| {
                e.element_type == TargetCertificateElement::TYPE_CERTIFICATE
                    && e.encoding == TargetCertificateElement::ENCODING_ASN1_DER
            })
            .map(|e| e.data.as_slice())
    }
}

fn utf16_len(s: &str) -> usize {
    (s.encode_utf16().count() + 1/* null terminator */) * 2
}

fn read_blob(src: &mut ReadCursor<'_>) -> DecodeResult<Vec<u8>> {
    ensure_size!(in: src, size: 4 /* length */);
    let length = usize::try_from(src.read_u32()).map_err(|_| invalid_field_err!("length", "too large", in: src))?;
    ensure_size!(in: src, size: length);
    Ok(src.read_slice(length).to_vec())
}

fn read_unicode(src: &mut ReadCursor<'_>) -> DecodeResult<String> {
    let bytes = read_blob(src)?;
    if !bytes.len().is_multiple_of(2) {
        return Err(invalid_field_err!("string", "odd UTF-16 length", in: src));
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&unit| unit != 0)
        .collect();
    String::from_utf16(&units).map_err(|_| invalid_field_err!("string", "invalid UTF-16", in: src))
}

fn write_blob(dst: &mut WriteCursor<'_>, blob: &[u8]) -> EncodeResult<()> {
    dst.write_u32(cast_length!("length", blob.len(), in: dst)?);
    dst.write_slice(blob);
    Ok(())
}

fn write_unicode(dst: &mut WriteCursor<'_>, s: &str) -> EncodeResult<()> {
    dst.write_u32(cast_length!("length", utf16_len(s), in: dst)?);
    for unit in s.encode_utf16() {
        dst.write_u16(unit);
    }
    dst.write_u16(0);
    Ok(())
}

const BASE64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with `=` padding (RFC 4648 section 4).
fn base64_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let sextet = |shift: u32| BASE64_ALPHABET[usize::try_from((n >> shift) & 0x3F).unwrap_or(0)];
        out.push(sextet(18));
        out.push(sextet(12));
        out.push(if chunk.len() > 1 { sextet(6) } else { b'=' });
        out.push(if chunk.len() > 2 { sextet(0) } else { b'=' });
    }
    out
}

/// Strict standard base64 decoding; `None` on any malformed input.
fn base64_decode(text: &[u8]) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(4) {
        return None;
    }

    let value = |c: u8| -> Option<u32> {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        Some(u32::from(v))
    };

    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let quads = text.len() / 4;
    for (i, quad) in text.chunks_exact(4).enumerate() {
        let padding = quad.iter().rev().take_while(|&&c| c == b'=').count();
        if padding > 2 || (padding > 0 && i + 1 != quads) {
            return None;
        }
        let mut n = 0u32;
        for &c in &quad[..4 - padding] {
            n = (n << 6) | value(c)?;
        }
        n <<= 6 * u32::try_from(padding).ok()?;
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..4 - padding]);
    }

    Some(out)
}
