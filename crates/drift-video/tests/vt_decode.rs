//! M1-3: VideoToolbox decode of captured g-r-d AVC420 streams (fixtures/h264).
//!
//! - `leg3.h264`: the Remote Login leg-3 capture from the spike (42 access units, 1280×800).
//! - `headless_anim.h264`: full-screen `anim.py` motion on the headless session (407 access
//!   units, 1280×800); see docs/adr/M1-3-video-decode.md for provenance.
//! - `sps_change_640x400.h264`: a second stream with different SPS (resolution change).
//!
//! Goldens (`fixtures/h264/goldens/*.nv12`) are ffmpeg-decoded NV12 frames.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use drift_core::video::H264Decoder;
use drift_core::{Nv12Planes, Size};
use drift_video::annexb::{nal_type, nal_unit_type, split_nals};
use drift_video::decode::{DecodedPicture, OUTPUT_PIXEL_FORMAT, VtDecoder};
use drift_video::quality::psnr_nv12;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(drift_testkit::fixtures::fixtures_dir().join(name)).unwrap()
}

/// Splits a captured byte stream into access units (each starts with an AUD, as g-r-d sends).
fn access_units(stream: &[u8]) -> Vec<Vec<u8>> {
    let mut aus: Vec<Vec<u8>> = Vec::new();
    for nal in split_nals(stream) {
        if nal_unit_type(nal) == Some(nal_type::AUD) || aus.is_empty() {
            aus.push(Vec::new());
        }
        let au = aus.last_mut().unwrap();
        au.extend_from_slice(&[0, 0, 0, 1]);
        au.extend_from_slice(nal);
    }
    aus
}

fn golden(name: &str, size: Size<u32>) -> Nv12Planes {
    let data = fixture(&format!("h264/goldens/{name}.nv12"));
    let luma = (size.width * size.height) as usize;
    Nv12Planes::new(size, data[..luma].to_vec(), data[luma..].to_vec()).unwrap()
}

fn decode_all(name: &str) -> (VtDecoder, Vec<DecodedPicture>) {
    let stream = fixture(&format!("h264/{name}.h264"));
    let mut dec = VtDecoder::new();
    let mut out = Vec::new();
    for au in access_units(&stream) {
        if let Some(p) = dec.decode_picture(&au).unwrap() {
            out.push(p);
        }
    }
    (dec, out)
}

fn check_goldens(pictures: &[DecodedPicture], name: &str, size: Size<u32>, frames: &[usize]) {
    for &n in frames {
        let got = pictures[n].to_planes().unwrap();
        let want = golden(&format!("{name}_f{n:03}"), size);
        let p = psnr_nv12(&got, &want).unwrap();
        eprintln!("{name} frame {n}: PSNR {p:.2} dB");
        assert!(p >= 40.0, "{name} frame {n}: PSNR {p:.2} dB < 40");
    }
}

#[test]
fn leg3_decodes_every_access_unit_into_iosurface_nv12_full_range() {
    let (dec, pics) = decode_all("leg3");
    assert_eq!(pics.len(), 42);
    assert_eq!(dec.session_builds(), 1);
    for p in &pics {
        assert_eq!(drift_core::Nv12Source::size(p), Size::new(1280, 800));
        assert_eq!(p.pixel_format(), OUTPUT_PIXEL_FORMAT);
        assert!(p.is_iosurface_backed());
    }
    check_goldens(&pics, "leg3", Size::new(1280, 800), &[0, 20, 41]);
}

#[test]
fn headless_motion_stream_decodes_407_frames_matching_ffmpeg() {
    let (dec, pics) = decode_all("headless_anim");
    assert_eq!(pics.len(), 407);
    assert_eq!(dec.session_builds(), 1);
    check_goldens(&pics, "headless_anim", Size::new(1280, 800), &[0, 203, 406]);
}

#[test]
fn sps_change_rebuilds_the_session() {
    let leg3 = access_units(&fixture("h264/leg3.h264"));
    let small = access_units(&fixture("h264/sps_change_640x400.h264"));
    let mut dec = VtDecoder::new();
    for au in &leg3[..5] {
        dec.decode_picture(au).unwrap().unwrap();
    }
    assert_eq!(dec.session_builds(), 1);
    let mut last = None;
    for au in &small {
        if let Some(p) = dec.decode_picture(au).unwrap() {
            last = Some(p);
        }
    }
    assert_eq!(dec.session_builds(), 2, "new SPS must rebuild the session");
    let last = last.unwrap();
    assert_eq!(drift_core::Nv12Source::size(&last), Size::new(640, 400));
    let p = psnr_nv12(&last.to_planes().unwrap(), &golden("sps_change_640x400_f011", Size::new(640, 400))).unwrap();
    assert!(p >= 40.0, "PSNR {p:.2}");
    // An identical SPS/PPS resend (the headless capture uses the same parameter sets as leg3)
    // switches back once and then keeps the session.
    let anim = access_units(&fixture("h264/headless_anim.h264"));
    for au in &anim[..3] {
        dec.decode_picture(au).unwrap().unwrap();
    }
    assert_eq!(dec.session_builds(), 3);
    for au in &leg3[..1] {
        dec.decode_picture(au).unwrap().unwrap();
    }
    assert_eq!(dec.session_builds(), 3, "identical parameter sets keep the session");
}

#[test]
fn h264_decoder_trait_yields_downcastable_frames() {
    let leg3 = access_units(&fixture("h264/leg3.h264"));
    let mut dec: Box<dyn H264Decoder> = Box::new(VtDecoder::new());
    let frame = dec.decode(&leg3[0]).unwrap().unwrap();
    assert_eq!(frame.size(), Size::new(1280, 800));
    assert!(frame.downcast_ref::<DecodedPicture>().is_some());
    // Frames are shareable with the render thread.
    let f2 = frame.clone();
    std::thread::spawn(move || assert_eq!(f2.size(), Size::new(1280, 800))).join().unwrap();
    // After reset the decoder needs parameter sets again: a P-slice alone yields nothing.
    dec.reset();
    assert!(dec.decode(&leg3[1]).unwrap().is_none());
    assert!(dec.decode(&leg3[0]).unwrap().is_some());
}

#[test]
fn slices_before_parameter_sets_and_empty_units_yield_nothing() {
    let leg3 = access_units(&fixture("h264/leg3.h264"));
    let mut dec = VtDecoder::new();
    assert!(dec.decode_picture(&leg3[1]).unwrap().is_none());
    assert!(dec.decode_picture(&[0, 0, 0, 1, 0x09, 0x30]).unwrap().is_none());
    assert_eq!(dec.session_builds(), 0);
}

#[test]
fn garbage_is_an_error_not_a_panic() {
    let leg3 = access_units(&fixture("h264/leg3.h264"));
    let mut dec = VtDecoder::new();
    assert!(dec.decode_picture(&[]).is_err());
    dec.decode_picture(&leg3[0]).unwrap().unwrap();
    // Corrupt SPS: format description creation must fail cleanly.
    let bad_sps = [0, 0, 0, 1, 0x67, 0xFF, 0, 0, 0, 1, 0x68, 0xEE, 0, 0, 0, 1, 0x65, 0x88, 0x80];
    assert!(dec.decode_picture(&bad_sps).is_err());
    // The decoder recovers on the next good IDR.
    assert!(dec.decode_picture(&leg3[0]).unwrap().is_some());
}
