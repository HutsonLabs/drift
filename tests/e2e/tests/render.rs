//! Real-host e2e for the **GPU path**: the session actor feeding the real Metal compositor
//! (`drift-render`) instead of a recording sink, so decode → NV12 → BT.709 shader → composite
//! is exercised against the live GNOME 50 desktop.
//!
//! Covers plan M1-6 "Done (manual M1)" (a live, correctly coloured desktop; `anim.py` at
//! ≥ 55 fps) and the M9-1 frame-rate budget at 1280×800. Run through `cargo xtask e2e`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use drift_core::{ConnectMode, SessionState, Size};
use drift_e2e::{E2eSession, headless_session_user, host, init_logging, port, require};
use drift_rdp::SessionEvent;
use drift_render::{Compositor, Gpu, OffscreenTarget, RenderThread};
use drift_testkit::FrameLog;
use drift_testkit::golden::{GoldenImage, save_png};

/// The headless session's desktop (plan §1.1/§1.4).
const DESKTOP: Size<u32> = Size { width: 1280, height: 800 };
/// Plan M1-6 "Done (manual M1)" and M9-1: ≥ 55 fps at 1280×800 with `anim.py`.
const MIN_FPS: f32 = 55.0;

/// Where the read-back composites land, for a human to look at.
fn artefact_dir() -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/e2e");
    std::fs::create_dir_all(&dir).expect("creating the artefact directory");
    dir
}

fn to_golden(image: &drift_render::BgraImage) -> GoldenImage {
    GoldenImage { width: image.size.width, height: image.size.height, bgra: image.data.clone() }
}

/// Fraction of pixels that are neither black nor white — a live desktop always has some.
fn coloured_fraction(image: &drift_render::BgraImage) -> f64 {
    let coloured = image
        .data
        .chunks_exact(4)
        .filter(|p| {
            let (b, g, r) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
            let max = b.max(g).max(r);
            let min = b.min(g).min(r);
            max - min > 12 || (16..240).contains(&max)
        })
        .count();
    coloured as f64 / (image.data.len() / 4) as f64
}

/// The live desktop reaches the real compositor in colour, and `anim.py` sustains ≥ 55 fps.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_live_desktop_pixels() {
    init_logging();
    let (rdp_user, pass) = (require("DRIFT_E2E_HL_USER"), require("DRIFT_E2E_HL_PASS"));
    let user = headless_session_user();
    host::ensure_unlocked_session(&user);
    host::kill_in_session(&user, "anim.py");

    // The production wiring (src-tauri/src/windows.rs): one Gpu, a Compositor on its own
    // render thread, the session actor holding a RenderSink handle.
    let gpu = Gpu::system_default().expect("a Metal device");
    eprintln!("[e2e] GPU {}", gpu.name());
    let target = OffscreenTarget::new(&gpu, DESKTOP).expect("an offscreen target");
    let render = RenderThread::spawn("e2e", move || Compositor::new(gpu, target)).expect("render thread");

    let profile = E2eSession::profile(ConnectMode::Headless, port("DRIFT_E2E_HL_PORT", 3392), &rdp_user);
    let mut s = E2eSession::start_with_sink(profile, &pass, Box::new(render.sink()), FrameLog::default());
    s.wait_state("Connected", Duration::from_secs(40), |st| matches!(st, SessionState::Connected { .. }))
        .await;

    // A still desktop first: whatever the session shows must arrive in colour.
    let _ = s.settle(Duration::from_secs(3)).await;
    let still = render.with(Compositor::read_target).expect("the render thread is alive");
    let still_path = artefact_dir().join("live-desktop-still.png");
    save_png(&still_path, &to_golden(&still)).expect("writing the still composite");
    eprintln!("[e2e] wrote {}", still_path.display());
    assert_eq!(still.size, DESKTOP, "the composite has the desktop size");
    let coloured = coloured_fraction(&still);
    assert!(coloured > 0.05, "the composite is not a blank frame ({:.1}% non-flat pixels)", coloured * 100.0);

    // Full-screen animation: the worst-case damage case the frame-rate budget is stated for.
    host::spawn_in_session(&user, &format!("python3 /home/{user}/anim.py 30 full"));
    let _ = s.settle(Duration::from_secs(6)).await;

    let samples: Vec<f32> = s
        .settle(Duration::from_secs(10))
        .await
        .into_iter()
        .filter_map(|e| match e {
            SessionEvent::Stats(stats) => Some(stats.fps),
            _ => None,
        })
        .collect();
    host::kill_in_session(&user, "anim.py");

    let moving = render.with(Compositor::read_target).expect("the render thread is alive");
    let moving_path = artefact_dir().join("live-desktop-anim.png");
    save_png(&moving_path, &to_golden(&moving)).expect("writing the animated composite");
    eprintln!("[e2e] wrote {}", moving_path.display());
    assert!(coloured_fraction(&moving) > 0.05, "the animated composite is not blank");
    assert_ne!(moving.data, still.data, "the picture changed while anim.py ran");

    assert!(!samples.is_empty(), "the session reported frame statistics");
    let best = samples.iter().copied().fold(0.0_f32, f32::max);
    eprintln!("[e2e] fps samples {samples:?} (peak {best:.1})");
    assert!(best >= MIN_FPS, "anim.py sustained >= {MIN_FPS} fps (peak {best:.1}, samples {samples:?})");

    s.close().await;
    render.shutdown();
}
