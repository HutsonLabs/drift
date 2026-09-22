//! M1-4 Red: a 64×64 progressive tile decodes in < 100 µs in a release build.
//!
//! Timing is only meaningful in an optimised, uninstrumented build on an idle machine, so
//! the test is ignored by default. Run it with
//! `cargo test --release -p drift-codec --test tile_budget -- --ignored`; the criterion
//! bench `cargo bench -p drift-codec --bench progressive_tile` reports the same figure.
#![allow(clippy::unwrap_used)]

mod support;

use std::time::{Duration, Instant};

use drift_codec::{ProgressiveCodec, TilePool};
use drift_core::Size;

const BUDGET: Duration = Duration::from_micros(100);

#[test]
#[ignore = "timing budget: run with --release -- --ignored (see module docs)"]
fn progressive_64x64_tile_decodes_within_budget() {
    let stream = support::real_tile_stream();
    let mut codec = ProgressiveCodec::new(TilePool::new(4).unwrap());
    let surface = Size::new(1280, 800);
    for _ in 0..200 {
        assert_eq!(codec.decode(0, 0, surface, &stream).unwrap().len(), 1);
    }
    let mut samples: Vec<Duration> = (0..2000)
        .map(|_| {
            let t = Instant::now();
            let tiles = codec.decode(0, 0, surface, &stream).unwrap();
            let d = t.elapsed();
            assert_eq!(tiles.len(), 1);
            d
        })
        .collect();
    samples.sort();
    let median = samples[samples.len() / 2];
    eprintln!("64x64 progressive tile: median {median:?}, p90 {:?}", samples[samples.len() * 9 / 10]);
    assert!(median < BUDGET, "median {median:?} exceeds {BUDGET:?}");
}
