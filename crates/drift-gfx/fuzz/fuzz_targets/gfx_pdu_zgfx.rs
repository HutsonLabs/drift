//! Fuzz target: arbitrary server bytes on the graphics channel (M1-2, M9-2).
//!
//! The first input byte picks the entry point:
//! - even: a raw DVC payload (`RDP_SEGMENTED_DATA` + ZGFX) through `GfxClient::process_payload`;
//! - odd: already-decompressed GFX PDUs through `GfxClient::process_pdus`.
//!
//! The client runs after a valid ResetGraphics + CreateSurface so drawing commands reach the
//! surface/cache checks and the CPU codecs. Any panic, overflow or OOM is a bug; errors are fine.
#![no_main]

use drift_codec::TilePool;
use drift_core::video::{DecodeError, H264Decoder, Nv12Frame};
use drift_core::{Bgra, Nv12Planes, Point, Rect, Size};
use drift_gfx::{FrameSink, GfxClient, PresentedCallback};
use libfuzzer_sys::fuzz_target;

struct NullSink;

impl FrameSink for NullSink {
    fn reset(&mut self, _: Size<u32>) {}
    fn create_surface(&mut self, _: u16, _: Size<u32>) {}
    fn delete_surface(&mut self, _: u16) {}
    fn map_surface_to_output(&mut self, _: u16, _: Point<u32>) {}
    fn blit_bgra(&mut self, _: u16, _: Rect, _: usize, _: &[u8]) {}
    fn blit_nv12(&mut self, _: u16, _: &Nv12Frame, _: &[Rect]) {}
    fn solid_fill(&mut self, _: u16, _: Bgra, _: &[Rect]) {}
    fn surface_to_surface(&mut self, _: u16, _: u16, _: Rect, _: &[Point<u32>]) {}
    fn surface_to_cache(&mut self, _: u16, _: Rect, _: u16) {}
    fn cache_to_surface(&mut self, _: u16, _: u16, _: &[Point<u32>]) {}
    fn evict_cache(&mut self, _: u16) {}
    fn end_frame(&mut self, _: u32, presented: PresentedCallback) {
        presented();
    }
    fn set_visible(&mut self, _: bool) {}
}

/// Returns a tiny picture for any access unit.
struct TinyDecoder;

impl H264Decoder for TinyDecoder {
    fn decode(&mut self, _: &[u8]) -> Result<Option<Nv12Frame>, DecodeError> {
        let planes = Nv12Planes::new(Size::new(16, 16), vec![0; 256], vec![128; 128])
            .ok_or_else(|| DecodeError("planes".into()))?;
        Ok(Some(Nv12Frame::new(planes)))
    }
    fn reset(&mut self) {}
}

fuzz_target!(|data: &[u8]| {
    let Some((&mode, rest)) = data.split_first() else { return };
    // One shared pool: spawning a rayon pool per input would dominate the run time.
    static POOL: std::sync::OnceLock<Option<TilePool>> = std::sync::OnceLock::new();
    let Some(pool) = POOL.get_or_init(|| TilePool::new(1).ok()).clone() else { return };
    let mut client = GfxClient::new(Box::new(NullSink), Box::new(TinyDecoder), pool);
    let _ = client.process_pdus(&setup_pdus());
    if mode % 2 == 0 {
        let _ = client.process_payload(rest);
    } else {
        let _ = client.process_pdus(rest);
    }
    let _ = client.acks().drain();
});

/// Decompressed ResetGraphics(64x64) + CreateSurface(1, 64x64).
fn setup_pdus() -> Vec<u8> {
    let mut out = Vec::new();
    // RDPGFX_RESET_GRAPHICS_PDU is always 340 bytes.
    out.extend(0x000Eu16.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(340u32.to_le_bytes());
    out.extend(64u32.to_le_bytes());
    out.extend(64u32.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.resize(340, 0);
    // RDPGFX_CREATE_SURFACE_PDU: surfaceId, width, height, pixelFormat.
    out.extend(0x0009u16.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(15u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(64u16.to_le_bytes());
    out.extend(64u16.to_le_bytes());
    out.push(0x20);
    out
}
