//! The shared Metal context: device, command queue and compiled pipelines.

use crate::error::RenderError;

/// A Metal device with Drift's pipelines. Cheap to clone.
#[derive(Clone)]
pub struct Gpu {
    _p: (),
}

impl Gpu {
    /// Uses the system default Metal device.
    pub fn system_default() -> Result<Self, RenderError> {
        todo!("M1-5")
    }
}
