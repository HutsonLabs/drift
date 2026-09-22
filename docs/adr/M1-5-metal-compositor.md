# M1-5 / M4-3 / M8-1 — Metal compositor design decisions

- Status: accepted
- Date: 2026-09-22
- Code: `crates/drift-render/`

## Context

Plan §2/§3 fix the shape (`FrameSink` on a per-session render thread, BGRA8 texture per
surface, BT.709 full-range NV12 shader, `presented` from command-buffer completion, nearest at
1:1 and linear + letterbox otherwise, IOSurface capture from a `CVPixelBufferPool`). Several
details were left open.

## Decisions

1. **Colour matrix = exact inverse of g-r-d's integer encoder, plus +0.5 floor compensation.**
   g-r-d 50.2 (`grd-avc-dual-view.comp`) uses 8.8 fixed point with `>> 8` (floor) and averages
   chroma over 2×2 blocks. The textbook BT.709 full-range inverse round-trips its output with
   errors up to 4; the exact inverse of `[[54,183,18],[-29,-99,128],[128,-116,-12]]/256` with
   `+0.5` added to Y, U and V round-trips the whole RGB cube within ±1 on the CPU (unit test)
   and within ±2 on the GPU (Red test). It is still "BT.709 full range" as g-r-d defines it.
   The MSL source is generated from `color::DECODE_MATRIX`/`FLOOR_BIAS`, and
   `color::nv12_to_rgb` is its CPU reference.
2. **NV12 placement.** `blit_nv12` treats the decoded picture as anchored at the surface
   origin and the region rects as surface coordinates (as FreeRDP's `gdi_SurfaceCommand_AVC420`
   does); each region is clipped to `min(surface, picture)` and converted by a compute kernel
   dispatched over exactly that rectangle, so nothing outside the regions is written.
3. **Finding the `CVPixelBuffer`.** `Nv12Frame` is an opaque `Arc<dyn Nv12Source>`
   (ADR M0-5). As that ADR says, drift-render depends on drift-video and downcasts to
   `drift_video::DecodedPicture`; this is done through a list of `PixelBufferAccessor`
   functions, with `DecodedPicture` and drift-render's own `PixelBufferNv12` built in, so other
   producers can be added with `Compositor::add_pixel_buffer_accessor`. CPU `Nv12Planes` are
   uploaded (goldens only).
4. **One command buffer per frame; `presented` exactly once.** All operations between two
   `end_frame`s are encoded into one command buffer (blit/compute encoders switched as needed;
   Metal's hazard tracking orders them). The callback is wrapped in a call-on-drop guard, so it
   runs exactly once even if the handler block or a render-thread message is dropped. While
   hidden, frames are committed (surface updates land) but not composed/presented; the callback
   still fires (the frame will never be shown), and `drift-gfx` suspends acks per M1-2/M6-3.
5. **Composite texture.** Surfaces are composed (opaque black, then mapped surfaces in id
   order) into a desktop-sized output texture each frame; the present pass samples it. At 1:1
   the fragment shader uses `texture.read` (bit-exact); otherwise bilinear, clamp-to-edge, into
   the aspect-preserving viewport from `layout::present_layout` over a black clear.
6. **Capture bound.** `kCVPixelBufferPoolAllocationThresholdKey = max_buffers` (default 6) plus
   a bounded channel (`queue_depth`, default 4); on exhaustion the capture frame is dropped and
   counted, never allocated. `kCVPixelBufferPoolMaximumBufferAgeKey = 0` keeps free buffers.
7. **Layer colour space.** `LayerTarget` tags the `CAMetalLayer` as sRGB so macOS colour
   matches the (sRGB) remote desktop on P3/XDR displays.
8. **Goldens live in `crates/drift-render/tests/goldens/`** (small PNGs, not git-lfs) and are
   generated from the pure-CPU `CpuCompositor` model (`DRIFT_UPDATE_GOLDENS=1`), never from the
   GPU output; each test also checks the golden is not stale versus the model.

## Consequences

- The render thread copies BGRA tile data once into its message (NV12 frames are shared
  handles); the compositor copies it once more into a staging `MTLBuffer`.
