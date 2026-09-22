//! Colour conversion: g-r-d's AVC420 encoder and our NV12 → RGB decoder (plan §1.4).
//!
//! g-r-d 50.2 converts BGRX to YUV with integer 8.8 fixed-point arithmetic
//! (`src/shaders/grd-avc-dual-view.comp`):
//!
//! ```text
//! Y = (54R + 183G + 18B) >> 8
//! U = ((-29R - 99G + 128B) >> 8) + 128
//! V = ((128R - 116G - 12B) >> 8) + 128
//! ```
//!
//! and averages U/V over each 2×2 block. That is BT.709, full range. Our decoder uses the
//! **exact inverse** of that integer matrix ([`DECODE_MATRIX`]) and adds `+0.5` to Y, U and V
//! first to undo the `>> 8` floor bias ([`FLOOR_BIAS`]). Over the whole RGB cube this
//! round-trips within ±1 per channel on the CPU (see the unit tests); the plain textbook
//! BT.709 inverse would be off by up to 4. See `docs/adr/M1-5-metal-compositor.md`.
//!
//! [`nv12_to_rgb`] is the CPU reference of the Metal shader (`shaders.rs` is generated from
//! the same constants).

use drift_core::{Nv12Planes, Size};

/// Row-major 3×3 matrix mapping `(Y, U-128, V-128)` (after [`FLOOR_BIAS`]) to `(R, G, B)`,
/// all in 0..=255 units. Exact inverse of g-r-d's encoder matrix divided by 256.
pub const DECODE_MATRIX: [[f32; 3]; 3] = [
    [256.0 / 255.0, 2304.0 / 340_765.0, 537_728.0 / 340_765.0],
    [256.0 / 255.0, -62_976.0 / 340_765.0, -158_592.0 / 340_765.0],
    [256.0 / 255.0, 633_344.0 / 340_765.0, -832.0 / 340_765.0],
];

/// Added to Y, U and V before [`DECODE_MATRIX`]: the mean error of g-r-d's `>> 8` floor.
pub const FLOOR_BIAS: f32 = 0.5;

/// g-r-d's integer encoder for one pixel: `(Y, U, V)` before chroma subsampling.
pub fn grd_rgb_to_yuv(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    todo!("M1-5")
}

/// Encodes a tightly packed BGRA8 image to NV12 exactly as g-r-d's AVC420 main view does:
/// per-pixel [`grd_rgb_to_yuv`], chroma = rounded mean of the 2×2 block, edge pixels
/// replicated for odd heights. Returns `None` for an odd width or a wrong buffer length.
pub fn grd_encode_nv12(size: Size<u32>, bgra: &[u8]) -> Option<Nv12Planes> {
    todo!("M1-5")
}

/// CPU reference of the NV12 → RGB shader: `[r, g, b]`.
pub fn nv12_to_rgb(y: u8, u: u8, v: u8) -> [u8; 3] {
    todo!("M1-5")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoder_matches_grd_formula_samples() {
        assert_eq!(grd_rgb_to_yuv(0, 0, 0), (0, 128, 128));
        assert_eq!(grd_rgb_to_yuv(255, 255, 255), (254, 128, 128));
        // Pure red: Y=(54*255)>>8=53, U=(-29*255>>8)+128=-29+128=99 (floor), V=127+128=255.
        assert_eq!(grd_rgb_to_yuv(255, 0, 0), (53, 99, 255));
        assert_eq!(grd_rgb_to_yuv(0, 0, 255), (17, 255, 116));
    }

    #[test]
    fn decoder_round_trips_the_whole_cube_within_1() {
        let mut worst = 0u8;
        for r in (0..=255u8).step_by(3) {
            for g in (0..=255u8).step_by(3) {
                for b in (0..=255u8).step_by(3) {
                    let (y, u, v) = grd_rgb_to_yuv(r, g, b);
                    let out = nv12_to_rgb(y, u, v);
                    for (o, e) in out.iter().zip([r, g, b]) {
                        worst = worst.max(o.abs_diff(e));
                    }
                }
            }
        }
        assert!(worst <= 1, "worst channel error {worst}");
    }

    #[test]
    fn nv12_encoding_averages_2x2_chroma_and_replicates_edges() {
        // 2×3 image: rows 0-1 red/blue mix, row 2 (odd height) replicated for chroma.
        let px = |r: u8, g: u8, b: u8| [b, g, r, 255];
        let bgra: Vec<u8> = [px(255, 0, 0), px(0, 0, 255), px(255, 0, 0), px(0, 0, 255), px(0, 255, 0), px(0, 255, 0)]
            .concat();
        let p = grd_encode_nv12(Size::new(2, 3), &bgra).unwrap();
        assert_eq!(p.y(), &[53, 17, 53, 17, 182, 182]);
        let (_, u0, v0) = grd_rgb_to_yuv(255, 0, 0);
        let (_, u1, v1) = grd_rgb_to_yuv(0, 0, 255);
        let avg = |a: u8, b: u8| ((2 * u32::from(a) + 2 * u32::from(b) + 2) / 4) as u8;
        assert_eq!(&p.uv()[..2], &[avg(u0, u1), avg(v0, v1)]);
        let (_, ug, vg) = grd_rgb_to_yuv(0, 255, 0);
        assert_eq!(&p.uv()[2..], &[ug, vg]);
        assert!(grd_encode_nv12(Size::new(3, 2), &[0; 24]).is_none());
        assert!(grd_encode_nv12(Size::new(2, 2), &[0; 15]).is_none());
    }
}
