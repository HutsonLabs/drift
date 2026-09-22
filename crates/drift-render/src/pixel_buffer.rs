//! IOSurface-backed NV12 `CVPixelBuffer` frames: the zero-copy decode path (plan §1.4).
//!
//! `drift-video` decodes into IOSurface-backed `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange`
//! buffers. The compositor imports both planes through a `CVMetalTextureCache` (no copy).
//! Because [`Nv12Frame`] is an opaque `Arc<dyn Nv12Source>`, the compositor finds the
//! `CVPixelBuffer` through a list of [`PixelBufferAccessor`]s; [`PixelBufferNv12`] (this
//! module's wrapper) is always registered, and the decoder's own frame type can be added with
//! [`Compositor::add_pixel_buffer_accessor`](crate::Compositor::add_pixel_buffer_accessor).
//! See `docs/adr/M1-5-metal-compositor.md`.

use std::any::Any;
use std::ptr::{NonNull, null_mut};

use objc2_core_foundation::{CFBoolean, CFDictionary, CFRetained, CFString, CFType};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddressOfPlane,
    CVPixelBufferGetBytesPerRowOfPlane, CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress, kCVPixelBufferIOSurfacePropertiesKey,
    kCVPixelBufferMetalCompatibilityKey, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, kCVReturnSuccess,
};

use drift_core::{Nv12Frame, Nv12Planes, Nv12Source, Size};

use crate::error::RenderError;
use crate::gpu::Shared;

/// Finds the `CVPixelBuffer` behind an [`Nv12Frame`], if the frame has one.
pub type PixelBufferAccessor = for<'a> fn(&'a Nv12Frame) -> Option<&'a CVPixelBuffer>;

/// The built-in accessor for [`PixelBufferNv12`].
pub fn pixel_buffer_nv12_accessor(frame: &Nv12Frame) -> Option<&CVPixelBuffer> {
    frame.downcast_ref::<PixelBufferNv12>().map(PixelBufferNv12::pixel_buffer)
}

/// An NV12 full-range `CVPixelBuffer` as an [`Nv12Source`].
pub struct PixelBufferNv12 {
    buffer: Shared<CFRetained<CVPixelBuffer>>,
    size: Size<u32>,
}

impl std::fmt::Debug for PixelBufferNv12 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PixelBufferNv12").field("size", &self.size).finish()
    }
}

impl PixelBufferNv12 {
    /// Wraps a decoded buffer. It must be bi-planar 4:2:0 full range.
    pub fn new(buffer: CFRetained<CVPixelBuffer>) -> Result<Self, RenderError> {
        if CVPixelBufferGetPixelFormatType(&buffer) != kCVPixelFormatType_420YpCbCr8BiPlanarFullRange {
            return Err(RenderError::Invalid("pixel buffer is not NV12 full range"));
        }
        let size = Size::new(
            u32::try_from(CVPixelBufferGetWidth(&buffer)).map_err(|_| RenderError::Invalid("width"))?,
            u32::try_from(CVPixelBufferGetHeight(&buffer)).map_err(|_| RenderError::Invalid("height"))?,
        );
        Ok(Self { buffer: Shared(buffer), size })
    }

    /// Copies CPU planes into a new IOSurface-backed, Metal-compatible pixel buffer.
    pub fn from_planes(planes: &Nv12Planes) -> Result<Self, RenderError> {
        let size = Nv12Source::size(planes);
        let (w, h) = (size.width as usize, size.height as usize);
        let empty = CFDictionary::<CFString, CFType>::empty();
        // SAFETY: CoreVideo's key constants are immutable CFStrings.
        let keys: [&CFString; 2] =
            unsafe { [kCVPixelBufferIOSurfacePropertiesKey, kCVPixelBufferMetalCompatibilityKey] };
        let values: [&CFType; 2] = [(*empty).as_ref(), CFBoolean::new(true).as_ref()];
        let attrs = CFDictionary::from_slices(&keys, &values);
        let mut raw: *mut CVPixelBuffer = null_mut();
        // SAFETY: `raw` is a valid out-pointer; the attributes dictionary is well-typed.
        let rc = unsafe {
            CVPixelBufferCreate(
                None,
                w,
                h,
                kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
                Some(attrs.as_opaque()),
                NonNull::from(&mut raw),
            )
        };
        if rc != kCVReturnSuccess {
            return Err(RenderError::CoreVideo { call: "CVPixelBufferCreate", code: rc });
        }
        let raw =
            NonNull::new(raw).ok_or(RenderError::CoreVideo { call: "CVPixelBufferCreate", code: rc })?;
        // SAFETY: CVPixelBufferCreate returned +1 ownership of a non-null buffer.
        let buffer = unsafe { CFRetained::from_raw(raw) };

        // SAFETY: lock/unlock bracket the CPU writes; plane pointers and strides come from
        // CoreVideo for this buffer, and each row write stays within `min(stride, w)` bytes of
        // a plane that has at least the rows written (h for Y, ceil(h/2) for UV).
        unsafe {
            let rc = CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags(0));
            if rc != kCVReturnSuccess {
                return Err(RenderError::CoreVideo { call: "CVPixelBufferLockBaseAddress", code: rc });
            }
            for (plane, rows, src) in [(0usize, h, planes.y()), (1, h.div_ceil(2), planes.uv())] {
                let base = CVPixelBufferGetBaseAddressOfPlane(&buffer, plane).cast::<u8>();
                let stride = CVPixelBufferGetBytesPerRowOfPlane(&buffer, plane);
                if base.is_null() || stride < w {
                    CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags(0));
                    return Err(RenderError::Invalid("pixel buffer plane layout"));
                }
                for row in 0..rows {
                    std::ptr::copy_nonoverlapping(src.as_ptr().add(row * w), base.add(row * stride), w);
                }
            }
            CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags(0));
        }
        Self::new(buffer)
    }

    /// The wrapped buffer.
    pub fn pixel_buffer(&self) -> &CVPixelBuffer {
        &self.buffer.0
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
