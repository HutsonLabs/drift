//! # drift-video
//!
//! VideoToolbox H.264: AVC420 decode into IOSurface-backed NV12 `CVPixelBuffer`s
//! (implementing [`drift_core::video::H264Decoder`] and [`drift_core::Nv12Source`]), and
//! the encoder/MP4 writer behind the `recording` feature.
//!
//! | Module | Task |
//! |---|---|
//! | [`decode`] | M1-3 |
//! | `encode` (feature `recording`) | M8-2, M8-3 |

pub mod decode;
