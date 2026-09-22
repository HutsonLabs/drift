//! # drift-render
//!
//! The Metal compositor implementing [`drift_gfx::FrameSink`] on a per-session render
//! thread: one BGRA8 texture per surface, a BT.709 full-range NV12→RGB shader, blit
//! encoders for surface/cache copies, and `CAMetalLayer` presentation with the
//! `presented` callback fired from the command-buffer completion handler.
//! Implemented by tasks **M1-5** and **M4-3**; composite capture by **M8-1**.
