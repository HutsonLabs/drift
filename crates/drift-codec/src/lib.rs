//! # drift-codec
//!
//! CPU decoders producing BGRA tiles for the RDPGFX pipeline (task **M1-4**):
//!
//! - [`ProgressiveCodec`]: RFX Progressive (`RDPGFX_CODECID_CAPROGRESSIVE`, the codec g-r-d
//!   uses when no hardware H.264 encoder is available, plan §1.4). The block stream is parsed
//!   and entropy-decoded with `ironrdp_graphics`' public primitives, and the per-tile work
//!   (RLGR/SRL decode, inverse DWT, colour conversion, BGRA swizzle) runs on a rayon
//!   [`TilePool`]. The output is bit-identical to `ironrdp_graphics::progressive::ProgressiveDecoder`
//!   (see `docs/adr/M1-4-cpu-codecs.md`).
//! - [`decode_planar`]: RDP 6.0 Planar (`RDPGFX_CODECID_PLANAR`).
//! - [`decode_uncompressed`]: `RDPGFX_CODECID_UNCOMPRESSED` (32 bpp XRGB/ARGB).
//! - [`ClearCodec`]: `RDPGFX_CODECID_CLEARCODEC` (stateful glyph/V-bar caches).
//!
//! Every decoder returns [`BgraTile`]s: a BGRA8 pixel buffer positioned in surface
//! coordinates plus the surface rectangles that must be blitted from it. [`BgraTile::blits`]
//! yields exactly the `(rect, stride, data)` triples `drift_gfx::FrameSink::blit_bgra` takes.
//!
//! Malformed server input never panics: every decoder returns [`CodecError`] instead
//! (property-tested in `tests/malformed.rs`).
//!
//! The crate is built with `opt-level = 3` even in dev (plan §0): a debug-built decoder acks
//! frames late and g-r-d then throttles the stream.

mod clear;
mod error;
mod planar;
mod pool;
mod progressive;
mod tile;
mod uncompressed;

pub use clear::ClearCodec;
pub use error::{CodecError, CodecKind};
pub use planar::decode_planar;
pub use pool::TilePool;
pub use progressive::{ProgressiveCodec, TILE_SIZE};
pub use tile::{BgraTile, Blit};
pub use uncompressed::{UncompressedFormat, decode_uncompressed};
