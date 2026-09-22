//! Present layout (task M4-3): how the composed desktop maps onto the drawable.
//!
//! When the drawable is exactly the desktop size the picture is copied texel-for-texel
//! ([`Filter::Nearest`], bit-exact, so text stays pixel-sharp on Retina). Otherwise it is
//! scaled with [`Filter::Linear`] into the largest aspect-preserving rectangle, centred, with
//! black bars (letterbox / pillarbox) around it.

use drift_core::{Rect, Size};

/// Texture sampling used by the present pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    /// Texel fetch, 1:1 (bit-exact).
    Nearest,
    /// Bilinear sampling.
    Linear,
}

/// Where and how the desktop is drawn inside the drawable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentLayout {
    /// Sampling filter.
    pub filter: Filter,
    /// Destination rectangle in drawable pixels; everything outside is black.
    pub viewport: Rect,
}

/// Computes the present layout for a `desktop`-sized picture on a `drawable`-sized target.
/// Returns `None` if either size is empty (nothing to draw).
pub fn present_layout(desktop: Size<u32>, drawable: Size<u32>) -> Option<PresentLayout> {
    let _ = (desktop, drawable);
    todo!("M4-3")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(dw: u32, dh: u32, tw: u32, th: u32) -> Option<PresentLayout> {
        present_layout(Size::new(dw, dh), Size::new(tw, th))
    }

    #[test]
    fn table() {
        use Filter::*;
        let cases = [
            // desktop, drawable → filter, viewport
            ((1280, 800), (1280, 800), Nearest, Rect::new(0, 0, 1280, 800)),
            ((1280, 800), (2560, 1600), Linear, Rect::new(0, 0, 2560, 1600)),
            ((1280, 800), (1280, 1000), Linear, Rect::new(0, 100, 1280, 800)),
            ((1280, 800), (1600, 800), Linear, Rect::new(160, 0, 1280, 800)),
            ((32, 16), (32, 32), Linear, Rect::new(0, 8, 32, 16)),
            ((10, 20), (40, 20), Linear, Rect::new(15, 0, 10, 20)),
            ((2560, 1600), (1280, 800), Linear, Rect::new(0, 0, 1280, 800)),
            // Odd leftover: bars differ by at most one pixel.
            ((100, 100), (101, 50), Linear, Rect::new(25, 0, 50, 50)),
            ((3, 2), (10, 10), Linear, Rect::new(0, 1, 10, 7)),
        ];
        for ((dw, dh), (tw, th), filter, viewport) in cases {
            assert_eq!(
                layout(dw, dh, tw, th),
                Some(PresentLayout { filter, viewport }),
                "{dw}x{dh} on {tw}x{th}"
            );
        }
        assert_eq!(layout(0, 10, 10, 10), None);
        assert_eq!(layout(10, 10, 10, 0), None);
    }

    proptest::proptest! {
        #[test]
        fn viewport_fits_is_centred_and_keeps_aspect(dw in 1u32..9000, dh in 1u32..9000, tw in 1u32..9000, th in 1u32..9000) {
            let l = layout(dw, dh, tw, th).unwrap();
            let v = l.viewport;
            proptest::prop_assert!(v.right() <= tw && v.bottom() <= th);
            proptest::prop_assert!(v.width >= 1 && v.height >= 1);
            // One axis fills the drawable.
            proptest::prop_assert!(v.width == tw || v.height == th);
            // Centred within one pixel.
            proptest::prop_assert!((tw - v.right()).abs_diff(v.x) <= 1);
            proptest::prop_assert!((th - v.bottom()).abs_diff(v.y) <= 1);
            // Aspect preserved within rounding (one pixel on the scaled axis).
            let ideal_w = f64::from(v.height) * f64::from(dw) / f64::from(dh);
            let ideal_h = f64::from(v.width) * f64::from(dh) / f64::from(dw);
            proptest::prop_assert!((f64::from(v.width) - ideal_w).abs() <= 1.0 || (f64::from(v.height) - ideal_h).abs() <= 1.0);
            proptest::prop_assert_eq!(l.filter == Filter::Nearest, dw == tw && dh == th);
        }
    }
}
