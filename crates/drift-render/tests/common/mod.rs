//! Shared helpers for drift-render GPU tests.
#![allow(dead_code, clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use drift_core::{Bgra, Nv12Frame, Point, Rect, Size};
use drift_gfx::FrameSink;
use drift_render::{BgraImage, Compositor, Gpu, OffscreenTarget, color};
use drift_testkit::golden::GoldenImage;

/// Metal device shared by the tests.
pub fn gpu() -> Gpu {
    Gpu::system_default().expect("a Metal device (Apple Silicon test machines always have one)")
}

/// Offscreen compositor with a `drawable`-sized target.
pub fn offscreen(drawable: Size<u32>) -> Compositor<OffscreenTarget> {
    let gpu = gpu();
    let target = OffscreenTarget::new(&gpu, drawable).unwrap();
    Compositor::new(gpu, target)
}

/// Path of a golden PNG owned by this crate.
pub fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens").join(format!("{name}.png"))
}

/// Converts a render image to a golden image.
pub fn g(img: &BgraImage) -> GoldenImage {
    GoldenImage { width: img.size.width, height: img.size.height, bgra: img.data.clone() }
}

/// Deterministic BGRA gradient (`width * 4 + pad` bytes per row).
pub fn gradient(w: u32, h: u32, pad: usize, seed: u8) -> (usize, Vec<u8>) {
    let stride = w as usize * 4 + pad;
    let mut v = vec![0xEE; stride * h as usize];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let o = y * stride + x * 4;
            v[o] = (x as u8).wrapping_mul(23).wrapping_add(seed);
            v[o + 1] = (y as u8).wrapping_mul(41).wrapping_add(seed / 2);
            v[o + 2] = ((x + y) as u8).wrapping_mul(13) ^ seed;
            v[o + 3] = 255;
        }
    }
    (stride, v)
}

/// NV12 frame of `size` produced by g-r-d's integer encoder from a BGRA gradient.
pub fn nv12_gradient(size: Size<u32>, seed: u8) -> Nv12Frame {
    let (_, bgra) = gradient(size.width, size.height, 0, seed);
    Nv12Frame::new(color::grd_encode_nv12(size, &bgra).unwrap())
}

/// Counts `presented` callbacks per frame id.
#[derive(Clone, Default)]
pub struct PresentLog(pub Arc<Mutex<Vec<u32>>>);

impl PresentLog {
    pub fn callback(&self, frame_id: u32) -> Box<dyn FnOnce() + Send> {
        let log = self.0.clone();
        Box::new(move || log.lock().unwrap().push(frame_id))
    }
    pub fn ids(&self) -> Vec<u32> {
        self.0.lock().unwrap().clone()
    }
}

/// Scene scripts shared by the GPU compositor and the CPU reference model.
pub mod scenes {
    use super::*;

    pub const OUT: Size<u32> = Size::new(32, 24);

    fn base(s: &mut dyn FrameSink) {
        s.reset(OUT);
        s.create_surface(1, OUT);
        s.map_surface_to_output(1, Point::new(0, 0));
    }

    pub fn solid_fill(s: &mut dyn FrameSink) {
        base(s);
        s.solid_fill(1, Bgra::new(0, 0, 255, 255), &[Rect::new(2, 2, 20, 10)]);
        s.solid_fill(1, Bgra::new(255, 0, 0, 255), &[Rect::new(12, 6, 18, 16), Rect::new(0, 20, 4, 4)]);
        // Partly outside the surface: clipped, never a panic.
        s.solid_fill(1, Bgra::new(0, 255, 0, 255), &[Rect::new(28, 20, 10, 10)]);
    }

    pub fn bgra_blit_offset(s: &mut dyn FrameSink) {
        base(s);
        s.solid_fill(1, Bgra::new(40, 40, 40, 255), &[Rect::new(0, 0, 32, 24)]);
        let (stride, data) = gradient(10, 7, 8, 3);
        s.blit_bgra(1, Rect::new(5, 3, 10, 7), stride, &data);
        let (stride, data) = gradient(6, 6, 0, 99);
        // Partly outside: only the visible part lands.
        s.blit_bgra(1, Rect::new(29, 20, 6, 6), stride, &data);
    }

    pub fn surface_to_surface_overlap(s: &mut dyn FrameSink) {
        s.reset(OUT);
        s.create_surface(1, OUT);
        s.create_surface(2, Size::new(16, 16));
        s.map_surface_to_output(1, Point::new(0, 0));
        let (stride, data) = gradient(32, 24, 0, 7);
        s.blit_bgra(1, Rect::new(0, 0, 32, 24), stride, &data);
        // Overlapping copy within the same surface must read the source before writing.
        s.surface_to_surface(1, 1, Rect::new(0, 0, 16, 12), &[Point::new(4, 2), Point::new(20, 15)]);
        // Cross-surface copy, then back onto surface 1.
        s.solid_fill(2, Bgra::new(0, 128, 255, 255), &[Rect::new(0, 0, 16, 16)]);
        s.surface_to_surface(2, 1, Rect::new(4, 4, 6, 6), &[Point::new(0, 18)]);
    }

    pub fn cache_round_trip(s: &mut dyn FrameSink) {
        base(s);
        let (stride, data) = gradient(32, 24, 0, 51);
        s.blit_bgra(1, Rect::new(0, 0, 32, 24), stride, &data);
        s.surface_to_cache(1, Rect::new(3, 4, 9, 7), 5);
        s.solid_fill(1, Bgra::new(0, 0, 0, 255), &[Rect::new(0, 0, 32, 24)]);
        s.cache_to_surface(5, 1, &[Point::new(0, 0), Point::new(20, 10), Point::new(28, 20)]);
        s.evict_cache(5);
        // Evicted slot: no-op.
        s.cache_to_surface(5, 1, &[Point::new(10, 10)]);
    }

    pub fn nv12_region_limited(s: &mut dyn FrameSink) {
        base(s);
        s.solid_fill(1, Bgra::new(0, 255, 0, 255), &[Rect::new(0, 0, 32, 24)]);
        let frame = nv12_gradient(OUT, 17);
        s.blit_nv12(1, &frame, &[Rect::new(0, 0, 8, 8), Rect::new(16, 8, 10, 6), Rect::new(30, 22, 8, 8)]);
    }

    pub fn reset(s: &mut dyn FrameSink) {
        solid_fill(s);
        s.reset(Size::new(40, 20));
        s.create_surface(2, Size::new(20, 20));
        s.map_surface_to_output(2, Point::new(10, 0));
        s.solid_fill(2, Bgra::new(200, 100, 50, 255), &[Rect::new(0, 0, 20, 20)]);
        // Surface 1 is gone after reset: operations on it are ignored.
        s.solid_fill(1, Bgra::new(255, 255, 255, 255), &[Rect::new(0, 0, 4, 4)]);
    }
}
