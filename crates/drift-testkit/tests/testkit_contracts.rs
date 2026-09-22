//! M0-5 Red: `ManualClock` and `RecordingFrameSink` behaviour.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use drift_core::{Bgra, Clock, Nv12Frame, Nv12Planes, Point, Rect, Size};
use drift_gfx::FrameSink;
use drift_testkit::frame_sink::fnv1a64;
use drift_testkit::{FrameSinkCall, ManualClock, PresentMode, RecordingFrameSink};

#[test]
fn manual_clock_only_moves_when_advanced_and_clones_share_time() {
    let clock = ManualClock::new();
    let t0 = clock.now();
    assert_eq!(clock.now(), t0);
    let shared: Arc<dyn Clock> = Arc::new(clock.clone());
    clock.advance(Duration::from_millis(250));
    assert_eq!(shared.now() - t0, Duration::from_millis(250));
    assert_eq!(clock.elapsed(), Duration::from_millis(250));
    clock.advance(Duration::from_secs(30));
    assert_eq!(clock.now() - t0, Duration::from_millis(30_250));
}

#[test]
fn recording_sink_records_every_call_in_order() {
    let (mut sink, log) = RecordingFrameSink::new(PresentMode::Immediate);
    let frame = Nv12Frame::new(Nv12Planes::new(Size::new(2, 2), vec![0; 4], vec![0; 2]).unwrap());
    let r = Rect::new(0, 0, 2, 1);
    sink.reset(Size::new(1280, 800));
    sink.create_surface(1, Size::new(1280, 800));
    sink.map_surface_to_output(1, Point::new(0, 0));
    sink.blit_bgra(1, r, 8, &[1, 2, 3, 4, 5, 6, 7, 8]);
    sink.blit_nv12(1, &frame, &[r]);
    sink.solid_fill(1, Bgra::new(0, 0, 255, 255), &[r]);
    sink.surface_to_surface(1, 1, r, &[Point::new(4, 4)]);
    sink.surface_to_cache(1, r, 7);
    sink.cache_to_surface(7, 1, &[Point::new(8, 8)]);
    sink.evict_cache(7);
    sink.set_visible(false);
    sink.delete_surface(1);
    let calls = log.calls();
    assert_eq!(
        calls,
        vec![
            FrameSinkCall::Reset { output: Size::new(1280, 800) },
            FrameSinkCall::CreateSurface { id: 1, size: Size::new(1280, 800) },
            FrameSinkCall::MapSurfaceToOutput { id: 1, origin: Point::new(0, 0) },
            FrameSinkCall::BlitBgra { id: 1, rect: r, stride: 8, len: 8, hash: fnv1a64(&[1, 2, 3, 4, 5, 6, 7, 8]) },
            FrameSinkCall::BlitNv12 { id: 1, size: Size::new(2, 2), regions: vec![r] },
            FrameSinkCall::SolidFill { id: 1, color: Bgra::new(0, 0, 255, 255), rects: vec![r] },
            FrameSinkCall::SurfaceToSurface { src: 1, dst: 1, rect: r, dests: vec![Point::new(4, 4)] },
            FrameSinkCall::SurfaceToCache { id: 1, rect: r, slot: 7 },
            FrameSinkCall::CacheToSurface { slot: 7, id: 1, dests: vec![Point::new(8, 8)] },
            FrameSinkCall::EvictCache { slot: 7 },
            FrameSinkCall::SetVisible { visible: false },
            FrameSinkCall::DeleteSurface { id: 1 },
        ]
    );
    assert_eq!(log.take().len(), 12);
    assert!(log.calls().is_empty());
}

#[test]
fn immediate_mode_presents_inside_end_frame() {
    let (mut sink, log) = RecordingFrameSink::new(PresentMode::Immediate);
    let acks = Arc::new(AtomicU32::new(0));
    let a = Arc::clone(&acks);
    sink.end_frame(5, Box::new(move || {
        a.fetch_add(1, Ordering::SeqCst);
    }));
    assert_eq!(acks.load(Ordering::SeqCst), 1);
    assert_eq!(log.calls(), vec![FrameSinkCall::EndFrame { frame_id: 5 }]);
    assert!(log.pending_frames().is_empty());
}

#[test]
fn deferred_mode_presents_exactly_once_on_demand() {
    let (mut sink, log) = RecordingFrameSink::new(PresentMode::Deferred);
    let acks = Arc::new(AtomicU32::new(0));
    for id in [1, 2] {
        let a = Arc::clone(&acks);
        sink.end_frame(id, Box::new(move || {
            a.fetch_add(1, Ordering::SeqCst);
        }));
    }
    assert_eq!(acks.load(Ordering::SeqCst), 0);
    assert_eq!(log.pending_frames(), vec![1, 2]);
    assert_eq!(log.present_pending(), 2);
    assert_eq!(acks.load(Ordering::SeqCst), 2);
    assert_eq!(log.present_pending(), 0);
    assert_eq!(acks.load(Ordering::SeqCst), 2);
}

#[test]
fn fnv1a64_known_vectors() {
    assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
}
