//! The shared Metal context: device, a shared command queue and compiled pipelines.

use std::ptr::NonNull;
use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBlitCommandEncoder, MTLBuffer, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue,
    MTLComputePipelineState, MTLCreateSystemDefaultDevice, MTLDevice, MTLLibrary, MTLOrigin, MTLPixelFormat,
    MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLResourceOptions, MTLSize, MTLStorageMode,
    MTLTexture, MTLTextureDescriptor, MTLTextureUsage,
};

use drift_core::Size;

use crate::error::RenderError;
use crate::image::BgraImage;
use crate::shaders;

pub(crate) type Device = ProtocolObject<dyn MTLDevice>;
pub(crate) type Queue = ProtocolObject<dyn MTLCommandQueue>;
pub(crate) type Texture = ProtocolObject<dyn MTLTexture>;

/// Wrapper asserting that a Metal/CoreVideo object may cross threads.
///
/// Metal devices, queues, pipeline states, buffers and textures, and CoreFoundation-based
/// CoreVideo objects, are reference counted with atomic retain/release and documented as
/// safe to use from any thread; Drift additionally only *encodes* with a given texture from
/// one thread at a time (the session render thread owns the compositor).
pub(crate) struct Shared<T>(pub T);

// SAFETY: see the type docs — the wrapped objects are thread-safe Metal/CF objects and all
// mutation is serialised by `&mut` access to the owning compositor.
unsafe impl<T> Send for Shared<T> {}
// SAFETY: as above; shared access only calls thread-safe Metal/CF methods.
unsafe impl<T> Sync for Shared<T> {}

struct Inner {
    device: Retained<Device>,
    queue: Retained<Queue>,
    nv12: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    fill: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    present: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
}

/// A Metal device with Drift's compiled pipelines. Cheap to clone; share one per process.
#[derive(Clone)]
pub struct Gpu {
    inner: Arc<Shared<Inner>>,
}

impl std::fmt::Debug for Gpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gpu").field("device", &self.name()).finish()
    }
}

impl Gpu {
    /// Uses the system default Metal device and compiles the shaders.
    pub fn system_default() -> Result<Self, RenderError> {
        let device = MTLCreateSystemDefaultDevice().ok_or(RenderError::NoDevice)?;
        Self::with_device(device)
    }

    /// Uses `device` and compiles the shaders.
    pub fn with_device(device: Retained<ProtocolObject<dyn MTLDevice>>) -> Result<Self, RenderError> {
        let lib = device
            .newLibraryWithSource_options_error(&NSString::from_str(&shaders::source()), None)
            .map_err(|e| RenderError::Shader(e.localizedDescription().to_string()))?;
        let function = |name: &str| {
            lib.newFunctionWithName(&NSString::from_str(name))
                .ok_or_else(|| RenderError::Shader(format!("missing function {name}")))
        };
        let compute = |name: &str| {
            let f = function(name)?;
            device
                .newComputePipelineStateWithFunction_error(&f)
                .map_err(|e| RenderError::Shader(e.localizedDescription().to_string()))
        };
        let nv12 = compute(shaders::NV12_KERNEL)?;
        let fill = compute(shaders::FILL_KERNEL)?;

        let desc = MTLRenderPipelineDescriptor::new();
        let (vertex, fragment) = (function(shaders::PRESENT_VERTEX)?, function(shaders::PRESENT_FRAGMENT)?);
        desc.setVertexFunction(Some(&vertex));
        desc.setFragmentFunction(Some(&fragment));
        // SAFETY: index 0 always exists in the colour attachment array.
        let attachment = unsafe { desc.colorAttachments().objectAtIndexedSubscript(0) };
        attachment.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        let present = device
            .newRenderPipelineStateWithDescriptor_error(&desc)
            .map_err(|e| RenderError::Shader(e.localizedDescription().to_string()))?;
        let queue = device.newCommandQueue().ok_or(RenderError::Allocation("command queue"))?;
        Ok(Self { inner: Arc::new(Shared(Inner { device, queue, nv12, fill, present })) })
    }

    /// The device name (for logs and the stats overlay).
    pub fn name(&self) -> String {
        self.device().name().to_string()
    }

    pub(crate) fn device(&self) -> &Device {
        &self.inner.0.device
    }

    /// A new command queue (one per compositor, so sessions never wait on each other), or
    /// the shared one if the device refuses.
    pub(crate) fn new_queue(&self) -> Retained<Queue> {
        self.device().newCommandQueue().unwrap_or_else(|| self.inner.0.queue.clone())
    }

    pub(crate) fn nv12_pipeline(&self) -> &ProtocolObject<dyn MTLComputePipelineState> {
        &self.inner.0.nv12
    }

    pub(crate) fn fill_pipeline(&self) -> &ProtocolObject<dyn MTLComputePipelineState> {
        &self.inner.0.fill
    }

    pub(crate) fn present_pipeline(&self) -> &ProtocolObject<dyn MTLRenderPipelineState> {
        &self.inner.0.present
    }

    /// A 2-D texture; `None` for an empty size or allocation failure.
    pub(crate) fn texture(
        &self,
        format: MTLPixelFormat,
        size: Size<u32>,
        usage: MTLTextureUsage,
        storage: MTLStorageMode,
    ) -> Option<Retained<Texture>> {
        if size.width == 0 || size.height == 0 {
            return None;
        }
        // SAFETY: plain descriptor factory; width/height are non-zero.
        let desc = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                format,
                size.width as usize,
                size.height as usize,
                false,
            )
        };
        desc.setUsage(usage);
        desc.setStorageMode(storage);
        self.device().newTextureWithDescriptor(&desc)
    }

    /// The BGRA8 texture used for surfaces, cache slots and the composed output.
    pub(crate) fn bgra_texture(&self, size: Size<u32>) -> Option<Retained<Texture>> {
        self.texture(
            MTLPixelFormat::BGRA8Unorm,
            size,
            MTLTextureUsage::ShaderRead | MTLTextureUsage::ShaderWrite | MTLTextureUsage::RenderTarget,
            MTLStorageMode::Private,
        )
    }

    /// A CPU-writable shared buffer of `len` bytes (`None` if `len == 0` or allocation fails).
    pub(crate) fn device_buffer(&self, len: usize) -> Option<Retained<ProtocolObject<dyn MTLBuffer>>> {
        if len == 0 {
            return None;
        }
        self.device().newBufferWithLength_options(len, MTLResourceOptions::StorageModeShared)
    }

    /// Copies a texture back to the CPU on `queue` (after everything already committed to it).
    pub(crate) fn read_texture(&self, queue: &Queue, texture: &Texture) -> Option<BgraImage> {
        let (w, h) = (texture.width(), texture.height());
        let size = Size::new(u32::try_from(w).ok()?, u32::try_from(h).ok()?);
        let len = w.checked_mul(h)?.checked_mul(4)?;
        let buffer = self.device().newBufferWithLength_options(len, MTLResourceOptions::StorageModeShared)?;
        let cb = queue.commandBuffer()?;
        let blit = cb.blitCommandEncoder()?;
        // SAFETY: the source region is the whole texture and the buffer holds w*h*4 bytes.
        unsafe {
            blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(
                texture,
                0,
                0,
                MTLOrigin { x: 0, y: 0, z: 0 },
                MTLSize { width: w, height: h, depth: 1 },
                &buffer,
                0,
                w * 4,
                len,
            );
        }
        blit.endEncoding();
        cb.commit();
        cb.waitUntilCompleted();
        let ptr: NonNull<std::ffi::c_void> = buffer.contents();
        // SAFETY: the shared buffer is `len` bytes, the GPU has finished writing it and it
        // stays alive (owned by `buffer`) for the duration of the copy.
        let data = unsafe { std::slice::from_raw_parts(ptr.as_ptr().cast::<u8>(), len) }.to_vec();
        Some(BgraImage { size, data })
    }
}
