//! The per-session render thread.

use std::marker::PhantomData;

use drift_core::{Bgra, Nv12Frame, Point, Rect, Size};
use drift_gfx::{FrameSink, PresentedCallback};

/// A render thread owning a sink `S`.
pub struct RenderThread<S> {
    _s: PhantomData<S>,
}

/// A [`FrameSink`] forwarding to a [`RenderThread`].
pub struct RenderSink<S> {
    _s: PhantomData<fn() -> S>,
}

impl<S: FrameSink + 'static> RenderThread<S> {
    /// Spawns `drift-render-<label>` and builds the sink on it.
    pub fn spawn(label: &str, make: impl FnOnce() -> S + Send + 'static) -> std::io::Result<Self> {
        let _ = (label, make);
        todo!("M1-5")
    }
    /// A sink handle.
    pub fn sink(&self) -> RenderSink<S> {
        todo!("M1-5")
    }
    /// Runs `f` on the render thread.
    pub fn with<R: Send + 'static>(&self, f: impl FnOnce(&mut S) -> R + Send + 'static) -> R {
        let _ = f;
        todo!("M1-5")
    }
    /// Stops the thread.
    pub fn shutdown(self) {
        todo!("M1-5")
    }
}

impl<S: FrameSink + 'static> FrameSink for RenderSink<S> {
    fn reset(&mut self, _output: Size<u32>) {
        todo!()
    }
    fn create_surface(&mut self, _id: u16, _size: Size<u32>) {
        todo!()
    }
    fn delete_surface(&mut self, _id: u16) {
        todo!()
    }
    fn map_surface_to_output(&mut self, _id: u16, _origin: Point<u32>) {
        todo!()
    }
    fn blit_bgra(&mut self, _id: u16, _rect: Rect, _stride: usize, _data: &[u8]) {
        todo!()
    }
    fn blit_nv12(&mut self, _id: u16, _frame: &Nv12Frame, _regions: &[Rect]) {
        todo!()
    }
    fn solid_fill(&mut self, _id: u16, _color: Bgra, _rects: &[Rect]) {
        todo!()
    }
    fn surface_to_surface(&mut self, _src: u16, _dst: u16, _rect: Rect, _dests: &[Point<u32>]) {
        todo!()
    }
    fn surface_to_cache(&mut self, _id: u16, _rect: Rect, _slot: u16) {
        todo!()
    }
    fn cache_to_surface(&mut self, _slot: u16, _id: u16, _dests: &[Point<u32>]) {
        todo!()
    }
    fn evict_cache(&mut self, _slot: u16) {
        todo!()
    }
    fn end_frame(&mut self, _frame_id: u32, _presented: PresentedCallback) {
        todo!()
    }
    fn set_visible(&mut self, _visible: bool) {
        todo!()
    }
}
