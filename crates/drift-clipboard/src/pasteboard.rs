//! NSPasteboard adapter (task **M5-3**, feature `macos`). A humble object: all decisions live
//! in [`crate::poll`], [`crate::sync`] and [`crate::formats`].
//!
//! Call it from the main thread (the app polls every [`crate::poll::POLL_INTERVAL`] from a
//! main-thread timer).

use drift_core::ClipboardPrefs;
use objc2::rc::Retained;
use objc2_app_kit::NSPasteboard;

use crate::ClipboardContents;
use crate::poll::PasteboardPort;

/// An `NSPasteboard` implementing [`PasteboardPort`].
#[derive(Debug)]
pub struct NsPasteboard {
    pb: Retained<NSPasteboard>,
    unique: bool,
}

impl NsPasteboard {
    /// The system general pasteboard (what Cmd+C / Cmd+V use).
    pub fn general() -> Self {
        Self { pb: NSPasteboard::generalPasteboard(), unique: false }
    }

    /// A private pasteboard with a unique name (tests); released globally on drop.
    pub fn unique() -> Self {
        Self { pb: NSPasteboard::pasteboardWithUniqueName(), unique: true }
    }
}

impl PasteboardPort for NsPasteboard {
    fn change_count(&self) -> i64 {
        todo!()
    }

    fn read(&self, level: ClipboardPrefs) -> ClipboardContents {
        let _ = level;
        todo!()
    }

    fn write(&self, contents: &ClipboardContents) -> i64 {
        let _ = contents;
        todo!()
    }
}

impl Drop for NsPasteboard {
    fn drop(&mut self) {
        if self.unique {
            // SAFETY: `-[NSPasteboard releaseGlobally]` takes no arguments and returns void; it
            // is not bound by objc2-app-kit 0.3. `self.pb` is a valid, retained pasteboard.
            let () = unsafe { objc2::msg_send![&*self.pb, releaseGlobally] };
        }
    }
}
