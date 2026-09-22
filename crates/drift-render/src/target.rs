//! Presentation targets: a `CAMetalLayer` (production) or an offscreen texture (goldens).

use objc2::Message;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_core_graphics::{CGColorSpace, kCGColorSpaceSRGB};
use objc2_metal::{MTLDrawable, MTLPixelFormat, MTLStorageMode, MTLTexture, MTLTextureUsage};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};

use drift_core::Size;

use crate::error::RenderError;
use crate::gpu::{Gpu, Shared, Texture};

/// One acquired render target for a frame.
pub struct TargetFrame {
    pub(crate) texture: Retained<Texture>,
    pub(crate) drawable: Option<Retained<ProtocolObject<dyn MTLDrawable>>>,
}

/// Where the present pass draws. Implemented by [`LayerTarget`] and [`OffscreenTarget`].
pub trait PresentTarget: Send {
    /// The texture to draw the next frame into, or `None` if none is available right now
    /// (e.g. a zero-sized layer); the frame is then not presented.
    fn acquire(&mut self) -> Option<TargetFrame>;
}

/// An offscreen BGRA8 texture target, for golden tests and headless rendering.
pub struct OffscreenTarget {
    gpu: Gpu,
    texture: Shared<Retained<Texture>>,
}

impl OffscreenTarget {
    /// Creates a `size` target (both dimensions must be non-zero).
    pub fn new(gpu: &Gpu, size: Size<u32>) -> Result<Self, RenderError> {
        let texture = gpu
            .texture(
                MTLPixelFormat::BGRA8Unorm,
                size,
                MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead,
                MTLStorageMode::Private,
            )
            .ok_or(RenderError::Allocation("offscreen target"))?;
        Ok(Self { gpu: gpu.clone(), texture: Shared(texture) })
    }

    /// Target size in pixels.
    pub fn size(&self) -> Size<u32> {
        let t = &self.texture.0;
        Size::new(t.width() as u32, t.height() as u32)
    }

    pub(crate) fn texture(&self) -> &Texture {
        &self.texture.0
    }

    pub(crate) fn gpu(&self) -> &Gpu {
        &self.gpu
    }
}

impl PresentTarget for OffscreenTarget {
    fn acquire(&mut self) -> Option<TargetFrame> {
        Some(TargetFrame { texture: self.texture.0.clone(), drawable: None })
    }
}

/// A `CAMetalLayer` target. The layer's `drawableSize` is owned by the view (drift-macos
/// `RemoteView` updates it in `setFrameSize:` / `viewDidChangeBackingProperties`); this
/// target only vends drawables on the render thread.
pub struct LayerTarget {
    layer: Shared<Retained<CAMetalLayer>>,
}

impl LayerTarget {
    /// Configures `layer` for Drift (device, `BGRA8Unorm`, sRGB colour space, opaque) and
    /// wraps it.
    pub fn new(gpu: &Gpu, layer: &CAMetalLayer) -> Self {
        layer.setDevice(Some(gpu.device()));
        layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        layer.setFramebufferOnly(true);
        layer.setOpaque(true);
        // The remote desktop is sRGB; tag it so macOS colour-matches to P3/XDR displays.
        // SAFETY: `kCGColorSpaceSRGB` is an immutable CoreGraphics constant.
        let srgb = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }));
        layer.setColorspace(srgb.as_deref());
        Self { layer: Shared(layer.retain()) }
    }
}

impl PresentTarget for LayerTarget {
    fn acquire(&mut self) -> Option<TargetFrame> {
        let size = self.layer.0.drawableSize();
        if size.width < 1.0 || size.height < 1.0 {
            return None;
        }
        let drawable: Retained<ProtocolObject<dyn CAMetalDrawable>> = self.layer.0.nextDrawable()?;
        let texture = drawable.texture();
        Some(TargetFrame { texture, drawable: Some(ProtocolObject::from_retained(drawable)) })
    }
}
