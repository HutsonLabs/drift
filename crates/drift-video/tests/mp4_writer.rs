//! M8-3 (writer part): AVAssetWriter passthrough MP4 (feature `recording`).
#![cfg(feature = "recording")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use drift_core::{Clock, Nv12Planes, Size};
use drift_testkit::ManualClock;
use drift_video::encode::{EncodedFrame, EncoderConfig, PixelBuffer, VtEncoder};
use drift_video::mp4::{
    AppendOutcome, FreeSpace, Recorder, RecorderState, RecordingError, RecordingEvent, StopReason, inspect,
};

fn frames(size: Size<u32>, count: u32, step: Duration) -> Vec<EncodedFrame> {
    let clock = ManualClock::new();
    let mut enc = VtEncoder::new(EncoderConfig::new(size)).unwrap();
    let mut out = Vec::new();
    for n in 0..count {
        let buf = PixelBuffer::new_nv12(size).unwrap();
        let luma = (size.width * size.height) as usize;
        let y: Vec<u8> = (0..luma).map(|i| ((i as u32 + n * 4) % 251) as u8).collect();
        let uv = vec![128u8; luma / 2];
        buf.write_nv12(&Nv12Planes::new(size, y, uv).unwrap()).unwrap();
        out.extend(enc.encode(&buf, clock.now()).unwrap());
        clock.advance(step);
    }
    out.extend(enc.flush().unwrap());
    out
}

#[test]
fn asset_reads_back_duration_track_and_dimensions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.mp4");
    let size = Size::new(640, 400);
    let step = Duration::from_micros(16_667);
    let encoded = frames(size, 120, step);
    let (tx, rx) = mpsc::channel();
    let mut rec = Recorder::new(tx);
    rec.start(&path).unwrap();
    assert_eq!(rec.state(), RecorderState::Recording);
    for f in &encoded {
        assert_eq!(rec.append(f).unwrap(), AppendOutcome::Recording);
    }
    let summary = rec.stop().unwrap();
    assert_eq!(summary.frames, 120);
    assert_eq!(rec.state(), RecorderState::Idle);
    assert_eq!(rx.try_recv().unwrap(), RecordingEvent::Started { path: path.clone() });
    assert_eq!(
        rx.try_recv().unwrap(),
        RecordingEvent::Stopped { path: path.clone(), reason: StopReason::Requested, frames: 120 }
    );

    let info = inspect(&path).unwrap();
    assert_eq!(info.video_tracks, 1);
    assert_eq!(info.dimensions, size);
    let expect = step * 120;
    let diff = info.duration.abs_diff(expect);
    assert!(diff <= step * 2, "duration {:?} vs {:?}", info.duration, expect);
}

#[test]
fn stop_without_start_is_an_error() {
    let (tx, rx) = mpsc::channel();
    let mut rec = Recorder::new(tx);
    assert_eq!(rec.stop(), Err(RecordingError::NotStarted));
    let f = frames(Size::new(320, 200), 1, Duration::from_millis(16));
    assert_eq!(rec.append(&f[0]), Err(RecordingError::NotStarted));
    assert!(rx.try_recv().is_err());
}

#[test]
fn start_twice_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = mpsc::channel();
    let mut rec = Recorder::new(tx);
    rec.start(&dir.path().join("a.mp4")).unwrap();
    assert_eq!(rec.start(&dir.path().join("b.mp4")), Err(RecordingError::AlreadyStarted));
    // Stopping a recording with no samples still succeeds and leaves the recorder idle.
    rec.stop().unwrap();
    assert_eq!(rec.state(), RecorderState::Idle);
}

/// Reports plenty of space until `remaining` appends have happened, then almost none.
struct FillingDisk(Arc<AtomicU64>);

impl FreeSpace for FillingDisk {
    fn available_bytes(&self, _path: &Path) -> std::io::Result<u64> {
        let left = self.0.load(Ordering::SeqCst);
        if left == 0 {
            return Ok(1024);
        }
        self.0.store(left - 1, Ordering::SeqCst);
        Ok(u64::MAX / 2)
    }
}

#[test]
fn disk_full_stops_gracefully_with_an_event() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("full.mp4");
    let size = Size::new(320, 200);
    let encoded = frames(size, 60, Duration::from_millis(16));
    let (tx, rx) = mpsc::channel();
    let mut rec = Recorder::new(tx).with_free_space(FillingDisk(Arc::new(AtomicU64::new(30))), 1 << 20);
    rec.start(&path).unwrap();
    let mut outcomes = Vec::new();
    for f in &encoded {
        match rec.append(f) {
            Ok(o) => outcomes.push(o),
            Err(e) => {
                assert_eq!(e, RecordingError::NotStarted, "appends after the stop are rejected");
                break;
            }
        }
    }
    assert_eq!(outcomes.last(), Some(&AppendOutcome::Stopped(StopReason::DiskFull)));
    assert_eq!(rec.state(), RecorderState::Idle);
    assert_eq!(rx.try_recv().unwrap(), RecordingEvent::Started { path: path.clone() });
    let stopped = rx.try_recv().unwrap();
    let RecordingEvent::Stopped { reason, frames, .. } = stopped else { panic!("{stopped:?}") };
    assert_eq!(reason, StopReason::DiskFull);
    assert_eq!(frames, 30);
    // The file written so far is finalised and playable.
    let info = inspect(&path).unwrap();
    assert_eq!(info.video_tracks, 1);
    assert_eq!(info.dimensions, size);
    // A later user stop reports that nothing is running.
    assert_eq!(rec.stop(), Err(RecordingError::NotStarted));
}
