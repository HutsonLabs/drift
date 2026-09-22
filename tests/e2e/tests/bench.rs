//! M9-1: the performance bench against the real GNOME 50 host.
//!
//! One headless session, driven with `anim.py` (full-screen motion, the worst damage case),
//! measured at both resolutions the plan names. It records
//!
//! | metric | plan M9-1 budget |
//! |---|---|
//! | `fps_1280x800` | 60 fps |
//! | `fps_2560x1600` | ≥ 55 fps |
//! | `decode_present_p95_ms` | < 8 ms |
//! | `input_to_wire_p99_ms` | < 2 ms |
//! | `rss_per_session_mb` | ≤ 300 MB |
//!
//! into the JSON report `cargo xtask e2e --bench` compares with `tests/e2e/bench-baseline.json`
//! (a regression of more than 10 % fails the run). The picture goes through the **real Metal
//! compositor**, like `render.rs`, so the measured latency is decode *and* present.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use drift_core::{ConnectMode, InputEvent, SessionState, Size, ViewGeometry};
use drift_e2e::bench::{BenchReport, peak, rss_mb};
use drift_e2e::{E2eSession, headless_session_user, host, init_logging, port, require};
use drift_render::{Compositor, Gpu, OffscreenTarget, RenderThread};
use drift_rdp::{SessionCommand, SessionEvent, SessionStats};
use drift_testkit::FrameLog;

const CONNECT: Duration = Duration::from_secs(40);
const RESIZE: Duration = Duration::from_secs(30);
/// How long each resolution is measured once the stream is running.
const MEASURE: Duration = Duration::from_secs(10);

/// Everything one measurement window produced.
#[derive(Debug, Default)]
struct Window {
    fps: Vec<f32>,
    frame_p95_ms: Vec<f32>,
    input_p99_ms: Vec<f32>,
}

impl Window {
    fn push(&mut self, s: &SessionStats) {
        self.fps.push(s.fps);
        self.frame_p95_ms.push(s.frame_latency_p95_ms);
        if s.input_to_wire_p99_ms > 0.0 {
            self.input_p99_ms.push(s.input_to_wire_p99_ms);
        }
    }
}

/// Collects statistics for `period` while moving the pointer, so input latency is measured
/// under the same load as the picture.
async fn measure(s: &mut E2eSession, period: Duration) -> Window {
    let deadline = tokio::time::Instant::now() + period;
    let mut window = Window::default();
    let mut x = 0_u16;
    while tokio::time::Instant::now() < deadline {
        x = (x + 13) % 1000;
        s.input(InputEvent::MouseMove { x: x + 100, y: 300 });
        for event in s.settle(Duration::from_millis(40)).await {
            if let SessionEvent::Stats(stats) = event {
                window.push(&stats);
            }
        }
    }
    window
}

/// The M9-1 numbers, measured on the live host and written to the bench report.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e --bench`"]
async fn e2e_bench_budgets() {
    init_logging();
    let mut report = BenchReport::open().expect(
        "the bench report path; run this test through `cargo xtask e2e --bench`",
    );
    let (rdp_user, pass) = (require("DRIFT_E2E_HL_USER"), require("DRIFT_E2E_HL_PASS"));
    let user = headless_session_user();
    host::ensure_unlocked_session(&user);
    host::kill_in_session(&user, "anim.py");

    let rss_before = rss_mb().expect("the resident set size before the session");
    let gpu = Gpu::system_default().expect("a Metal device");
    report.note("gpu", gpu.name());
    // 2560×1600 is the largest surface the session will ask for.
    let target = OffscreenTarget::new(&gpu, Size::new(2560, 1600)).expect("an offscreen target");
    let render = RenderThread::spawn("bench", move || Compositor::new(gpu, target)).expect("render thread");

    let mut profile = E2eSession::profile(ConnectMode::Headless, port("DRIFT_E2E_HL_PORT", 3392), &rdp_user);
    profile.display.adaptive = true;
    let mut s = E2eSession::start_with_sink(profile, &pass, Box::new(render.sink()), FrameLog::default());
    s.wait_state("Connected", CONNECT, |st| matches!(st, SessionState::Connected { .. })).await;

    host::spawn_in_session(&user, &format!("python3 /home/{user}/anim.py 60 full"));
    let _ = s.settle(Duration::from_secs(5)).await;

    for (points, backing, expected, key) in [
        (Size::new(1280.0, 800.0), 1.0, Size::new(1280, 800), "fps_1280x800"),
        (Size::new(1280.0, 800.0), 2.0, Size::new(2560, 1600), "fps_2560x1600"),
    ] {
        s.send(SessionCommand::Resize(ViewGeometry { points, backing_scale: backing }));
        s.wait_state("the measured desktop", RESIZE, move |st| {
            matches!(st, SessionState::Connected { desktop, .. }
                if desktop.width == expected.width && desktop.height == expected.height)
        })
        .await;
        // The first seconds after a ResetGraphics are a new surface and a keyframe: not the
        // steady state the budget is about.
        let _ = s.settle(Duration::from_secs(4)).await;

        let w = measure(&mut s, MEASURE).await;
        assert!(!w.fps.is_empty(), "the session reported statistics at {expected:?}");
        report.record(key, peak(&w.fps));
        report.note(&format!("{key}_samples"), format!("{:?}", w.fps));
        if key == "fps_2560x1600" {
            // Latency is reported once, from the heavier of the two resolutions.
            report.record("decode_present_p95_ms", f64::from(worst(&w.frame_p95_ms)));
            assert!(!w.input_p99_ms.is_empty(), "input latency was measured while the pointer moved");
            report.record("input_to_wire_p99_ms", f64::from(worst(&w.input_p99_ms)));
            report.note("decode_present_p95_samples", format!("{:?}", w.frame_p95_ms));
            report.note("input_to_wire_p99_samples", format!("{:?}", w.input_p99_ms));
        }
    }

    let rss_after = rss_mb().expect("the resident set size with one session");
    report.record("rss_per_session_mb", (rss_after - rss_before).max(0.0));
    report.note("rss_total_mb", format!("{rss_after:.1}"));
    report.note("host_load", host::try_ssh("cut -d' ' -f1-3 /proc/loadavg").unwrap_or_default());

    host::kill_in_session(&user, "anim.py");
    s.close().await;
    render.shutdown();
    report.write().expect("writing the bench report");
}

/// The worst per-second sample: the budget must hold for every second, not on average.
fn worst(samples: &[f32]) -> f32 {
    samples.iter().copied().fold(0.0_f32, f32::max)
}
