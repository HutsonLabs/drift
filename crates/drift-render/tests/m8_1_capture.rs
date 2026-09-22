//! M8-1 Red: composite capture into IOSurface-backed BGRA buffers from a CVPixelBufferPool,
//! only while recording.
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::time::Duration;

use common::*;
use drift_core::{Bgra, Rect};
use drift_gfx::FrameSink;
use drift_render::{CaptureConfig, CpuCompositor};
use drift_testkit::golden::{assert_golden, compare};

#[test]
fn captured_buffer_equals_on_screen_composite() {
    let mut gpu = offscreen(scenes::OUT);
    let rx = gpu.start_recording(CaptureConfig::default());
    scenes::bgra_blit_offset(&mut gpu);
    gpu.end_frame(7, Box::new(|| {}));
    gpu.wait_idle();
    let frame = rx.recv_timeout(Duration::from_secs(5)).expect("a captured frame");
    assert_eq!(frame.frame_id(), 7);
    assert!(frame.iosurface_id().is_some(), "capture buffer is IOSurface-backed");
    let captured = frame.to_image().unwrap();
    let on_screen = gpu.read_target();
    compare(&g(&on_screen), &g(&captured), 0).unwrap();
    compare(&g(&gpu.read_output().unwrap()), &g(&captured), 0).unwrap();
    let mut cpu = CpuCompositor::new();
    scenes::bgra_blit_offset(&mut cpu);
    assert_golden(&golden_path("bgra_blit_offset"), &g(&cpu.composite()), &g(&captured), 0);
}

#[test]
fn nothing_is_captured_unless_recording() {
    let mut gpu = offscreen(scenes::OUT);
    scenes::solid_fill(&mut gpu);
    gpu.end_frame(1, Box::new(|| {}));
    gpu.wait_idle();
    assert_eq!(gpu.capture_stats().captured, 0);
    assert!(!gpu.is_recording());

    let rx = gpu.start_recording(CaptureConfig::default());
    assert!(gpu.is_recording());
    gpu.end_frame(2, Box::new(|| {}));
    gpu.wait_idle();
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap().frame_id(), 2);
    gpu.stop_recording();
    assert!(!gpu.is_recording());
    gpu.end_frame(3, Box::new(|| {}));
    gpu.wait_idle();
    assert!(rx.recv_timeout(Duration::from_millis(200)).is_err(), "no capture after stop");
    assert_eq!(gpu.capture_stats().captured, 1);
}

#[test]
fn pool_does_not_grow_over_1000_frames() {
    let mut gpu = offscreen(scenes::OUT);
    let config = CaptureConfig::default();
    let rx = gpu.start_recording(config);
    scenes::solid_fill(&mut gpu);
    let mut surfaces = BTreeSet::new();
    for id in 0..1000u32 {
        gpu.solid_fill(1, Bgra::new(id as u8, (id >> 8) as u8, 0, 255), &[Rect::new(0, 0, 8, 8)]);
        gpu.end_frame(id, Box::new(|| {}));
        let frame = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(frame.frame_id(), id);
        surfaces.insert(frame.iosurface_id().unwrap());
        drop(frame); // the encoder is done with it → back to the pool
    }
    assert!(
        surfaces.len() <= config.max_buffers,
        "pool grew to {} distinct IOSurfaces (cap {})",
        surfaces.len(),
        config.max_buffers
    );
    assert_eq!(gpu.capture_stats().captured, 1000);
}

#[test]
fn slow_consumer_drops_frames_instead_of_growing_the_pool() {
    let mut gpu = offscreen(scenes::OUT);
    let config = CaptureConfig { max_buffers: 3, queue_depth: 8 };
    let rx = gpu.start_recording(config);
    scenes::solid_fill(&mut gpu);
    for id in 0..50u32 {
        gpu.end_frame(id, Box::new(|| {}));
    }
    gpu.wait_idle();
    let held: Vec<_> = rx.try_iter().collect();
    let ids: BTreeSet<_> = held.iter().map(|f| f.iosurface_id().unwrap()).collect();
    assert!(ids.len() <= 3, "{} buffers alive", ids.len());
    let stats = gpu.capture_stats();
    assert_eq!(stats.captured + stats.dropped, 50);
    assert!(stats.dropped > 0);
}
