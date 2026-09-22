//! H.264 encoder for session recording via `VTCompressionSession`. Owned by task **M8-2**.
//!
//! Fixed configuration (plan M8-2): hardware encoder, High profile, real-time, no frame
//! reordering, 8 Mbit/s average, a keyframe at least every 2 s, variable frame rate: each frame's
//! presentation time comes from a [`drift_core::Clock`] instant, relative to the first frame.
//! A frame whose size differs from the session's rebuilds the session, and the first frame of
//! every session is a keyframe.

use std::time::{Duration, Instant};

use drift_core::Size;

/// Errors from the encoder.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    /// A VideoToolbox / CoreMedia / CoreVideo call failed.
    #[error("{call} failed: OSStatus {status}")]
    Os {
        /// The failing call.
        call: &'static str,
        /// Its status code.
        status: i32,
    },
    /// Timestamps must strictly increase.
    #[error("non-monotonic timestamp: {pts:?} after {previous:?}")]
    NonMonotonic {
        /// The rejected presentation time.
        pts: Duration,
        /// The previous presentation time.
        previous: Duration,
    },
    /// The frame is not usable (zero or odd size, …).
    #[error("invalid frame: {0}")]
    InvalidFrame(String),
    /// An encoded sample could not be read back.
    #[error("bad encoded sample: {0}")]
    BadSample(String),
}

/// Encoder settings. [`EncoderConfig::new`] gives the plan's fixed values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncoderConfig {
    /// Frame size in pixels.
    pub size: Size<u32>,
    /// Average bit rate in bits per second (8 Mbit/s).
    pub average_bitrate: u32,
    /// Maximum time between keyframes (2 s).
    pub max_keyframe_interval: Duration,
    /// Expected frame rate, a hint for rate control (60).
    pub expected_fps: u32,
}

impl EncoderConfig {
    /// The plan's configuration for a given frame size.
    pub fn new(size: Size<u32>) -> Self {
        Self { size, average_bitrate: 0, max_keyframe_interval: Duration::ZERO, expected_fps: 0 }
    }
}

/// Maps clock instants to presentation times (pure): the first frame is at zero, and times must
/// strictly increase.
#[derive(Debug, Clone, Default)]
pub struct Timeline {
    origin: Option<Instant>,
    last: Option<Duration>,
}

impl Timeline {
    /// A timeline whose origin is the first instant it sees.
    pub fn new() -> Self {
        Self::default()
    }

    /// The presentation time of a frame captured at `at`.
    pub fn pts(&mut self, at: Instant) -> Result<Duration, EncodeError> {
        let _ = (at, self.origin, self.last);
        Ok(Duration::ZERO)
    }
}

/// Gaps between consecutive keyframe presentation times.
pub fn keyframe_gaps(keyframe_pts: &[Duration]) -> Vec<Duration> {
    let _ = keyframe_pts;
    Vec::new()
}

/// One encoded picture: an AVCC `CMSampleBuffer` as produced by VideoToolbox (passed through to
/// the MP4 writer unchanged), plus its presentation time and keyframe flag.
#[derive(Debug)]
pub struct EncodedFrame {
    pts: Duration,
    keyframe: bool,
}

impl EncodedFrame {
    /// Presentation time relative to the first frame of the recording.
    pub fn pts(&self) -> Duration {
        self.pts
    }

    /// Whether this is a sync sample (IDR).
    pub fn is_keyframe(&self) -> bool {
        self.keyframe
    }

    /// The SPS/PPS of the sample's format description.
    pub fn parameter_sets(&self) -> Result<crate::params::ParameterSets, EncodeError> {
        Err(EncodeError::BadSample("not implemented".into()))
    }

    /// The picture as an Annex-B access unit (AUD first; SPS/PPS included on keyframes), the
    /// same shape g-r-d sends, so it can be fed to [`crate::decode::VtDecoder`].
    pub fn to_annex_b(&self) -> Result<Vec<u8>, EncodeError> {
        Err(EncodeError::BadSample("not implemented".into()))
    }
}

/// VideoToolbox H.264 encoder.
#[derive(Debug)]
pub struct VtEncoder {
    config: EncoderConfig,
}

impl VtEncoder {
    /// Creates the encoder and its first compression session.
    pub fn new(config: EncoderConfig) -> Result<Self, EncodeError> {
        let _ = config;
        Err(EncodeError::InvalidFrame("not implemented".into()))
    }

    /// Current configuration (its size follows the last frame).
    pub fn config(&self) -> EncoderConfig {
        self.config
    }

    /// Number of compression sessions built (1 + number of resizes).
    pub fn session_builds(&self) -> u64 {
        0
    }

    /// Whether VideoToolbox reports a hardware encoder for the current session.
    pub fn is_hardware_accelerated(&self) -> bool {
        false
    }

    /// Encodes one frame captured at `at`. Returns the pictures that completed so far.
    pub fn encode(&mut self, frame: &PixelBuffer, at: Instant) -> Result<Vec<EncodedFrame>, EncodeError> {
        let _ = (frame, at);
        Ok(Vec::new())
    }

    /// Emits every pending picture.
    pub fn flush(&mut self) -> Result<Vec<EncodedFrame>, EncodeError> {
        Ok(Vec::new())
    }
}

/// An IOSurface-backed `CVPixelBuffer` to encode (BGRA from the compositor, or NV12).
#[derive(Debug)]
pub struct PixelBuffer {
    size: Size<u32>,
}

impl PixelBuffer {
    /// A new IOSurface-backed BGRA buffer.
    pub fn new_bgra(size: Size<u32>) -> Result<Self, EncodeError> {
        Ok(Self { size })
    }

    /// A new IOSurface-backed NV12 full-range buffer.
    pub fn new_nv12(size: Size<u32>) -> Result<Self, EncodeError> {
        Ok(Self { size })
    }

    /// Size in pixels.
    pub fn size(&self) -> Size<u32> {
        self.size
    }

    /// Fills a BGRA buffer: `f(x, y)` returns `[b, g, r, a]`.
    pub fn fill_bgra(&self, f: impl Fn(u32, u32) -> [u8; 4]) -> Result<(), EncodeError> {
        let _ = f;
        Ok(())
    }

    /// Writes NV12 planes into an NV12 buffer.
    pub fn write_nv12(&self, planes: &drift_core::Nv12Planes) -> Result<(), EncodeError> {
        let _ = planes;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_matches_plan() {
        let c = EncoderConfig::new(Size::new(1280, 800));
        assert_eq!(c.average_bitrate, 8_000_000);
        assert_eq!(c.max_keyframe_interval, Duration::from_secs(2));
        assert_eq!(c.expected_fps, 60);
        assert_eq!(c.size, Size::new(1280, 800));
    }

    #[test]
    fn timeline_starts_at_zero_and_is_variable_rate() {
        let t0 = Instant::now();
        let mut tl = Timeline::new();
        assert_eq!(tl.pts(t0 + Duration::from_millis(500)).unwrap(), Duration::ZERO);
        assert_eq!(tl.pts(t0 + Duration::from_millis(516)).unwrap(), Duration::from_millis(16));
        assert_eq!(tl.pts(t0 + Duration::from_millis(900)).unwrap(), Duration::from_millis(400));
    }

    #[test]
    fn timeline_rejects_non_increasing_times() {
        let t0 = Instant::now();
        let mut tl = Timeline::new();
        tl.pts(t0).unwrap();
        tl.pts(t0 + Duration::from_millis(10)).unwrap();
        assert_eq!(
            tl.pts(t0 + Duration::from_millis(10)),
            Err(EncodeError::NonMonotonic { pts: Duration::from_millis(10), previous: Duration::from_millis(10) })
        );
        // an instant before the origin saturates to zero, which is not after the last pts
        assert!(tl.pts(t0 - Duration::from_millis(1)).is_err());
        assert_eq!(tl.pts(t0 + Duration::from_millis(11)).unwrap(), Duration::from_millis(11));
    }

    #[test]
    fn gaps() {
        let ms = Duration::from_millis;
        assert_eq!(keyframe_gaps(&[ms(0), ms(2000), ms(3500)]), vec![ms(2000), ms(1500)]);
        assert!(keyframe_gaps(&[ms(5)]).is_empty());
    }
}
