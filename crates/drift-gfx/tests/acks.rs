//! M1-2 Red: the frame-acknowledgement policy (plan §1.4).
//!
//! - `FrameAcknowledge` goes out from the `presented` callback, never before.
//! - `queueDepth` is honest: frames handed to the renderer but not yet presented.
//! - While hidden the client sends `SUSPEND_FRAME_ACKNOWLEDGEMENT` once and stops acking
//!   (it relies on Suppress Output); on show, acks resume with the next presented frame.
#![allow(clippy::unwrap_used)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use drift_core::Size;
use drift_testkit::{FrameSinkCall, PresentMode};
use ironrdp_core::encode_vec;
use ironrdp_dvc::DvcProcessor;
use ironrdp_egfx::pdu::{FrameAcknowledgePdu, GfxPdu, QueueDepth};
use support::{create, end, harness, reset, start, wire};

fn ack(frame_id: u32, depth: QueueDepth, total: u32) -> FrameAcknowledgePdu {
    FrameAcknowledgePdu { queue_depth: depth, frame_id, total_frames_decoded: total }
}

fn frames(ids: std::ops::Range<u32>) -> Vec<u8> {
    let pdus: Vec<GfxPdu> = ids.flat_map(|i| [start(i), end(i)]).collect();
    wire(&pdus)
}

fn deferred() -> support::Harness {
    let mut h = harness(PresentMode::Deferred, Size::new(64, 64));
    h.client.process_payload(&wire(&[reset(64, 64), create(1, 64, 64)])).unwrap();
    h
}

#[test]
fn ack_is_sent_only_after_present() {
    let mut h = deferred();
    h.client.process_payload(&frames(0..1)).unwrap();
    assert!(h.acks.drain().is_empty(), "no ack before the frame is presented");
    assert_eq!(h.acks.in_flight(), 1);
    assert_eq!(h.log.present_pending(), 1);
    assert_eq!(h.acks.drain(), vec![ack(0, QueueDepth::Unavailable, 1)]);
    assert_eq!(h.acks.in_flight(), 0);
}

#[test]
fn queue_depth_counts_frames_waiting_for_present() {
    let mut h = deferred();
    h.client.process_payload(&frames(10..13)).unwrap();
    assert_eq!(h.acks.in_flight(), 3);
    assert_eq!(h.log.present_pending(), 3);
    // When frame 10 is presented, 11 and 12 are still queued behind it; and so on.
    assert_eq!(
        h.acks.drain(),
        vec![
            ack(10, QueueDepth::AvailableBytes(2), 3),
            ack(11, QueueDepth::AvailableBytes(1), 3),
            ack(12, QueueDepth::Unavailable, 3),
        ]
    );
}

#[test]
fn immediate_present_acks_inside_process() {
    let mut h = harness(PresentMode::Immediate, Size::new(64, 64));
    h.client.process_payload(&wire(&[reset(64, 64), create(1, 64, 64)])).unwrap();
    // Through the DvcProcessor interface, acks that are ready go out with the response.
    let out = h.client.process(3, &frames(0..2)).unwrap();
    let bytes: Vec<Vec<u8>> = out.iter().map(|m| encode_vec(m.as_ref()).unwrap()).collect();
    let expected: Vec<Vec<u8>> = [ack(0, QueueDepth::Unavailable, 1), ack(1, QueueDepth::Unavailable, 2)]
        .into_iter()
        .map(|a| encode_vec(&GfxPdu::FrameAcknowledge(a)).unwrap())
        .collect();
    assert_eq!(bytes, expected);
    assert!(h.acks.drain().is_empty(), "returned acks are not sent twice");
}

#[test]
fn notifier_fires_when_an_ack_is_queued() {
    let mut h = deferred();
    let hits = Arc::new(AtomicUsize::new(0));
    let hits2 = hits.clone();
    h.acks.set_notifier(move || {
        hits2.fetch_add(1, Ordering::SeqCst);
    });
    h.client.process_payload(&frames(0..2)).unwrap();
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    h.log.present_pending();
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[test]
fn drain_messages_encodes_frame_acknowledge_pdus() {
    let mut h = deferred();
    h.client.process_payload(&frames(4..5)).unwrap();
    h.log.present_pending();
    let msgs = h.acks.drain_messages();
    assert_eq!(msgs.len(), 1);
    assert_eq!(
        encode_vec(msgs[0].as_ref()).unwrap(),
        encode_vec(&GfxPdu::FrameAcknowledge(ack(4, QueueDepth::Unavailable, 1))).unwrap()
    );
}

#[test]
fn hidden_sends_suspend_once_and_stops_acking() {
    let mut h = deferred();
    h.client.process_payload(&frames(0..2)).unwrap();
    h.log.present_pending();
    h.acks.drain();

    // Nothing is in flight: every frame is already acked, so there is nothing to suspend
    // yet (g-r-d ignores acks for frame ids it no longer tracks).
    h.client.set_visible(false);
    assert_eq!(h.log.take().last(), Some(&FrameSinkCall::SetVisible { visible: false }));
    assert!(h.acks.drain().is_empty());

    // A frame that still arrives while hidden (before Suppress Output takes effect) carries
    // SUSPEND_FRAME_ACKNOWLEDGEMENT (0xFFFFFFFF), exactly once.
    h.client.process_payload(&frames(2..3)).unwrap();
    h.log.present_pending();
    assert_eq!(h.acks.drain(), vec![ack(2, QueueDepth::Suspend, 3)]);
    h.client.set_visible(false);
    h.client.process_payload(&frames(3..4)).unwrap();
    h.log.present_pending();
    assert!(h.acks.drain().is_empty(), "suspended: later frames are not acked");
    assert_eq!(h.acks.in_flight(), 0);

    // Visible again: the next presented frame is acked normally, which resumes acks.
    h.client.set_visible(true);
    assert_eq!(h.log.take().last(), Some(&FrameSinkCall::SetVisible { visible: true }));
    assert!(h.acks.drain().is_empty());
    h.client.process_payload(&frames(4..5)).unwrap();
    h.log.present_pending();
    assert_eq!(h.acks.drain(), vec![ack(4, QueueDepth::Unavailable, 5)]);
    assert_eq!(h.client.total_frames_decoded(), 5);
}

#[test]
fn frame_in_flight_when_hidden_carries_the_suspend() {
    let mut h = deferred();
    h.client.process_payload(&frames(0..2)).unwrap();
    h.client.set_visible(false);
    assert!(h.acks.drain().is_empty(), "the suspend rides on a frame the server still tracks");
    h.log.present_pending();
    assert_eq!(h.acks.drain(), vec![ack(0, QueueDepth::Suspend, 2)]);
}

#[test]
fn show_before_the_suspend_was_sent_cancels_it() {
    let mut h = deferred();
    h.client.process_payload(&frames(0..1)).unwrap();
    h.client.set_visible(false);
    h.client.set_visible(true);
    h.log.present_pending();
    assert_eq!(h.acks.drain(), vec![ack(0, QueueDepth::Unavailable, 1)]);
}
