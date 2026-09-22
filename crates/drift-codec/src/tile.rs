//! [`BgraTile`]: the common output of every CPU codec.

use drift_core::{Point, Rect, Size};

/// Bytes per BGRA8 pixel.
pub(crate) const BPP: usize = 4;

/// A decoded BGRA8 pixel buffer placed on a surface.
///
/// `data` holds `size.width × size.height` pixels, row-major, tightly packed
/// (`stride = size.width × 4`), whose top-left pixel lands on surface pixel `origin`.
/// Only the pixels inside `update_rects` (surface coordinates, each contained in the
/// buffer's footprint) may be written to the surface: RFX Progressive tiles are always
/// 64×64 but are clipped to the REGION rectangles and to the surface bounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BgraTile {
    /// Surface position of the buffer's top-left pixel.
    pub origin: Point<u32>,
    /// Buffer dimensions in pixels.
    pub size: Size<u32>,
    /// BGRA8 pixels, `size.width * size.height * 4` bytes.
    pub data: Vec<u8>,
    /// Surface rectangles to blit from this buffer.
    pub update_rects: Vec<Rect>,
}

/// One blit taken from a [`BgraTile`], in the shape `FrameSink::blit_bgra` expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blit<'a> {
    /// Destination rectangle on the surface.
    pub rect: Rect,
    /// Bytes between the starts of consecutive rows in `data`.
    pub stride: usize,
    /// Pixels starting at the rectangle's top-left pixel. Holds at least
    /// `(rect.height - 1) * stride + rect.width * 4` bytes.
    pub data: &'a [u8],
}

impl BgraTile {
    /// A tile covering `rect` exactly, updated in full.
    pub(crate) fn full(rect: Rect, data: Vec<u8>) -> Self {
        Self { origin: Point::new(rect.x, rect.y), size: rect.size(), data, update_rects: vec![rect] }
    }

    /// Bytes per row of `data`.
    pub fn stride(&self) -> usize {
        self.size.width as usize * BPP
    }

    /// The buffer's footprint on the surface.
    pub fn footprint(&self) -> Rect {
        Rect::new(self.origin.x, self.origin.y, self.size.width, self.size.height)
    }

    /// The blits for every update rectangle. Rectangles that are empty or fall outside the
    /// buffer's footprint are skipped (never produced by this crate's decoders).
    pub fn blits(&self) -> impl Iterator<Item = Blit<'_>> + '_ {
        let _ = self;
        let v: Vec<Blit<'_>> = todo!("M1-4: BgraTile::blits");
        #[allow(unreachable_code)]
        v.into_iter()
    }

    /// Copies the update rectangles into a BGRA8 surface buffer of `surface` size
    /// (tightly packed). Rectangles outside the surface are clipped.
    pub fn blit_into(&self, surface: Size<u32>, dst: &mut [u8]) {
        let _ = (surface, dst);
        todo!("M1-4: BgraTile::blit_into")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered(w: u32, h: u32) -> Vec<u8> {
        (0..w * h).flat_map(|i| [i as u8, (i >> 8) as u8, 0, 255]).collect()
    }

    #[test]
    fn blits_start_at_the_rect_and_use_the_buffer_stride() {
        let tile = BgraTile {
            origin: Point::new(64, 128),
            size: Size::new(64, 64),
            data: numbered(64, 64),
            update_rects: vec![Rect::new(64, 128, 64, 64), Rect::new(70, 130, 10, 2)],
        };
        let blits: Vec<_> = tile.blits().collect();
        assert_eq!(blits.len(), 2);
        assert_eq!(blits[0].rect, Rect::new(64, 128, 64, 64));
        assert_eq!(blits[0].stride, 256);
        assert_eq!(blits[0].data.len(), 64 * 64 * 4);
        // (70,130) is local pixel (6,2) = index 2*64+6 = 134.
        assert_eq!(blits[1].stride, 256);
        assert_eq!(&blits[1].data[..4], &[134, 0, 0, 255]);
        assert!(blits[1].data.len() >= 256 + 10 * 4);
    }

    #[test]
    fn blits_skip_rects_outside_the_footprint() {
        let tile = BgraTile {
            origin: Point::new(0, 0),
            size: Size::new(4, 4),
            data: numbered(4, 4),
            update_rects: vec![Rect::new(2, 2, 4, 1), Rect::new(1, 1, 0, 3), Rect::new(0, 0, 4, 4)],
        };
        let rects: Vec<_> = tile.blits().map(|b| b.rect).collect();
        assert_eq!(rects, vec![Rect::new(0, 0, 4, 4)]);
    }

    #[test]
    fn blit_into_writes_only_update_rects_and_clips_to_the_surface() {
        let tile = BgraTile {
            origin: Point::new(2, 1),
            size: Size::new(4, 4),
            data: numbered(4, 4),
            update_rects: vec![Rect::new(3, 2, 3, 3)],
        };
        let surface = Size::new(5, 4);
        let mut dst = vec![9u8; 5 * 4 * 4];
        tile.blit_into(surface, &mut dst);
        let px = |x: usize, y: usize| dst[(y * 5 + x) * 4];
        assert_eq!(px(2, 1), 9, "outside update rect");
        assert_eq!(px(3, 2), 5, "local (1,1)");
        assert_eq!(px(4, 3), 10, "local (2,2)");
        assert_eq!(px(0, 0), 9);
        // Row 4 and column 5 are outside the 5x4 surface: nothing panics, nothing wraps.
        assert_eq!(dst.len(), 80);
    }

    #[test]
    fn full_tile_covers_its_rect() {
        let t = BgraTile::full(Rect::new(3, 4, 2, 1), vec![0; 8]);
        assert_eq!(t.footprint(), Rect::new(3, 4, 2, 1));
        assert_eq!(t.update_rects, vec![Rect::new(3, 4, 2, 1)]);
        assert_eq!(t.stride(), 8);
    }
}
