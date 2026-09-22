//! Fuzz target: the remote pointer decoder (tasks M2-5 / M9-2).
//!
//! `PointerDecoder` turns server-controlled fast-path updates into bitmaps: the width, height,
//! bits-per-pixel, hotspot, cache slot and fragment lengths all come from the wire, and the
//! XOR/AND mask decoding walks buffers sized from those numbers. It also keeps state across
//! PDUs (fragment reassembly and the pointer cache), so the target feeds a **sequence** of
//! updates into one decoder rather than a single PDU: a `FIRST` fragment followed by a
//! mismatched `LAST`, or a `CACHED` update pointing at a slot filled by an earlier input, is
//! exactly the shape that only shows up across PDUs.
//!
//! After every accepted update the cached images are checked for self-consistency (the bitmap
//! length must match the advertised size and the hotspot must be inside it) — a corrupted
//! cache entry would otherwise only crash later, in AppKit.
//!
//! Seed the corpus with the records of `fixtures/pdus/fastpath_pointer_scale{100,200}.rec`.
#![no_main]

use drift_macos::cursor::PointerDecoder;
use libfuzzer_sys::fuzz_target;

/// Cache slots Drift advertises.
const CACHE_SLOTS: usize = 25;

fuzz_target!(|data: &[u8]| {
    let mut decoder = PointerDecoder::new(CACHE_SLOTS);
    let mut rest = data;
    // Each PDU is prefixed with a u16 length; a truncated prefix ends the run.
    while rest.len() >= 2 {
        let len = usize::from(u16::from_le_bytes([rest[0], rest[1]]));
        let take = len.min(rest.len() - 2);
        let (pdu, tail) = rest[2..].split_at(take);
        rest = tail;
        if let Ok(events) = decoder.decode_output_pdu(pdu) {
            std::hint::black_box(events.len());
        }
        for slot in 0..CACHE_SLOTS as u16 {
            if let Some(image) = decoder.cached(slot) {
                let expected = image.size.width as usize * image.size.height as usize * 4;
                assert_eq!(image.bgra.len(), expected, "cached bitmap disagrees with its size");
                assert!(image.hotspot.x < image.size.width.max(1), "hotspot outside the bitmap");
                assert!(image.hotspot.y < image.size.height.max(1), "hotspot outside the bitmap");
            }
        }
    }
});
