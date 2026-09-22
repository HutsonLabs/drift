//! Fuzz target: the ZGFX decompressor on its own (M9-2).
//!
//! `gfx_pdu_zgfx` drives the whole graphics client, so most of its budget goes into the GFX
//! PDU dispatch and the CPU codecs. This target keeps one long-lived
//! `ironrdp_graphics::zgfx::Decompressor` — the object `drift_gfx::GfxClient` owns for the
//! life of a connection — and feeds it segment after segment, which is the only way to reach
//! the history buffer's cross-segment matches (RDP_SEGMENTED_DATA multipart, the match
//! distances that reference earlier output).
//!
//! The input is split into 1..=n chunks on a length prefix so one run exercises a sequence of
//! segments rather than a single one. Any panic, overflow or unbounded allocation is a bug;
//! `ZgfxError` is the expected outcome for damaged input.
#![no_main]

use ironrdp_graphics::zgfx;
use libfuzzer_sys::fuzz_target;

/// Cap on the decompressed output kept across segments, so a fuzz case cannot OOM the runner
/// with legitimately expanding input (the real client bounds this through the PDU sizes).
const MAX_OUTPUT: usize = 8 * 1024 * 1024;

fuzz_target!(|data: &[u8]| {
    let mut decompressor = zgfx::Decompressor::new();
    let mut output = Vec::new();
    let mut rest = data;
    while !rest.is_empty() {
        // First byte: how many of the remaining bytes form the next segment.
        let (&len, tail) = match rest.split_first() {
            Some(split) => split,
            None => break,
        };
        let take = usize::from(len).min(tail.len());
        let (segment, tail) = tail.split_at(take);
        rest = tail;
        if decompressor.decompress(segment, &mut output).is_err() {
            // A damaged segment poisons the history: the real client drops the connection.
            break;
        }
        if output.len() > MAX_OUTPUT {
            break;
        }
    }
    std::hint::black_box(output.len());
});
