//! RFX Progressive ([MS-RDPEGFX] 2.2.4.2) decoded on the rayon [`TilePool`].

use drift_core::Size;

use crate::error::CodecError;
use crate::pool::TilePool;
use crate::tile::BgraTile;

/// Edge length of an RFX Progressive tile in pixels.
pub const TILE_SIZE: u32 = 64;

/// A stateful RFX Progressive decoder for one session.
///
/// It mirrors `ironrdp_graphics::progressive::ProgressiveDecoder` (per
/// `(surface, codec context)` tile state, surface-scoped DWT references for difference
/// tiles, REGION clipping across the payloads of one RDPGFX frame) and produces
/// bit-identical pixels, but decodes the tiles of each REGION in parallel on a
/// [`TilePool`].
pub struct ProgressiveCodec {
    pool: TilePool,
}

impl std::fmt::Debug for ProgressiveCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgressiveCodec").field("pool", &self.pool).finish_non_exhaustive()
    }
}

impl ProgressiveCodec {
    /// Creates a decoder that runs tile work on `pool`.
    pub fn new(pool: TilePool) -> Self {
        Self { pool }
    }

    /// Marks the start of an RDPGFX frame (`StartFrame`): REGION blocks in later payloads of
    /// the same frame may reference tiles decoded by earlier payloads.
    pub fn begin_frame(&mut self) {
        todo!("M1-4: begin_frame")
    }

    /// Marks the end of an RDPGFX frame (`EndFrame`).
    pub fn end_frame(&mut self) {
        todo!("M1-4: end_frame")
    }

    /// Decodes one `WireToSurface2` progressive payload for `surface_id` /
    /// `codec_context_id`, on a surface of `surface` pixels.
    ///
    /// Returns one 64×64 [`BgraTile`] per tile that has visible pixels in this payload's
    /// REGION rectangles; `update_rects` are clipped to the REGION and the surface.
    ///
    /// # Errors
    /// [`CodecError::Malformed`] for any stream, tile or entropy-coding error, and
    /// [`CodecError::SurfaceTooLarge`] for surfaces beyond 32768 px per axis.
    pub fn decode(
        &mut self,
        surface_id: u16,
        codec_context_id: u32,
        surface: Size<u32>,
        data: &[u8],
    ) -> Result<Vec<BgraTile>, CodecError> {
        let _ = (surface_id, codec_context_id, surface, data);
        todo!("M1-4: ProgressiveCodec::decode")
    }

    /// Drops a codec context (`DeleteEncodingContext`).
    pub fn delete_context(&mut self, surface_id: u16, codec_context_id: u32) {
        let _ = (surface_id, codec_context_id);
        todo!("M1-4: delete_context")
    }

    /// Drops every context and reference of a surface (`DeleteSurface`), so a new surface
    /// reusing the id cannot inherit stale tiles.
    pub fn delete_surface(&mut self, surface_id: u16) {
        let _ = surface_id;
        todo!("M1-4: delete_surface")
    }

    /// Drops all codec contexts (`ResetGraphics`), keeping surface references like IronRDP.
    pub fn reset(&mut self) {
        todo!("M1-4: reset")
    }
}

#[cfg(test)]
#[path = "../tests/support/synth.rs"]
mod synth;

#[cfg(test)]
mod tests {
    use drift_core::{Point, Rect};
    use ironrdp_graphics::progressive::ProgressiveDecoder;
    use ironrdp_pdu::codecs::rfx::progressive::{ComponentCodecQuant, TILE_FLAG_DIFFERENCE};

    use super::synth::*;
    use super::*;

    const SURFACE: Size<u32> = Size::new(200, 130);

    fn pool() -> TilePool {
        TilePool::new(4).unwrap_or_else(|e| panic!("pool: {e}"))
    }

    /// Runs IronRDP's sequential decoder and converts its output to BGRA tiles.
    fn reference(dec: &mut ProgressiveDecoder, surface_id: u16, ctx: u32, size: Size<u32>, data: &[u8]) -> Option<Vec<BgraTile>> {
        let tiles = dec.decode_bitmap(surface_id, ctx, size.width as u16, size.height as u16, data).ok()?;
        Some(
            tiles
                .into_iter()
                .map(|t| BgraTile {
                    origin: Point::new(u32::from(t.x_idx) * 64, u32::from(t.y_idx) * 64),
                    size: Size::new(64, 64),
                    data: t.pixels.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect(),
                    update_rects: t
                        .update_rectangles
                        .iter()
                        .filter_map(|r| {
                            Rect::from_ltrb(u32::from(r.left), u32::from(r.top), u32::from(r.right), u32::from(r.bottom))
                        })
                        .collect(),
                })
                .collect(),
        )
    }

    /// Feeds the same calls to both decoders and asserts identical results.
    fn assert_same(calls: &[(u16, u32, &[u8])]) -> Vec<Option<Vec<BgraTile>>> {
        let mut ours = ProgressiveCodec::new(pool());
        let mut theirs = ProgressiveDecoder::new();
        let mut outs = Vec::new();
        for (i, (surface, ctx, data)) in calls.iter().enumerate() {
            let a = ours.decode(*surface, *ctx, SURFACE, data).ok();
            let b = reference(&mut theirs, *surface, *ctx, SURFACE, data);
            assert_eq!(a.is_some(), b.is_some(), "call {i}: success differs");
            assert_eq!(a, b, "call {i}: output differs");
            outs.push(a);
        }
        outs
    }

    #[test]
    fn simple_tiles_match_ironrdp_and_clip_to_region_and_surface() {
        let tiles: Vec<_> = [(0, 0, 1), (1, 0, 2), (3, 2, 3)]
            .iter()
            .map(|&(x, y, s)| encode_tile(x, y, &pattern(s), &GRD_QUANT, &ComponentCodecQuant::LOSSLESS))
            .collect();
        let data = stream(
            true,
            vec![region(
                vec![rect(0, 0, 100, 64), rect(192, 128, 64, 64)],
                vec![],
                tiles.iter().map(|t| simple(t, 0)).collect(),
            )],
        );
        let out = assert_same(&[(1, 0, &data)]);
        let out = out[0].as_ref().map(Vec::as_slice).unwrap_or_default();
        assert_eq!(out.len(), 3);
        assert_eq!(out[1].update_rects, vec![Rect::new(64, 0, 36, 64)], "clipped to the REGION rect");
        assert_eq!(out[2].update_rects, vec![Rect::new(192, 128, 8, 2)], "clipped to the 200x130 surface");
    }

    #[test]
    fn first_upgrade_and_difference_tiles_match_ironrdp() {
        let t0 = encode_tile(0, 0, &pattern(7), &GRD_QUANT, &COARSE);
        let t1 = encode_tile(1, 1, &pattern(8), &GRD_QUANT, &ComponentCodecQuant::LOSSLESS);
        let full = vec![rect(0, 0, 200, 130)];
        let first_pass = stream(
            true,
            vec![region(full.clone(), vec![prog_quant(0, COARSE)], vec![first(&t0, 0), simple(&t1, 0)])],
        );
        // Upgrade (0,0); difference-tile (1,1) against its retained reference; upgrade a
        // never-decoded tile (2,1), which IronRDP silently skips.
        let second = stream(
            false,
            vec![region(
                full.clone(),
                vec![prog_quant(0, COARSE)],
                vec![noop_upgrade(0, 0, 0), simple(&t1, TILE_FLAG_DIFFERENCE), noop_upgrade(2, 1, 0)],
            )],
        );
        // The same tile twice in one region: the second decode must see the first.
        let dup = stream(false, vec![region(full, vec![], vec![simple(&t1, 0), simple(&t1, TILE_FLAG_DIFFERENCE)])]);
        let outs = assert_same(&[(1, 0, &first_pass), (1, 0, &second), (1, 0, &dup)]);
        assert!(outs.iter().all(Option::is_some));
    }

    #[test]
    fn errors_match_ironrdp_and_never_panic() {
        let t = encode_tile(0, 0, &pattern(1), &GRD_QUANT, &ComponentCodecQuant::LOSSLESS);
        let out_of_bounds = encode_tile(9, 9, &pattern(1), &GRD_QUANT, &ComponentCodecQuant::LOSSLESS);
        let no_context = stream(false, vec![region(vec![rect(0, 0, 64, 64)], vec![], vec![simple(&t, 0)])]);
        let oob = stream(true, vec![region(vec![rect(0, 0, 64, 64)], vec![], vec![simple(&out_of_bounds, 0)])]);
        let no_reference = stream(true, vec![region(vec![], vec![], vec![simple(&t, TILE_FLAG_DIFFERENCE)])]);
        let bad_quality = stream(true, vec![region(vec![], vec![], vec![first(&t, 3)])]);
        let mut bad_quant = region(vec![], vec![], vec![simple(&t, 0)]);
        bad_quant.quant_vals.clear();
        let bad_quant = stream(true, vec![bad_quant]);
        let outs = assert_same(&[
            (1, 0, &no_context),
            (1, 0, &oob),
            (2, 0, &no_reference),
            (3, 0, &bad_quality),
            (4, 0, &bad_quant),
            (5, 0, &[0xC0, 0xCC, 0xFF]),
        ]);
        assert!(outs.iter().all(Option::is_none));
        let mut codec = ProgressiveCodec::new(pool());
        assert!(matches!(
            codec.decode(1, 0, Size::new(40_000, 10), &single_tile_stream(0, 0, 1)),
            Err(CodecError::SurfaceTooLarge { .. } | CodecError::Malformed { .. })
        ));
    }

    #[test]
    fn frame_bracketing_shares_tiles_between_payloads_like_ironrdp() {
        let t0 = encode_tile(0, 0, &pattern(1), &GRD_QUANT, &ComponentCodecQuant::LOSSLESS);
        let a = stream(true, vec![region(vec![rect(0, 0, 32, 32)], vec![], vec![simple(&t0, 0)])]);
        // A second payload with no tiles whose REGION rect is covered by the tile above.
        let b = stream(false, vec![region(vec![rect(10, 10, 20, 20)], vec![], vec![])]);

        let mut ours = ProgressiveCodec::new(pool());
        let mut theirs = ProgressiveDecoder::new();
        ours.begin_frame();
        theirs.begin_frame();
        for data in [&a, &b] {
            let x = ours.decode(1, 0, SURFACE, data).ok();
            let y = reference(&mut theirs, 1, 0, SURFACE, data);
            assert_eq!(x, y);
        }
        ours.end_frame();
        theirs.end_frame();
        // Outside a frame the second payload stands alone: nothing to show.
        let x = ours.decode(1, 0, SURFACE, &b).ok();
        assert_eq!(x, reference(&mut theirs, 1, 0, SURFACE, &b));
        assert_eq!(x.map(|v| v.len()), Some(0));
    }

    #[test]
    fn context_lifecycle_matches_ironrdp() {
        let with_ctx = single_tile_stream(0, 0, 3);
        let t = encode_tile(0, 0, &pattern(4), &GRD_QUANT, &ComponentCodecQuant::LOSSLESS);
        let without_ctx = stream(false, vec![region(vec![rect(0, 0, 64, 64)], vec![], vec![simple(&t, 0)])]);
        let diff = stream(false, vec![region(vec![rect(0, 0, 64, 64)], vec![], vec![simple(&t, TILE_FLAG_DIFFERENCE)])]);

        let mut ours = ProgressiveCodec::new(pool());
        let mut theirs = ProgressiveDecoder::new();
        let mut step = |ours: &mut ProgressiveCodec, theirs: &mut ProgressiveDecoder, ctx: u32, data: &[u8]| {
            let x = ours.decode(1, ctx, SURFACE, data).ok();
            assert_eq!(x, reference(theirs, 1, ctx, SURFACE, data));
            x.is_some()
        };
        assert!(step(&mut ours, &mut theirs, 0, &with_ctx));
        // A new context id without CONTEXT block inherits the surface's flag (Windows behaviour).
        assert!(step(&mut ours, &mut theirs, 7, &without_ctx));
        ours.delete_context(1, 7);
        theirs.delete_context(1, 7);
        assert!(step(&mut ours, &mut theirs, 7, &without_ctx));
        ours.reset();
        theirs.reset();
        // References survive reset: a difference tile still decodes.
        assert!(step(&mut ours, &mut theirs, 0, &diff));
        ours.delete_surface(1);
        theirs.delete_surface(1);
        // After deleting the surface there is no CONTEXT flag and no reference.
        assert!(!step(&mut ours, &mut theirs, 0, &without_ctx));
        assert!(!step(&mut ours, &mut theirs, 0, &diff));
        assert!(format!("{ours:?}").contains("ProgressiveCodec"));
    }

    #[test]
    fn surface_resize_reallocates_the_tile_grid() {
        let data = single_tile_stream(2, 1, 5);
        let mut ours = ProgressiveCodec::new(pool());
        let mut theirs = ProgressiveDecoder::new();
        for size in [SURFACE, Size::new(100, 64), SURFACE] {
            let x = ours.decode(1, 0, size, &data).ok();
            assert_eq!(x, reference(&mut theirs, 1, 0, size, &data), "size {size:?}");
        }
    }
}
