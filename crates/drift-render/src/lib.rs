//! # drift-render
//!
//! The Metal compositor implementing [`drift_gfx::FrameSink`] on a per-session render
//! thread: one BGRA8 texture per surface, a BT.709 full-range NV12→RGB shader, blit
//! encoders for surface/cache copies, and `CAMetalLayer` presentation with the
//! `presented` callback fired from the command-buffer completion handler.
//! Implemented by tasks **M1-5** and **M4-3**; composite capture by **M8-1**.

pub mod capture;
pub mod clip;
pub mod color;
pub mod compositor;
pub mod error;
pub mod gpu;
pub mod image;
pub mod layout;
pub mod pixel_buffer;
pub mod reference;
pub mod shaders;
pub mod target;
pub mod thread;

pub use capture::{CaptureConfig, CaptureStats, CapturedFrame};
pub use compositor::{Compositor, RenderStats};
pub use error::RenderError;
pub use gpu::Gpu;
pub use image::BgraImage;
pub use layout::{Filter, PresentLayout, present_layout};
pub use pixel_buffer::PixelBufferNv12;
pub use reference::CpuCompositor;
pub use target::{LayerTarget, OffscreenTarget, PresentTarget};
pub use thread::{RenderSink, RenderThread};
