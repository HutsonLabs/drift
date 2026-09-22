//! # drift-clipboard
//!
//! Clipboard synchronisation between NSPasteboard and CLIPRDR (plan §1.6).
//!
//! | Module | Task |
//! |---|---|
//! | [`contents`] | M0-5: [`ClipboardContents`], the payload exchanged with the session actor |
//! | [`formats`] | M5-1: pure format mapping/conversion (UTF-16/CRLF, PNG/TIFF/DIB) |
//! | [`sync`] | M5-2: CLIPRDR sync state (initial list, echo suppression, newest-wins) |
//! | `pasteboard` (feature `macos`) | M5-3: NSPasteboard adapter |

pub mod contents;
pub mod formats;
pub mod sync;

pub use contents::{ClipboardContents, ClipboardItem};
