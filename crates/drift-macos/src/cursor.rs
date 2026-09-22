//! Remote pointer decoding (task **M2-5**, pure part).
//!
//! g-r-d sends pointer shapes as fast-path output updates (MS-RDPBCGR 2.2.9.1.2.1): `PTR_NULL`
//! (hidden), `PTR_DEFAULT`, `POSITION`, `COLOR` (24 bpp), `POINTER` ("new", any bpp; 32 bpp
//! with alpha from g-r-d), `LARGE` (up to 384×384) and `CACHED` (re-use a cache slot). Large
//! shapes arrive split in `FIRST`/`NEXT`/`LAST` fragments (the 86×86 pointer at scale 200 does).
//!
//! [`PointerDecoder`] reassembles fragments, decodes the XOR/AND masks with
//! `ironrdp_graphics::pointer` into straight RGBA, and keeps the pointer cache: every
//! colour/new/large update stores its image in its cache slot (evicting the previous one) and
//! `CACHED` re-uses it. Output images are **premultiplied BGRA, top-down** (the same layout as
//! `drift_rdp::CursorBitmap`).
//!
//! g-r-d already scales pointer bitmaps with the desktop scale factor (43×43 at 100, 86×86 at
//! 200, plan §1.4), so the on-screen size in points is `bitmap / (scale / 100)`
//! ([`CursorImage::size_points`]).

use std::sync::Arc;

use drift_core::{Point, Size};

/// Default number of pointer cache slots Drift advertises (`TS_POINTER_CAPABILITYSET`).
pub const DEFAULT_CACHE_SIZE: usize = 25;

/// A decoded pointer image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorImage {
    /// Bitmap size in desktop pixels.
    pub size: Size<u32>,
    /// Hotspot in bitmap pixels (inside the bitmap).
    pub hotspot: Point<u32>,
    /// Premultiplied BGRA8 pixels, top-down, `size.width * 4` bytes per row.
    pub bgra: Arc<[u8]>,
}

impl CursorImage {
    /// Size in view points for a desktop scale factor in percent (`bitmap * 100 / scale`).
    pub fn size_points(&self, scale_percent: u32) -> Size<f64> {
        let _ = scale_percent;
        Size::new(0.0, 0.0)
    }

    /// Hotspot in view points for a desktop scale factor in percent.
    pub fn hotspot_points(&self, scale_percent: u32) -> Point<f64> {
        let _ = scale_percent;
        Point::new(0.0, 0.0)
    }

    /// The pixel at `(x, y)` as premultiplied `[b, g, r, a]`, or `None` outside the bitmap.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        let _ = (x, y);
        None
    }
}

/// What the local pointer should look like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorShape {
    /// Hide the pointer over the remote view (`PTR_NULL`).
    Hidden,
    /// The system arrow (`PTR_DEFAULT`).
    Default,
    /// A remote pointer image.
    Image(Arc<CursorImage>),
}

/// One decoded pointer update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PointerEvent {
    /// The pointer shape changed.
    Shape(CursorShape),
    /// The server moved the pointer (desktop pixels). Drift does not warp the Mac pointer.
    Position(Point<u16>),
}

/// Malformed or inconsistent pointer data (never a panic).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PointerError {
    /// The fast-path PDU or update could not be parsed.
    #[error("malformed pointer update: {0}")]
    Malformed(String),
    /// The XOR/AND masks could not be decoded.
    #[error("undecodable pointer bitmap: {0}")]
    Bitmap(String),
    /// A cache index outside the advertised cache.
    #[error("pointer cache index {index} out of range (cache size {size})")]
    CacheIndexOutOfRange {
        /// The index sent by the server.
        index: u16,
        /// The cache size.
        size: usize,
    },
    /// `CACHED` referenced an empty slot.
    #[error("pointer cache slot {0} is empty")]
    CacheMiss(u16),
    /// A `NEXT`/`LAST` fragment without a `FIRST`, or fragments of different update types.
    #[error("pointer update fragment out of sequence")]
    Fragment,
}

/// Stateful fast-path pointer decoder with the pointer cache.
#[derive(Debug, Clone)]
pub struct PointerDecoder {
    cache: Vec<Option<Arc<CursorImage>>>,
}

impl Default for PointerDecoder {
    fn default() -> Self {
        Self::new(DEFAULT_CACHE_SIZE)
    }
}

impl PointerDecoder {
    /// A decoder with `cache_size` slots (as advertised in the pointer capability set).
    pub fn new(cache_size: usize) -> Self {
        Self { cache: vec![None; cache_size] }
    }

    /// Number of cache slots.
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }

    /// The image in cache slot `index`, if any.
    pub fn cached(&self, index: u16) -> Option<Arc<CursorImage>> {
        self.cache.get(usize::from(index)).cloned().flatten()
    }

    /// Clears the cache and any partial fragment (new connection / deactivation-reactivation).
    pub fn reset(&mut self) {}

    /// Decodes one complete fast-path output PDU (`TS_FP_UPDATE_PDU`: header, length, updates).
    /// Non-pointer updates are skipped. Fragments are kept until their `LAST` part arrives.
    pub fn decode_output_pdu(&mut self, pdu: &[u8]) -> Result<Vec<PointerEvent>, PointerError> {
        let _ = pdu;
        Err(PointerError::Malformed("not implemented".into()))
    }
}
