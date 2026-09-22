//! The Metal compositor: [`FrameSink`] on the GPU.
//!
//! - One private `BGRA8Unorm` texture per GFX surface and per cache slot, plus one for the
//!   composed output (desktop-sized).
//! - Uploads (`blit_bgra`) and copies (surface↔surface, surface↔cache, surface→output) are
//!   blit-encoder copies; `solid_fill` and NV12 conversion are compute kernels that write
//!   only the requested rectangles.
//! - All work between two `end_frame`s goes into one command buffer. `end_frame` composes the
//!   output, optionally copies it into a capture buffer (M8-1), draws it into the target with
//!   the M4-3 layout, presents, commits, and fires `presented` from the command buffer's
//!   completion handler — exactly once, even if the compositor is dropped meanwhile.
//! - While hidden (`set_visible(false)`) nothing is composed or presented; surface updates
//!   still land, and `presented` still fires (the frame will never be shown).

use std::collections::{BTreeMap, HashMap};
use std::ptr::{NonNull, null_mut};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Condvar, Mutex};

use block2::RcBlock;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::ProtocolObject;
use objc2_core_foundation::CFRetained;
use objc2_core_video::{
    CVMetalTexture, CVMetalTextureCache, CVMetalTextureGetTexture, CVPixelBuffer,
    CVPixelBufferGetHeightOfPlane, CVPixelBufferGetWidthOfPlane, kCVReturnSuccess,
};
use objc2_metal::{
    MTLBlitCommandEncoder, MTLBuffer, MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue,
    MTLComputeCommandEncoder, MTLComputePipelineState, MTLLoadAction, MTLOrigin, MTLPixelFormat,
    MTLPrimitiveType, MTLRegion, MTLRenderCommandEncoder, MTLRenderPassDescriptor, MTLSize, MTLStorageMode,
    MTLStoreAction, MTLTexture, MTLTextureUsage, MTLViewport,
};
use tracing::{debug, warn};

use drift_core::{Bgra, Nv12Frame, Nv12Planes, Point, Rect, Size};
use drift_gfx::{FrameSink, PresentedCallback};

use crate::capture::{CaptureConfig, CaptureStats, CapturedFrame, Counters, Recorder};
use crate::clip::{clip_copy, clip_rect};
use crate::gpu::{Gpu, Queue, Shared, Texture};
use crate::image::BgraImage;
use crate::layout::{Filter, present_layout};
use crate::pixel_buffer::{PixelBufferAccessor, decoded_picture_accessor, pixel_buffer_nv12_accessor};
use crate::shaders::{FillParams, PresentParams, RegionParams};
use crate::target::{OffscreenTarget, PresentTarget, TargetFrame};

type CommandBuffer = ProtocolObject<dyn MTLCommandBuffer>;
type KeepAlive = Box<dyn Send>;

/// Presentation counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// `end_frame` calls.
    pub frames: u64,
    /// Frames drawn into the target (and presented, for a layer).
    pub presents: u64,
}

struct Surface {
    size: Size<u32>,
    texture: Option<Shared<Retained<Texture>>>,
    origin: Option<Point<u32>>,
}

enum Encoder {
    None,
    Blit(Retained<ProtocolObject<dyn MTLBlitCommandEncoder>>),
    Compute(Retained<ProtocolObject<dyn MTLComputeCommandEncoder>>),
}

/// The command buffer being recorded for the current frame.
struct Work {
    cb: Retained<CommandBuffer>,
    encoder: Encoder,
    keep: Vec<KeepAlive>,
}

impl Work {
    fn end_encoder(&mut self) {
        match std::mem::replace(&mut self.encoder, Encoder::None) {
            Encoder::None => {}
            Encoder::Blit(e) => e.endEncoding(),
            Encoder::Compute(e) => e.endEncoding(),
        }
    }

    fn blit(&mut self) -> Option<&ProtocolObject<dyn MTLBlitCommandEncoder>> {
        if !matches!(self.encoder, Encoder::Blit(_)) {
            self.end_encoder();
            self.encoder = Encoder::Blit(self.cb.blitCommandEncoder()?);
        }
        match &self.encoder {
            Encoder::Blit(e) => Some(e),
            _ => None,
        }
    }

    fn compute(&mut self) -> Option<&ProtocolObject<dyn MTLComputeCommandEncoder>> {
        if !matches!(self.encoder, Encoder::Compute(_)) {
            self.end_encoder();
            self.encoder = Encoder::Compute(self.cb.computeCommandEncoder()?);
        }
        match &self.encoder {
            Encoder::Compute(e) => Some(e),
            _ => None,
        }
    }
}

/// Counts committed vs completed command buffers so `wait_idle` can also wait for the
/// completion *handlers* (which Metal may run after `waitUntilCompleted` returns).
#[derive(Default)]
struct Inflight {
    completed: Mutex<u64>,
    cond: Condvar,
}

/// Runs a `presented` callback exactly once: when called, or when dropped uncalled.
pub(crate) struct CallOnDrop(Option<PresentedCallback>);

impl CallOnDrop {
    pub(crate) fn new(f: PresentedCallback) -> Self {
        Self(Some(f))
    }
    pub(crate) fn call(mut self) {
        if let Some(f) = self.0.take() {
            f();
        }
    }
}

impl Drop for CallOnDrop {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            f();
        }
    }
}

/// Metal implementation of [`FrameSink`], presenting into a [`PresentTarget`].
pub struct Compositor<T: PresentTarget> {
    gpu: Gpu,
    queue: Shared<Retained<Queue>>,
    target: T,
    output_size: Size<u32>,
    output: Option<Shared<Retained<Texture>>>,
    surfaces: BTreeMap<u16, Surface>,
    cache: HashMap<u16, (Size<u32>, Shared<Retained<Texture>>)>,
    work: Option<Shared<Work>>,
    committed: u64,
    last_cb: Option<Shared<Retained<CommandBuffer>>>,
    inflight: Arc<Inflight>,
    visible: bool,
    stats: RenderStats,
    recorder: Option<Recorder>,
    capture_counters: Arc<Counters>,
    nv12_cache: Option<Shared<CFRetained<CVMetalTextureCache>>>,
    accessors: Vec<PixelBufferAccessor>,
}

impl<T: PresentTarget> Compositor<T> {
    /// Creates a compositor with its own command queue, presenting into `target`.
    pub fn new(gpu: Gpu, target: T) -> Self {
        let queue = Shared(gpu.new_queue());
        Self {
            gpu,
            queue,
            target,
            output_size: Size::new(0, 0),
            output: None,
            surfaces: BTreeMap::new(),
            cache: HashMap::new(),
            work: None,
            committed: 0,
            last_cb: None,
            inflight: Arc::default(),
            visible: true,
            stats: RenderStats::default(),
            recorder: None,
            capture_counters: Arc::default(),
            nv12_cache: None,
            accessors: vec![decoded_picture_accessor, pixel_buffer_nv12_accessor],
        }
    }

    /// Registers another way to find the `CVPixelBuffer` behind an [`Nv12Frame`] (e.g. for
    /// `drift-video`'s decoder output type), enabling the zero-copy import for it.
    pub fn add_pixel_buffer_accessor(&mut self, accessor: PixelBufferAccessor) {
        self.accessors.push(accessor);
    }

    /// The presentation target.
    pub fn target(&self) -> &T {
        &self.target
    }

    /// Mutable access to the presentation target.
    pub fn target_mut(&mut self) -> &mut T {
        &mut self.target
    }

    /// Commits pending work and blocks until the GPU and all completion handlers are done.
    pub fn wait_idle(&mut self) {
        self.commit(None);
        if let Some(cb) = &self.last_cb {
            cb.0.waitUntilCompleted();
        }
        let target = self.committed;
        let mut done = self.inflight.completed.lock().unwrap_or_else(|p| p.into_inner());
        while *done < target {
            done = self.inflight.cond.wait(done).unwrap_or_else(|p| p.into_inner());
        }
    }

    /// Reads a surface back to the CPU (waits for pending work). `None` if it doesn't exist.
    pub fn read_surface(&mut self, id: u16) -> Option<BgraImage> {
        let surface = self.surfaces.get(&id)?;
        let (size, texture) = (surface.size, surface.texture.as_ref().map(|t| t.0.clone()));
        self.commit(None);
        match texture {
            Some(t) => self.gpu.read_texture(&self.queue.0, &t),
            None => Some(BgraImage { size, data: Vec::new() }),
        }
    }

    /// Reads the most recently composed output back (waits for pending work). `None` before
    /// the first `reset` or for an empty output.
    pub fn read_output(&mut self) -> Option<BgraImage> {
        let texture = self.output.as_ref()?.0.clone();
        self.commit(None);
        self.gpu.read_texture(&self.queue.0, &texture)
    }

    /// Presentation counters.
    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    /// Starts capturing every composed frame (M8-1). Frames arrive on the returned channel
    /// after their GPU work completes. Replaces any previous recording.
    pub fn start_recording(&mut self, config: CaptureConfig) -> Receiver<CapturedFrame> {
        let (recorder, rx) = Recorder::new(&self.gpu, config, self.capture_counters.clone());
        self.recorder = Some(recorder);
        rx
    }

    /// Stops capturing and releases the pool. Frames already in flight are still delivered.
    pub fn stop_recording(&mut self) {
        self.recorder = None;
    }

    /// Whether composite capture is on.
    pub fn is_recording(&self) -> bool {
        self.recorder.is_some()
    }

    /// Capture counters (cumulative).
    pub fn capture_stats(&self) -> CaptureStats {
        self.capture_counters.snapshot()
    }

    fn work(&mut self) -> Option<&mut Work> {
        if self.work.is_none() {
            let cb = self.queue.0.commandBuffer()?;
            self.work = Some(Shared(Work { cb, encoder: Encoder::None, keep: Vec::new() }));
        }
        self.work.as_mut().map(|w| &mut w.0)
    }

    fn surface_texture(&self, id: u16) -> Option<(Size<u32>, Retained<Texture>)> {
        let s = self.surfaces.get(&id)?;
        Some((s.size, s.texture.as_ref()?.0.clone()))
    }

    /// Commits the current command buffer (creating an empty one if needed so callbacks stay
    /// ordered), running `on_complete` from its completion handler.
    fn commit(&mut self, on_complete: Option<Box<dyn FnOnce() + Send>>) {
        let work = match self.work.take() {
            Some(w) => w.0,
            None => {
                let Some(callback) = on_complete else {
                    return;
                };
                match self.queue.0.commandBuffer() {
                    Some(cb) => {
                        return self.commit_work(
                            Work { cb, encoder: Encoder::None, keep: Vec::new() },
                            Some(callback),
                        );
                    }
                    None => {
                        warn!("no command buffer; completing frame without GPU work");
                        callback();
                        return;
                    }
                }
            }
        };
        self.commit_work(work, on_complete);
    }

    fn commit_work(&mut self, mut work: Work, on_complete: Option<Box<dyn FnOnce() + Send>>) {
        work.end_encoder();
        let Work { cb, keep, .. } = work;
        let payload = Mutex::new(Some((on_complete, keep)));
        let inflight = self.inflight.clone();
        let handler = RcBlock::new(move |_cb: NonNull<CommandBuffer>| {
            let taken = payload.lock().unwrap_or_else(|p| p.into_inner()).take();
            if let Some((on_complete, keep)) = taken {
                drop(keep);
                if let Some(f) = on_complete {
                    f();
                }
                let mut done = inflight.completed.lock().unwrap_or_else(|p| p.into_inner());
                *done += 1;
                inflight.cond.notify_all();
            }
        });
        // SAFETY: the block is retained (copied) by Metal for the command buffer's lifetime
        // and only captures `Send` data guarded by a mutex.
        unsafe { cb.addCompletedHandler(RcBlock::as_ptr(&handler)) };
        cb.commit();
        self.committed += 1;
        self.last_cb = Some(Shared(cb));
    }

    fn fill(&mut self, texture: &Texture, size: Size<u32>, color: [f32; 4], rects: &[Rect]) {
        let gpu = self.gpu.clone();
        let Some(enc) = self.work().and_then(Work::compute) else {
            return;
        };
        enc.setComputePipelineState(gpu.fill_pipeline());
        // SAFETY: index 0 is the kernel's only texture argument.
        unsafe { enc.setTexture_atIndex(Some(texture), 0) };
        for r in rects.iter().filter_map(|r| clip_rect(*r, size)) {
            let params = FillParams { color, region: region(r) };
            // SAFETY: `params` is a `#[repr(C)]` value matching the MSL `FillParams`, copied
            // by Metal before `setBytes` returns.
            unsafe { enc.setBytes_length_atIndex(NonNull::from(&params).cast(), size_of::<FillParams>(), 0) };
            dispatch(enc, gpu.fill_pipeline(), r.size());
        }
    }

    fn copy(&mut self, src: &Texture, from: Point<u32>, dst: &Texture, to: Point<u32>, size: Size<u32>) {
        let Some(enc) = self.work().and_then(Work::blit) else {
            return;
        };
        // SAFETY: callers pass regions produced by `clip_copy`, inside both textures.
        unsafe {
            enc.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                src, 0, 0, origin(from), mtl_size(size), dst, 0, 0, origin(to),
            );
        }
    }

    fn keep(&mut self, item: KeepAlive) {
        if let Some(w) = self.work() {
            w.keep.push(item);
        }
    }

    /// NV12 planes as Metal textures: zero-copy for pixel buffers, uploaded for CPU planes.
    fn nv12_textures(&mut self, frame: &Nv12Frame) -> Option<(Retained<Texture>, Retained<Texture>)> {
        let accessors = self.accessors.clone();
        if let Some(pb) = accessors.iter().find_map(|a| a(frame)) {
            if self.nv12_cache.is_none() {
                self.nv12_cache = crate::capture::texture_cache(&self.gpu);
            }
            let cache = self.nv12_cache.as_ref()?.0.clone();
            let (y_cv, y) = plane_texture(&cache, pb, MTLPixelFormat::R8Unorm, 0)?;
            let (uv_cv, uv) = plane_texture(&cache, pb, MTLPixelFormat::RG8Unorm, 1)?;
            self.keep(Box::new(Shared((y_cv, uv_cv))));
            return Some((y, uv));
        }
        let planes = frame.downcast_ref::<Nv12Planes>()?;
        let size = frame.size();
        let uv_size = Size::new(size.width / 2, size.height.div_ceil(2));
        let upload = |format, size: Size<u32>, bytes: &[u8], row: usize| {
            let t = self.gpu.texture(format, size, MTLTextureUsage::ShaderRead, MTLStorageMode::Shared)?;
            let region = MTLRegion { origin: origin(Point::new(0, 0)), size: mtl_size(size) };
            // SAFETY: `bytes` holds `row * height` bytes laid out with `row` bytes per row
            // (validated by `Nv12Planes::new`), matching the texture's format and size.
            unsafe {
                t.replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                    region,
                    0,
                    NonNull::new(bytes.as_ptr().cast_mut().cast())?,
                    row,
                )
            };
            Some(t)
        };
        let y = upload(MTLPixelFormat::R8Unorm, size, planes.y(), size.width as usize)?;
        let uv = upload(MTLPixelFormat::RG8Unorm, uv_size, planes.uv(), size.width as usize)?;
        Some((y, uv))
    }

    fn compose(&mut self, output: &Texture) {
        let size = self.output_size;
        self.fill(output, size, [0.0, 0.0, 0.0, 1.0], &[Rect::new(0, 0, size.width, size.height)]);
        let mapped: Vec<_> = self
            .surfaces
            .values()
            .filter_map(|s| Some((s.size, s.texture.as_ref()?.0.clone(), s.origin?)))
            .collect();
        for (ssize, texture, at) in mapped {
            if let Some(c) = clip_copy(Rect::new(0, 0, ssize.width, ssize.height), ssize, at, size) {
                self.copy(&texture, c.src, output, c.dst, c.size);
            }
        }
    }

    fn encode_present(&mut self, output: &Texture, frame: &TargetFrame) -> bool {
        let target_size = Size::new(frame.texture.width() as u32, frame.texture.height() as u32);
        let layout = present_layout(self.output_size, target_size);
        let gpu = self.gpu.clone();
        let Some(work) = self.work() else {
            return false;
        };
        work.end_encoder();
        let pass = MTLRenderPassDescriptor::new();
        // SAFETY: index 0 always exists in the colour attachment array.
        let attachment = unsafe { pass.colorAttachments().objectAtIndexedSubscript(0) };
        attachment.setTexture(Some(&frame.texture));
        attachment.setLoadAction(MTLLoadAction::Clear);
        attachment.setStoreAction(MTLStoreAction::Store);
        attachment.setClearColor(MTLClearColor { red: 0.0, green: 0.0, blue: 0.0, alpha: 1.0 });
        let Some(enc) = work.cb.renderCommandEncoderWithDescriptor(&pass) else {
            return false;
        };
        if let Some(layout) = layout {
            let vp = layout.viewport;
            let params = PresentParams {
                origin: [vp.x as f32, vp.y as f32],
                size: [vp.width as f32, vp.height as f32],
                mode: u32::from(layout.filter == Filter::Linear),
                _pad: 0,
            };
            enc.setRenderPipelineState(gpu.present_pipeline());
            enc.setViewport(MTLViewport {
                originX: f64::from(vp.x),
                originY: f64::from(vp.y),
                width: f64::from(vp.width),
                height: f64::from(vp.height),
                znear: 0.0,
                zfar: 1.0,
            });
            // SAFETY: texture index 0 and buffer index 0 match the fragment shader; `params` is
            // `#[repr(C)]` matching MSL `PresentParams` and copied before `setFragmentBytes`
            // returns; the draw uses 3 generated vertices and no vertex buffers.
            unsafe {
                enc.setFragmentTexture_atIndex(Some(output), 0);
                enc.setFragmentBytes_length_atIndex(
                    NonNull::from(&params).cast(),
                    size_of::<PresentParams>(),
                    0,
                );
                enc.drawPrimitives_vertexStart_vertexCount(MTLPrimitiveType::Triangle, 0, 3);
            }
        }
        enc.endEncoding();
        if let Some(drawable) = &frame.drawable {
            work.cb.presentDrawable(drawable);
        }
        true
    }

    fn end_frame_inner(&mut self, frame_id: u32, presented: PresentedCallback) {
        self.stats.frames += 1;
        let presented = CallOnDrop::new(presented);
        let mut delivery = None;
        let output = self.output.as_ref().map(|o| o.0.clone());
        if let Some(output) = output.filter(|_| self.visible || self.recorder.is_some()) {
            self.compose(&output);
            let size = self.output_size;
            if let Some(slot) = self.recorder.as_mut().and_then(|r| r.slot(size)) {
                let capture = slot.texture.clone();
                self.copy(&output, Point::new(0, 0), &capture, Point::new(0, 0), size);
                delivery = self.recorder.as_ref().map(|r| r.delivery(slot, frame_id));
            }
            if self.visible {
                if let Some(frame) = self.target.acquire() {
                    if self.encode_present(&output, &frame) {
                        self.stats.presents += 1;
                    }
                } else {
                    debug!(frame_id, "no drawable available; frame not shown");
                }
            }
        }
        self.commit(Some(Box::new(move || {
            if let Some(d) = delivery {
                d.deliver();
            }
            presented.call();
        })));
        if let Some(r) = &self.recorder {
            r.flush();
        }
        if let Some(cache) = &self.nv12_cache {
            cache.0.flush(0);
        }
    }
}

impl Compositor<OffscreenTarget> {
    /// Reads the offscreen target back (waits for pending work).
    pub fn read_target(&mut self) -> BgraImage {
        self.commit(None);
        let gpu = self.target.gpu().clone();
        let size = self.target.size();
        gpu.read_texture(&self.queue.0, self.target.texture())
            .unwrap_or_else(|| BgraImage::filled(size, [0, 0, 0, 0]))
    }
}

impl<T: PresentTarget> Drop for Compositor<T> {
    fn drop(&mut self) {
        // Submit whatever was recorded; completion handlers still run after we are gone.
        self.commit(None);
    }
}

impl<T: PresentTarget> FrameSink for Compositor<T> {
    fn reset(&mut self, output: Size<u32>) {
        self.surfaces.clear();
        self.cache.clear();
        self.output_size = output;
        self.output = self.gpu.bgra_texture(output).map(Shared);
        if self.output.is_none() && output.width > 0 && output.height > 0 {
            warn!(?output, "cannot allocate output texture");
        }
    }

    fn create_surface(&mut self, id: u16, size: Size<u32>) {
        let texture = self.gpu.bgra_texture(size);
        if let Some(t) = &texture {
            self.fill(t, size, [0.0, 0.0, 0.0, 1.0], &[Rect::new(0, 0, size.width, size.height)]);
        } else if size.width > 0 && size.height > 0 {
            warn!(id, ?size, "cannot allocate surface texture");
        }
        self.surfaces.insert(id, Surface { size, texture: texture.map(Shared), origin: None });
    }

    fn delete_surface(&mut self, id: u16) {
        self.surfaces.remove(&id);
    }

    fn map_surface_to_output(&mut self, id: u16, origin: Point<u32>) {
        if let Some(s) = self.surfaces.get_mut(&id) {
            s.origin = Some(origin);
        }
    }

    fn blit_bgra(&mut self, id: u16, rect: Rect, stride: usize, data: &[u8]) {
        let row = rect.width as usize * 4;
        let h = rect.height as usize;
        let needed = stride.checked_mul(h.saturating_sub(1)).and_then(|n| n.checked_add(row));
        if rect.is_empty() || stride < row || needed.is_none_or(|n| data.len() < n) {
            debug!(id, ?rect, stride, len = data.len(), "ignoring malformed BGRA blit");
            return;
        }
        let Some((ssize, texture)) = self.surface_texture(id) else {
            return;
        };
        let Some(c) = clip_copy(
            Rect::new(0, 0, rect.width, rect.height),
            rect.size(),
            Point::new(rect.x, rect.y),
            ssize,
        ) else {
            return;
        };
        let (w, h) = (c.size.width as usize, c.size.height as usize);
        let Some(buffer) = self.gpu.device_buffer(w * 4 * h) else {
            return;
        };
        let dst = buffer.contents().cast::<u8>();
        for y in 0..h {
            let src = (c.src.y as usize + y) * stride + c.src.x as usize * 4;
            // SAFETY: `src..src + w*4` is inside `data` (validated above: the clipped region
            // lies within the rect whose rows all fit), and the buffer holds `w*4*h` bytes.
            unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr().add(src), dst.as_ptr().add(y * w * 4), w * 4)
            };
        }
        let Some(enc) = self.work().and_then(Work::blit) else {
            return;
        };
        // SAFETY: the buffer holds a tightly packed `w`×`h` BGRA image; the destination region
        // is inside the surface (clip_copy).
        unsafe {
            enc.copyFromBuffer_sourceOffset_sourceBytesPerRow_sourceBytesPerImage_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                &buffer,
                0,
                w * 4,
                w * 4 * h,
                mtl_size(c.size),
                &texture,
                0,
                0,
                origin(c.dst),
            );
        }
    }

    fn blit_nv12(&mut self, id: u16, frame: &Nv12Frame, regions: &[Rect]) {
        let Some((ssize, texture)) = self.surface_texture(id) else {
            return;
        };
        let fsize = frame.size();
        let bounds = Size::new(ssize.width.min(fsize.width), ssize.height.min(fsize.height));
        let rects: Vec<Rect> = regions.iter().filter_map(|r| clip_rect(*r, bounds)).collect();
        if rects.is_empty() {
            return;
        }
        let Some((y, uv)) = self.nv12_textures(frame) else {
            debug!(?frame, "unsupported NV12 frame backing; ignored");
            return;
        };
        self.keep(Box::new(frame.clone()));
        let gpu = self.gpu.clone();
        let Some(enc) = self.work().and_then(Work::compute) else {
            return;
        };
        enc.setComputePipelineState(gpu.nv12_pipeline());
        // SAFETY: indices 0..=2 are the kernel's texture arguments (Y, UV, destination).
        unsafe {
            enc.setTexture_atIndex(Some(&y), 0);
            enc.setTexture_atIndex(Some(&uv), 1);
            enc.setTexture_atIndex(Some(&texture), 2);
        }
        for r in rects {
            let params = region(r);
            // SAFETY: `params` is `#[repr(C)]` matching MSL `RegionParams`, copied by Metal.
            unsafe {
                enc.setBytes_length_atIndex(NonNull::from(&params).cast(), size_of::<RegionParams>(), 0)
            };
            dispatch(enc, gpu.nv12_pipeline(), r.size());
        }
    }

    fn solid_fill(&mut self, id: u16, color: Bgra, rects: &[Rect]) {
        let Some((size, texture)) = self.surface_texture(id) else {
            return;
        };
        let c = |v: u8| f32::from(v) / 255.0;
        self.fill(&texture, size, [c(color.r), c(color.g), c(color.b), c(color.a)], rects);
    }

    fn surface_to_surface(&mut self, src: u16, dst: u16, rect: Rect, dests: &[Point<u32>]) {
        let (Some((ssize, stex)), Some((dsize, dtex))) =
            (self.surface_texture(src), self.surface_texture(dst))
        else {
            return;
        };
        let Some(r) = clip_rect(rect, ssize) else {
            return;
        };
        // Same surface: stage through a scratch texture so overlapping copies read the
        // source as it was before any destination is written.
        let (source, at) = if src == dst {
            let Some(scratch) = self.gpu.bgra_texture(r.size()) else {
                return;
            };
            self.copy(&stex, Point::new(r.x, r.y), &scratch, Point::new(0, 0), r.size());
            (scratch, Rect::new(0, 0, r.width, r.height))
        } else {
            (stex, r)
        };
        let source_bounds = if src == dst { r.size() } else { ssize };
        for d in dests {
            if let Some(c) = clip_copy(at, source_bounds, *d, dsize) {
                self.copy(&source, c.src, &dtex, c.dst, c.size);
            }
        }
    }

    fn surface_to_cache(&mut self, id: u16, rect: Rect, slot: u16) {
        let Some((ssize, stex)) = self.surface_texture(id) else {
            return;
        };
        let Some(r) = clip_rect(rect, ssize) else {
            return;
        };
        let entry = match self.cache.get(&slot) {
            Some((size, t)) if *size == r.size() => Some(t.0.clone()),
            _ => self.gpu.bgra_texture(r.size()),
        };
        let Some(entry) = entry else {
            return;
        };
        self.copy(&stex, Point::new(r.x, r.y), &entry, Point::new(0, 0), r.size());
        self.cache.insert(slot, (r.size(), Shared(entry)));
    }

    fn cache_to_surface(&mut self, slot: u16, id: u16, dests: &[Point<u32>]) {
        let Some((csize, ctex)) = self.cache.get(&slot).map(|(s, t)| (*s, t.0.clone())) else {
            return;
        };
        let Some((dsize, dtex)) = self.surface_texture(id) else {
            return;
        };
        for d in dests {
            if let Some(c) = clip_copy(Rect::new(0, 0, csize.width, csize.height), csize, *d, dsize) {
                self.copy(&ctex, c.src, &dtex, c.dst, c.size);
            }
        }
    }

    fn evict_cache(&mut self, slot: u16) {
        self.cache.remove(&slot);
    }

    fn end_frame(&mut self, frame_id: u32, presented: PresentedCallback) {
        autoreleasepool(|_| self.end_frame_inner(frame_id, presented));
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
}

fn origin(p: Point<u32>) -> MTLOrigin {
    MTLOrigin { x: p.x as usize, y: p.y as usize, z: 0 }
}

fn mtl_size(s: Size<u32>) -> MTLSize {
    MTLSize { width: s.width as usize, height: s.height as usize, depth: 1 }
}

fn region(r: Rect) -> RegionParams {
    RegionParams { origin: [r.x, r.y], extent: [r.width, r.height] }
}

fn dispatch(
    enc: &ProtocolObject<dyn MTLComputeCommandEncoder>,
    pipeline: &ProtocolObject<dyn MTLComputePipelineState>,
    size: Size<u32>,
) {
    let w = pipeline.threadExecutionWidth().max(1);
    let h = (pipeline.maxTotalThreadsPerThreadgroup() / w).clamp(1, 16);
    enc.dispatchThreads_threadsPerThreadgroup(mtl_size(size), MTLSize { width: w, height: h, depth: 1 });
}

/// A Metal view of one plane of a pixel buffer, through the texture cache.
fn plane_texture(
    cache: &CVMetalTextureCache,
    pb: &CVPixelBuffer,
    format: MTLPixelFormat,
    plane: usize,
) -> Option<(CFRetained<CVMetalTexture>, Retained<Texture>)> {
    let (w, h) = (CVPixelBufferGetWidthOfPlane(pb, plane), CVPixelBufferGetHeightOfPlane(pb, plane));
    let mut raw: *mut CVMetalTexture = null_mut();
    // SAFETY: valid cache, buffer and out-pointer; plane/format/size come from the buffer.
    let rc = unsafe {
        CVMetalTextureCache::create_texture_from_image(
            None,
            cache,
            pb,
            None,
            format,
            w,
            h,
            plane,
            NonNull::from(&mut raw),
        )
    };
    if rc != kCVReturnSuccess {
        warn!(rc, plane, "CVMetalTextureCacheCreateTextureFromImage failed");
        return None;
    }
    // SAFETY: success returns +1 ownership of a non-null texture.
    let cv = unsafe { CFRetained::from_raw(NonNull::new(raw)?) };
    let texture = CVMetalTextureGetTexture(&cv)?;
    Some((cv, texture))
}
