//! AVC420 decode via `VTDecompressionSession`. Owned by task **M1-3**.
//!
//! [`VtDecoder`] implements [`drift_core::video::H264Decoder`]: it takes one Annex-B access unit
//! (the bitstream after the `RFX_AVC420_METABLOCK`, see [`crate::metablock`]), converts it to
//! AVCC ([`crate::annexb`]), (re)builds the `CMVideoFormatDescription` + session when the SPS/PPS
//! change ([`crate::params`]), and decodes synchronously into an IOSurface-backed
//! `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` `CVPixelBuffer` ([`DecodedPicture`]).

use drift_core::video::{DecodeError, H264Decoder, Nv12Frame, Nv12Source};
use drift_core::{Nv12Planes, Size};

/// `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` (`'420f'`): the decoder's output format.
pub const OUTPUT_PIXEL_FORMAT: u32 = 0x3432_3066;

/// A decoded picture: an IOSurface-backed NV12 full-range `CVPixelBuffer`.
#[derive(Debug)]
pub struct DecodedPicture {
    size: Size<u32>,
}

impl DecodedPicture {
    /// The picture's `CVPixelBuffer` pixel format (always [`OUTPUT_PIXEL_FORMAT`]).
    pub fn pixel_format(&self) -> u32 {
        0
    }

    /// Whether the pixel buffer is backed by an IOSurface (required for zero-copy Metal import).
    pub fn is_iosurface_backed(&self) -> bool {
        false
    }

    /// Copies the planes to CPU memory (tests, goldens, recording fallbacks).
    pub fn to_planes(&self) -> Result<Nv12Planes, DecodeError> {
        Err(DecodeError("not implemented".into()))
    }
}

impl Nv12Source for DecodedPicture {
    fn size(&self) -> Size<u32> {
        self.size
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// VideoToolbox H.264 decoder for AVC420 surfaces.
#[derive(Debug, Default)]
pub struct VtDecoder {
    builds: u64,
}

impl VtDecoder {
    /// Creates a decoder; the session is built lazily from the first SPS/PPS.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many decompression sessions have been built so far (one per SPS/PPS change).
    pub fn session_builds(&self) -> u64 {
        self.builds
    }

    /// Decodes one Annex-B access unit.
    ///
    /// Returns `Ok(None)` when the access unit carries no picture or cannot be decoded yet
    /// (no SPS/PPS seen).
    pub fn decode_picture(&mut self, annex_b: &[u8]) -> Result<Option<DecodedPicture>, DecodeError> {
        let _ = annex_b;
        Err(DecodeError("not implemented".into()))
    }
}

impl H264Decoder for VtDecoder {
    fn decode(&mut self, annex_b: &[u8]) -> Result<Option<Nv12Frame>, DecodeError> {
        Ok(self.decode_picture(annex_b)?.map(Nv12Frame::new))
    }

    fn reset(&mut self) {}
}
