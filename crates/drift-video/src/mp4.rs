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

use block2::RcBlock;
use drift_core::Size;
use objc2::rc::Retained;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterStatus, AVFileTypeMPEG4, AVMediaTypeVideo, AVURLAsset,
};
use objc2_core_media::CMTime;
use objc2_foundation::{NSFileManager, NSFileSystemFreeSize, NSNumber, NSString, NSURL};

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
        match self {
            Self::Idle => Ok(Self::Recording),
            Self::Recording => Err(RecordingError::AlreadyStarted),
        }
    }

    /// Transition for `stop` / an automatic stop.
    pub fn stop(self) -> Result<Self, RecordingError> {
        match self {
            Self::Idle => Err(RecordingError::NotStarted),
            Self::Recording => Ok(Self::Idle),
        }
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
        let io_err = |msg: String| std::io::Error::other(msg);
        let path = path.to_str().ok_or_else(|| io_err("non-UTF-8 path".into()))?;
        let attrs = NSFileManager::defaultManager()
            .attributesOfFileSystemForPath_error(&NSString::from_str(path))
            .map_err(|e| io_err(e.localizedDescription().to_string()))?;
        // SAFETY: NSFileSystemFreeSize is an immutable framework string constant.
        let key = unsafe { NSFileSystemFreeSize };
        let value = attrs.objectForKey(key).ok_or_else(|| io_err("no NSFileSystemFreeSize".into()))?;
        let number =
            value.downcast::<NSNumber>().map_err(|_| io_err("NSFileSystemFreeSize not a number".into()))?;
        Ok(number.unsignedLongLongValue())
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
    free_space: Box<dyn FreeSpace>,
    min_free_bytes: u64,
    writer: Option<Mp4Writer>,
}

impl std::fmt::Debug for Recorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorder")
            .field("state", &self.state)
            .field("min_free_bytes", &self.min_free_bytes)
            .finish_non_exhaustive()
    }
}

impl Recorder {
    /// A recorder that reports lifecycle events on `events`, using [`VolumeFreeSpace`] and
    /// [`DEFAULT_MIN_FREE_BYTES`].
    pub fn new(events: Sender<RecordingEvent>) -> Self {
        Self {
            state: RecorderState::Idle,
            events,
            free_space: Box::new(VolumeFreeSpace),
            min_free_bytes: DEFAULT_MIN_FREE_BYTES,
            writer: None,
        }
    }

    /// Replaces the free-space probe and threshold.
    pub fn with_free_space(mut self, probe: impl FreeSpace + 'static, min_free_bytes: u64) -> Self {
        self.free_space = Box::new(probe);
        self.min_free_bytes = min_free_bytes;
        self
    }

    /// Current state.
    pub fn state(&self) -> RecorderState {
        self.state
    }

    /// Starts writing an MP4 file at `path` (replaced if it exists).
    pub fn start(&mut self, path: &Path) -> Result<(), RecordingError> {
        let next = self.state.start()?;
        let writer = Mp4Writer::create(path)?;
        self.writer = Some(writer);
        self.state = next;
        self.emit(RecordingEvent::Started { path: path.to_path_buf() });
        Ok(())
    }

    /// Appends one encoded frame.
    ///
    /// Running out of disk space or a writer failure finalises the file, emits
    /// [`RecordingEvent::Stopped`] and returns [`AppendOutcome::Stopped`]; later calls return
    /// [`RecordingError::NotStarted`].
    pub fn append(&mut self, frame: &EncodedFrame) -> Result<AppendOutcome, RecordingError> {
        let Some(writer) = &mut self.writer else { return Err(RecordingError::NotStarted) };
        let dir = writer.path.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let low_space = match self.free_space.available_bytes(&dir) {
            Ok(n) => n < self.min_free_bytes,
            Err(e) => {
                tracing::warn!(error = %e, "free-space probe failed; continuing");
                false
            }
        };
        if low_space {
            return Ok(AppendOutcome::Stopped(self.finish(StopReason::DiskFull)));
        }
        match writer.append(frame) {
            Ok(()) => Ok(AppendOutcome::Recording),
            Err(RecordingError::Writer(msg)) => {
                Ok(AppendOutcome::Stopped(self.finish(StopReason::WriterFailed(msg))))
            }
            Err(e) => Err(e),
        }
    }

    /// Finalises the file.
    pub fn stop(&mut self) -> Result<RecordingSummary, RecordingError> {
        self.state.stop()?;
        let Some(writer) = self.writer.take() else { return Err(RecordingError::NotStarted) };
        self.state = RecorderState::Idle;
        let path = writer.path.clone();
        let frames = writer.frames;
        let result = writer.finish();
        let reason = match &result {
            Ok(()) => StopReason::Requested,
            Err(e) => StopReason::WriterFailed(e.to_string()),
        };
        self.emit(RecordingEvent::Stopped { path: path.clone(), reason, frames });
        result.map(|()| RecordingSummary { path, frames })
    }

    /// Automatic stop: finalise what was written and report why.
    fn finish(&mut self, reason: StopReason) -> StopReason {
        self.state = RecorderState::Idle;
        if let Some(writer) = self.writer.take() {
            let path = writer.path.clone();
            let frames = writer.frames;
            if let Err(e) = writer.finish() {
                tracing::warn!(error = %e, "finalising recording after {reason:?} failed");
            }
            self.emit(RecordingEvent::Stopped { path, reason: reason.clone(), frames });
        }
        reason
    }

    fn emit(&self, event: RecordingEvent) {
        if self.events.send(event).is_err() {
            tracing::debug!("recording event receiver dropped");
        }
    }
}

/// Humble `AVAssetWriter` wrapper: one passthrough H.264 video track.
pub struct Mp4Writer {
    path: PathBuf,
    writer: Retained<AVAssetWriter>,
    input: Option<Retained<AVAssetWriterInput>>,
    frames: u64,
    first_pts: Option<Duration>,
    last_pts: Option<Duration>,
    last_gap: Option<Duration>,
}

impl std::fmt::Debug for Mp4Writer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mp4Writer")
            .field("path", &self.path)
            .field("frames", &self.frames)
            .finish_non_exhaustive()
    }
}

// SAFETY: AVAssetWriter and its inputs may be used from any (single) thread at a time; the
// wrapper is only driven through `&mut self` / by value.
unsafe impl Send for Mp4Writer {}

impl Mp4Writer {
    /// Creates the writer; an existing file at `path` is removed first.
    pub fn create(path: &Path) -> Result<Self, RecordingError> {
        let writer_err = |msg: String| RecordingError::Writer(msg);
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(writer_err(format!("removing {}: {e}", path.display()))),
        }
        let path_str = path.to_str().ok_or_else(|| writer_err("non-UTF-8 path".into()))?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(path_str));
        // SAFETY: framework constant.
        let file_type =
            unsafe { AVFileTypeMPEG4 }.ok_or_else(|| writer_err("AVFileTypeMPEG4 missing".into()))?;
        // SAFETY: valid file URL and a declared container UTI.
        let writer = unsafe { AVAssetWriter::assetWriterWithURL_fileType_error(&url, file_type) }
            .map_err(|e| writer_err(e.localizedDescription().to_string()))?;
        Ok(Self {
            path: path.to_path_buf(),
            writer,
            input: None,
            frames: 0,
            first_pts: None,
            last_pts: None,
            last_gap: None,
        })
    }

    /// Appends one encoded sample, starting the session on the first one.
    pub fn append(&mut self, frame: &EncodedFrame) -> Result<(), RecordingError> {
        let input = match &self.input {
            Some(input) => input.clone(),
            None => self.begin(frame)?,
        };
        // Real-time input: wait briefly for the writer to drain rather than dropping a sample
        // (a dropped keyframe would corrupt video until the next one).
        let mut waited = 0;
        // SAFETY: valid input.
        while !unsafe { input.isReadyForMoreMediaData() } {
            if waited >= 200 {
                return Err(self.failure("writer not ready for more media data"));
            }
            std::thread::sleep(Duration::from_millis(1));
            waited += 1;
        }
        // SAFETY: valid input; the sample is a compressed H.264 sample matching the format hint.
        if !unsafe { input.appendSampleBuffer(frame.sample_buffer()) } {
            return Err(self.failure("appendSampleBuffer failed"));
        }
        if let Some(last) = self.last_pts {
            self.last_gap = frame.pts().checked_sub(last);
        }
        self.last_pts = Some(frame.pts());
        self.frames += 1;
        Ok(())
    }

    fn begin(&mut self, frame: &EncodedFrame) -> Result<Retained<AVAssetWriterInput>, RecordingError> {
        // SAFETY: framework constant.
        let video = unsafe { AVMediaTypeVideo }
            .ok_or_else(|| RecordingError::Writer("AVMediaTypeVideo missing".into()))?;
        // SAFETY: the sample buffer is valid; its format description is retained.
        let hint = unsafe { frame.sample_buffer().format_description() };
        // SAFETY: nil output settings = passthrough; the hint describes the H.264 samples.
        let input = unsafe {
            AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings_sourceFormatHint(
                video,
                None,
                hint.as_deref(),
            )
        };
        // SAFETY: valid input and writer, configured before writing starts.
        unsafe {
            input.setExpectsMediaDataInRealTime(true);
            if !self.writer.canAddInput(&input) {
                return Err(RecordingError::Writer("cannot add H.264 passthrough input".into()));
            }
            self.writer.addInput(&input);
            if !self.writer.startWriting() {
                return Err(self.failure("startWriting failed"));
            }
            self.writer.startSessionAtSourceTime(cm_time(frame.pts()));
        }
        self.first_pts = Some(frame.pts());
        self.input = Some(input.clone());
        Ok(input)
    }

    fn failure(&self, what: &str) -> RecordingError {
        // SAFETY: valid writer.
        let detail = unsafe { self.writer.error() }.map(|e| e.localizedDescription().to_string());
        RecordingError::Writer(match detail {
            Some(d) => format!("{what}: {d}"),
            None => what.to_string(),
        })
    }

    /// Finishes the file (blocking until `AVAssetWriter` completes).
    pub fn finish(self) -> Result<(), RecordingError> {
        let Some(input) = &self.input else {
            // Nothing was written: no session was started, leave no file behind.
            // SAFETY: valid writer that never started writing.
            unsafe { self.writer.cancelWriting() };
            return Ok(());
        };
        // SAFETY: valid writer/input.
        let status = unsafe { self.writer.status() };
        if status != AVAssetWriterStatus::Writing {
            return Err(self.failure("writer is not writing"));
        }
        // The last sample's duration is unknown (variable frame rate): assume the previous gap.
        let end = self.last_pts.unwrap_or_default() + self.last_gap.unwrap_or(Duration::from_micros(16_667));
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let done = RcBlock::new(move || {
            let _ = tx.send(());
        });
        // SAFETY: valid input and writer; all appends have returned; the completion block only
        // sends on a channel (Send).
        unsafe {
            input.markAsFinished();
            self.writer.endSessionAtSourceTime(cm_time(end));
            self.writer.finishWritingWithCompletionHandler(&done);
        }
        rx.recv_timeout(Duration::from_secs(30))
            .map_err(|_| RecordingError::Writer("finishWriting timed out".into()))?;
        // SAFETY: valid writer.
        if unsafe { self.writer.status() } != AVAssetWriterStatus::Completed {
            return Err(self.failure("finishWriting failed"));
        }
        Ok(())
    }
}

fn cm_time(d: Duration) -> CMTime {
    let micros = i64::try_from(d.as_micros()).unwrap_or(i64::MAX);
    // SAFETY: CMTimeMake (CMTime::new) has no preconditions.
    unsafe { CMTime::new(micros, 1_000_000) }
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
#[allow(deprecated)] // synchronous AVAsset accessors: fine for tests/diagnostics off the main thread
pub fn inspect(path: &Path) -> Result<Mp4Info, RecordingError> {
    let err = |msg: &str| RecordingError::Writer(msg.to_string());
    let path_str = path.to_str().ok_or_else(|| err("non-UTF-8 path"))?;
    if !path.exists() {
        return Err(err("no such file"));
    }
    let url = NSURL::fileURLWithPath(&NSString::from_str(path_str));
    // SAFETY: valid file URL, no options.
    let asset = unsafe { AVURLAsset::URLAssetWithURL_options(&url, None) };
    // SAFETY: framework constant.
    let video = unsafe { AVMediaTypeVideo }.ok_or_else(|| err("AVMediaTypeVideo missing"))?;
    // SAFETY: valid asset.
    let duration = unsafe { asset.duration() };
    // SAFETY: CMTimeGetSeconds has no preconditions.
    let seconds = unsafe { duration.seconds() };
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(err("invalid duration"));
    }
    // SAFETY: valid asset and media type.
    let tracks = unsafe { asset.tracksWithMediaType(video) };
    let dimensions = match tracks.firstObject() {
        Some(track) => {
            // SAFETY: valid track.
            let size = unsafe { track.naturalSize() };
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "pixel sizes")]
            Size::new(size.width.round() as u32, size.height.round() as u32)
        }
        None => Size::new(0, 0),
    };
    Ok(Mp4Info { duration: Duration::from_secs_f64(seconds), video_tracks: tracks.count(), dimensions })
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
