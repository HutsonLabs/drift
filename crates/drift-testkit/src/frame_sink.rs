//! A recording [`FrameSink`] for GFX state-machine tests and snapshots.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use drift_core::{Bgra, Nv12Frame, Point, Rect, Size};
use drift_gfx::{FrameSink, PresentedCallback};
use serde::Serialize;

/// One recorded [`FrameSink`] call. Pixel payloads are reduced to length + FNV-1a hash so
/// snapshots stay small and stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum FrameSinkCall {
    /// [`FrameSink::reset`].
    Reset {
        /// Output size.
        output: Size<u32>,
    },
    /// [`FrameSink::create_surface`].
    CreateSurface {
        /// Surface id.
        id: u16,
        /// Surface size.
        size: Size<u32>,
    },
    /// [`FrameSink::delete_surface`].
    DeleteSurface {
        /// Surface id.
        id: u16,
    },
    /// [`FrameSink::map_surface_to_output`].
    MapSurfaceToOutput {
        /// Surface id.
        id: u16,
        /// Output origin.
        origin: Point<u32>,
    },
    /// [`FrameSink::blit_bgra`].
    BlitBgra {
        /// Surface id.
        id: u16,
        /// Destination rectangle.
        rect: Rect,
        /// Source stride in bytes.
        stride: usize,
        /// Payload length in bytes.
        len: usize,
        /// FNV-1a 64 hash of the payload.
        hash: u64,
    },
    /// [`FrameSink::blit_nv12`].
    BlitNv12 {
        /// Surface id.
        id: u16,
        /// Picture size.
        size: Size<u32>,
        /// Region rectangles.
        regions: Vec<Rect>,
    },
    /// [`FrameSink::solid_fill`].
    SolidFill {
        /// Surface id.
        id: u16,
        /// Fill colour.
        color: Bgra,
        /// Filled rectangles.
        rects: Vec<Rect>,
    },
    /// [`FrameSink::surface_to_surface`].
    SurfaceToSurface {
        /// Source surface.
        src: u16,
        /// Destination surface.
        dst: u16,
        /// Source rectangle.
        rect: Rect,
        /// Destination points.
        dests: Vec<Point<u32>>,
    },
    /// [`FrameSink::surface_to_cache`].
    SurfaceToCache {
        /// Surface id.
        id: u16,
        /// Source rectangle.
        rect: Rect,
        /// Cache slot.
        slot: u16,
    },
    /// [`FrameSink::cache_to_surface`].
    CacheToSurface {
        /// Cache slot.
        slot: u16,
        /// Surface id.
        id: u16,
        /// Destination points.
        dests: Vec<Point<u32>>,
    },
    /// [`FrameSink::evict_cache`].
    EvictCache {
        /// Cache slot.
        slot: u16,
    },
    /// [`FrameSink::end_frame`].
    EndFrame {
        /// Frame id.
        frame_id: u32,
    },
    /// [`FrameSink::set_visible`].
    SetVisible {
        /// Visibility.
        visible: bool,
    },
}

/// When [`RecordingFrameSink`] runs the `presented` callbacks it receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PresentMode {
    /// Call `presented` inside `end_frame` (an instantly presenting renderer).
    #[default]
    Immediate,
    /// Queue callbacks until [`FrameLog::present_pending`] (simulates GPU latency).
    Deferred,
}

#[derive(Default)]
struct Shared {
    calls: Vec<FrameSinkCall>,
    pending: Vec<(u32, PresentedCallback)>,
}

/// Test-side handle onto what a [`RecordingFrameSink`] has seen. Clonable, `Send`.
#[derive(Clone, Default)]
pub struct FrameLog {
    shared: Arc<Mutex<Shared>>,
}

impl std::fmt::Debug for FrameLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameLog").finish_non_exhaustive()
    }
}

/// A [`FrameSink`] that records calls into a shared [`FrameLog`].
#[derive(Debug)]
pub struct RecordingFrameSink {
    log: FrameLog,
    mode: PresentMode,
}

impl RecordingFrameSink {
    /// Creates a sink and the log handle that observes it.
    pub fn new(mode: PresentMode) -> (Self, FrameLog) {
        let log = FrameLog::default();
        (Self { log: log.clone(), mode }, log)
    }

    fn record(&self, call: FrameSinkCall) {
        self.log.lock().calls.push(call);
    }
}

impl FrameLog {
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Snapshot of all calls so far.
    pub fn calls(&self) -> Vec<FrameSinkCall> {
        self.lock().calls.clone()
    }

    /// Removes and returns all calls so far.
    pub fn take(&self) -> Vec<FrameSinkCall> {
        std::mem::take(&mut self.lock().calls)
    }

    /// Frame ids whose `presented` callback has not run yet (Deferred mode).
    pub fn pending_frames(&self) -> Vec<u32> {
        self.lock().pending.iter().map(|(id, _)| *id).collect()
    }

    /// Runs every queued `presented` callback in frame order; returns how many ran.
    /// Callbacks run after the internal lock is released, so they may use the log.
    pub fn present_pending(&self) -> usize {
        let pending = std::mem::take(&mut self.lock().pending);
        let n = pending.len();
        for (_, presented) in pending {
            presented();
        }
        n
    }
}

/// FNV-1a 64-bit hash, used to fingerprint pixel payloads.
pub fn fnv1a64(data: &[u8]) -> u64 {
    data.iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
}

impl FrameSink for RecordingFrameSink {
    fn reset(&mut self, output: Size<u32>) {
        self.record(FrameSinkCall::Reset { output });
    }
    fn create_surface(&mut self, id: u16, size: Size<u32>) {
        self.record(FrameSinkCall::CreateSurface { id, size });
    }
    fn delete_surface(&mut self, id: u16) {
        self.record(FrameSinkCall::DeleteSurface { id });
    }
    fn map_surface_to_output(&mut self, id: u16, origin: Point<u32>) {
        self.record(FrameSinkCall::MapSurfaceToOutput { id, origin });
    }
    fn blit_bgra(&mut self, id: u16, rect: Rect, stride: usize, data: &[u8]) {
        self.record(FrameSinkCall::BlitBgra { id, rect, stride, len: data.len(), hash: fnv1a64(data) });
    }
    fn blit_nv12(&mut self, id: u16, frame: &Nv12Frame, regions: &[Rect]) {
        self.record(FrameSinkCall::BlitNv12 { id, size: frame.size(), regions: regions.to_vec() });
    }
    fn solid_fill(&mut self, id: u16, color: Bgra, rects: &[Rect]) {
        self.record(FrameSinkCall::SolidFill { id, color, rects: rects.to_vec() });
    }
    fn surface_to_surface(&mut self, src: u16, dst: u16, rect: Rect, dests: &[Point<u32>]) {
        self.record(FrameSinkCall::SurfaceToSurface { src, dst, rect, dests: dests.to_vec() });
    }
    fn surface_to_cache(&mut self, id: u16, rect: Rect, slot: u16) {
        self.record(FrameSinkCall::SurfaceToCache { id, rect, slot });
    }
    fn cache_to_surface(&mut self, slot: u16, id: u16, dests: &[Point<u32>]) {
        self.record(FrameSinkCall::CacheToSurface { slot, id, dests: dests.to_vec() });
    }
    fn evict_cache(&mut self, slot: u16) {
        self.record(FrameSinkCall::EvictCache { slot });
    }
    fn end_frame(&mut self, frame_id: u32, presented: PresentedCallback) {
        self.record(FrameSinkCall::EndFrame { frame_id });
        match self.mode {
            PresentMode::Immediate => presented(),
            PresentMode::Deferred => self.log.lock().pending.push((frame_id, presented)),
        }
    }
    fn set_visible(&mut self, visible: bool) {
        self.record(FrameSinkCall::SetVisible { visible });
    }
}
