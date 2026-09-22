//! M1-2 Red: replaying captured g-r-d 50.2 GFX streams (as received: `RDP_SEGMENTED_DATA` +
//! ZGFX) into a `RecordingFrameSink` gives a stable, reviewed sequence of sink calls
//! (`insta` snapshots), and the frame acknowledgements equal the ones the verified reference
//! client sent in the same session.
#![allow(clippy::unwrap_used)]

mod support;

use drift_core::Size;
use drift_gfx::GfxClient;
use drift_testkit::fixtures::{self, names};
use drift_testkit::{FrameSinkCall, PresentMode};
use ironrdp_egfx::pdu::{FrameAcknowledgePdu, GfxPdu};
use support::{decode_all, harness, summarize};

/// Feeds every raw DVC payload of `raw` to the client.
fn replay(client: &mut GfxClient, raw: &str) -> usize {
    let payloads = fixtures::records(raw);
    for (i, p) in payloads.iter().enumerate() {
        client.process_payload(p).unwrap_or_else(|e| panic!("{raw}: payload {i}: {e}"));
    }
    payloads.len()
}

/// The `FrameAcknowledge` PDUs of a captured client stream.
fn captured_acks(client_rec: &str) -> Vec<FrameAcknowledgePdu> {
    fixtures::records(client_rec)
        .iter()
        .flat_map(|r| decode_all(r))
        .filter_map(|p| match p {
            GfxPdu::FrameAcknowledge(a) => Some(a),
            _ => None,
        })
        .collect()
}

fn end_frames(calls: &[FrameSinkCall]) -> usize {
    calls.iter().filter(|c| matches!(c, FrameSinkCall::EndFrame { .. })).count()
}

#[test]
fn greeter_avc420_replay_snapshot() {
    let mut h = harness(PresentMode::Immediate, Size::new(1280, 800));
    replay(&mut h.client, names::GFX_LEG2_GREETER_AVC420_RAW);
    let calls = h.log.calls();
    assert_eq!(end_frames(&calls), 8, "the greeter capture holds 8 frames");
    assert_eq!(h.client.total_frames_decoded(), 8);
    insta::assert_yaml_snapshot!("greeter_avc420", summarize(&calls));
}

#[test]
fn greeter_avc420_acks_match_the_reference_client() {
    let mut h = harness(PresentMode::Immediate, Size::new(1280, 800));
    replay(&mut h.client, names::GFX_LEG2_GREETER_AVC420_RAW);
    assert_eq!(h.acks.drain(), captured_acks(names::GFX_LEG2_GREETER_AVC420_CLIENT));
}

#[test]
fn greeter_progressive_replay_snapshot() {
    let mut h = harness(PresentMode::Immediate, Size::new(1280, 800));
    replay(&mut h.client, names::GFX_LEG2_GREETER_PROGRESSIVE_RAW);
    let calls = h.log.calls();
    assert!(
        calls.iter().any(|c| matches!(c, FrameSinkCall::BlitBgra { .. })),
        "progressive tiles reach the sink"
    );
    assert_eq!(h.h264.decodes.load(std::sync::atomic::Ordering::SeqCst), 0);
    insta::assert_yaml_snapshot!("greeter_progressive", summarize(&calls));
}

#[test]
fn greeter_progressive_acks_match_the_reference_client() {
    let mut h = harness(PresentMode::Immediate, Size::new(1280, 800));
    replay(&mut h.client, names::GFX_LEG2_GREETER_PROGRESSIVE_RAW);
    assert_eq!(h.acks.drain(), captured_acks(names::GFX_LEG2_GREETER_PROGRESSIVE_CLIENT));
}

#[test]
fn headless_scale200_replay_snapshot() {
    let mut h = harness(PresentMode::Immediate, Size::new(2560, 1600));
    replay(&mut h.client, names::GFX_HEADLESS_SCALE200_AVC420_RAW);
    let calls = h.log.calls();
    assert!(calls.contains(&FrameSinkCall::Reset { output: Size::new(2560, 1600) }));
    insta::assert_yaml_snapshot!("headless_scale200_avc420", summarize(&calls));
}

#[test]
fn headless_motion_replays_every_frame_and_acks_like_the_reference_client() {
    let mut h = harness(PresentMode::Immediate, Size::new(1280, 800));
    let payloads = replay(&mut h.client, names::GFX_HEADLESS_MOTION_AVC420_RAW);
    assert_eq!(payloads, 1279);
    let calls = h.log.calls();
    let nv12 = calls.iter().filter(|c| matches!(c, FrameSinkCall::BlitNv12 { .. })).count();
    assert_eq!(end_frames(&calls), 425);
    assert_eq!(nv12, h.h264.decodes.load(std::sync::atomic::Ordering::SeqCst));
    assert!(nv12 >= 425, "every frame carries at least one AVC420 picture");
    assert_eq!(h.acks.drain(), captured_acks(names::GFX_HEADLESS_MOTION_AVC420_CLIENT));
    insta::assert_yaml_snapshot!("headless_motion_avc420_head", summarize(&calls[..calls.len().min(12)]));
}

#[test]
fn decompressed_and_raw_streams_give_the_same_calls() {
    let mut raw = harness(PresentMode::Immediate, Size::new(1280, 800));
    replay(&mut raw.client, names::GFX_LEG2_GREETER_AVC420_RAW);
    let mut plain = harness(PresentMode::Immediate, Size::new(1280, 800));
    for batch in fixtures::records(names::GFX_LEG2_GREETER_AVC420) {
        plain.client.process_pdus(&batch).unwrap();
    }
    assert_eq!(raw.log.calls(), plain.log.calls());
}
