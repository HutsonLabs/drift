//! # drift-codec
//!
//! CPU decoders producing BGRA tiles: RFX Progressive, Planar and Uncompressed, wrapping
//! `ironrdp_graphics` with a rayon tile pool. Built with `opt-level = 3` even in dev
//! (plan §0). Implemented by task **M1-4**.
