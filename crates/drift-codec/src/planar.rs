//! `RDPGFX_CODECID_PLANAR`: RDP 6.0 bitmap streams ([MS-RDPEGDI] 2.2.2.5.1).

use drift_core::Rect;

use crate::error::{CodecError, CodecKind};
use crate::tile::BgraTile;

/// Decodes a Planar `WireToSurface1` payload for `dest` into an opaque BGRA tile.
///
/// Planar carries colour only; per-pixel opacity travels in a separate `CODECID_ALPHA`
/// command, so every output pixel is opaque (as in `ironrdp_egfx`).
///
/// # Errors
/// [`CodecError::InvalidRect`] for an empty or oversized `dest`, [`CodecError::Malformed`]
/// when the bitmap stream does not decode.
pub fn decode_planar(dest: Rect, data: &[u8]) -> Result<BgraTile, CodecError> {
    let _ = (dest, data, CodecKind::Planar, BgraTile::full as fn(_, _) -> _);
    todo!("M1-4: decode_planar")
}

#[cfg(test)]
mod tests {
    use ironrdp_graphics::rdp6::{BitmapStreamEncoder, RgbChannels};

    use super::*;

    fn encode(w: usize, h: usize, rgb: &[u8], rle: bool) -> Vec<u8> {
        let mut out = vec![0u8; rgb.len() * 4 + 64];
        let len = BitmapStreamEncoder::new(w, h).encode_bitmap::<RgbChannels>(rgb, &mut out, rle).unwrap_or(0);
        out.truncate(len);
        out
    }

    #[test]
    fn planar_round_trips_to_opaque_bgra() {
        for rle in [false, true] {
            let (w, h) = (5usize, 3usize);
            let rgb: Vec<u8> = (0..w * h).flat_map(|i| [i as u8 * 10, 100, 255 - i as u8]).collect();
            let tile = decode_planar(Rect::new(7, 9, 5, 3), &encode(w, h, &rgb, rle));
            let tile = tile.ok();
            let expected: Vec<u8> = rgb.chunks_exact(3).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
            assert_eq!(tile.as_ref().map(|t| &t.data), Some(&expected), "rle={rle}");
            assert_eq!(tile.map(|t| t.update_rects), Some(vec![Rect::new(7, 9, 5, 3)]));
        }
    }

    #[test]
    fn garbage_and_bad_rects_are_errors() {
        assert!(matches!(decode_planar(Rect::new(0, 0, 4, 4), &[0xFF; 3]), Err(CodecError::Malformed { .. })));
        assert!(matches!(decode_planar(Rect::new(0, 0, 0, 4), &[]), Err(CodecError::InvalidRect { .. })));
        assert!(matches!(decode_planar(Rect::new(0, 0, 1, 70_000), &[]), Err(CodecError::InvalidRect { .. })));
    }
}
