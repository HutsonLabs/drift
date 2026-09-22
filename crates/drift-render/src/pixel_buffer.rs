//! IOSurface-backed NV12 `CVPixelBuffer` frames (the zero-copy decode path).

use std::any::Any;

use drift_core::{Nv12Planes, Nv12Source, Size};

use crate::error::RenderError;

/// An NV12 full-range `CVPixelBuffer` as an [`Nv12Source`].
#[derive(Debug)]
pub struct PixelBufferNv12 {
    size: Size<u32>,
}

impl PixelBufferNv12 {
    /// Copies CPU planes into a new IOSurface-backed pixel buffer.
    pub fn from_planes(planes: &Nv12Planes) -> Result<Self, RenderError> {
        let _ = planes;
        todo!("M1-5")
    }
}

impl Nv12Source for PixelBufferNv12 {
    fn size(&self) -> Size<u32> {
        self.size
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
