//! Server pointer updates → [`SessionEvent::Cursor`](crate::SessionEvent::Cursor) (M2-5 actor side). Pure.
//!
//! IronRDP's `ActiveStage` already decodes and caches fast-path pointer updates
//! (colour/new/large/cached/null/default/position). With hardware pointer rendering it hands
//! out **straight-alpha RGBA** bitmaps; [`CursorBitmap`] carries **premultiplied BGRA**, the
//! layout `NSBitmapImageRep`/`NSCursor` wants, plus the desktop scale so the app can size the
//! cursor in points (`bitmap × 100 / scale`, plan §1.4).

use ironrdp_session::ActiveStageOutput;

use crate::session::CursorUpdate;

/// The cursor update for a pointer output of the active stage, `None` for other outputs.
pub fn cursor_update(output: &ActiveStageOutput, scale: u32) -> Option<CursorUpdate> {
    todo!("{:?} {scale}", std::mem::discriminant(output))
}

/// Straight-alpha RGBA → premultiplied BGRA (same length; a trailing partial pixel is dropped).
pub fn rgba_to_premultiplied_bgra(rgba: &[u8]) -> Vec<u8> {
    todo!("{}", rgba.len())
}
