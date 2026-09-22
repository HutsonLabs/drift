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
    /// Computes the placement of `desktop` in `view`. Degenerate geometry (zero, NaN or infinite
    /// sizes or scale) falls back to one point per pixel at the top-left corner.
    pub fn new(view: ViewGeometry, desktop: DesktopSize, mode: ScaleMode) -> Self {
        let (vw, vh) = (view.points.width, view.points.height);
        let (dw, dh) = (f64::from(desktop.width), f64::from(desktop.height));
        let scale = match mode {
            ScaleMode::Fit => (vw / dw).min(vh / dh),
            ScaleMode::OneToOne => 1.0 / view.backing_scale,
        };
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        // Centre when the image is smaller than the view; anchor top-left when it is larger.
        // `f64::max` ignores NaN, so a NaN view size also lands at 0.
        let origin = Point::new(((vw - dw * scale) / 2.0).max(0.0), ((vh - dh * scale) / 2.0).max(0.0));
        Self { origin, points_per_pixel: scale, desktop }
    }

    /// Size of the drawn desktop image in view points.
    pub fn image_size(&self) -> Size<f64> {
        Size::new(
            f64::from(self.desktop.width) * self.points_per_pixel,
            f64::from(self.desktop.height) * self.points_per_pixel,
        )
    }

    /// `true` when `point` (view points) lies on the desktop image, not on a bar.
    pub fn contains(&self, point: Point<f64>) -> bool {
        let size = self.image_size();
        let (x, y) = (point.x - self.origin.x, point.y - self.origin.y);
        (0.0..size.width).contains(&x) && (0.0..size.height).contains(&y)
    }

    /// Maps a view point to a desktop pixel, clamped to the desktop (so drags past the edge or
    /// over a letterbox bar pin to the nearest edge pixel). `None` for an empty desktop.
    pub fn view_to_desktop(&self, point: Point<f64>) -> Option<Point<u16>> {
        if self.desktop.width == 0 || self.desktop.height == 0 {
            return None;
        }
        let x = to_pixel((point.x - self.origin.x) / self.points_per_pixel, self.desktop.width);
        let y = to_pixel((point.y - self.origin.y) / self.points_per_pixel, self.desktop.height);
        Some(Point::new(x, y))
    }
}

/// Floors a fractional pixel coordinate into `0..extent`, then into `u16`.
fn to_pixel(v: f64, extent: u32) -> u16 {
    // Absorb rounding noise such as 20.999999999 for an exact pixel boundary.
    const EPSILON: f64 = 1e-7;
    if v.is_nan() {
        return 0;
    }
    let max = f64::from(extent.saturating_sub(1).min(u32::from(u16::MAX)));
    // The value is clamped to `0..=u16::MAX`, so the cast is exact.
    (v + EPSILON).floor().clamp(0.0, max) as u16
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
