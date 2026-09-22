//! Enhanced Security Server Redirection PDU ([MS-RDPBCGR] 2.2.13.3.1).
//!
//! The byte vectors are synthetic but follow the exact shape GNOME Remote Desktop 50 sends on a
//! Remote Login leg: `redirFlags = 0x1C016`, a `Cookie: msts=<u32>\r\n` load-balance info, a
//! 16-character one-time user name, a 34-byte opaque password, a 50-byte UTF-16 base64 GUID and
//! a UTF-16 base64 target certificate container. No real credential appears in them.

use ironrdp_core::{WriteBuf, decode, encode_vec};
use ironrdp_pdu::mcs::SendDataIndicationCtx;
use ironrdp_pdu::rdp::headers::{ShareControlHeader, ShareControlPdu, decode_share_control};
use ironrdp_pdu::rdp::server_redirection::{
    ServerRedirectionFlags, ServerRedirectionPdu, TargetCertificateContainer, TargetCertificateElement,
};

const LB_INFO: &[u8] = b"Cookie: msts=3735928559\r\n";
const USERNAME: &str = "drift-otp-user01";
const PASSWORD_BLOB: [u8; 34] = [0xA5; 34];
/// base64(00 01 .. 0f)
const GUID_BASE64: &str = "AAECAwQFBgcICQoLDA0ODw==";
/// base64 of a container with a single `{type 0x20, encoding 1 (DER), len 32, FAKE_DER}` element.
const CERT_BASE64: &str = "IAAAAAEAAAAgAAAAMIIAHEBBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWls=";

fn fake_der() -> Vec<u8> {
    let mut der = vec![0x30, 0x82, 0x00, 0x1c];
    der.extend(0x40u8..0x5c);
    der
}

fn utf16(s: &str, nul: bool) -> Vec<u8> {
    let mut out: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
    if nul {
        out.extend_from_slice(&[0, 0]);
    }
    out
}

fn push_blob(dst: &mut Vec<u8>, blob: &[u8]) {
    dst.extend_from_slice(&u32::try_from(blob.len()).unwrap().to_le_bytes());
    dst.extend_from_slice(blob);
}

/// RDP_SERVER_REDIRECTION_PACKET as g-r-d 50 lays it out (flags 0x0400 onwards, with 8-byte pad).
fn redirection_packet(cert_utf16_base64: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&0u32.to_le_bytes()); // sessionId
    body.extend_from_slice(&0x0001_C016u32.to_le_bytes()); // redirFlags
    push_blob(&mut body, LB_INFO);
    push_blob(&mut body, &utf16(USERNAME, true));
    push_blob(&mut body, &PASSWORD_BLOB);
    push_blob(&mut body, &utf16(GUID_BASE64, true));
    push_blob(&mut body, cert_utf16_base64);
    body.extend_from_slice(&[0; 8]); // pad

    let mut packet = Vec::new();
    packet.extend_from_slice(&0x0400u16.to_le_bytes()); // flags = SEC_REDIRECTION_PKT
    packet.extend_from_slice(&u16::try_from(4 + body.len()).unwrap().to_le_bytes()); // length
    packet.extend_from_slice(&body);
    packet
}

/// Share Control header (pduType 0xA, version 1) + pad2Octets + packet.
fn share_control_redirection(cert_utf16_base64: &[u8]) -> Vec<u8> {
    let packet = redirection_packet(cert_utf16_base64);
    let total = 6 /* share control header */ + 2 /* pad2Octets */ + packet.len();

    let mut pdu = Vec::new();
    pdu.extend_from_slice(&u16::try_from(total).unwrap().to_le_bytes()); // totalLength
    pdu.extend_from_slice(&0x001Au16.to_le_bytes()); // pduType = 0x10 | PDUTYPE_SERVER_REDIR_PKT
    pdu.extend_from_slice(&0x03EAu16.to_le_bytes()); // pduSource
    pdu.extend_from_slice(&[0, 0]); // pad2Octets
    pdu.extend_from_slice(&packet);
    pdu
}

fn gnome_redirection() -> Vec<u8> {
    share_control_redirection(&utf16(CERT_BASE64, false))
}

fn decode_redirection(bytes: &[u8]) -> ServerRedirectionPdu {
    let header = decode::<ShareControlHeader>(bytes).unwrap();
    match header.share_control_pdu {
        ShareControlPdu::ServerRedirect(pdu) => pdu,
        other => panic!("unexpected share control PDU: {}", other.as_short_name()),
    }
}

#[test]
fn decodes_gnome_remote_login_redirection() {
    let pdu = decode_redirection(&gnome_redirection());

    assert_eq!(pdu.session_id, 0);
    assert_eq!(pdu.redirection_flags.bits(), 0x0001_C016);
    assert_eq!(
        pdu.redirection_flags,
        ServerRedirectionFlags::LOAD_BALANCE_INFO
            | ServerRedirectionFlags::USERNAME
            | ServerRedirectionFlags::PASSWORD
            | ServerRedirectionFlags::PASSWORD_IS_PK_ENCRYPTED
            | ServerRedirectionFlags::REDIRECTION_GUID
            | ServerRedirectionFlags::TARGET_CERTIFICATE
    );
    assert_eq!(pdu.load_balance_info.as_deref(), Some(LB_INFO));
    assert!(pdu.load_balance_info.as_deref().unwrap().starts_with(b"Cookie: msts="));
    assert!(pdu.load_balance_info.as_deref().unwrap().ends_with(b"\r\n"));
    assert_eq!(pdu.username.as_deref(), Some(USERNAME));
    assert_eq!(pdu.username.as_deref().unwrap().chars().count(), 16);
    assert_eq!(pdu.password.as_deref(), Some(PASSWORD_BLOB.as_slice()));
    assert_eq!(pdu.redirection_guid.as_deref().map(<[u8]>::len), Some(50));
    assert_eq!(
        pdu.redirection_guid.as_deref(),
        Some(utf16(GUID_BASE64, true).as_slice())
    );
    assert_eq!(pdu.target_net_address, None);
    assert_eq!(pdu.domain, None);
    assert_eq!(pdu.target_fqdn, None);
    assert_eq!(pdu.target_netbios_name, None);
    assert_eq!(pdu.tsv_url, None);
    assert_eq!(pdu.target_net_addresses, None);
    assert_eq!(
        pdu.target_certificate.as_deref(),
        Some(utf16(CERT_BASE64, false).as_slice())
    );
}

#[test]
fn parses_target_certificate_container() {
    let pdu = decode_redirection(&gnome_redirection());

    let container = pdu.decode_target_certificate().unwrap().expect("certificate present");
    assert_eq!(
        container.elements,
        vec![TargetCertificateElement {
            element_type: TargetCertificateElement::TYPE_CERTIFICATE,
            encoding: TargetCertificateElement::ENCODING_ASN1_DER,
            data: fake_der(),
        }]
    );
    assert_eq!(container.der_certificate(), Some(fake_der().as_slice()));
}

#[test]
fn target_certificate_container_skips_unknown_elements_and_line_breaks() {
    let wire = utf16(
        "AQAAAAAAAAADAAAAYWJjIAAAAAEAAAAg\r\nAAAAMIIAHEBBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWls=",
        true,
    );
    let container = TargetCertificateContainer::decode_wire(&wire).unwrap();

    assert_eq!(container.elements.len(), 2);
    assert_eq!(container.elements[0].element_type, 1);
    assert_eq!(container.elements[0].data, b"abc");
    assert_eq!(container.der_certificate(), Some(fake_der().as_slice()));
}

#[test]
fn target_certificate_container_round_trips() {
    let container = TargetCertificateContainer {
        elements: vec![TargetCertificateElement {
            element_type: TargetCertificateElement::TYPE_CERTIFICATE,
            encoding: TargetCertificateElement::ENCODING_ASN1_DER,
            data: fake_der(),
        }],
    };

    let wire = container.encode_wire().unwrap();
    assert_eq!(wire, utf16(CERT_BASE64, false));
    assert_eq!(TargetCertificateContainer::decode_wire(&wire).unwrap(), container);
}

#[test]
fn target_certificate_container_rejects_bad_input() {
    // Not base64.
    assert!(TargetCertificateContainer::decode_wire(&utf16("!!!!", false)).is_err());
    // Odd UTF-16 length.
    assert!(TargetCertificateContainer::decode_wire(&[0x41, 0x00, 0x41]).is_err());
    // Element header claims more data than present: {0x20, 1, len=100} + 0 bytes.
    assert!(TargetCertificateContainer::decode_wire(&utf16("IAAAAAEAAABkAAAA", false)).is_err());
}

#[test]
fn round_trips_redirection_pdu() {
    let bytes = gnome_redirection();
    let header = decode::<ShareControlHeader>(bytes.as_slice()).unwrap();

    assert_eq!(encode_vec(&header).unwrap(), bytes);
}

#[test]
fn decodes_through_send_data_indication_context() {
    let bytes = gnome_redirection();
    let ctx = SendDataIndicationCtx {
        initiator_id: 1002,
        channel_id: 1003,
        user_data: &bytes,
    };

    let ctx = decode_share_control(ctx).unwrap();
    assert_eq!(ctx.share_id, 0);
    assert!(matches!(ctx.pdu, ShareControlPdu::ServerRedirect(_)));
}

#[test]
fn decodes_without_trailing_padding() {
    // Some servers omit the 8-byte pad at the end of the packet.
    let mut bytes = gnome_redirection();
    bytes.truncate(bytes.len() - 8);
    let total = u16::try_from(bytes.len()).unwrap().to_le_bytes();
    bytes[0..2].copy_from_slice(&total);
    let packet_len = u16::try_from(bytes.len() - 8).unwrap().to_le_bytes();
    bytes[10..12].copy_from_slice(&packet_len);

    let pdu = decode_redirection(&bytes);
    assert_eq!(pdu.username.as_deref(), Some(USERNAME));
}

#[test]
fn every_truncation_is_an_error_not_a_panic() {
    let bytes = gnome_redirection();
    for len in 0..bytes.len() - 8 {
        assert!(
            decode::<ShareControlHeader>(&bytes[..len]).is_err(),
            "truncated to {len} bytes decoded successfully"
        );
    }
}

#[test]
fn rejects_packet_without_redirection_flag() {
    let mut bytes = gnome_redirection();
    bytes[8..10].copy_from_slice(&0x0040u16.to_le_bytes()); // SEC_INFO_PKT instead of SEC_REDIRECTION_PKT

    assert!(decode::<ShareControlHeader>(bytes.as_slice()).is_err());
}

#[test]
fn rejects_field_length_overrunning_packet() {
    let mut bytes = gnome_redirection();
    // LB info length (first field after sessionId and redirFlags).
    bytes[20..24].copy_from_slice(&0xFFFF_FF00u32.to_le_bytes());

    assert!(decode::<ShareControlHeader>(bytes.as_slice()).is_err());
}

#[test]
fn round_trips_all_optional_fields() {
    let pdu = ServerRedirectionPdu {
        session_id: 7,
        redirection_flags: ServerRedirectionFlags::TARGET_NET_ADDRESS
            | ServerRedirectionFlags::LOAD_BALANCE_INFO
            | ServerRedirectionFlags::USERNAME
            | ServerRedirectionFlags::DOMAIN
            | ServerRedirectionFlags::PASSWORD
            | ServerRedirectionFlags::TARGET_FQDN
            | ServerRedirectionFlags::TARGET_NETBIOS_NAME
            | ServerRedirectionFlags::CLIENT_TSV_URL
            | ServerRedirectionFlags::TARGET_NET_ADDRESSES
            | ServerRedirectionFlags::REDIRECTION_GUID
            | ServerRedirectionFlags::TARGET_CERTIFICATE,
        target_net_address: Some("192.0.2.10".to_owned()),
        load_balance_info: Some(LB_INFO.to_vec()),
        username: Some("user".to_owned()),
        domain: Some("DOMAIN".to_owned()),
        password: Some(vec![1, 2, 3, 4]),
        target_fqdn: Some("host.example.test".to_owned()),
        target_netbios_name: Some("HOST".to_owned()),
        tsv_url: Some(vec![9, 9]),
        target_net_addresses: Some(vec!["192.0.2.10".to_owned(), "198.51.100.7".to_owned()]),
        redirection_guid: Some(utf16(GUID_BASE64, true)),
        target_certificate: Some(utf16(CERT_BASE64, false)),
    };
    let header = ShareControlHeader {
        share_control_pdu: ShareControlPdu::ServerRedirect(pdu.clone()),
        pdu_source: 0x03EA,
        share_id: 0,
    };

    let mut buf = WriteBuf::new();
    ironrdp_core::encode_buf(&header, &mut buf).unwrap();
    let decoded = decode_redirection(buf.filled());

    assert_eq!(decoded, pdu);
}

#[test]
fn encode_rejects_missing_field_for_set_flag() {
    let pdu = ServerRedirectionPdu {
        redirection_flags: ServerRedirectionFlags::USERNAME,
        ..ServerRedirectionPdu::default()
    };

    assert!(encode_vec(&pdu).is_err());
}

#[test]
fn debug_output_redacts_password() {
    let pdu = decode_redirection(&gnome_redirection());
    let debug = format!("{pdu:?}");

    assert!(!debug.contains("165"), "password bytes leaked: {debug}");
    assert!(!debug.to_lowercase().contains("a5"), "password bytes leaked: {debug}");
}
