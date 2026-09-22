//! View-to-desktop coordinate mapping. Owned by task **M2-3**.
//!
//! A [`Viewport`] describes where the remote desktop is drawn inside the `RemoteView`: an origin
//! and a uniform scale in points per desktop pixel. The same value must drive both the Metal
//! compositor's transform and pointer mapping, so clicks land exactly where the pixels are.
//!
//! View coordinates are **points with a top-left origin** (`RemoteView` is flipped, or
//! `drift-macos` converts with `height - y`). Desktop coordinates are pixels.

use drift_core::{DesktopSize, Point, Size, ViewGeometry};

/// How the desktop is placed in the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ScaleMode {
    /// Scale uniformly to fit the view, centred, letterboxed or pillarboxed as needed. With an
    /// adaptive desktop (desktop = view size in points or, with Retina, in pixels) this is an
    /// exact 1:1 mapping with no bars.
    #[default]
    Fit,
    /// One desktop pixel per backing (physical) pixel. Centred when smaller than the view,
    /// anchored top-left (and clipped) when larger.
    OneToOne,
}

/// Placement of the desktop inside the view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Top-left corner of the desktop image, in view points (may be positive for bars).
    pub origin: Point<f64>,
    /// View points per desktop pixel.
    pub points_per_pixel: f64,
    /// Desktop size in pixels.
    pub desktop: DesktopSize,
}

impl Viewport {
    /// Computes the placement of `desktop` in `view`.
    pub fn new(view: ViewGeometry, desktop: DesktopSize, mode: ScaleMode) -> Self {
        let _ = (view, mode);
        Self { origin: Point::new(0.0, 0.0), points_per_pixel: 1.0, desktop }
    }

    /// Size of the drawn desktop image in view points.
    pub fn image_size(&self) -> Size<f64> {
        Size::new(0.0, 0.0)
    }

    /// `true` when `point` (view points) lies on the desktop image, not on a bar.
    pub fn contains(&self, point: Point<f64>) -> bool {
        let _ = point;
        false
    }

    /// Maps a view point to a desktop pixel, clamped to the desktop (so drags past the edge or
    /// over a letterbox bar pin to the nearest edge pixel). `None` for an empty desktop.
    pub fn view_to_desktop(&self, point: Point<f64>) -> Option<Point<u16>> {
        let _ = point;
        None
    }
}

/// Convenience: `Viewport::new(view, desktop, mode).view_to_desktop(point)`.
pub fn view_to_desktop(
    point: Point<f64>,
    view: ViewGeometry,
    desktop: DesktopSize,
    mode: ScaleMode,
) -> Option<Point<u16>> {
    Viewport::new(view, desktop, mode).view_to_desktop(point)
}

#[cfg(test)]
#[path = "tests/coords.rs"]
mod tests;
