//! M1-5 Red: the per-session render thread and the CAMetalLayer target.
#![allow(clippy::unwrap_used)]

mod common;

use std::time::{Duration, Instant};

use common::*;
use drift_core::{Bgra, Rect, Size};
use drift_gfx::FrameSink;
use drift_render::{Compositor, CpuCompositor, LayerTarget, OffscreenTarget, RenderThread};
use drift_testkit::golden::compare;
use objc2_quartz_core::CAMetalLayer;

fn wait_for(log: &PresentLog, n: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while log.ids().len() < n && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn render_thread_runs_the_compositor_on_its_own_named_thread() {
    let thread = RenderThread::spawn("tab-1", || {
        let gpu = gpu();
        let target = OffscreenTarget::new(&gpu, scenes::OUT).unwrap();
        Compositor::new(gpu, target)
    })
    .unwrap();
    let mut sink = thread.sink();
    scenes::cache_round_trip(&mut sink);
    let log = PresentLog::default();
    for id in 0..30 {
        sink.solid_fill(1, Bgra::new(1, 2, 3, 255), &[Rect::new(31, 23, 1, 1)]);
        sink.end_frame(id, log.callback(id));
    }
    wait_for(&log, 30);
    assert_eq!(log.ids(), (0..30).collect::<Vec<_>>());

    let (name, image) = thread.with(|c| {
        c.wait_idle();
        (std::thread::current().name().map(str::to_owned), c.read_output().unwrap())
    });
    assert_eq!(name.as_deref(), Some("drift-render-tab-1"));
    let mut cpu = CpuCompositor::new();
    scenes::cache_round_trip(&mut cpu);
    cpu.solid_fill(1, Bgra::new(1, 2, 3, 255), &[Rect::new(31, 23, 1, 1)]);
    compare(&g(&cpu.composite()), &g(&image), 2).unwrap();
    drop(sink);
    thread.shutdown();
}

#[test]
fn render_thread_shutdown_releases_pending_frames() {
    let thread = RenderThread::spawn("tab-2", || {
        let gpu = gpu();
        let target = OffscreenTarget::new(&gpu, Size::new(8, 8)).unwrap();
        Compositor::new(gpu, target)
    })
    .unwrap();
    let mut sink = thread.sink();
    let log = PresentLog::default();
    sink.reset(Size::new(8, 8));
    for id in 0..5 {
        sink.end_frame(id, log.callback(id));
    }
    thread.shutdown();
    wait_for(&log, 5);
    assert_eq!(log.ids(), (0..5).collect::<Vec<_>>());
    // After shutdown the sink degrades to "never shown": callbacks fire immediately.
    let late = PresentLog::default();
    sink.end_frame(9, late.callback(9));
    assert_eq!(late.ids(), vec![9]);
}

#[test]
fn layer_target_presents_to_a_cametallayer() {
    let gpu = gpu();
    let layer = CAMetalLayer::new();
    let target = LayerTarget::new(&gpu, &layer);
    layer.setDrawableSize(objc2_core_foundation::CGSize { width: 32.0, height: 24.0 });
    let thread = RenderThread::spawn("layer", move || Compositor::new(gpu, target)).unwrap();
    let mut sink = thread.sink();
    scenes::solid_fill(&mut sink);
    let log = PresentLog::default();
    for id in 0..10 {
        sink.end_frame(id, log.callback(id));
    }
    wait_for(&log, 10);
    assert_eq!(log.ids(), (0..10).collect::<Vec<_>>(), "presented exactly once, in order");
    thread.shutdown();
}
