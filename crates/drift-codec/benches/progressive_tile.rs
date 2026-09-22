//! Criterion bench for plan M1-4: one 64×64 RFX Progressive tile (a real g-r-d greeter tile)
//! must decode to BGRA in < 100 µs in a release build. Also reports the largest greeter
//! payload on one thread and on the default pool.
#![allow(missing_docs, clippy::unwrap_used)]

#[path = "../tests/support/mod.rs"]
mod support;

use criterion::{Criterion, criterion_group, criterion_main};
use drift_codec::{ProgressiveCodec, TilePool};
use drift_core::Size;

fn bench(c: &mut Criterion) {
    let surface = Size::new(1280, 800);
    let tile = support::real_tile_stream();
    let mut codec = ProgressiveCodec::new(TilePool::with_default_threads().unwrap());
    c.bench_function("progressive_64x64_tile", |b| b.iter(|| codec.decode(0, 0, surface, &tile).unwrap()));

    let capture = support::read_fixture("gfx/greeter_v81noavc.gfx");
    let payloads = support::progressive_payloads(&capture);
    let (s, ctx, frame) = payloads.iter().max_by_key(|p| p.2.len()).unwrap().clone();
    for (name, pool) in [("1_thread", TilePool::new(1).unwrap()), ("default_pool", TilePool::with_default_threads().unwrap())] {
        let mut codec = ProgressiveCodec::new(pool);
        // Establish the codec context (the first payload carries SYNC + CONTEXT).
        codec.decode(payloads[0].0, payloads[0].1, surface, &payloads[0].2).unwrap();
        c.bench_function(&format!("progressive_largest_greeter_payload_{name}"), |b| {
            b.iter(|| codec.decode(s, ctx, surface, &frame).unwrap())
        });
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
