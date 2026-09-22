//! Composite capture for session recording (task M8-1).
//!
//! While recording, every composed frame is also blitted (GPU only, no CPU copy) into an
//! IOSurface-backed BGRA `CVPixelBuffer` taken from a `CVPixelBufferPool`; after the command
//! buffer completes the buffer is sent to the encoder over a bounded channel. The pool is
//! capped with `kCVPixelBufferPoolAllocationThresholdKey`, so a slow encoder makes Drift
//! *drop* capture frames rather than allocate without bound. Nothing is allocated or copied
//! unless recording.

use std::ptr::{NonNull, null_mut};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};

use objc2::rc::Retained;
use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_core_video::{
    CVMetalTexture, CVMetalTextureCache, CVMetalTextureGetTexture, CVPixelBuffer,
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight,
    CVPixelBufferGetIOSurface, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferPool, CVPixelBufferUnlockBaseAddress, kCVPixelBufferHeightKey,
    kCVPixelBufferIOSurfacePropertiesKey, kCVPixelBufferMetalCompatibilityKey,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferPoolAllocationThresholdKey,
    kCVPixelBufferPoolMaximumBufferAgeKey, kCVPixelBufferWidthKey, kCVPixelFormatType_32BGRA,
    kCVReturnSuccess,
};
use objc2_metal::MTLPixelFormat;

use drift_core::Size;

use crate::error::RenderError;
use crate::gpu::{Gpu, Shared, Texture};
use crate::image::BgraImage;

/// Capture configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureConfig {
    /// Maximum number of pool buffers alive at once (in flight + queued + held by the
    /// encoder). When exhausted, frames are dropped and counted in [`CaptureStats::dropped`].
    pub max_buffers: usize,
    /// Maximum number of captured frames waiting in the channel.
    pub queue_depth: usize,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self { max_buffers: 6, queue_depth: 4 }
    }
}

/// Capture counters (cumulative over the compositor's life).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaptureStats {
    /// Frames delivered to the receiver.
    pub captured: u64,
    /// Frames dropped (pool exhausted or receiver full/closed).
    pub dropped: u64,
}

#[derive(Default)]
pub(crate) struct Counters {
    captured: AtomicU64,
    dropped: AtomicU64,
}

impl Counters {
    pub(crate) fn snapshot(&self) -> CaptureStats {
        CaptureStats {
            captured: self.captured.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
        }
    }
    pub(crate) fn drop_one(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }
}

/// One captured composite: an IOSurface-backed BGRA `CVPixelBuffer` from the pool. Dropping
/// it returns the buffer to the pool.
pub struct CapturedFrame {
    frame_id: u32,
    size: Size<u32>,
    buffer: Shared<CFRetained<CVPixelBuffer>>,
}

impl std::fmt::Debug for CapturedFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapturedFrame").field("frame_id", &self.frame_id).field("size", &self.size).finish()
    }
}

impl CapturedFrame {
    /// The GFX frame id this composite belongs to.
    pub fn frame_id(&self) -> u32 {
        self.frame_id
    }

    /// Pixel size.
    pub fn size(&self) -> Size<u32> {
        self.size
    }

    /// The pixel buffer, for `VTCompressionSessionEncodeFrame` (M8-2).
    pub fn pixel_buffer(&self) -> &CVPixelBuffer {
        &self.buffer.0
    }

    /// The id of the IOSurface backing the buffer.
    pub fn iosurface_id(&self) -> Option<u32> {
        CVPixelBufferGetIOSurface(Some(&self.buffer.0)).map(|s| s.id())
    }

    /// Copies the pixels out (tests and debugging; the encoder never needs this).
    pub fn to_image(&self) -> Result<BgraImage, RenderError> {
        let pb = &self.buffer.0;
        let (w, h) = (CVPixelBufferGetWidth(pb), CVPixelBufferGetHeight(pb));
        let flags = CVPixelBufferLockFlags::ReadOnly;
        // SAFETY: lock/unlock bracket the read; base and stride come from CoreVideo for this
        // buffer, and every row read is `w * 4 <= stride` bytes within the locked plane.
        unsafe {
            let rc = CVPixelBufferLockBaseAddress(pb, flags);
            if rc != kCVReturnSuccess {
                return Err(RenderError::CoreVideo { call: "CVPixelBufferLockBaseAddress", code: rc });
            }
            let base = CVPixelBufferGetBaseAddress(pb).cast::<u8>();
            let stride = CVPixelBufferGetBytesPerRow(pb);
            if base.is_null() || stride < w * 4 {
                CVPixelBufferUnlockBaseAddress(pb, flags);
                return Err(RenderError::Invalid("capture buffer layout"));
            }
            let mut data = Vec::with_capacity(w * h * 4);
            for row in 0..h {
                data.extend_from_slice(std::slice::from_raw_parts(base.add(row * stride), w * 4));
            }
            CVPixelBufferUnlockBaseAddress(pb, flags);
            Ok(BgraImage { size: Size::new(w as u32, h as u32), data })
        }
    }
}

/// The per-compositor recording state.
pub(crate) struct Recorder {
    config: CaptureConfig,
    tx: SyncSender<CapturedFrame>,
    counters: Arc<Counters>,
    pool: Option<(Size<u32>, Shared<CFRetained<CVPixelBufferPool>>)>,
    cache: Option<Shared<CFRetained<CVMetalTextureCache>>>,
}

/// A capture target for one frame: the pool buffer and its Metal view.
pub(crate) struct CaptureSlot {
    pub(crate) texture: Retained<Texture>,
    // Kept alive until the GPU copy completes.
    cv_texture: Shared<CFRetained<CVMetalTexture>>,
    buffer: Shared<CFRetained<CVPixelBuffer>>,
    size: Size<u32>,
}

/// Delivers a completed capture (called from the command-buffer completion handler).
pub(crate) struct Delivery {
    tx: SyncSender<CapturedFrame>,
    counters: Arc<Counters>,
    frame: CapturedFrame,
    _cv_texture: Shared<CFRetained<CVMetalTexture>>,
}

impl Delivery {
    pub(crate) fn deliver(self) {
        let Self { tx, counters, frame, _cv_texture } = self;
        drop(_cv_texture);
        match tx.try_send(frame) {
            Ok(()) => counters.captured.fetch_add(1, Ordering::Relaxed),
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                counters.dropped.fetch_add(1, Ordering::Relaxed)
            }
        };
    }
}

impl Recorder {
    pub(crate) fn new(
        gpu: &Gpu,
        config: CaptureConfig,
        counters: Arc<Counters>,
    ) -> (Self, Receiver<CapturedFrame>) {
        let (tx, rx) = sync_channel(config.queue_depth.max(1));
        let cache = texture_cache(gpu);
        (Self { config, tx, counters, pool: None, cache }, rx)
    }

    /// Takes a buffer for a `size` composite, or `None` (counted as dropped) when the pool
    /// is exhausted or CoreVideo fails.
    pub(crate) fn slot(&mut self, size: Size<u32>) -> Option<CaptureSlot> {
        let slot = self.try_slot(size);
        if slot.is_none() {
            self.counters.drop_one();
        }
        slot
    }

    fn try_slot(&mut self, size: Size<u32>) -> Option<CaptureSlot> {
        if self.pool.as_ref().is_none_or(|(s, _)| *s != size) {
            self.pool = Some((size, Shared(create_pool(size)?)));
        }
        let (_, pool) = self.pool.as_ref()?;
        let threshold = CFNumber::new_i32(i32::try_from(self.config.max_buffers).unwrap_or(i32::MAX));
        // SAFETY: CoreVideo's key constants are immutable CFStrings.
        let key = unsafe { kCVPixelBufferPoolAllocationThresholdKey };
        let aux = CFDictionary::<CFString, CFType>::from_slices(&[key], &[threshold.as_ref()]);
        let mut raw: *mut CVPixelBuffer = null_mut();
        // SAFETY: `raw` is a valid out-pointer and `aux` is a well-typed dictionary.
        let rc = unsafe {
            CVPixelBufferPool::create_pixel_buffer_with_aux_attributes(
                None,
                &pool.0,
                Some(aux.as_opaque()),
                NonNull::from(&mut raw),
            )
        };
        if rc != kCVReturnSuccess {
            return None; // kCVReturnWouldExceedAllocationThreshold: the encoder is behind.
        }
        // SAFETY: success returns +1 ownership of a non-null buffer.
        let buffer = unsafe { CFRetained::from_raw(NonNull::new(raw)?) };
        let cache = self.cache.as_ref()?;
        let mut cv_raw: *mut CVMetalTexture = null_mut();
        // SAFETY: valid cache, buffer and out-pointer; plane 0 of a BGRA buffer is the whole
        // image at `size`.
        let rc = unsafe {
            CVMetalTextureCache::create_texture_from_image(
                None,
                &cache.0,
                &buffer,
                None,
                MTLPixelFormat::BGRA8Unorm,
                size.width as usize,
                size.height as usize,
                0,
                NonNull::from(&mut cv_raw),
            )
        };
        if rc != kCVReturnSuccess {
            return None;
        }
        // SAFETY: success returns +1 ownership of a non-null CVMetalTexture.
        let cv_texture = unsafe { CFRetained::from_raw(NonNull::new(cv_raw)?) };
        let texture = CVMetalTextureGetTexture(&cv_texture)?;
        Some(CaptureSlot { texture, cv_texture: Shared(cv_texture), buffer: Shared(buffer), size })
    }

    /// Turns a filled slot into a delivery for the completion handler.
    pub(crate) fn delivery(&self, slot: CaptureSlot, frame_id: u32) -> Delivery {
        let CaptureSlot { texture, cv_texture, buffer, size } = slot;
        drop(texture);
        Delivery {
            tx: self.tx.clone(),
            counters: self.counters.clone(),
            frame: CapturedFrame { frame_id, size, buffer },
            _cv_texture: cv_texture,
        }
    }

    /// Releases cache entries whose buffers went back to the pool.
    pub(crate) fn flush(&self) {
        if let Some(cache) = &self.cache {
            cache.0.flush(0);
        }
    }
}

/// A `CVMetalTextureCache` for `gpu`'s device.
pub(crate) fn texture_cache(gpu: &Gpu) -> Option<Shared<CFRetained<CVMetalTextureCache>>> {
    let mut raw: *mut CVMetalTextureCache = null_mut();
    // SAFETY: valid device and out-pointer.
    let rc = unsafe { CVMetalTextureCache::create(None, None, gpu.device(), None, NonNull::from(&mut raw)) };
    if rc != kCVReturnSuccess {
        return None;
    }
    // SAFETY: success returns +1 ownership of a non-null cache.
    NonNull::new(raw).map(|p| Shared(unsafe { CFRetained::from_raw(p) }))
}

fn create_pool(size: Size<u32>) -> Option<CFRetained<CVPixelBufferPool>> {
    let empty = CFDictionary::<CFString, CFType>::empty();
    let format = CFNumber::new_i32(kCVPixelFormatType_32BGRA as i32);
    let width = CFNumber::new_i32(i32::try_from(size.width).ok()?);
    let height = CFNumber::new_i32(i32::try_from(size.height).ok()?);
    // SAFETY: CoreVideo's key constants are immutable CFStrings.
    let buffer_keys: [&CFString; 5] = unsafe {
        [
            kCVPixelBufferPixelFormatTypeKey,
            kCVPixelBufferWidthKey,
            kCVPixelBufferHeightKey,
            kCVPixelBufferIOSurfacePropertiesKey,
            kCVPixelBufferMetalCompatibilityKey,
        ]
    };
    let buffer_values: [&CFType; 5] =
        [format.as_ref(), width.as_ref(), height.as_ref(), (*empty).as_ref(), CFBoolean::new(true).as_ref()];
    let buffer_attrs = CFDictionary::from_slices(&buffer_keys, &buffer_values);
    // Age 0 disables ageing: free buffers stay in the pool instead of being reallocated.
    let age = CFNumber::new_f64(0.0);
    // SAFETY: as above.
    let pool_keys: [&CFString; 1] = unsafe { [kCVPixelBufferPoolMaximumBufferAgeKey] };
    let pool_attrs = CFDictionary::<CFString, CFType>::from_slices(&pool_keys, &[age.as_ref()]);
    let mut raw: *mut CVPixelBufferPool = null_mut();
    // SAFETY: valid dictionaries and out-pointer.
    let rc = unsafe {
        CVPixelBufferPool::create(
            None,
            Some(pool_attrs.as_opaque()),
            Some(buffer_attrs.as_opaque()),
            NonNull::from(&mut raw),
        )
    };
    if rc != kCVReturnSuccess {
        return None;
    }
    // SAFETY: success returns +1 ownership of a non-null pool.
    NonNull::new(raw).map(|p| unsafe { CFRetained::from_raw(p) })
}
