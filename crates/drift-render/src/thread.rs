//! The per-session render thread (plan §2: "Metal compositor on a per-session render thread").
//!
//! [`RenderThread::spawn`] starts a thread named `drift-render-<label>`, builds the sink
//! (normally a [`Compositor`](crate::Compositor) over a [`LayerTarget`](crate::LayerTarget))
//! *on* that thread, and then executes [`FrameSink`] calls received from [`RenderSink`]
//! handles in order, each inside an autorelease pool. `presented` callbacks are guaranteed
//! to run exactly once: if a frame can no longer reach the thread (after shutdown), its
//! callback runs immediately, as for a frame that will never be shown.

use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;

use objc2::rc::autoreleasepool;

use drift_core::{Bgra, Nv12Frame, Point, Rect, Size};
use drift_gfx::{FrameSink, PresentedCallback};

use crate::compositor::CallOnDrop;

type Call<S> = Box<dyn FnOnce(&mut S) + Send>;

enum Msg<S> {
    Call(Call<S>),
    EndFrame(u32, CallOnDrop),
    Shutdown,
}

/// A render thread owning a sink `S`.
pub struct RenderThread<S> {
    tx: Sender<Msg<S>>,
    handle: Option<JoinHandle<()>>,
}

/// A [`FrameSink`] that forwards every call to a [`RenderThread`].
///
/// BGRA data, region lists and destination lists are copied into the message; NV12 frames
/// are reference-counted handles, so the zero-copy path stays zero-copy.
pub struct RenderSink<S> {
    tx: Sender<Msg<S>>,
}

impl<S: FrameSink + 'static> RenderThread<S> {
    /// Spawns `drift-render-<label>` and builds the sink on it with `make`.
    pub fn spawn(label: &str, make: impl FnOnce() -> S + Send + 'static) -> std::io::Result<Self> {
        let (tx, rx) = channel::<Msg<S>>();
        let handle = std::thread::Builder::new().name(format!("drift-render-{label}")).spawn(move || {
            let mut sink = autoreleasepool(|_| make());
            for msg in rx.iter() {
                let keep_going = autoreleasepool(|_| match msg {
                    Msg::Call(f) => {
                        f(&mut sink);
                        true
                    }
                    Msg::EndFrame(id, cb) => {
                        sink.end_frame(id, Box::new(move || cb.call()));
                        true
                    }
                    Msg::Shutdown => false,
                });
                if !keep_going {
                    break;
                }
            }
            autoreleasepool(|_| drop(sink));
            // Anything still queued is dropped here; `CallOnDrop` fires pending callbacks.
            drop(rx);
        })?;
        Ok(Self { tx, handle: Some(handle) })
    }

    /// A new sink handle feeding this thread.
    pub fn sink(&self) -> RenderSink<S> {
        RenderSink { tx: self.tx.clone() }
    }

    /// Runs `f` with the sink on the render thread and returns its result, or `None` if the
    /// thread has stopped.
    pub fn with<R: Send + 'static>(&self, f: impl FnOnce(&mut S) -> R + Send + 'static) -> Option<R> {
        let (rtx, rrx) = channel();
        let call: Call<S> = Box::new(move |s| {
            let _ = rtx.send(f(s));
        });
        self.tx.send(Msg::Call(call)).ok()?;
        rrx.recv().ok()
    }

    /// Processes everything already queued, drops the sink on its thread and joins it.
    /// (Dropping the `RenderThread` does the same.)
    pub fn shutdown(self) {
        drop(self);
    }
}

impl<S> Drop for RenderThread<S> {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Shutdown);
        if let Some(h) = self.handle.take()
            && h.join().is_err()
        {
            tracing::error!("render thread panicked");
        }
    }
}

impl<S: FrameSink + 'static> RenderSink<S> {
    fn call(&self, f: impl FnOnce(&mut S) + Send + 'static) {
        // A stopped thread simply drops the operation.
        let _ = self.tx.send(Msg::Call(Box::new(f)));
    }

    /// Like [`RenderThread::with`], from any thread holding a sink: runs `f` on the render
    /// thread after everything queued before it and returns its result, or `None` if the thread
    /// has stopped. (The app reads live thumbnails this way off the main thread.)
    pub fn with<R: Send + 'static>(&self, f: impl FnOnce(&mut S) -> R + Send + 'static) -> Option<R> {
        let (rtx, rrx) = channel();
        self.call(move |s| {
            let _ = rtx.send(f(s));
        });
        rrx.recv().ok()
    }
}

impl<S: FrameSink + 'static> FrameSink for RenderSink<S> {
    fn reset(&mut self, output: Size<u32>) {
        self.call(move |s| s.reset(output));
    }

    fn create_surface(&mut self, id: u16, size: Size<u32>) {
        self.call(move |s| s.create_surface(id, size));
    }

    fn delete_surface(&mut self, id: u16) {
        self.call(move |s| s.delete_surface(id));
    }

    fn map_surface_to_output(&mut self, id: u16, origin: Point<u32>) {
        self.call(move |s| s.map_surface_to_output(id, origin));
    }

    fn blit_bgra(&mut self, id: u16, rect: Rect, stride: usize, data: &[u8]) {
        let data = data.to_vec();
        self.call(move |s| s.blit_bgra(id, rect, stride, &data));
    }

    fn blit_nv12(&mut self, id: u16, frame: &Nv12Frame, regions: &[Rect]) {
        let (frame, regions) = (frame.clone(), regions.to_vec());
        self.call(move |s| s.blit_nv12(id, &frame, &regions));
    }

    fn solid_fill(&mut self, id: u16, color: Bgra, rects: &[Rect]) {
        let rects = rects.to_vec();
        self.call(move |s| s.solid_fill(id, color, &rects));
    }

    fn surface_to_surface(&mut self, src: u16, dst: u16, rect: Rect, dests: &[Point<u32>]) {
        let dests = dests.to_vec();
        self.call(move |s| s.surface_to_surface(src, dst, rect, &dests));
    }

    fn surface_to_cache(&mut self, id: u16, rect: Rect, slot: u16) {
        self.call(move |s| s.surface_to_cache(id, rect, slot));
    }

    fn cache_to_surface(&mut self, slot: u16, id: u16, dests: &[Point<u32>]) {
        let dests = dests.to_vec();
        self.call(move |s| s.cache_to_surface(slot, id, &dests));
    }

    fn evict_cache(&mut self, slot: u16) {
        self.call(move |s| s.evict_cache(slot));
    }

    fn end_frame(&mut self, frame_id: u32, presented: PresentedCallback) {
        // On send failure the message (and its `CallOnDrop`) is dropped: the callback runs now.
        let _ = self.tx.send(Msg::EndFrame(frame_id, CallOnDrop::new(presented)));
    }

    fn set_visible(&mut self, visible: bool) {
        self.call(move |s| s.set_visible(visible));
    }
}
