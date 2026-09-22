//! Shared helpers for the drift-gfx integration tests.
#![allow(dead_code, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use drift_codec::TilePool;
use drift_core::video::{DecodeError, H264Decoder, Nv12Frame};
use drift_core::{Nv12Planes, Size};
use drift_gfx::{AckOutbox, GfxClient};
use drift_testkit::frame_sink::fnv1a64;
use drift_testkit::{FrameLog, FrameSinkCall, PresentMode, RecordingFrameSink};
use ironrdp_core::encode_vec;
use ironrdp_egfx::pdu::{
    Codec1Type, CreateSurfacePdu, EndFramePdu, GfxPdu, MapSurfaceToOutputPdu, PixelFormat, ResetGraphicsPdu,
    StartFramePdu, Timestamp, WireToSurface1Pdu,
};
use ironrdp_graphics::zgfx;
use ironrdp_pdu::geometry::ExclusiveRectangle;
use serde::Serialize;

/// A stand-in for `drift-video`'s VideoToolbox decoder: every access unit yields a black
/// NV12 picture of `size`, or an error when the access unit starts with `0xEE`.
#[derive(Debug, Clone)]
pub struct FakeH264 {
    pub size: Size<u32>,
    pub decodes: Arc<AtomicUsize>,
    pub resets: Arc<AtomicUsize>,
}

impl FakeH264 {
    pub fn new(size: Size<u32>) -> Self {
        Self { size, decodes: Arc::default(), resets: Arc::default() }
    }
}

impl H264Decoder for FakeH264 {
    fn decode(&mut self, annex_b: &[u8]) -> Result<Option<Nv12Frame>, DecodeError> {
        if annex_b.first() == Some(&0xEE) {
            return Err(DecodeError("fake decoder rejects 0xEE".into()));
        }
        self.decodes.fetch_add(1, Ordering::SeqCst);
        let (w, h) = (self.size.width as usize, self.size.height as usize);
        let planes = Nv12Planes::new(self.size, vec![0; w * h], vec![128; w * h.div_ceil(2)]).unwrap();
        Ok(Some(Nv12Frame::new(planes)))
    }

    fn reset(&mut self) {
        self.resets.fetch_add(1, Ordering::SeqCst);
    }
}

/// A client wired to a recording sink and a fake H.264 decoder.
pub struct Harness {
    pub client: GfxClient,
    pub log: FrameLog,
    pub acks: AckOutbox,
    pub h264: FakeH264,
}

pub fn harness(mode: PresentMode, h264_size: Size<u32>) -> Harness {
    let (sink, log) = RecordingFrameSink::new(mode);
    let h264 = FakeH264::new(h264_size);
    let client = GfxClient::new(Box::new(sink), Box::new(h264.clone()), TilePool::new(2).unwrap());
    let acks = client.acks();
    Harness { client, log, acks, h264 }
}

/// Encodes PDUs back to back and wraps them as one uncompressed `RDP_SEGMENTED_DATA` payload.
pub fn wire(pdus: &[GfxPdu]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for p in pdus {
        bytes.extend(encode_vec(p).unwrap());
    }
    zgfx::wrap_uncompressed(&bytes)
}

pub fn reset(w: u32, h: u32) -> GfxPdu {
    GfxPdu::ResetGraphics(ResetGraphicsPdu { width: w, height: h, monitors: Vec::new() })
}

pub fn create(id: u16, w: u16, h: u16) -> GfxPdu {
    GfxPdu::CreateSurface(CreateSurfacePdu {
        surface_id: id,
        width: w,
        height: h,
        pixel_format: PixelFormat::XRgb,
    })
}

pub fn map(id: u16, x: u32, y: u32) -> GfxPdu {
    GfxPdu::MapSurfaceToOutput(MapSurfaceToOutputPdu {
        surface_id: id,
        output_origin_x: x,
        output_origin_y: y,
    })
}

pub fn start(frame_id: u32) -> GfxPdu {
    GfxPdu::StartFrame(StartFramePdu {
        timestamp: Timestamp { milliseconds: 0, seconds: 0, minutes: 0, hours: 0 },
        frame_id,
    })
}

pub fn end(frame_id: u32) -> GfxPdu {
    GfxPdu::EndFrame(EndFramePdu { frame_id })
}

pub fn rect16(left: u16, top: u16, right: u16, bottom: u16) -> ExclusiveRectangle {
    ExclusiveRectangle { left, top, right, bottom }
}

pub fn w2s1(id: u16, codec: Codec1Type, dest: ExclusiveRectangle, data: Vec<u8>) -> GfxPdu {
    GfxPdu::WireToSurface1(WireToSurface1Pdu {
        surface_id: id,
        codec_id: codec,
        pixel_format: PixelFormat::XRgb,
        destination_rectangle: dest,
        bitmap_data: data,
    })
}

/// Decodes a buffer of back-to-back GFX PDUs.
pub fn decode_all(mut data: &[u8]) -> Vec<GfxPdu> {
    let mut out = Vec::new();
    while !data.is_empty() {
        let mut cursor = ironrdp_core::ReadCursor::new(data);
        out.push(ironrdp_core::decode_cursor::<GfxPdu>(&mut cursor).unwrap());
        data = &data[cursor.pos()..];
    }
    out
}

/// A compact, snapshot-friendly view of a [`FrameSinkCall`] sequence: consecutive
/// `BlitBgra` calls on one surface collapse into one entry with a combined hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Summary {
    Call(FrameSinkCall),
    BlitBgraRun { id: u16, blits: usize, pixels: usize, hash: u64 },
}

pub fn summarize(calls: &[FrameSinkCall]) -> Vec<Summary> {
    let mut out: Vec<Summary> = Vec::new();
    for call in calls {
        if let FrameSinkCall::BlitBgra { id, rect, hash, .. } = call {
            let px = rect.width as usize * rect.height as usize;
            if let Some(Summary::BlitBgraRun { id: run_id, blits, pixels, hash: h }) = out.last_mut() {
                if run_id == id {
                    *blits += 1;
                    *pixels += px;
                    *h = fnv1a64(&[h.to_le_bytes(), hash.to_le_bytes()].concat());
                    continue;
                }
            }
            out.push(Summary::BlitBgraRun { id: *id, blits: 1, pixels: px, hash: *hash });
        } else {
            out.push(Summary::Call(call.clone()));
        }
    }
    out
}
