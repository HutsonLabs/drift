# M1-4 — CPU codecs: parallel RFX Progressive, BGRA tiles, GFX capture fixtures

- Status: accepted
- Date: 2026-09-22
- Code: `crates/drift-codec/`, `fixtures/gfx/`, `fixtures/goldens/*_progressive.png`

## Context

Plan §2 routes RFX Progressive, Planar and Uncompressed (and ClearCodec, which IronRDP also
implements) through `drift-codec`, "wrapping `ironrdp_graphics` with a rayon tile pool".
g-r-d 50.2 falls back to RFX Progressive whenever it cannot open a hardware H.264 session
(plan §1.4), so on such hosts every frame is progressive and decode time feeds straight into
the frame-ack latency that g-r-d throttles on.

`ironrdp_graphics::progressive::ProgressiveDecoder` is correct but strictly sequential: its
`decode_bitmap` decodes each tile (RLGR, dequantisation, inverse DWT, YCbCr→RGB) one after the
other, and it offers no hook to run tiles elsewhere. A 1280×800 full refresh is 260 tiles.

## Decisions

1. **Own the progressive state machine, reuse IronRDP's primitives.** `ProgressiveCodec`
   re-implements `ProgressiveDecoder::decode_bitmap`'s orchestration (per `(surface, context)`
   `SurfaceTiles`, surface-scoped DWT references for difference tiles, the CONTEXT-flag
   fallbacks, REGION clipping with the same work limit, frame bracketing via
   `begin_frame`/`end_frame`, `delete_context`/`delete_surface`/`reset` semantics) using only
   IronRDP's public API: `decode_progressive_stream`, `TileState::{decode_first,
   decode_upgrade, reconstruct_to_rgba}`, `SurfaceTiles`, `Region`. Difference tiles are
   reproduced by adding the retained reference after `decode_first` (exactly what IronRDP's
   private `decode_first_with_difference` does). No vendored IronRDP change was needed.
2. **Parallelism.** Each REGION is split into per-tile jobs. A job takes its `Box<TileState>`
   (and reference) out of the grid, so jobs share nothing and run with `par_iter_mut` on a
   dedicated rayon `TilePool` (default `min(available_parallelism, 8)` workers, shareable across
   sessions). A tile repeated within one REGION goes into a later "round" so each occurrence
   sees the previous one. Single-job rounds run inline (no pool hop), which keeps the one-tile
   latency at the sequential cost. Tiles re-rendered from state for clipping are also
   parallel.
3. **Equivalence is tested, not assumed.** Unit tests and the fixture test compare
   `ProgressiveCodec` against IronRDP's `ProgressiveDecoder` on the same inputs and require
   identical pixels and rectangles (simple/first/upgrade/difference tiles, duplicates, clipping,
   frame bracketing, context lifecycle, resize, error cases). Known, intentional difference:
   after an *error* the partial state may differ (we validate a whole round before decoding
   it, IronRDP stops at the first bad tile); the payload is rejected either way and the GFX
   layer maps it to `ProtocolError`.
4. **Output type.** Every codec returns `BgraTile { origin, size, data (BGRA8, tightly packed),
   update_rects }`. `BgraTile::blits()` yields `(rect, stride, &data[offset..])`, exactly the
   arguments of `FrameSink::blit_bgra`, so `drift-gfx` forwards tiles without copying.
   IronRDP's RGBA output is swizzled to BGRA inside the parallel job. Uncompressed XRGB is
   forced opaque, ARGB keeps alpha; Planar and ClearCodec are opaque (alpha travels in
   `CODECID_ALPHA`, as in `ironrdp_egfx`).
5. **Hostile input.** Every decoder returns `CodecError` and never panics (proptests over
   random bytes and over mutated real/synthetic streams). `WireToSurface1` destination
   rectangles are limited to 16-bit dimensions and 32 Mpx to bound allocations; progressive
   surfaces are limited to IronRDP's `MAX_SURFACE_DIM` (32768).

## Fixtures (captured 2026-09-22 against g-r-d 50.2)

The M0-3 fixture set had no progressive stream, so this task captured two with
`~/code/drift-spikes/progcap` (a copy of `probe2` whose GFX `DvcProcessor` is wrapped in a tee):

| File | Source |
|---|---|
| `fixtures/gfx/greeter_v81noavc.gfx` | Remote Login :3389, leg 2 (GDM greeter after RDSTLS), caps `[V8_1{}]` |
| `fixtures/gfx/headless_v81noavc.gfx` | Headless daemon :3392 (`drifttest2` session), caps `[V8_1{}]` |
| `fixtures/goldens/greeter_progressive.png` | reference image for the greeter capture |
| `fixtures/goldens/headless_progressive.png` | reference image for the headless capture |

- **DRFTGFX1 format:** the 8 bytes `DRFTGFX1`, then for every GFX DVC payload in arrival order
  a little-endian `u32` length and the payload bytes *as received* (RDP_SEGMENTED_DATA, still
  ZGFX-compressed). Replaying needs a fresh `zgfx::Decompressor` fed from the first record.
  The format is reusable for `drift-gfx` (M1-2) replay tests; the reader lives in
  `crates/drift-codec/tests/support/mod.rs` and can move to `drift-testkit` if shared.
- **Goldens** are the images IronRDP's `GraphicsPipelineClient` compositor produced from the
  very same bytes at capture time (alpha forced to 255). They validate drift-codec's decode,
  clipping and BGRA placement end to end; they cannot catch a bug shared with IronRDP's
  entropy/DWT primitives (the spikes verified those render the greeter correctly, plan §1.4).
  The plan's `fixtures/goldens/greeter.png` is an AVC-era screenshot taken at a different time
  (the greeter shows a clock), so the progressive golden has its own name.
- The captures contain only graphics (no credentials; the secret scan stays green). The
  greeter shows the host's user display names.

## Performance

`tests/tile_budget.rs` (ignored by default; `cargo test --release -p drift-codec --test
tile_budget -- --ignored`) decodes the largest real greeter tile repeatedly: median **61 µs**
per 64×64 tile on the dev machine while it was heavily loaded (load average ≈ 56 on 14 cores),
under the 100 µs budget. `cargo bench -p drift-codec --bench progressive_tile` reports the same
tile plus the largest greeter payload on 1 thread vs. the default pool.

## Workspace change

Release builds failed on this macOS 27 machine: `strip -S` (Cargo's default
`strip = "debuginfo"` for release) corrupts proc-macro dylibs ("mis-aligned LINKEDIT string
pool"), and rustc then fails with E0463. The workspace now sets
`[profile.release.build-override] strip = false` (host-only build scripts and proc-macros are
left unstripped; shipped binaries are unaffected).
