# M8-2/M8-3 — H.264 encoder and MP4 recorder (drift-video, feature `recording`)

- Status: accepted
- Date: 2026-09-22
- Code: `crates/drift-video/src/{encode,mp4}.rs`, `xtask/src/main.rs` (`TEST_FEATURES`)

## Decisions

1. **Encoder session.** `VTCompressionSession` with the encoder specification
   `EnableHardwareAcceleratedVideoEncoder = true` (not `Require…`): CI VMs may lack the media
   engine; the test asserts `UsingHardwareAcceleratedVideoEncoder` except when `CI` is set.
   Properties: `RealTime`, `ProfileLevel = H264_High_AutoLevel`, `AllowFrameReordering = false`,
   `AverageBitRate = 8 000 000`, `MaxKeyFrameIntervalDuration = 2 s` **and**
   `MaxKeyFrameInterval = 120` frames (at the expected 60 fps; whichever comes first),
   `ExpectedFrameRate = 60`, BT.709 primaries/transfer/matrix.
2. **Variable frame rate.** Each frame's PTS is `Clock` instant − first instant (µs timescale,
   duration `kCMTimeInvalid`). `Timeline` rejects non-increasing times with
   `EncodeError::NonMonotonic` rather than silently reordering.
3. **Resize.** A frame whose size differs from the session's completes the old session, builds a
   new one (same timeline) and forces a keyframe (`kVTEncodeFrameOptionKey_ForceKeyFrame`).
4. **Encoded output.** `EncodedFrame` keeps VideoToolbox's `CMSampleBuffer` (AVCC) for MP4
   passthrough, a copy of the AVCC bytes, the keyframe flag (IDR NAL present) and
   `to_annex_b()` (AUD + SPS/PPS on keyframes) so it can be fed to `VtDecoder` exactly like a
   g-r-d stream. Input is any IOSurface-backed `CVPixelBuffer` (`PixelBuffer::from_cv` for the
   M8-1 compositor pool, BGRA; tests use NV12 full range to compare planes directly).
5. **MP4 writer.** `AVAssetWriter` (MPEG-4) with one `AVAssetWriterInput` created with nil output
   settings and the first sample's format description as source-format hint (passthrough, no
   re-encode), `expectsMediaDataInRealTime = true`. The session starts at the first sample's PTS
   and ends at last PTS + last inter-frame gap. `Recorder` is the lifecycle/state machine;
   `Mp4Writer` is the humble object.
6. **Disk full.** Before every append the recorder consults an injectable `FreeSpace` probe
   (default: `NSFileManager` `NSFileSystemFreeSize`, floor 256 MiB). Low space — or an
   `AVAssetWriter` failure — finalises the file (still playable) and emits
   `RecordingEvent::Stopped { reason: DiskFull | WriterFailed(..) }`; `append` returns
   `AppendOutcome::Stopped`, later calls `RecordingError::NotStarted`.
7. **CI covers the feature.** `cargo xtask ci/check` pass `--features drift-video/recording` to
   clippy and nextest, so feature-gated code is linted and tested on every PR.

## Known limitations (for the M8-3 app hook)

- A resize mid-recording changes the H.264 format description; one MP4 video track with a single
  format hint is not guaranteed to accept it. The hook should stop and start a new file on
  resize (or record at a fixed capture size).
- `inspect()` uses the deprecated synchronous `AVAsset` accessors; it is a test/diagnostic helper,
  not for the main thread.
