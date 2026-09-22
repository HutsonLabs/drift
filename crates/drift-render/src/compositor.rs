//! The Metal compositor.

use std::sync::mpsc::Receiver;

use drift_core::{Bgra, Nv12Frame, Point, Rect, Size};
use drift_gfx::{FrameSink, PresentedCallback};

use crate::capture::{CaptureConfig, CaptureStats, CapturedFrame};
use crate::gpu::Gpu;
use crate::image::BgraImage;
use crate::target::{OffscreenTarget, PresentTarget};

/// Presentation counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// `end_frame` calls.
    pub frames: u64,
    /// Frames presented to the target.
    pub presents: u64,
}

/// Metal implementation of [`FrameSink`].
pub struct Compositor<T: PresentTarget> {
    _t: T,
}

impl<T: PresentTarget> Compositor<T> {
    /// Creates a compositor.
    pub fn new(gpu: Gpu, target: T) -> Self {
        let _ = gpu;
        Self { _t: target }
    }
    /// Waits for all GPU work.
    pub fn wait_idle(&mut self) {
        todo!("M1-5")
    }
    /// Reads a surface back.
    pub fn read_surface(&mut self, id: u16) -> Option<BgraImage> {
        let _ = id;
        todo!("M1-5")
    }
    /// Reads the composite back.
    pub fn read_output(&mut self) -> Option<BgraImage> {
        todo!("M1-5")
    }
    /// Counters.
    pub fn stats(&self) -> RenderStats {
        todo!("M1-5")
    }
    /// Starts recording.
    pub fn start_recording(&mut self, config: CaptureConfig) -> Receiver<CapturedFrame> {
        let _ = config;
        todo!("M8-1")
    }
    /// Stops recording.
    pub fn stop_recording(&mut self) {
        todo!("M8-1")
    }
    /// Recording?
    pub fn is_recording(&self) -> bool {
        todo!("M8-1")
    }
    /// Capture counters.
    pub fn capture_stats(&self) -> CaptureStats {
        todo!("M8-1")
    }
}

impl Compositor<OffscreenTarget> {
    /// Reads the offscreen target back.
    pub fn read_target(&mut self) -> BgraImage {
        todo!("M4-3")
    }
}

impl<T: PresentTarget> FrameSink for Compositor<T> {
    fn reset(&mut self, _output: Size<u32>) {
        todo!("M1-5")
    }
    fn create_surface(&mut self, _id: u16, _size: Size<u32>) {
        todo!("M1-5")
    }
    fn delete_surface(&mut self, _id: u16) {
        todo!("M1-5")
    }
    fn map_surface_to_output(&mut self, _id: u16, _origin: Point<u32>) {
        todo!("M1-5")
    }
    fn blit_bgra(&mut self, _id: u16, _rect: Rect, _stride: usize, _data: &[u8]) {
        todo!("M1-5")
    }
    fn blit_nv12(&mut self, _id: u16, _frame: &Nv12Frame, _regions: &[Rect]) {
        todo!("M1-5")
    }
    fn solid_fill(&mut self, _id: u16, _color: Bgra, _rects: &[Rect]) {
        todo!("M1-5")
    }
    fn surface_to_surface(&mut self, _src: u16, _dst: u16, _rect: Rect, _dests: &[Point<u32>]) {
        todo!("M1-5")
    }
    fn surface_to_cache(&mut self, _id: u16, _rect: Rect, _slot: u16) {
        todo!("M1-5")
    }
    fn cache_to_surface(&mut self, _slot: u16, _id: u16, _dests: &[Point<u32>]) {
        todo!("M1-5")
    }
    fn evict_cache(&mut self, _slot: u16) {
        todo!("M1-5")
    }
    fn end_frame(&mut self, _frame_id: u32, _presented: PresentedCallback) {
        todo!("M1-5")
    }
    fn set_visible(&mut self, _visible: bool) {
        todo!("M1-5")
    }
}
