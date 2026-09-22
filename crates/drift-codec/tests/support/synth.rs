//! Synthetic RFX Progressive streams built with IronRDP's encoder primitives.
//!
//! Shared by the unit tests (`src/progressive.rs`), the integration tests and the criterion
//! bench via `#[path]`, so it must compile standalone.
#![allow(dead_code, clippy::unwrap_used, clippy::cast_possible_truncation)]

use ironrdp_graphics::progressive::{COEFFICIENTS_PER_COMPONENT, encode_first_pass, rgba_to_ycbcr};
use ironrdp_pdu::codecs::rfx::RfxRectangle;
use ironrdp_pdu::codecs::rfx::progressive::{
    ComponentCodecQuant, ProgressiveBlock, ProgressiveCodecQuant, ProgressiveContextPdu,
    ProgressiveFrameBeginPdu, ProgressiveFrameEndPdu, ProgressiveRegion, ProgressiveSyncPdu, ProgressiveTile,
    TileFirst, TileSimple, TileUpgrade, encode_progressive_stream,
};

/// The RemoteFX default quantisation g-r-d (FreeRDP) uses: LL3 6 … HH1 9.
pub const GRD_QUANT: ComponentCodecQuant =
    ComponentCodecQuant { ll3: 6, hl3: 6, lh3: 6, hh3: 6, hl2: 7, lh2: 7, hh2: 8, hl1: 8, lh1: 8, hh1: 9 };

/// A coarse progressive first pass: two extra bits dropped in every band.
pub const COARSE: ComponentCodecQuant =
    ComponentCodecQuant { ll3: 2, hl3: 2, lh3: 2, hh3: 2, hl2: 2, lh2: 2, hh2: 2, hl1: 2, lh1: 2, hh1: 2 };

/// A deterministic 64×64 RGBA test pattern (gradients, an edge and some texture).
pub fn pattern(seed: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 * 64 * 4);
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    for y in 0..64u32 {
        for x in 0..64u32 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let noise = (state & 0x0F) as u8;
            let edge = if (x + seed) % 64 > 40 { 120u8 } else { 0 };
            out.extend_from_slice(&[
                (x * 4) as u8 ^ edge,
                ((y * 4) as u8).wrapping_add(noise),
                ((x + y) * 2) as u8 ^ (seed as u8),
                255,
            ]);
        }
    }
    out
}

/// Entropy-coded component streams for one tile.
#[derive(Clone)]
pub struct EncodedTile {
    pub x_idx: u16,
    pub y_idx: u16,
    pub data: [Vec<u8>; 3],
}

/// Encodes a 64×64 RGBA tile as a first pass with `quant` and progressive `prog`.
pub fn encode_tile(
    x_idx: u16,
    y_idx: u16,
    rgba: &[u8],
    quant: &ComponentCodecQuant,
    prog: &ComponentCodecQuant,
) -> EncodedTile {
    let mut planes = [[0i16; COEFFICIENTS_PER_COMPONENT]; 3];
    {
        let [y, cb, cr] = &mut planes;
        rgba_to_ycbcr(rgba, y, cb, cr);
    }
    let data = planes.map(|mut plane| {
        let mut buf = vec![0u8; 16 * 1024];
        let len = encode_first_pass(&mut plane, &mut buf, quant, prog, false).unwrap();
        buf.truncate(len);
        buf
    });
    EncodedTile { x_idx, y_idx, data }
}

/// A TILE_SIMPLE block (quant index 0 for all components).
pub fn simple(t: &EncodedTile, flags: u8) -> ProgressiveTile<'_> {
    ProgressiveTile::Simple(TileSimple {
        quant_idx_y: 0,
        quant_idx_cb: 0,
        quant_idx_cr: 0,
        x_idx: t.x_idx,
        y_idx: t.y_idx,
        flags,
        y_data: &t.data[0],
        cb_data: &t.data[1],
        cr_data: &t.data[2],
        tail_data: &[],
    })
}

/// A TILE_FIRST block with progressive `quality` (index into the region's prog quant table).
pub fn first(t: &EncodedTile, quality: u8) -> ProgressiveTile<'_> {
    ProgressiveTile::First(TileFirst {
        quant_idx_y: 0,
        quant_idx_cb: 0,
        quant_idx_cr: 0,
        x_idx: t.x_idx,
        y_idx: t.y_idx,
        flags: 0,
        quality,
        y_data: &t.data[0],
        cb_data: &t.data[1],
        cr_data: &t.data[2],
        tail_data: &[],
    })
}

/// A TILE_UPGRADE block that refines nothing (same quality as the first pass, empty data).
pub fn noop_upgrade(x_idx: u16, y_idx: u16, quality: u8) -> ProgressiveTile<'static> {
    ProgressiveTile::Upgrade(TileUpgrade {
        quant_idx_y: 0,
        quant_idx_cb: 0,
        quant_idx_cr: 0,
        x_idx,
        y_idx,
        quality,
        y_srl_data: &[],
        y_raw_data: &[],
        cb_srl_data: &[],
        cb_raw_data: &[],
        cr_srl_data: &[],
        cr_raw_data: &[],
    })
}

/// A progressive quality table entry using `q` for all three components.
pub fn prog_quant(quality: u8, q: ComponentCodecQuant) -> ProgressiveCodecQuant {
    ProgressiveCodecQuant { quality, y_quant: q, cb_quant: q, cr_quant: q }
}

/// An RFX rectangle.
pub fn rect(x: u16, y: u16, width: u16, height: u16) -> RfxRectangle {
    RfxRectangle { x, y, width, height }
}

/// A REGION with the g-r-d quant table and `prog` as the progressive table.
pub fn region<'a>(
    rects: Vec<RfxRectangle>,
    prog: Vec<ProgressiveCodecQuant>,
    tiles: Vec<ProgressiveTile<'a>>,
) -> ProgressiveRegion<'a> {
    ProgressiveRegion {
        tile_size: 0x40,
        rects,
        quant_vals: vec![GRD_QUANT],
        quant_prog_vals: prog,
        flags: 0,
        tiles,
    }
}

/// A complete bitmap stream: [SYNC + CONTEXT] + FRAME_BEGIN + regions + FRAME_END,
/// shaped like g-r-d's `rfx_progressive_write_message`.
pub fn stream(with_context: bool, regions: Vec<ProgressiveRegion<'_>>) -> Vec<u8> {
    let mut blocks = Vec::new();
    if with_context {
        blocks.push(ProgressiveBlock::Sync(ProgressiveSyncPdu));
        blocks.push(ProgressiveBlock::Context(ProgressiveContextPdu {
            context_id: 0,
            tile_size: 0x40,
            flags: 0,
        }));
    }
    blocks.push(ProgressiveBlock::FrameBegin(ProgressiveFrameBeginPdu {
        frame_index: 0,
        region_count: regions.len() as u16,
    }));
    blocks.extend(regions.into_iter().map(ProgressiveBlock::Region));
    blocks.push(ProgressiveBlock::FrameEnd(ProgressiveFrameEndPdu));
    encode_progressive_stream(&blocks).unwrap()
}

/// A one-tile g-r-d style stream: the tile at (x_idx, y_idx), fully covered by its rect.
pub fn single_tile_stream(x_idx: u16, y_idx: u16, seed: u32) -> Vec<u8> {
    let t = encode_tile(x_idx, y_idx, &pattern(seed), &GRD_QUANT, &ComponentCodecQuant::LOSSLESS);
    stream(true, vec![region(vec![rect(x_idx * 64, y_idx * 64, 64, 64)], vec![], vec![simple(&t, 0)])])
}
