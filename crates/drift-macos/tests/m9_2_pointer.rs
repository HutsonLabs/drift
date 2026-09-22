//! M9-2 Red: the pointer decoder survives damaged fast-path updates.
//!
//! `PointerDecoder` (M2-5) parses server-controlled bitmaps with server-controlled sizes, the
//! classic place for an out-of-bounds slice. `crates/drift-macos/fuzz` hunts for crashes
//! overnight; this is the deterministic merge-gate mirror: damaged copies of the captured
//! g-r-d pointer updates must return an error, never panic, and never leave the decoder
//! holding a bitmap that disagrees with its own size.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use drift_macos::cursor::PointerDecoder;
use drift_testkit::fixtures;

const SCALE100: &str = "pdus/fastpath_pointer_scale100.rec";
const SCALE200: &str = "pdus/fastpath_pointer_scale200.rec";

/// Reproducible xorshift PRNG.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 { 0 } else { (self.next_u64() % bound as u64) as usize }
    }

    /// A damaged copy of `seed`: byte flips plus an occasional truncation or extension.
    fn damage(&mut self, seed: &[u8]) -> Vec<u8> {
        let mut input = seed.to_vec();
        match self.next_u64() % 4 {
            0 => input.truncate(self.below(input.len().max(1))),
            1 => {
                let extra = self.below(64);
                input.extend((0..extra).map(|_| (self.next_u64() >> 24) as u8));
            }
            _ => {}
        }
        for _ in 0..1 + self.below(6) {
            if input.is_empty() {
                break;
            }
            let at = self.below(input.len());
            input[at] = (self.next_u64() >> 24) as u8;
        }
        input
    }
}

#[test]
fn damaged_pointer_updates_never_panic_and_keep_the_cache_consistent() {
    let mut rng = Rng(0x9002_5eed);
    let seeds: Vec<Vec<u8>> = [SCALE100, SCALE200]
        .iter()
        .flat_map(|name| fixtures::records(name).iter().map(|r| r.to_vec()).collect::<Vec<_>>())
        .collect();
    assert!(!seeds.is_empty(), "captured pointer updates are available");

    let mut decoder = PointerDecoder::new(25);
    let mut decoded = 0_usize;
    for _ in 0..200 {
        for seed in &seeds {
            let input = rng.damage(seed);
            if let Ok(events) = decoder.decode_output_pdu(&input) {
                decoded += events.len();
            }
            // Whatever the decoder cached must describe its own bitmap exactly.
            for slot in 0..25u16 {
                if let Some(image) = decoder.cached(slot) {
                    let expected = image.size.width as usize * image.size.height as usize * 4;
                    assert_eq!(image.bgra.len(), expected, "cache slot {slot} bitmap size");
                    assert!(image.hotspot.x < image.size.width.max(1), "hotspot inside the bitmap");
                    assert!(image.hotspot.y < image.size.height.max(1), "hotspot inside the bitmap");
                }
            }
        }
    }
    assert!(decoded > 0, "the sweep must decode some pointers, not only reject them");
}
