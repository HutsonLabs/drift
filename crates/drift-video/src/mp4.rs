//! MP4 recording via `AVAssetWriter` in passthrough mode. Owned by task **M8-3** (writer part).
//!
//! [`Recorder`] owns the recording lifecycle (pure state machine [`RecorderState`] + a humble
//! [`Mp4Writer`] around `AVAssetWriter`). Encoded samples from [`crate::encode::VtEncoder`] are
//! appended unchanged (no re-encode). Before each append the recorder checks free disk space via
//! an injectable [`FreeSpace`] probe; when space runs out (or the writer fails) it finalises the
//! file and emits [`RecordingEvent::Stopped`] instead of failing the session.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

use drift_core::Size;

use crate::encode::EncodedFrame;

/// Errors from the recorder API.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecordingError {
    /// `stop` or `append` without a running recording.
    #[error("no recording in progress")]
    NotStarted,
    /// `start` while a recording is running.
    #[error("a recording is already in progress")]
    AlreadyStarted,
    /// `AVAssetWriter` (or file I/O) failed.
    #[error("MP4 writer error: {0}")]
    Writer(String),
}

/// Why a recording ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// The user stopped it.
    Requested,
    /// Free disk space fell below the threshold.
    DiskFull,
    /// The writer failed; the message comes from `AVAssetWriter.error`.
    WriterFailed(String),
}

/// Lifecycle notifications for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingEvent {
    /// A recording started writing to `path`.
    Started {
        /// Output file.
        path: PathBuf,
    },
    /// A recording ended; the file at `path` is finalised and playable.
    Stopped {
        /// Output file.
        path: PathBuf,
        /// Why it ended.
        reason: StopReason,
        /// Number of samples written.
        frames: u64,
    },
}

/// Recording state (pure).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecorderState {
    /// Nothing running.
    #[default]
    Idle,
    /// Writing samples.
    Recording,
}

impl RecorderState {
    /// Transition for `start`.
    pub fn start(self) -> Result<Self, RecordingError> {
        Err(RecordingError::NotStarted)
    }

    /// Transition for `stop` / an automatic stop.
    pub fn stop(self) -> Result<Self, RecordingError> {
        Err(RecordingError::AlreadyStarted)
    }
}

/// Free-space probe (injectable for tests).
pub trait FreeSpace: Send {
    /// Bytes available to the user on the volume containing `path`.
    fn available_bytes(&self, path: &Path) -> std::io::Result<u64>;
}

/// The real probe (`NSFileManager` file-system attributes).
#[derive(Debug, Clone, Copy, Default)]
pub struct VolumeFreeSpace;

impl FreeSpace for VolumeFreeSpace {
    fn available_bytes(&self, path: &Path) -> std::io::Result<u64> {
        let _ = path;
        Ok(0)
    }
}

/// Default free-space floor: stop when less than this remains (256 MiB).
pub const DEFAULT_MIN_FREE_BYTES: u64 = 256 * 1024 * 1024;

/// Summary returned by [`Recorder::stop`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingSummary {
    /// Output file.
    pub path: PathBuf,
    /// Samples written.
    pub frames: u64,
}

/// Result of [`Recorder::append`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppendOutcome {
    /// Sample written; still recording.
    Recording,
    /// The recording stopped (the event was already sent).
    Stopped(StopReason),
}

/// Session recorder: MP4 passthrough of encoded frames with graceful automatic stop.
pub struct Recorder {
    state: RecorderState,
    events: Sender<RecordingEvent>,
}

impl std::fmt::Debug for Recorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorder").field("state", &self.state).finish_non_exhaustive()
    }
}

impl Recorder {
    /// A recorder that reports lifecycle events on `events`.
    pub fn new(events: Sender<RecordingEvent>) -> Self {
        Self { state: RecorderState::Idle, events }
    }

    /// Replaces the free-space probe and threshold.
    pub fn with_free_space(self, probe: impl FreeSpace + 'static, min_free_bytes: u64) -> Self {
        let _ = (probe, min_free_bytes);
        self
    }

    /// Current state.
    pub fn state(&self) -> RecorderState {
        self.state
    }

    /// Starts writing an MP4 file at `path` (replaced if it exists).
    pub fn start(&mut self, path: &Path) -> Result<(), RecordingError> {
        let _ = (path, &self.events);
        Err(RecordingError::Writer("not implemented".into()))
    }

    /// Appends one encoded frame.
    pub fn append(&mut self, frame: &EncodedFrame) -> Result<AppendOutcome, RecordingError> {
        let _ = frame;
        Err(RecordingError::Writer("not implemented".into()))
    }

    /// Finalises the file.
    pub fn stop(&mut self) -> Result<RecordingSummary, RecordingError> {
        Err(RecordingError::Writer("not implemented".into()))
    }
}

/// What an MP4 file contains, read back with `AVAsset` (tests and diagnostics).
#[derive(Debug, Clone, PartialEq)]
pub struct Mp4Info {
    /// Asset duration.
    pub duration: Duration,
    /// Number of video tracks.
    pub video_tracks: usize,
    /// Natural size of the first video track.
    pub dimensions: Size<u32>,
}

/// Reads duration, tracks and dimensions of an MP4 file with `AVURLAsset`.
pub fn inspect(path: &Path) -> Result<Mp4Info, RecordingError> {
    let _ = path;
    Err(RecordingError::Writer("not implemented".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_machine() {
        let s = RecorderState::Idle;
        assert_eq!(s.stop(), Err(RecordingError::NotStarted));
        let r = s.start().unwrap();
        assert_eq!(r, RecorderState::Recording);
        assert_eq!(r.start(), Err(RecordingError::AlreadyStarted));
        assert_eq!(r.stop(), Ok(RecorderState::Idle));
    }

    #[test]
    fn stop_without_start_is_an_error() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut rec = Recorder::new(tx);
        assert_eq!(rec.stop(), Err(RecordingError::NotStarted));
        assert!(rx.try_recv().is_err(), "no event for a rejected stop");
    }

    #[test]
    fn real_probe_reports_space_for_temp_dir() {
        let n = VolumeFreeSpace.available_bytes(&std::env::temp_dir()).unwrap();
        assert!(n > 0);
    }
}
