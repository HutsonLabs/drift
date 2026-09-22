//! M1-4 Red: malformed server input never panics; every decoder returns `CodecError`.
#![allow(clippy::unwrap_used)]

mod support;

use std::sync::OnceLock;

use drift_codec::{ClearCodec, ProgressiveCodec, TilePool, UncompressedFormat, decode_planar, decode_uncompressed};
use drift_core::{Rect, Size};
use proptest::prelude::*;
use support::synth;

fn pool() -> TilePool {
    static POOL: OnceLock<TilePool> = OnceLock::new();
    POOL.get_or_init(|| TilePool::new(4).unwrap()).clone()
}

/// Real progressive payloads from the greeter capture (mutation seeds).
fn seeds() -> &'static [(u16, u32, Vec<u8>)] {
    static SEEDS: OnceLock<Vec<(u16, u32, Vec<u8>)>> = OnceLock::new();
    SEEDS.get_or_init(|| {
        let capture = support::read_fixture("gfx/greeter_v81noavc.gfx");
        let mut p = support::progressive_payloads(&capture);
        // Keep the payload that carries SYNC + CONTEXT plus a few ordinary ones.
        p.truncate(6);
        p
    })
}

#[derive(Debug, Clone)]
enum Mutation {
    Flip { at: usize, xor: u8 },
    Truncate { at: usize },
    Splice { at: usize, bytes: Vec<u8> },
}

fn mutation() -> impl Strategy<Value = Mutation> {
    prop_oneof![
        (any::<usize>(), 1u8..).prop_map(|(at, xor)| Mutation::Flip { at, xor }),
        any::<usize>().prop_map(|at| Mutation::Truncate { at }),
        (any::<usize>(), proptest::collection::vec(any::<u8>(), 1..16)).prop_map(|(at, bytes)| Mutation::Splice { at, bytes }),
    ]
}

fn mutate(mut data: Vec<u8>, muts: &[Mutation]) -> Vec<u8> {
    for m in muts {
        if data.is_empty() {
            break;
        }
        match m {
            Mutation::Flip { at, xor } => {
                let i = at % data.len();
                data[i] ^= xor;
            }
            Mutation::Truncate { at } => data.truncate(at % data.len()),
            Mutation::Splice { at, bytes } => {
                let i = at % data.len();
                data.splice(i..i, bytes.iter().copied());
            }
        }
    }
    data
}

fn small_rect() -> impl Strategy<Value = Rect> {
    (0u32..2000, 0u32..2000, 0u32..80, 0u32..80).prop_map(|(x, y, w, h)| Rect::new(x, y, w, h))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn progressive_random_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..512),
                                            w in 0u32..4000, h in 0u32..4000) {
        let mut codec = ProgressiveCodec::new(pool());
        let _ = codec.decode(1, 0, Size::new(w, h), &data);
    }

    #[test]
    fn progressive_mutated_captures_never_panic(idx in 0usize..6, muts in proptest::collection::vec(mutation(), 1..6)) {
        let seeds = seeds();
        let mut codec = ProgressiveCodec::new(pool());
        // Establish the context with the first (SYNC + CONTEXT) payload, then feed a mutant.
        let (s, c, first) = &seeds[0];
        let _ = codec.decode(*s, *c, Size::new(1280, 800), first);
        let (s, c, data) = &seeds[idx % seeds.len()];
        codec.begin_frame();
        let _ = codec.decode(*s, *c, Size::new(1280, 800), &mutate(data.clone(), &muts));
        codec.end_frame();
    }

    #[test]
    fn progressive_mutated_synthetic_tiles_never_panic(seed in any::<u32>(), x in 0u16..4, y in 0u16..3,
                                                       muts in proptest::collection::vec(mutation(), 1..8)) {
        let data = synth::single_tile_stream(x, y, seed);
        let mut codec = ProgressiveCodec::new(pool());
        let _ = codec.decode(3, 1, Size::new(250, 190), &mutate(data, &muts));
    }

    #[test]
    fn planar_never_panics(dest in small_rect(), data in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = decode_planar(dest, &data);
    }

    #[test]
    fn clearcodec_never_panics(dest in small_rect(), data in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let mut codec = ClearCodec::new();
        let _ = codec.decode(dest, &data);
    }

    #[test]
    fn uncompressed_never_panics(dest in small_rect(), argb in any::<bool>(),
                                 data in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let fmt = if argb { UncompressedFormat::Argb } else { UncompressedFormat::Xrgb };
        let ok = decode_uncompressed(dest, fmt, &data).is_ok();
        prop_assert_eq!(ok, !dest.is_empty() && data.len() == (dest.width * dest.height * 4) as usize);
    }
}
