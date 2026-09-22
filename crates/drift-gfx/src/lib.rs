//! # drift-gfx
//!
//! Drift's own client for the `Microsoft::Windows::RDS::Graphics` dynamic virtual channel
//! (plan §2 decision 2). It reuses IronRDP's PDU codecs and ZGFX but owns surface/cache
//! state, codec dispatch and the frame-acknowledgement policy.
//!
//! Status: the [`FrameSink`] contract is fixed (M0-5). The DVC processor, surface state
//! and ack policy are implemented by task **M1-2**.

pub mod sink;

pub use sink::{FrameSink, PresentedCallback};
