//! M1-2 Red: malformed or unsupported server input is a `ProtocolError`, never a panic.
#![allow(clippy::unwrap_used)]

mod support;

use drift_core::{DisconnectReason, Size};
use drift_gfx::GfxError;
use drift_testkit::PresentMode;
use ironrdp_core::encode_vec;
use ironrdp_dvc::DvcProcessor;
use ironrdp_egfx::pdu::{
    Avc420Region, Codec1Type, FrameAcknowledgePdu, GfxPdu, QueueDepth, encode_avc420_bitmap_stream,
};
use ironrdp_graphics::zgfx;
use proptest::prelude::*;
use support::{create, harness, rect16, reset, w2s1, wire};

fn ready() -> support::Harness {
    let mut h = harness(PresentMode::Immediate, Size::new(64, 64));
    h.client.process_payload(&wire(&[reset(64, 64), create(1, 64, 64)])).unwrap();
    h
}

fn assert_protocol_error(err: &GfxError) {
    match err.disconnect_reason() {
        DisconnectReason::ProtocolError(msg) => assert!(!msg.is_empty(), "the reason names the problem"),
        other => panic!("expected ProtocolError, got {other:?}"),
    }
}

#[test]
fn unknown_codec_id_is_a_protocol_error() {
    let mut h = ready();
    let mut pdu = encode_vec(&w2s1(1, Codec1Type::Uncompressed, rect16(0, 0, 1, 1), vec![0; 4])).unwrap();
    // RDPGFX_HEADER (8 bytes), surfaceId (2), then codecId: 0x0007 is not an RDPGFX codec.
    pdu[10..12].copy_from_slice(&0x0007u16.to_le_bytes());
    let err = h.client.process_payload(&zgfx::wrap_uncompressed(&pdu)).unwrap_err();
    assert!(matches!(err, GfxError::Decode(_)), "{err:?}");
    assert_protocol_error(&err);
}

#[test]
fn codecs_drift_does_not_advertise_are_protocol_errors() {
    for codec in [Codec1Type::RemoteFx, Codec1Type::Alpha, Codec1Type::Avc444, Codec1Type::Avc444v2] {
        let mut h = ready();
        let err =
            h.client.process_payload(&wire(&[w2s1(1, codec, rect16(0, 0, 4, 4), vec![0; 16])])).unwrap_err();
        assert!(matches!(err, GfxError::UnsupportedCodec(_)), "{codec:?}: {err:?}");
        assert_protocol_error(&err);
    }
}

#[test]
fn malformed_zgfx_is_a_protocol_error() {
    let mut h = ready();
    let err = h.client.process_payload(&[0xAA, 0x00, 0x00]).unwrap_err();
    assert!(matches!(err, GfxError::Zgfx(_)), "{err:?}");
    assert_protocol_error(&err);
}

#[test]
fn truncated_pdu_is_a_protocol_error() {
    let mut h = ready();
    let pdu = encode_vec(&create(2, 8, 8)).unwrap();
    let err = h.client.process_payload(&zgfx::wrap_uncompressed(&pdu[..pdu.len() - 3])).unwrap_err();
    assert!(matches!(err, GfxError::Decode(_)), "{err:?}");
}

#[test]
fn codec_failures_are_protocol_errors() {
    let mut h = ready();
    // Uncompressed payload of the wrong size.
    let err = h
        .client
        .process_payload(&wire(&[w2s1(1, Codec1Type::Uncompressed, rect16(0, 0, 4, 4), vec![0; 3])]))
        .unwrap_err();
    assert!(matches!(err, GfxError::Codec(_)), "{err:?}");
    // H.264 decoder failure.
    let avc = encode_avc420_bitmap_stream(&[Avc420Region::new(0, 0, 16, 16, 22, 100)], &[0xEE, 0, 0, 1]);
    let err = h
        .client
        .process_payload(&wire(&[w2s1(1, Codec1Type::Avc420, rect16(0, 0, 64, 64), avc)]))
        .unwrap_err();
    assert!(matches!(err, GfxError::H264(_)), "{err:?}");
    // Truncated AVC420 metablock.
    let err = h
        .client
        .process_payload(&wire(&[w2s1(1, Codec1Type::Avc420, rect16(0, 0, 64, 64), vec![5, 0, 0, 0, 1])]))
        .unwrap_err();
    assert!(matches!(err, GfxError::Decode(_)), "{err:?}");
    assert_protocol_error(&err);
}

#[test]
fn client_only_pdus_from_the_server_are_protocol_errors() {
    let mut h = ready();
    let ack = GfxPdu::FrameAcknowledge(FrameAcknowledgePdu {
        queue_depth: QueueDepth::Unavailable,
        frame_id: 1,
        total_frames_decoded: 1,
    });
    let err = h.client.process_payload(&wire(&[ack])).unwrap_err();
    assert!(matches!(err, GfxError::UnexpectedPdu(_)), "{err:?}");
}

#[test]
fn dvc_process_surfaces_the_error_to_the_actor() {
    let mut h = ready();
    assert!(h.client.process(3, &[0xE0, 0x04, 0x01]).is_err());
    let err = h.client.take_error().expect("the failure is kept for the actor");
    assert_protocol_error(&err);
    assert!(h.client.take_error().is_none());
    assert_eq!(DisconnectReason::from(err.clone()), err.disconnect_reason());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Arbitrary bytes as a raw DVC payload never panic.
    #[test]
    fn arbitrary_payloads_never_panic(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let mut h = ready();
        let _ = h.client.process_payload(&data);
    }

    /// Arbitrary bytes as decompressed GFX PDUs (after a valid header) never panic.
    #[test]
    fn arbitrary_pdu_bodies_never_panic(cmd in 1u16..=0x18, body in proptest::collection::vec(any::<u8>(), 0..256)) {
        let mut h = ready();
        let mut pdu = Vec::new();
        pdu.extend(cmd.to_le_bytes());
        pdu.extend(0u16.to_le_bytes());
        pdu.extend(u32::try_from(body.len() + 8).unwrap().to_le_bytes());
        pdu.extend(&body);
        let _ = h.client.process_payload(&zgfx::wrap_uncompressed(&pdu));
    }
}
