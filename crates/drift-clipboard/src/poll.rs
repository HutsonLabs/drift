//! Pure pasteboard polling logic (task **M5-3**).
//!
//! NSPasteboard has no change notification, so the app polls `changeCount` every
//! [`POLL_INTERVAL`] on the main thread. [`PasteboardWatcher`] decides when a read is needed
//! and fans the change out; the platform side is the [`PasteboardPort`] trait, implemented by
//! `pasteboard::NsPasteboard` (feature `macos`) and by fakes in tests.

use std::time::Duration;

use drift_core::ClipboardPrefs;

use crate::ClipboardContents;

/// How often the main thread polls `changeCount`.
pub const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// The minimal pasteboard surface Drift needs. Implementations are humble objects.
pub trait PasteboardPort {
    /// The pasteboard's current `changeCount`.
    fn change_count(&self) -> i64;
    /// Reads text (and, when `level` allows, PNG/TIFF images).
    fn read(&self, level: ClipboardPrefs) -> ClipboardContents;
    /// Replaces the pasteboard contents; returns the `changeCount` after the write.
    fn write(&self, contents: &ClipboardContents) -> i64;
}

/// A detected local change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalChange {
    /// `changeCount` after the change.
    pub change_count: i64,
    /// The contents read at that `changeCount`.
    pub contents: ClipboardContents,
}

/// Tracks the last seen `changeCount` of one pasteboard.
#[derive(Debug, Clone, Default)]
pub struct PasteboardWatcher {
    last_seen: Option<i64>,
}

impl PasteboardWatcher {
    /// A watcher that treats `initial` as already seen (the state at app start is not a
    /// change; it is advertised through the initial format list instead).
    pub fn new(initial: i64) -> Self {
        let _ = initial;
        todo!()
    }

    /// Polls `port`; returns the change when `changeCount` moved. Changes caused by a
    /// session's own writes are reported too (other sessions must see them); each
    /// [`crate::sync::ClipboardSync`] ignores its own writes. `level` is the most
    /// permissive preference among the sessions (so images are not read when no one wants
    /// them); [`ClipboardPrefs::Off`] skips the read entirely.
    pub fn poll<P: PasteboardPort + ?Sized>(
        &mut self,
        port: &P,
        level: ClipboardPrefs,
    ) -> Option<LocalChange> {
        let _ = (port, level);
        todo!()
    }

    /// Reads the current contents regardless of `changeCount` (for an initial format list).
    pub fn snapshot<P: PasteboardPort + ?Sized>(port: &P, level: ClipboardPrefs) -> LocalChange {
        let _ = (port, level);
        todo!()
    }
}
