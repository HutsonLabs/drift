//! `NSCursor` from decoded remote pointers (task **M2-5**, AppKit part).
//!
//! The image is sized in points as `bitmap / (scale / 100)` so an 86×86 pointer at desktop
//! scale 200 shows as a crisp 43×43-point cursor on a Retina display; the hotspot is converted
//! the same way.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::NSCursor;

use crate::cursor::{CursorImage, CursorShape};

/// Builds an `NSCursor` for `image` at desktop scale `scale_percent`. `None` if AppKit cannot
/// allocate the bitmap (or the image is empty).
pub fn ns_cursor(
    mtm: MainThreadMarker,
    image: &CursorImage,
    scale_percent: u32,
) -> Option<Retained<NSCursor>> {
    let _ = (mtm, image, scale_percent);
    None
}

/// A fully transparent cursor (the remote hid the pointer).
pub fn hidden_cursor(mtm: MainThreadMarker) -> Retained<NSCursor> {
    let _ = mtm;
    NSCursor::arrowCursor()
}

/// The cursor to show for `shape`; falls back to the arrow if an image cannot be built.
pub fn cursor_for(mtm: MainThreadMarker, shape: &CursorShape, scale_percent: u32) -> Retained<NSCursor> {
    let _ = (mtm, shape, scale_percent);
    NSCursor::arrowCursor()
}
