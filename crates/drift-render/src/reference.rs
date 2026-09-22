//! A pure-CPU model of the compositor, used to generate and cross-check the GPU goldens.
//!
//! [`CpuCompositor`] implements [`FrameSink`] with the same semantics (and the same
//! [`clip`](crate::clip) rules) as the Metal [`Compositor`](crate::Compositor), plus the
//! M4-3 present step ([`CpuCompositor::present`]). It only understands
//! [`Nv12Planes`](drift_core::Nv12Planes) frames.

use std::collections::{BTreeMap, HashMap};

use drift_core::{Bgra, Nv12Frame, Nv12Planes, Point, Rect, Size};
use drift_gfx::{FrameSink, PresentedCallback};

use crate::clip::{clip_copy, clip_rect};
use crate::color::nv12_to_rgb;
use crate::image::BgraImage;
use crate::layout::{Filter, present_layout};

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
        let mut out = BgraImage::filled(self.output, [0, 0, 0, 255]);
        for (img, origin) in self.surfaces.values() {
            if let Some(origin) = origin {
                copy(img, full(img), &mut out, *origin);
            }
        }
        out
    }

    /// The composed output as presented on a `drawable`-sized target (M4-3 layout).
    pub fn present(&self, drawable: Size<u32>) -> BgraImage {
        let src = self.composite();
        let mut out = BgraImage::filled(drawable, [0, 0, 0, 255]);
        let Some(layout) = present_layout(src.size, drawable) else {
            return out;
        };
        let vp = layout.viewport;
        for y in 0..vp.height {
            for x in 0..vp.width {
                let px = match layout.filter {
                    Filter::Nearest => src.pixel(x, y),
                    Filter::Linear => bilinear(&src, vp, x, y),
                };
                out.set_pixel(vp.x + x, vp.y + y, [px[0], px[1], px[2], 255]);
            }
        }
        out
    }

    fn surface_mut(&mut self, id: u16) -> Option<&mut BgraImage> {
        self.surfaces.get_mut(&id).map(|(img, _)| img)
    }
}

fn full(img: &BgraImage) -> Rect {
    Rect::new(0, 0, img.size.width, img.size.height)
}

/// Copies `rect` of `src` to `dst` at `at`, clipped.
fn copy(src: &BgraImage, rect: Rect, dst: &mut BgraImage, at: Point<u32>) {
    if let Some(c) = clip_copy(rect, src.size, at, dst.size) {
        for y in 0..c.size.height {
            for x in 0..c.size.width {
                dst.set_pixel(c.dst.x + x, c.dst.y + y, src.pixel(c.src.x + x, c.src.y + y));
            }
        }
    }
}

/// Bilinear, clamp-to-edge sample of `src` for viewport pixel `(x, y)` (pixel centres).
fn bilinear(src: &BgraImage, vp: Rect, x: u32, y: u32) -> [u8; 4] {
    let coord = |p: u32, vp_len: u32, len: u32| {
        let t = (p as f32 + 0.5) / vp_len as f32 * len as f32 - 0.5;
        let t = t.clamp(0.0, (len - 1) as f32);
        let i0 = t.floor();
        // `t` is clamped to 0..len-1, so the casts are exact.
        (i0 as u32, (i0 as u32 + 1).min(len - 1), t - i0)
    };
    let (x0, x1, fx) = coord(x, vp.width, src.size.width);
    let (y0, y1, fy) = coord(y, vp.height, src.size.height);
    let (a, b, c, d) = (src.pixel(x0, y0), src.pixel(x1, y0), src.pixel(x0, y1), src.pixel(x1, y1));
    std::array::from_fn(|i| {
        let top = f32::from(a[i]) * (1.0 - fx) + f32::from(b[i]) * fx;
        let bottom = f32::from(c[i]) * (1.0 - fx) + f32::from(d[i]) * fx;
        (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8
    })
}

impl FrameSink for CpuCompositor {
    fn reset(&mut self, output: Size<u32>) {
        self.output = output;
        self.surfaces.clear();
        self.cache.clear();
    }

    fn create_surface(&mut self, id: u16, size: Size<u32>) {
        self.surfaces.insert(id, (BgraImage::filled(size, [0, 0, 0, 255]), None));
    }

    fn delete_surface(&mut self, id: u16) {
        self.surfaces.remove(&id);
    }

    fn map_surface_to_output(&mut self, id: u16, origin: Point<u32>) {
        if let Some((_, o)) = self.surfaces.get_mut(&id) {
            *o = Some(origin);
        }
    }

    fn blit_bgra(&mut self, id: u16, rect: Rect, stride: usize, data: &[u8]) {
        let Some(tile) = bgra_tile(rect, stride, data) else {
            return;
        };
        if let Some(dst) = self.surface_mut(id) {
            copy(&tile, full(&tile), dst, Point::new(rect.x, rect.y));
        }
    }

    fn blit_nv12(&mut self, id: u16, frame: &Nv12Frame, regions: &[Rect]) {
        let Some(planes) = frame.downcast_ref::<Nv12Planes>() else {
            return;
        };
        let fsize = frame.size();
        let Some(dst) = self.surface_mut(id) else {
            return;
        };
        let bounds = Size::new(dst.size.width.min(fsize.width), dst.size.height.min(fsize.height));
        let w = fsize.width as usize;
        for r in regions.iter().filter_map(|r| clip_rect(*r, bounds)) {
            for y in r.y..r.bottom() {
                for x in r.x..r.right() {
                    let (xu, yu) = (x as usize, y as usize);
                    let yv = planes.y()[yu * w + xu];
                    let c = (yu / 2) * w + (xu / 2) * 2;
                    let [r8, g8, b8] = nv12_to_rgb(yv, planes.uv()[c], planes.uv()[c + 1]);
                    dst.set_pixel(x, y, [b8, g8, r8, 255]);
                }
            }
        }
    }

    fn solid_fill(&mut self, id: u16, color: Bgra, rects: &[Rect]) {
        let Some(dst) = self.surface_mut(id) else {
            return;
        };
        let size = dst.size;
        for r in rects.iter().filter_map(|r| clip_rect(*r, size)) {
            for y in r.y..r.bottom() {
                for x in r.x..r.right() {
                    dst.set_pixel(x, y, color.to_bytes());
                }
            }
        }
    }

    fn surface_to_surface(&mut self, src: u16, dst: u16, rect: Rect, dests: &[Point<u32>]) {
        let Some(snapshot) = self.surface(src).and_then(|s| clip_rect(rect, s.size).map(|r| s.crop(r)))
        else {
            return;
        };
        if let Some(dst) = self.surface_mut(dst) {
            for at in dests {
                copy(&snapshot, full(&snapshot), dst, *at);
            }
        }
    }

    fn surface_to_cache(&mut self, id: u16, rect: Rect, slot: u16) {
        if let Some(entry) = self.surface(id).and_then(|s| clip_rect(rect, s.size).map(|r| s.crop(r))) {
            self.cache.insert(slot, entry);
        }
    }

    fn cache_to_surface(&mut self, slot: u16, id: u16, dests: &[Point<u32>]) {
        let Some(entry) = self.cache.get(&slot) else {
            return;
        };
        if let Some((dst, _)) = self.surfaces.get_mut(&id) {
            for at in dests {
                copy(entry, full(entry), dst, *at);
            }
        }
    }

    fn evict_cache(&mut self, slot: u16) {
        self.cache.remove(&slot);
    }

    fn end_frame(&mut self, _frame_id: u32, presented: PresentedCallback) {
        presented();
    }

    fn set_visible(&mut self, _visible: bool) {}
}

/// Validates a BGRA upload (`stride` bytes per row) and repacks it tightly.
/// `None` when the rectangle is empty or `data` is too short for it.
pub(crate) fn bgra_tile(rect: Rect, stride: usize, data: &[u8]) -> Option<BgraImage> {
    let row = (rect.width as usize).checked_mul(4)?;
    let h = rect.height as usize;
    if rect.is_empty() || stride < row || data.len() < stride.checked_mul(h - 1)?.checked_add(row)? {
        return None;
    }
    let mut out = Vec::with_capacity(row.checked_mul(h)?);
    for y in 0..h {
        out.extend_from_slice(&data[y * stride..y * stride + row]);
    }
    Some(BgraImage { size: rect.size(), data: out })
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
