//! The `FrameSink` contract between `drift-gfx` (producer) and `drift-render` (consumer).

use drift_core::{Bgra, Nv12Frame, Point, Rect, Size};

/// Callback invoked once the frame has actually been presented on screen.
/// `drift-gfx` sends the GFX `FrameAcknowledge` from inside it (plan §1.4: ack after present).
pub type PresentedCallback = Box<dyn FnOnce() + Send>;

/// Receiver of decoded graphics operations, called on the session render thread (plan §3).
///
/// Surface ids and cache slots are the server's GFX identifiers. All rectangles are in
/// surface pixels and have already been bounds-checked by `drift-gfx` against the surface
/// they target; implementations may still clip defensively but must never panic.
pub trait FrameSink: Send {
    /// `ResetGraphics`: drop every surface and cache slot; the output is now `output` pixels.
    fn reset(&mut self, output: Size<u32>);
    /// `CreateSurface`: allocate a BGRA surface.
    fn create_surface(&mut self, id: u16, size: Size<u32>);
    /// `DeleteSurface`.
    fn delete_surface(&mut self, id: u16);
    /// `MapSurfaceToOutput`: place a surface on the output at `origin`.
    fn map_surface_to_output(&mut self, id: u16, origin: Point<u32>);
    /// Copy BGRA pixels (`stride` bytes per row) into `rect` of surface `id`.
    fn blit_bgra(&mut self, id: u16, rect: Rect, stride: usize, data: &[u8]);
    /// Convert a decoded NV12 picture (BT.709 full range) into surface `id`, writing only
    /// the pixels inside `regions` (the AVC420 metablock rectangles).
    fn blit_nv12(&mut self, id: u16, frame: &Nv12Frame, regions: &[Rect]);
    /// `SolidFill`.
    fn solid_fill(&mut self, id: u16, color: Bgra, rects: &[Rect]);
    /// `SurfaceToSurface`: copy `rect` of `src` to each destination point in `dst`.
    fn surface_to_surface(&mut self, src: u16, dst: u16, rect: Rect, dests: &[Point<u32>]);
    /// `SurfaceToCache`: store `rect` of surface `id` in cache `slot`.
    fn surface_to_cache(&mut self, id: u16, rect: Rect, slot: u16);
    /// `CacheToSurface`: draw cache `slot` into surface `id` at each destination point.
    fn cache_to_surface(&mut self, slot: u16, id: u16, dests: &[Point<u32>]);
    /// `EvictCacheEntry`.
    fn evict_cache(&mut self, slot: u16);
    /// `EndFrame`: present the composed output. `presented` must be called exactly once,
    /// after the frame is on screen (or immediately if it will never be shown).
    fn end_frame(&mut self, frame_id: u32, presented: PresentedCallback);
    /// Pause (`false`) or resume (`true`) presentation, e.g. for occluded tabs.
    fn set_visible(&mut self, visible: bool);
}
