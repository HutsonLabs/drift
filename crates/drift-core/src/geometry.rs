//! Geometry primitives shared by the protocol, render and platform layers.

use serde::{Deserialize, Serialize};

/// A 2-D size. `Size<u32>` is used for pixels, `Size<f64>` for AppKit points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Size<T> {
    /// Horizontal extent.
    pub width: T,
    /// Vertical extent.
    pub height: T,
}

impl<T> Size<T> {
    /// Creates a size.
    pub const fn new(width: T, height: T) -> Self {
        Self { width, height }
    }
}

/// Remote desktop size in pixels.
pub type DesktopSize = Size<u32>;

/// A 2-D point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Point<T> {
    /// Horizontal coordinate (grows to the right).
    pub x: T,
    /// Vertical coordinate (grows downwards, RDP convention).
    pub y: T,
}

impl<T> Point<T> {
    /// Creates a point.
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned pixel rectangle: origin `(x, y)` plus `width` × `height`.
///
/// Edges are half-open: the rectangle covers columns `x..x+width` and rows `y..y+height`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Rect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Rect {
    /// Creates a rectangle from origin and size.
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    /// Creates a rectangle from inclusive-left/top and exclusive-right/bottom edges
    /// (the RDP `RECT16` convention). Returns `None` when `right < left` or `bottom < top`.
    pub fn from_ltrb(left: u32, top: u32, right: u32, bottom: u32) -> Option<Self> {
        Some(Self { x: left, y: top, width: right.checked_sub(left)?, height: bottom.checked_sub(top)? })
    }

    /// Exclusive right edge (saturating).
    pub const fn right(&self) -> u32 {
        self.x.saturating_add(self.width)
    }

    /// Exclusive bottom edge (saturating).
    pub const fn bottom(&self) -> u32 {
        self.y.saturating_add(self.height)
    }

    /// Size of the rectangle.
    pub const fn size(&self) -> Size<u32> {
        Size::new(self.width, self.height)
    }

    /// `true` when the rectangle covers no pixels.
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// `true` when `self` lies entirely inside a surface of `bounds`.
    pub const fn fits_within(&self, bounds: Size<u32>) -> bool {
        self.right() <= bounds.width && self.bottom() <= bounds.height
    }
}

/// A BGRA8 colour (the byte order used by GFX `SolidFill` and Metal `BGRA8Unorm`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Bgra {
    /// Blue.
    pub b: u8,
    /// Green.
    pub g: u8,
    /// Red.
    pub r: u8,
    /// Alpha.
    pub a: u8,
}

impl Bgra {
    /// Creates a colour from its components.
    pub const fn new(b: u8, g: u8, r: u8, a: u8) -> Self {
        Self { b, g, r, a }
    }

    /// The colour as `[b, g, r, a]` bytes.
    pub const fn to_bytes(self) -> [u8; 4] {
        [self.b, self.g, self.r, self.a]
    }
}

/// Geometry of the local remote view, as reported by AppKit.
///
/// `points` is the view's bounds in points; `backing_scale` is
/// `NSWindow.backingScaleFactor` (2.0 on Retina displays).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ViewGeometry {
    /// View size in points.
    pub points: Size<f64>,
    /// Backing scale factor (pixels per point).
    pub backing_scale: f64,
}

impl ViewGeometry {
    /// Size of the view in physical pixels (rounded to nearest, never negative).
    pub fn pixels(&self) -> Size<u32> {
        fn px(v: f64) -> u32 {
            let r = v.round();
            if r.is_nan() || r <= 0.0 {
                0
            } else if r >= f64::from(u32::MAX) {
                u32::MAX
            } else {
                // `r` is finite and within `0..u32::MAX`, so the cast is exact.
                r as u32
            }
        }
        Size::new(px(self.points.width * self.backing_scale), px(self.points.height * self.backing_scale))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_edges_and_ltrb() {
        let r = Rect::new(10, 20, 30, 40);
        assert_eq!((r.right(), r.bottom()), (40, 60));
        assert_eq!(Rect::from_ltrb(10, 20, 40, 60), Some(r));
        assert_eq!(Rect::from_ltrb(10, 20, 5, 60), None);
        assert!(r.fits_within(Size::new(40, 60)));
        assert!(!r.fits_within(Size::new(39, 60)));
        assert!(Rect::new(0, 0, 0, 5).is_empty());
        assert_eq!(r.size(), Size::new(30, 40));
    }

    #[test]
    fn bgra_bytes() {
        assert_eq!(Bgra::new(1, 2, 3, 4).to_bytes(), [1, 2, 3, 4]);
    }

    #[test]
    fn view_geometry_pixels() {
        let g = ViewGeometry { points: Size::new(1280.0, 800.0), backing_scale: 2.0 };
        assert_eq!(g.pixels(), Size::new(2560, 1600));
        let g = ViewGeometry { points: Size::new(-1.0, f64::NAN), backing_scale: 1.0 };
        assert_eq!(g.pixels(), Size::new(0, 0));
        let g = ViewGeometry { points: Size::new(1e20, 1.0), backing_scale: 1.0 };
        assert_eq!(g.pixels().width, u32::MAX);
        assert_eq!(Point::new(1, 2), Point { x: 1, y: 2 });
    }
}
