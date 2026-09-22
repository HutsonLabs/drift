//! M8-2: VideoToolbox H.264 encoder (feature `recording`).
#![cfg(feature = "recording")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use drift_core::{Clock, Nv12Planes, Size};
use drift_testkit::ManualClock;
use drift_video::annexb::{AccessUnit, sps_profile_idc};
use drift_video::decode::VtDecoder;
use drift_video::encode::{EncodedFrame, EncoderConfig, PixelBuffer, VtEncoder, keyframe_gaps};
use drift_video::quality::psnr_nv12;

/// A smooth moving test picture in NV12 (full range).
fn synthetic(size: Size<u32>, n: u32) -> Nv12Planes {
    let (w, h) = (size.width, size.height);
    let mut y = Vec::with_capacity((w * h) as usize);
    for row in 0..h {
        for col in 0..w {
            let v = ((col + 3 * n) / 4 + row / 8) % 256;
            let in_box = (col + 1000 - (n * 6) % 1000) % 1000 < 160 && row > h / 3 && row < h / 3 + 160;
            y.push(if in_box { 235 } else { v as u8 });
        }
    }
    let mut uv = Vec::with_capacity((w * h.div_ceil(2)) as usize);
    for row in 0..h.div_ceil(2) {
        for col in 0..w / 2 {
            uv.push((96 + (col + n) / 8 % 64) as u8);
            uv.push((160 - (row / 4) % 64) as u8);
        }
    }
    Nv12Planes::new(size, y, uv).unwrap()
}

fn is_ci_vm() -> bool {
    std::env::var_os("CI").is_some()
}

struct Run {
    frames: Vec<EncodedFrame>,
    inputs: Vec<Nv12Planes>,
    encoder: VtEncoder,
}

/// Encodes `count` frames at 60 fps (with a little jitter: variable frame rate).
fn encode_frames(size: Size<u32>, count: u32) -> Run {
    let clock = ManualClock::new();
    let mut encoder = VtEncoder::new(EncoderConfig::new(size)).unwrap();
    let mut frames = Vec::new();
    let mut inputs = Vec::new();
    for n in 0..count {
        let planes = synthetic(size, n);
        let buf = PixelBuffer::new_nv12(size).unwrap();
        buf.write_nv12(&planes).unwrap();
        frames.extend(encoder.encode(&buf, clock.now()).unwrap());
        inputs.push(planes);
        clock.advance(Duration::from_micros(if n % 7 == 3 { 25_000 } else { 16_667 }));
    }
    frames.extend(encoder.flush().unwrap());
    Run { frames, inputs, encoder }
}

#[test]
fn roundtrip_120_frames_through_our_decoder() {
    let size = Size::new(1280, 800);
    let run = encode_frames(size, 120);
    assert_eq!(run.frames.len(), 120);
    if !is_ci_vm() {
        assert!(run.encoder.is_hardware_accelerated(), "expected the hardware H.264 encoder");
    }
    assert!(run.frames[0].is_keyframe());

    // SPS says High profile.
    let ps = run.frames[0].parameter_sets().unwrap();
    assert_eq!(sps_profile_idc(&ps.sps), Some(100));

    // PTS monotonic (and no reordering: output order == input order).
    for w in run.frames.windows(2) {
        assert!(w[1].pts() > w[0].pts(), "{:?} !> {:?}", w[1].pts(), w[0].pts());
    }
    assert_eq!(run.frames[0].pts(), Duration::ZERO);

    // Decode with drift-video's own decoder (Annex-B, as g-r-d would send) and compare.
    let mut dec = VtDecoder::new();
    let mut worst = f64::INFINITY;
    for (i, f) in run.frames.iter().enumerate() {
        let au = f.to_annex_b().unwrap();
        let parsed = AccessUnit::from_annex_b(&au).unwrap();
        assert_eq!(parsed.is_idr, f.is_keyframe());
        let pic = dec.decode_picture(&au).unwrap().expect("picture");
        let p = psnr_nv12(&pic.to_planes().unwrap(), &run.inputs[i]).unwrap();
        worst = worst.min(p);
    }
    eprintln!("roundtrip worst PSNR {worst:.2} dB");
    assert!(worst >= 35.0, "worst PSNR {worst:.2} dB < 35");
}

#[test]
fn keyframe_interval_is_honoured() {
    // 5 s of 60 fps (with jitter) must contain keyframes at most 2 s apart.
    let run = encode_frames(Size::new(640, 400), 300);
    let keys: Vec<Duration> = run.frames.iter().filter(|f| f.is_keyframe()).map(EncodedFrame::pts).collect();
    assert!(keys.len() >= 3, "keyframes at {keys:?}");
    let slack = Duration::from_millis(30);
    for gap in keyframe_gaps(&keys) {
        assert!(gap <= Duration::from_secs(2) + slack, "keyframe gap {gap:?} in {keys:?}");
    }
    let last = run.frames.last().unwrap().pts();
    assert!(last - *keys.last().unwrap() <= Duration::from_secs(2) + slack);
}

#[test]
fn resize_rebuilds_the_encoder_and_emits_a_keyframe() {
    let clock = ManualClock::new();
    let a = Size::new(640, 400);
    let b = Size::new(800, 600);
    let mut enc = VtEncoder::new(EncoderConfig::new(a)).unwrap();
    let mut out = Vec::new();
    for n in 0..10 {
        let buf = PixelBuffer::new_nv12(a).unwrap();
        buf.write_nv12(&synthetic(a, n)).unwrap();
        out.extend(enc.encode(&buf, clock.now()).unwrap());
        clock.advance(Duration::from_millis(16));
    }
    assert_eq!(enc.session_builds(), 1);
    let resize_pts = clock.elapsed();
    for n in 0..5 {
        let buf = PixelBuffer::new_nv12(b).unwrap();
        buf.write_nv12(&synthetic(b, n)).unwrap();
        out.extend(enc.encode(&buf, clock.now()).unwrap());
        clock.advance(Duration::from_millis(16));
    }
    out.extend(enc.flush().unwrap());
    assert_eq!(enc.session_builds(), 2);
    assert_eq!(enc.config().size, b);
    assert_eq!(out.len(), 15);
    let first_b = out.iter().find(|f| f.pts() >= resize_pts).unwrap();
    assert!(first_b.is_keyframe(), "first frame after resize must be a keyframe");
    // PTS stay monotonic across the rebuild.
    for w in out.windows(2) {
        assert!(w[1].pts() > w[0].pts());
    }
    // And the new stream decodes at the new size.
    let mut dec = VtDecoder::new();
    let mut last = None;
    for f in &out {
        last = dec.decode_picture(&f.to_annex_b().unwrap()).unwrap();
    }
    assert_eq!(drift_core::Nv12Source::size(&last.unwrap()), b);
}

#[test]
fn bgra_input_from_the_compositor_encodes() {
    let clock = ManualClock::new();
    let size = Size::new(320, 200);
    let mut enc = VtEncoder::new(EncoderConfig::new(size)).unwrap();
    let mut out = Vec::new();
    for n in 0..4u32 {
        let buf = PixelBuffer::new_bgra(size).unwrap();
        buf.fill_bgra(|x, y| [(x + n) as u8, y as u8, 128, 255]).unwrap();
        out.extend(enc.encode(&buf, clock.now()).unwrap());
        clock.advance(Duration::from_millis(16));
    }
    out.extend(enc.flush().unwrap());
    assert_eq!(out.len(), 4);
    assert!(out[0].is_keyframe());
}

#[test]
fn non_monotonic_timestamps_are_rejected() {
    let clock = ManualClock::new();
    let size = Size::new(320, 200);
    let mut enc = VtEncoder::new(EncoderConfig::new(size)).unwrap();
    let buf = PixelBuffer::new_nv12(size).unwrap();
    buf.write_nv12(&synthetic(size, 0)).unwrap();
    enc.encode(&buf, clock.now()).unwrap();
    assert!(enc.encode(&buf, clock.now()).is_err());
}
