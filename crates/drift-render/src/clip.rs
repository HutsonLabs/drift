//! Defensive clipping shared by the Metal compositor and the CPU reference model.
//!
//! `drift-gfx` already bounds-checks every command, but the compositor must never panic or
//! hand Metal an out-of-range region (which aborts the process), so every operation is
//! clipped here first. All arithmetic saturates.

use drift_core::{Point, Rect, Size};

/// Clips `rect` to a surface of `bounds`. `None` if nothing remains.
pub fn clip_rect(rect: Rect, bounds: Size<u32>) -> Option<Rect> {
    let _ = (rect, bounds);
    todo!("M1-5")
}

/// A copy of `size` pixels from `src` (in the source) to `dst` (in the destination).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyRegion {
    /// Top-left of the copied pixels in the source.
    pub src: Point<u32>,
    /// Top-left of the destination.
    pub dst: Point<u32>,
    /// Size of the copy.
    pub size: Size<u32>,
}

/// Clips a copy of `src_rect` (inside a source of `src_bounds`) to destination point `dst`
/// (inside a destination of `dst_bounds`). `None` if nothing remains.
pub fn clip_copy(src_rect: Rect, src_bounds: Size<u32>, dst: Point<u32>, dst_bounds: Size<u32>) -> Option<CopyRegion> {
    let _ = (src_rect, src_bounds, dst, dst_bounds);
    todo!("M1-5")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_rect_table() {
        let b = Size::new(32, 24);
        assert_eq!(clip_rect(Rect::new(2, 2, 10, 10), b), Some(Rect::new(2, 2, 10, 10)));
        assert_eq!(clip_rect(Rect::new(28, 20, 10, 10), b), Some(Rect::new(28, 20, 4, 4)));
        assert_eq!(clip_rect(Rect::new(32, 0, 1, 1), b), None);
        assert_eq!(clip_rect(Rect::new(0, 0, 0, 5), b), None);
        assert_eq!(clip_rect(Rect::new(u32::MAX, u32::MAX, u32::MAX, u32::MAX), b), None);
        assert_eq!(clip_rect(Rect::new(1, 1, u32::MAX, u32::MAX), b), Some(Rect::new(1, 1, 31, 23)));
    }

    #[test]
    fn clip_copy_table() {
        let s = Size::new(32, 24);
        let r = |x, y, w, h| Rect::new(x, y, w, h);
        let c = |sx, sy, dx, dy, w, h| {
            Some(CopyRegion { src: Point::new(sx, sy), dst: Point::new(dx, dy), size: Size::new(w, h) })
        };
        assert_eq!(clip_copy(r(0, 0, 16, 12), s, Point::new(4, 2), s), c(0, 0, 4, 2, 16, 12));
        // Destination overflow trims the far edge.
        assert_eq!(clip_copy(r(0, 0, 16, 12), s, Point::new(20, 15), s), c(0, 0, 20, 15, 12, 9));
        // Source overflow trims the far edge too.
        assert_eq!(clip_copy(r(28, 20, 8, 8), s, Point::new(0, 0), s), c(28, 20, 0, 0, 4, 4));
        assert_eq!(clip_copy(r(40, 0, 8, 8), s, Point::new(0, 0), s), None);
        assert_eq!(clip_copy(r(0, 0, 8, 8), s, Point::new(u32::MAX, 0), s), None);
        assert_eq!(clip_copy(r(0, 0, 8, 8), s, Point::new(0, 0), Size::new(0, 0)), None);
    }

    proptest::proptest! {
        #[test]
        fn clipped_copies_stay_in_bounds(
            sx in 0u32..100, sy in 0u32..100, w in 0u32..100, h in 0u32..100,
            dx in 0u32..100, dy in 0u32..100,
            bw in 0u32..64, bh in 0u32..64, dw in 0u32..64, dh in 0u32..64,
        ) {
            if let Some(c) = clip_copy(Rect::new(sx, sy, w, h), Size::new(bw, bh), Point::new(dx, dy), Size::new(dw, dh)) {
                proptest::prop_assert!(c.size.width > 0 && c.size.height > 0);
                proptest::prop_assert!(c.src.x + c.size.width <= bw && c.src.y + c.size.height <= bh);
                proptest::prop_assert!(c.dst.x + c.size.width <= dw && c.dst.y + c.size.height <= dh);
                proptest::prop_assert_eq!((c.src.x, c.src.y, c.dst.x, c.dst.y), (sx, sy, dx, dy));
            }
        }
    }
}
