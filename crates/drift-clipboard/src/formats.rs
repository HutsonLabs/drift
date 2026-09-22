//! Pure clipboard format mapping (UTF-16LE/CRLF text, PNG, TIFF, DIB). Owned by task **M5-1**.
//!
//! | Local (NSPasteboard) | Remote (CLIPRDR) |
//! |---|---|
//! | `public.utf8-plain-text` | `CF_UNICODETEXT` (UTF-16LE + NUL, LF↔CRLF); `CF_TEXT` accepted only when Unicode is absent |
//! | `public.png` | named format `"image/png"` (bytes pass through unchanged) |
//! | `public.tiff` | `CF_TIFF` |
//! | `public.png` | `CF_DIB` (inbound: decoded and re-encoded as PNG; outbound: for compatibility) |

use drift_core::ClipboardPrefs;
use serde::{Deserialize, Serialize};

use crate::{ClipboardContents, ClipboardItem};

/// `CF_TEXT`: ANSI text.
pub const CF_TEXT: u32 = 1;
/// `CF_TIFF`: TIFF image.
pub const CF_TIFF: u32 = 6;
/// `CF_DIB`: packed device-independent bitmap (`BITMAPINFO` + bits).
pub const CF_DIB: u32 = 8;
/// `CF_UNICODETEXT`: UTF-16LE text with a terminating NUL, CRLF line endings.
pub const CF_UNICODETEXT: u32 = 13;
/// Name of the registered PNG format used by g-r-d (and by Drift outbound).
pub const PNG_FORMAT_NAME: &str = "image/png";
/// The id Drift registers for its outbound `"image/png"` format. Registered ids are per-side
/// (g-r-d uses `0xD011`), so inbound PNG is recognised by name, never by id.
pub const LOCAL_PNG_FORMAT_ID: u32 = 0xC0F0;
/// Largest clipboard payload Drift accepts or offers, in bytes (32 MiB). Also bounds the
/// decoded RGBA size of a DIB.
pub const MAX_CLIPBOARD_BYTES: usize = 32 * 1024 * 1024;

/// A CLIPRDR format as it appears in a Format List PDU.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClipFormat {
    /// Format id (standard `CF_*` or a registered id ≥ `0xC000`).
    pub id: u32,
    /// Format name for registered formats.
    pub name: Option<String>,
}

impl ClipFormat {
    /// A standard (unnamed) format.
    pub fn standard(id: u32) -> Self {
        Self { id, name: None }
    }

    /// A registered (named) format.
    pub fn named(id: u32, name: &str) -> Self {
        Self { id, name: Some(name.to_owned()) }
    }

    /// What Drift understands this format to be, if anything.
    pub fn kind(&self) -> Option<FormatKind> {
        todo!()
    }
}

/// The formats Drift knows how to convert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FormatKind {
    /// `CF_UNICODETEXT`.
    UnicodeText,
    /// `CF_TEXT`.
    AnsiText,
    /// Named `"image/png"`.
    Png,
    /// `CF_TIFF`.
    Tiff,
    /// `CF_DIB`.
    Dib,
}

impl FormatKind {
    /// `true` for image formats (gated by [`ClipboardPrefs::TextAndImages`]).
    pub fn is_image(self) -> bool {
        todo!()
    }
}

/// Why a clipboard payload was refused. Surfaced to the actor as a rejection event.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum ClipError {
    /// The payload exceeds [`MAX_CLIPBOARD_BYTES`].
    #[error("clipboard payload of {size} bytes exceeds the {max}-byte limit")]
    TooLarge {
        /// Payload (or decoded) size in bytes.
        size: usize,
        /// The limit.
        max: usize,
    },
    /// The image bytes could not be decoded or encoded.
    #[error("invalid clipboard image: {0}")]
    InvalidImage(String),
    /// The requested format is not one Drift offered.
    #[error("clipboard format {0:#x} is not offered")]
    Unavailable(u32),
    /// The remote side answered a data request with an error.
    #[error("the remote clipboard returned an error")]
    RemoteError,
}

/// Fails with [`ClipError::TooLarge`] when `size` exceeds [`MAX_CLIPBOARD_BYTES`].
pub fn check_size(size: usize) -> Result<(), ClipError> {
    let _ = size;
    todo!()
}

/// Local text → `CF_UNICODETEXT` bytes: LF becomes CRLF (existing CRLF is kept), UTF-16LE,
/// terminated by a NUL code unit.
pub fn encode_unicode_text(text: &str) -> Vec<u8> {
    let _ = text;
    todo!()
}

/// `CF_UNICODETEXT` bytes → local text: stops at the first NUL, CRLF becomes LF, unpaired
/// surrogates become U+FFFD, a trailing odd byte is ignored.
pub fn decode_unicode_text(bytes: &[u8]) -> String {
    let _ = bytes;
    todo!()
}

/// `CF_TEXT` bytes → local text: stops at the first NUL, CRLF becomes LF. UTF-8 is used when
/// valid (g-r-d synthesises it from UTF-8), otherwise the bytes are read as Latin-1.
pub fn decode_ansi_text(bytes: &[u8]) -> String {
    let _ = bytes;
    todo!()
}

/// Decodes a packed DIB (`CF_DIB`: `BITMAPINFOHEADER` [+ masks/palette] + pixels; 24/32 bpp,
/// top-down or bottom-up) and re-encodes it as PNG.
pub fn dib_to_png(dib: &[u8]) -> Result<Vec<u8>, ClipError> {
    let _ = dib;
    todo!()
}

/// Encodes a PNG as a 32 bpp bottom-up `CF_DIB` (for remote apps that only read DIBs).
pub fn png_to_dib(png: &[u8]) -> Result<Vec<u8>, ClipError> {
    let _ = png;
    todo!()
}

/// Validates a PNG payload (signature and size) and returns it unchanged.
pub fn png_passthrough(png: &[u8]) -> Result<Vec<u8>, ClipError> {
    let _ = png;
    todo!()
}

/// Keeps only the items `prefs` allows and that fit under the size cap. Returns the kept
/// contents and one rejection per oversized item.
pub fn filter_local(
    contents: &ClipboardContents,
    prefs: ClipboardPrefs,
) -> (ClipboardContents, Vec<ClipError>) {
    let _ = (contents, prefs);
    todo!()
}

/// The Format List Drift advertises for local `contents` under `prefs`:
/// `CF_UNICODETEXT` for text; `"image/png"` + `CF_DIB` for PNG; `CF_TIFF` for a TIFF-only
/// clipboard. Empty when `prefs` is `Off`.
pub fn outbound_formats(contents: &ClipboardContents, prefs: ClipboardPrefs) -> Vec<ClipFormat> {
    let _ = (contents, prefs);
    todo!()
}

/// The remote formats to fetch after a remote copy, in fetch order: at most one text format
/// (`CF_UNICODETEXT`, else `CF_TEXT`) then, if `prefs` allows images, at most one image format
/// (`"image/png"`, else `CF_TIFF`, else `CF_DIB`).
pub fn select_inbound(formats: &[ClipFormat], prefs: ClipboardPrefs) -> Vec<(ClipFormat, FormatKind)> {
    let _ = (formats, prefs);
    todo!()
}

/// Converts fetched remote bytes of `kind` into a local item (size-capped).
pub fn decode_inbound(kind: FormatKind, data: &[u8]) -> Result<ClipboardItem, ClipError> {
    let _ = (kind, data);
    todo!()
}

/// Produces the bytes for a server Format Data Request of `format_id`, from local `contents`
/// under `prefs` (size-capped).
pub fn encode_outbound(
    format_id: u32,
    contents: &ClipboardContents,
    prefs: ClipboardPrefs,
) -> Result<Vec<u8>, ClipError> {
    let _ = (format_id, contents, prefs);
    todo!()
}
