//! # drift-testkit
//!
//! Shared test support (dev-dependency only):
//!
//! - [`ManualClock`]: a [`drift_core::Clock`] that only moves when told to.
//! - [`RecordingFrameSink`]: a [`drift_gfx::FrameSink`] that records every call for
//!   assertions and `insta` snapshots, with controllable `presented` callbacks.
//! - `fixtures` (M0-3), `FakeServer` (M1-1) and golden-image utilities (M1-5) are added
//!   by their owning tasks.

pub mod clock;
pub mod fixtures;
pub mod frame_sink;

pub use clock::ManualClock;
pub use frame_sink::{FrameLog, FrameSinkCall, PresentMode, RecordingFrameSink};
