# M0-5 — Where `Nv12Frame` lives

- Status: accepted
- Date: 2026-09-22
- Code: `crates/drift-core/src/video.rs`

## Context

`FrameSink::blit_nv12(&mut self, id, frame: &Nv12Frame, regions)` (plan §3) is declared in
`drift-gfx` and implemented in `drift-render`; the frames are produced by `drift-video`
(VideoToolbox, IOSurface-backed `CVPixelBuffer`). `drift-gfx` must stay pure and testable,
`drift-render` must not depend on `drift-gfx`'s internals, and neither should depend on the other
just for this type.

## Decision

- `drift_core::video` defines the seam:
  - `trait Nv12Source: Send + Sync + Debug + 'static { fn size(&self) -> Size<u32>; fn as_any(&self) -> &dyn Any; }`
  - `struct Nv12Frame` — an opaque `Arc<dyn Nv12Source>` handle (`Clone`, cheap), with
    `size()` and `downcast_ref::<T>()`.
  - `struct Nv12Planes` — a CPU NV12 picture implementing `Nv12Source`, for tests and goldens.
  - `trait H264Decoder: Send { fn decode(&mut self, annex_b: &[u8]) -> Result<Option<Nv12Frame>, DecodeError>; fn reset(&mut self); }`
    — the AVC420 seam: `drift-gfx` owns a `Box<dyn H264Decoder>`; `drift-video` implements it.
- `drift-video` implements `Nv12Source` for its `CVPixelBuffer` wrapper; `drift-render` downcasts
  to that type (depending on `drift-video`) for zero-copy `CVMetalTextureCache` import, and
  handles `Nv12Planes` by uploading planes (golden tests).

Dependency graph: `drift-core` ← `drift-gfx` ← `drift-render` → `drift-video` → `drift-core`;
`drift-gfx` never depends on `drift-video`/`drift-render`.

## Consequences

- No extra crate; `drift-core` stays FFI-free (the trait object hides the CoreVideo type).
- Colour space is fixed by contract: BT.709 full range (plan §1.4).
