//! Presentation targets: a `CAMetalLayer` (production) or an offscreen texture (goldens).

use drift_core::Size;
use objc2_quartz_core::CAMetalLayer;

use crate::error::RenderError;
use crate::gpu::Gpu;

/// Where the present pass draws.
pub trait PresentTarget: Send {}

/// An offscreen BGRA8 texture target.
pub struct OffscreenTarget {
    _p: (),
}

impl OffscreenTarget {
    /// Creates a `size` target.
    pub fn new(gpu: &Gpu, size: Size<u32>) -> Result<Self, RenderError> {
        let _ = (gpu, size);
        todo!("M1-5")
    }
}

impl PresentTarget for OffscreenTarget {}

/// A `CAMetalLayer` target.
pub struct LayerTarget {
    _p: (),
}

impl LayerTarget {
    /// Configures `layer` for Drift and wraps it.
    pub fn new(gpu: &Gpu, layer: &CAMetalLayer) -> Self {
        let _ = (gpu, layer);
        todo!("M1-5")
    }
}

impl PresentTarget for LayerTarget {}
