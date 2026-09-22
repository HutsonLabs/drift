//! A pure-CPU model of the compositor, used to generate and cross-check the GPU goldens.
//!
//! [`CpuCompositor`] implements [`FrameSink`] with the same semantics (and the same
//! [`clip`](crate::clip) rules) as the Metal [`Compositor`](crate::Compositor), plus the
//! M4-3 present step ([`CpuCompositor::present`]). It only understands
//! [`Nv12Planes`](drift_core::Nv12Planes) frames.

use std::collections::{BTreeMap, HashMap};

use drift_core::{Bgra, Nv12Frame, Point, Rect, Size};
use drift_gfx::{FrameSink, PresentedCallback};

use crate::image::BgraImage;

/// CPU reference compositor.
#[derive(Debug, Default)]
pub struct CpuCompositor {
    output: Size<u32>,
    surfaces: BTreeMap<u16, (BgraImage, Option<Point<u32>>)>,
    cache: HashMap<u16, BgraImage>,
}

impl CpuCompositor {
    /// An empty compositor (0×0 output until the first `reset`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Surface pixels, if the surface exists.
    pub fn surface(&self, id: u16) -> Option<&BgraImage> {
        self.surfaces.get(&id).map(|(img, _)| img)
    }

    /// The composed output: opaque black, then every mapped surface in id order.
    pub fn composite(&self) -> BgraImage {
        todo!("M1-5")
    }

    /// The composed output as presented on a `drawable`-sized target (M4-3 layout).
    pub fn present(&self, drawable: Size<u32>) -> BgraImage {
        let _ = drawable;
        todo!("M4-3")
    }
}

impl FrameSink for CpuCompositor {
    fn reset(&mut self, output: Size<u32>) {
        let _ = output;
        todo!("M1-5")
    }
    fn create_surface(&mut self, id: u16, size: Size<u32>) {
        let _ = (id, size);
        todo!("M1-5")
    }
    fn delete_surface(&mut self, id: u16) {
        let _ = id;
        todo!("M1-5")
    }
    fn map_surface_to_output(&mut self, id: u16, origin: Point<u32>) {
        let _ = (id, origin);
        todo!("M1-5")
    }
    fn blit_bgra(&mut self, id: u16, rect: Rect, stride: usize, data: &[u8]) {
        let _ = (id, rect, stride, data);
        todo!("M1-5")
    }
    fn blit_nv12(&mut self, id: u16, frame: &Nv12Frame, regions: &[Rect]) {
        let _ = (id, frame, regions);
        todo!("M1-5")
    }
    fn solid_fill(&mut self, id: u16, color: Bgra, rects: &[Rect]) {
        let _ = (id, color, rects);
        todo!("M1-5")
    }
    fn surface_to_surface(&mut self, src: u16, dst: u16, rect: Rect, dests: &[Point<u32>]) {
        let _ = (src, dst, rect, dests);
        todo!("M1-5")
    }
    fn surface_to_cache(&mut self, id: u16, rect: Rect, slot: u16) {
        let _ = (id, rect, slot);
        todo!("M1-5")
    }
    fn cache_to_surface(&mut self, slot: u16, id: u16, dests: &[Point<u32>]) {
        let _ = (slot, id, dests);
        todo!("M1-5")
    }
    fn evict_cache(&mut self, slot: u16) {
        let _ = slot;
        todo!("M1-5")
    }
    fn end_frame(&mut self, _frame_id: u32, presented: PresentedCallback) {
        presented();
    }
    fn set_visible(&mut self, _visible: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Bgra = Bgra::new(0, 0, 255, 255);

    #[test]
    fn overlapping_self_copy_reads_source_first() {
        let mut c = CpuCompositor::new();
        c.reset(Size::new(4, 1));
        c.create_surface(1, Size::new(4, 1));
        c.blit_bgra(1, Rect::new(0, 0, 4, 1), 16, &[1, 0, 0, 255, 2, 0, 0, 255, 3, 0, 0, 255, 4, 0, 0, 255]);
        c.surface_to_surface(1, 1, Rect::new(0, 0, 3, 1), &[Point::new(1, 0)]);
        let s = c.surface(1).unwrap();
        assert_eq!([s.pixel(0, 0)[0], s.pixel(1, 0)[0], s.pixel(2, 0)[0], s.pixel(3, 0)[0]], [1, 1, 2, 3]);
    }

    #[test]
    fn new_surfaces_are_opaque_black_and_composite_in_id_order() {
        let mut c = CpuCompositor::new();
        c.reset(Size::new(4, 2));
        c.create_surface(2, Size::new(2, 2));
        c.create_surface(1, Size::new(4, 2));
        c.solid_fill(1, RED, &[Rect::new(0, 0, 4, 2)]);
        c.map_surface_to_output(2, Point::new(2, 0));
        c.map_surface_to_output(1, Point::new(0, 0));
        let out = c.composite();
        assert_eq!(out.pixel(0, 0), RED.to_bytes());
        assert_eq!(out.pixel(3, 1), [0, 0, 0, 255], "surface 2 drawn over surface 1");
    }

    #[test]
    fn cache_and_reset() {
        let mut c = CpuCompositor::new();
        c.reset(Size::new(4, 4));
        c.create_surface(1, Size::new(4, 4));
        c.solid_fill(1, RED, &[Rect::new(0, 0, 2, 2)]);
        c.surface_to_cache(1, Rect::new(0, 0, 2, 2), 3);
        c.cache_to_surface(3, 1, &[Point::new(2, 2), Point::new(3, 3)]);
        assert_eq!(c.surface(1).unwrap().pixel(3, 3), RED.to_bytes());
        c.reset(Size::new(2, 2));
        assert!(c.surface(1).is_none());
        c.create_surface(1, Size::new(2, 2));
        c.cache_to_surface(3, 1, &[Point::new(0, 0)]);
        assert_eq!(c.surface(1).unwrap().pixel(0, 0), [0, 0, 0, 255], "reset clears the cache");
    }

    #[test]
    fn present_nearest_at_1to1_and_letterbox_otherwise() {
        let mut c = CpuCompositor::new();
        c.reset(Size::new(2, 1));
        c.create_surface(1, Size::new(2, 1));
        c.map_surface_to_output(1, Point::new(0, 0));
        c.solid_fill(1, RED, &[Rect::new(0, 0, 1, 1)]);
        assert_eq!(c.present(Size::new(2, 1)), c.composite());
        let p = c.present(Size::new(2, 3));
        assert_eq!(p.pixel(0, 0), [0, 0, 0, 255]);
        assert_eq!(p.pixel(0, 1), RED.to_bytes());
        assert_eq!(p.pixel(0, 2), [0, 0, 0, 255]);
    }
}
