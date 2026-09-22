//! Composite capture for session recording (task M8-1).

use drift_core::Size;

use crate::error::RenderError;
use crate::image::BgraImage;

/// Capture configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureConfig {
    /// Maximum number of pool buffers alive at once.
    pub max_buffers: usize,
    /// Maximum number of captured frames waiting in the channel.
    pub queue_depth: usize,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self { max_buffers: 6, queue_depth: 4 }
    }
}

/// Capture counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaptureStats {
    /// Frames delivered to the receiver.
    pub captured: u64,
    /// Frames dropped (pool exhausted or receiver full).
    pub dropped: u64,
}

/// One captured composite.
pub struct CapturedFrame {
    _p: (),
}

impl CapturedFrame {
    /// The GFX frame id.
    pub fn frame_id(&self) -> u32 {
        todo!("M8-1")
    }
    /// Pixel size.
    pub fn size(&self) -> Size<u32> {
        todo!("M8-1")
    }
    /// The IOSurface id backing the buffer.
    pub fn iosurface_id(&self) -> Option<u32> {
        todo!("M8-1")
    }
    /// Copies the pixels out.
    pub fn to_image(&self) -> Result<BgraImage, RenderError> {
        todo!("M8-1")
    }
}
