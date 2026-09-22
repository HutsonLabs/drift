//! M2-5: fast-path pointer decode on the captured g-r-d pointer PDUs, cache behaviour and
//! point-size computation.
#![allow(missing_docs, clippy::unwrap_used)]

use std::sync::Arc;

use drift_core::{Point, Size};
use drift_macos::cursor::{CursorImage, CursorShape, PointerDecoder, PointerError, PointerEvent};
use drift_testkit::fixtures;
use proptest::prelude::*;

const SCALE100: &str = "pdus/fastpath_pointer_scale100.rec";
const SCALE200: &str = "pdus/fastpath_pointer_scale200.rec";

// ---------------------------------------------------------------------------------------
// Synthetic fast-path PDU builders (MS-RDPBCGR 2.2.9.1.2).

const PTR_NULL: u8 = 0x5;
const PTR_DEFAULT: u8 = 0x6;
const PTR_POSITION: u8 = 0x8;
const PTR_COLOR: u8 = 0x9;
const PTR_CACHED: u8 = 0xA;
const PTR_POINTER: u8 = 0xB;
const PTR_LARGE: u8 = 0xC;
const BITMAP: u8 = 0x1;

const FRAG_SINGLE: u8 = 0;
const FRAG_LAST: u8 = 1;
const FRAG_FIRST: u8 = 2;

fn update(code: u8, frag: u8, body: &[u8]) -> Vec<u8> {
    let mut u = vec![code | (frag << 4)];
    u.extend_from_slice(&(body.len() as u16).to_le_bytes());
    u.extend_from_slice(body);
    u
}

fn pdu(updates: &[Vec<u8>]) -> Vec<u8> {
    let payload: Vec<u8> = updates.concat();
    let len = payload.len() + 3;
    let mut p = vec![0x00, 0x80 | ((len >> 8) as u8), (len & 0xFF) as u8];
    p.extend_from_slice(&payload);
    p
}

/// A 32 bpp "new" pointer body: `w`×`h`, every pixel `bgra` (bottom-up rows), no AND mask.
fn pointer32_body(cache_index: u16, w: u16, h: u16, hotspot: (u16, u16), bgra: [u8; 4]) -> Vec<u8> {
    let xor: Vec<u8> = (0..usize::from(w) * usize::from(h)).flat_map(|_| bgra).collect();
    let mut b = Vec::new();
    for v in [32, cache_index, hotspot.0, hotspot.1, w, h, 0, xor.len() as u16] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&xor);
    b
}

fn only_shape(events: Vec<PointerEvent>) -> CursorShape {
    assert_eq!(events.len(), 1, "expected exactly one event, got {events:?}");
    match events.into_iter().next().unwrap() {
        PointerEvent::Shape(s) => s,
        other => panic!("expected a shape, got {other:?}"),
    }
}

fn only_image(events: Vec<PointerEvent>) -> Arc<CursorImage> {
    match only_shape(events) {
        CursorShape::Image(img) => img,
        other => panic!("expected an image, got {other:?}"),
    }
}

fn premultiply(bgra: [u8; 4]) -> [u8; 4] {
    let a = u32::from(bgra[3]);
    let m = |c: u8| ((u32::from(c) * a + 127) / 255) as u8;
    [m(bgra[0]), m(bgra[1]), m(bgra[2]), bgra[3]]
}

/// Independent reference: the 32 bpp XOR mask of a single-update `POINTER` PDU is bottom-up
/// BGRA; the decoder must output it top-down and premultiplied.
fn assert_matches_raw_xor(img: &CursorImage, xor: &[u8]) {
    let (w, h) = (img.size.width as usize, img.size.height as usize);
    assert_eq!(xor.len(), w * h * 4);
    assert_eq!(img.bgra.len(), w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let src = &xor[((h - 1 - y) * w + x) * 4..][..4];
            let want = premultiply([src[0], src[1], src[2], src[3]]);
            assert_eq!(img.pixel(x as u32, y as u32), Some(want), "pixel ({x},{y})");
        }
    }
}

// ---------------------------------------------------------------------------------------
// Captured g-r-d pointer updates.

#[test]
fn scale100_fixture_null_then_43x43_then_cached() {
    let records = fixtures::records(SCALE100);
    assert_eq!(records.len(), 3);
    let mut dec = PointerDecoder::default();

    assert_eq!(only_shape(dec.decode_output_pdu(&records[0]).unwrap()), CursorShape::Hidden);

    let img = only_image(dec.decode_output_pdu(&records[1]).unwrap());
    assert_eq!(img.size, Size::new(43, 43));
    assert_eq!(img.hotspot, Point::new(5, 5));
    // Record layout: fp header (3) + update header (3) + 16 bytes of pointer attributes.
    let xor = &records[1][3 + 3 + 16..][..43 * 43 * 4];
    assert_matches_raw_xor(&img, xor);
    assert!(img.bgra.chunks(4).any(|p| p[3] == 0xFF), "the arrow has opaque pixels");
    assert!(img.bgra.chunks(4).any(|p| p[3] == 0), "and transparent ones");

    let cached = only_image(dec.decode_output_pdu(&records[2]).unwrap());
    assert!(Arc::ptr_eq(&img, &cached), "CACHED 0 re-uses the stored image");
    assert!(Arc::ptr_eq(&img, &dec.cached(0).unwrap()));
}

#[test]
fn scale200_fixture_reassembles_86x86_fragments() {
    let records = fixtures::records(SCALE200);
    assert_eq!(records.len(), 4);
    let mut dec = PointerDecoder::default();

    assert_eq!(only_shape(dec.decode_output_pdu(&records[0]).unwrap()), CursorShape::Hidden);
    assert_eq!(dec.decode_output_pdu(&records[1]).unwrap(), vec![], "FIRST fragment: nothing yet");
    let img = only_image(dec.decode_output_pdu(&records[2]).unwrap());
    assert_eq!(img.size, Size::new(86, 86));
    assert_eq!(img.hotspot, Point::new(10, 10));

    // The same arrow as at scale 100, pixel-doubled: same size and hotspot in points.
    assert_eq!(img.size_points(200), Size::new(43.0, 43.0));
    assert_eq!(img.hotspot_points(200), Point::new(5.0, 5.0));

    let cached = only_image(dec.decode_output_pdu(&records[3]).unwrap());
    assert!(Arc::ptr_eq(&img, &cached));
}

#[test]
fn premultiplied_invariant_holds_for_fixture_pointers() {
    for rel in [SCALE100, SCALE200] {
        let mut dec = PointerDecoder::default();
        for rec in fixtures::records(rel) {
            for ev in dec.decode_output_pdu(&rec).unwrap() {
                if let PointerEvent::Shape(CursorShape::Image(img)) = ev {
                    for p in img.bgra.chunks(4) {
                        assert!(
                            p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3],
                            "{rel}: {p:?} not premultiplied"
                        );
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Points computation.

fn image(w: u32, h: u32, hx: u32, hy: u32) -> CursorImage {
    CursorImage {
        size: Size::new(w, h),
        hotspot: Point::new(hx, hy),
        bgra: vec![0u8; (w * h * 4) as usize].into(),
    }
}

#[test]
fn points_size_is_bitmap_over_scale() {
    let cases = [
        // (bitmap, hotspot, scale %, points, hotspot points)
        ((43, 43), (5, 5), 100, (43.0, 43.0), (5.0, 5.0)),
        ((86, 86), (10, 10), 200, (43.0, 43.0), (5.0, 5.0)),
        ((32, 32), (0, 0), 100, (32.0, 32.0), (0.0, 0.0)),
        ((60, 40), (15, 10), 150, (40.0, 40.0 * 2.0 / 3.0), (10.0, 10.0 * 2.0 / 3.0)),
        ((64, 64), (32, 32), 400, (16.0, 16.0), (8.0, 8.0)),
    ];
    for ((w, h), (hx, hy), scale, (pw, ph), (px, py)) in cases {
        let img = image(w, h, hx, hy);
        let s = img.size_points(scale);
        let p = img.hotspot_points(scale);
        assert!((s.width - pw).abs() < 1e-9 && (s.height - ph).abs() < 1e-9, "{w}x{h}@{scale}: {s:?}");
        assert!((p.x - px).abs() < 1e-9 && (p.y - py).abs() < 1e-9, "{w}x{h}@{scale}: {p:?}");
    }
}

#[test]
fn points_size_treats_bogus_scale_as_100() {
    let img = image(43, 43, 5, 5);
    assert_eq!(img.size_points(0), Size::new(43.0, 43.0));
    assert_eq!(img.hotspot_points(0), Point::new(5.0, 5.0));
}

// ---------------------------------------------------------------------------------------
// Cache.

#[test]
fn new_pointer_in_a_used_slot_evicts_the_old_image() {
    let mut dec = PointerDecoder::new(4);
    let red = only_image(
        dec.decode_output_pdu(&pdu(&[update(
            PTR_POINTER,
            FRAG_SINGLE,
            &pointer32_body(1, 2, 2, (0, 0), [0, 0, 255, 255]),
        )]))
        .unwrap(),
    );
    let blue = only_image(
        dec.decode_output_pdu(&pdu(&[update(
            PTR_POINTER,
            FRAG_SINGLE,
            &pointer32_body(1, 3, 3, (1, 1), [255, 0, 0, 255]),
        )]))
        .unwrap(),
    );
    assert_ne!(red, blue);
    let cached = only_image(
        dec.decode_output_pdu(&pdu(&[update(PTR_CACHED, FRAG_SINGLE, &1u16.to_le_bytes())])).unwrap(),
    );
    assert!(Arc::ptr_eq(&cached, &blue), "slot 1 now holds the newer pointer");
    assert_eq!(cached.pixel(0, 0), Some([255, 0, 0, 255]));
}

#[test]
fn other_slots_survive_an_eviction() {
    let mut dec = PointerDecoder::new(4);
    let a = only_image(
        dec.decode_output_pdu(&pdu(&[update(
            PTR_POINTER,
            FRAG_SINGLE,
            &pointer32_body(0, 2, 2, (0, 0), [1, 2, 3, 255]),
        )]))
        .unwrap(),
    );
    dec.decode_output_pdu(&pdu(&[update(
        PTR_POINTER,
        FRAG_SINGLE,
        &pointer32_body(1, 2, 2, (0, 0), [4, 5, 6, 255]),
    )]))
    .unwrap();
    dec.decode_output_pdu(&pdu(&[update(
        PTR_POINTER,
        FRAG_SINGLE,
        &pointer32_body(1, 2, 2, (0, 0), [7, 8, 9, 255]),
    )]))
    .unwrap();
    assert!(Arc::ptr_eq(&dec.cached(0).unwrap(), &a));
}

#[test]
fn cached_reference_to_an_empty_slot_is_a_cache_miss() {
    let mut dec = PointerDecoder::new(4);
    let err =
        dec.decode_output_pdu(&pdu(&[update(PTR_CACHED, FRAG_SINGLE, &2u16.to_le_bytes())])).unwrap_err();
    assert_eq!(err, PointerError::CacheMiss(2));
}

#[test]
fn cache_index_outside_the_cache_is_rejected() {
    let mut dec = PointerDecoder::new(2);
    let err = dec
        .decode_output_pdu(&pdu(&[update(
            PTR_POINTER,
            FRAG_SINGLE,
            &pointer32_body(5, 2, 2, (0, 0), [0, 0, 0, 255]),
        )]))
        .unwrap_err();
    assert_eq!(err, PointerError::CacheIndexOutOfRange { index: 5, size: 2 });
    let err =
        dec.decode_output_pdu(&pdu(&[update(PTR_CACHED, FRAG_SINGLE, &7u16.to_le_bytes())])).unwrap_err();
    assert_eq!(err, PointerError::CacheIndexOutOfRange { index: 7, size: 2 });
}

#[test]
fn reset_evicts_everything() {
    let mut dec = PointerDecoder::new(4);
    dec.decode_output_pdu(&pdu(&[update(
        PTR_POINTER,
        FRAG_SINGLE,
        &pointer32_body(0, 2, 2, (0, 0), [0, 0, 0, 255]),
    )]))
    .unwrap();
    assert!(dec.cached(0).is_some());
    dec.reset();
    assert!(dec.cached(0).is_none());
    assert_eq!(dec.cache_size(), 4);
    let err =
        dec.decode_output_pdu(&pdu(&[update(PTR_CACHED, FRAG_SINGLE, &0u16.to_le_bytes())])).unwrap_err();
    assert_eq!(err, PointerError::CacheMiss(0));
}

// ---------------------------------------------------------------------------------------
// Other update types.

#[test]
fn default_hidden_and_position_updates() {
    let mut dec = PointerDecoder::default();
    assert_eq!(
        only_shape(dec.decode_output_pdu(&pdu(&[update(PTR_DEFAULT, FRAG_SINGLE, &[])])).unwrap()),
        CursorShape::Default
    );
    assert_eq!(
        only_shape(dec.decode_output_pdu(&pdu(&[update(PTR_NULL, FRAG_SINGLE, &[])])).unwrap()),
        CursorShape::Hidden
    );
    let mut pos = Vec::new();
    pos.extend_from_slice(&640u16.to_le_bytes());
    pos.extend_from_slice(&400u16.to_le_bytes());
    assert_eq!(
        dec.decode_output_pdu(&pdu(&[update(PTR_POSITION, FRAG_SINGLE, &pos)])).unwrap(),
        vec![PointerEvent::Position(Point::new(640, 400))]
    );
}

#[test]
fn several_updates_in_one_pdu_and_non_pointer_updates_are_skipped() {
    let mut dec = PointerDecoder::default();
    let bitmap_update = update(BITMAP, FRAG_SINGLE, &[0, 0, 0, 0]);
    let events = dec
        .decode_output_pdu(&pdu(&[
            update(PTR_NULL, FRAG_SINGLE, &[]),
            bitmap_update,
            update(PTR_DEFAULT, FRAG_SINGLE, &[]),
        ]))
        .unwrap();
    assert_eq!(
        events,
        vec![PointerEvent::Shape(CursorShape::Hidden), PointerEvent::Shape(CursorShape::Default)]
    );
}

#[test]
fn color_pointer_24bpp_with_and_mask_transparency() {
    // 2x2, 24 bpp: rows are padded to 2 bytes (6 → 6), AND rows padded to 2 bytes.
    // Bottom row (first in the data): black, white. Top row: black (AND=1 → transparent), red.
    let mut body = Vec::new();
    for v in [3u16 /* cache index */, 1, 0, 2, 2] {
        body.extend_from_slice(&v.to_le_bytes());
    }
    let xor: Vec<u8> = vec![
        0, 0, 0, 255, 255, 255, // bottom row: black, white (BGR)
        0, 0, 0, 0, 0, 255, // top row: black, red
    ];
    // AND mask rows (bottom-up), MSB first: bottom row 00, top row 10 (first pixel transparent).
    let and: Vec<u8> = vec![0x00, 0x00, 0x80, 0x00];
    body.extend_from_slice(&(and.len() as u16).to_le_bytes());
    body.extend_from_slice(&(xor.len() as u16).to_le_bytes());
    body.extend_from_slice(&xor);
    body.extend_from_slice(&and);
    let mut dec = PointerDecoder::default();
    let img = only_image(dec.decode_output_pdu(&pdu(&[update(PTR_COLOR, FRAG_SINGLE, &body)])).unwrap());
    assert_eq!(img.size, Size::new(2, 2));
    assert_eq!(img.hotspot, Point::new(1, 0));
    assert_eq!(img.pixel(0, 0), Some([0, 0, 0, 0]), "AND=1 over black is transparent");
    assert_eq!(img.pixel(1, 0), Some([0, 0, 255, 255]), "red");
    assert_eq!(img.pixel(0, 1), Some([0, 0, 0, 255]), "black");
    assert_eq!(img.pixel(1, 1), Some([255, 255, 255, 255]), "white");
    assert!(dec.cached(3).is_some());
}

#[test]
fn large_pointer_update() {
    let (w, h) = (96u16, 96u16);
    let xor: Vec<u8> = (0..usize::from(w) * usize::from(h)).flat_map(|_| [10u8, 20, 30, 255]).collect();
    let mut body = Vec::new();
    for v in [32u16, 4, 48, 40, w, h] {
        body.extend_from_slice(&v.to_le_bytes());
    }
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&(xor.len() as u32).to_le_bytes());
    body.extend_from_slice(&xor);
    let mut dec = PointerDecoder::default();
    let img = only_image(dec.decode_output_pdu(&pdu(&[update(PTR_LARGE, FRAG_SINGLE, &body)])).unwrap());
    assert_eq!(img.size, Size::new(96, 96));
    assert_eq!(img.hotspot, Point::new(48, 40));
    assert_eq!(img.pixel(95, 95), Some([10, 20, 30, 255]));
    assert!(dec.cached(4).is_some());
}

#[test]
fn hotspot_outside_the_bitmap_is_clamped() {
    let mut dec = PointerDecoder::default();
    let img = only_image(
        dec.decode_output_pdu(&pdu(&[update(
            PTR_POINTER,
            FRAG_SINGLE,
            &pointer32_body(0, 4, 4, (9, 200), [0, 0, 0, 255]),
        )]))
        .unwrap(),
    );
    assert_eq!(img.hotspot, Point::new(3, 3));
}

#[test]
fn zero_sized_pointer_is_hidden() {
    let mut dec = PointerDecoder::default();
    let shape = only_shape(
        dec.decode_output_pdu(&pdu(&[update(
            PTR_POINTER,
            FRAG_SINGLE,
            &pointer32_body(0, 0, 0, (0, 0), [0; 4]),
        )]))
        .unwrap(),
    );
    assert_eq!(shape, CursorShape::Hidden);
}

// ---------------------------------------------------------------------------------------
// Fragments and malformed input.

#[test]
fn last_fragment_without_first_is_an_error() {
    let mut dec = PointerDecoder::default();
    let err = dec.decode_output_pdu(&pdu(&[update(PTR_POINTER, FRAG_LAST, &[1, 2, 3])])).unwrap_err();
    assert_eq!(err, PointerError::Fragment);
}

#[test]
fn synthetic_fragments_reassemble() {
    let body = pointer32_body(2, 8, 8, (3, 4), [9, 8, 7, 255]);
    let (a, b) = body.split_at(100);
    let mut dec = PointerDecoder::default();
    assert!(dec.decode_output_pdu(&pdu(&[update(PTR_POINTER, FRAG_FIRST, a)])).unwrap().is_empty());
    let img = only_image(dec.decode_output_pdu(&pdu(&[update(PTR_POINTER, FRAG_LAST, b)])).unwrap());
    assert_eq!(img.size, Size::new(8, 8));
    assert_eq!(img.pixel(7, 7), Some([9, 8, 7, 255]));
}

#[test]
fn truncated_pdus_are_errors_not_panics() {
    let records = fixtures::records(SCALE100);
    let full = &records[1];
    for cut in [0, 1, 2, 3, 5, 10, 22, full.len() / 2, full.len() - 1] {
        let mut dec = PointerDecoder::default();
        assert!(dec.decode_output_pdu(&full[..cut]).is_err(), "cut at {cut}");
    }
}

#[test]
fn xor_mask_size_mismatch_is_a_bitmap_error() {
    let mut body = pointer32_body(0, 4, 4, (0, 0), [0, 0, 0, 255]);
    // Claim a 5x4 bitmap with the 4x4 mask.
    body[8] = 5;
    let mut dec = PointerDecoder::default();
    let err = dec.decode_output_pdu(&pdu(&[update(PTR_POINTER, FRAG_SINGLE, &body)])).unwrap_err();
    assert!(matches!(err, PointerError::Bitmap(_) | PointerError::Malformed(_)), "{err:?}");
}

proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let mut dec = PointerDecoder::new(4);
        let _ = dec.decode_output_pdu(&data);
    }

    #[test]
    fn arbitrary_pointer_bodies_never_panic(
        code in prop::sample::select(vec![PTR_COLOR, PTR_POINTER, PTR_LARGE, PTR_CACHED, PTR_POSITION]),
        frag in 0u8..4,
        body in proptest::collection::vec(any::<u8>(), 0..300),
    ) {
        let mut dec = PointerDecoder::new(4);
        let _ = dec.decode_output_pdu(&pdu(&[update(code, frag, &body)]));
        let _ = dec.decode_output_pdu(&pdu(&[update(code, FRAG_LAST, &body)]));
    }
}
