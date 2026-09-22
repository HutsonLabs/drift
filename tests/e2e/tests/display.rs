//! M4-2 / M6-3 e2e: adaptive resize and background throttling against the real GNOME host.
//! Run through `cargo xtask e2e`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use drift_core::{ConnectMode, DesktopSize, SessionState, Size, ViewGeometry};
use drift_e2e::{E2eSession, host, init_logging, port, require};
use drift_rdp::SessionCommand;
use drift_testkit::FrameSinkCall;

const CONNECT: Duration = Duration::from_secs(40);
const RESIZE: Duration = Duration::from_secs(30);

fn view(width: f64, height: f64, backing_scale: f64) -> ViewGeometry {
    ViewGeometry { points: Size::new(width, height), backing_scale }
}

async fn headless() -> E2eSession {
    host::ensure_unlocked_session(&drift_e2e::headless_session_user());
    let (rdp_user, pass) = (require("DRIFT_E2E_HL_USER"), require("DRIFT_E2E_HL_PASS"));
    let mut s = E2eSession::start(ConnectMode::Headless, port("DRIFT_E2E_HL_PORT", 3392), &rdp_user, &pass);
    s.wait_state("Connected", CONNECT, |st| matches!(st, SessionState::Connected { .. })).await;
    s
}

/// Each view geometry produces the matching remote desktop (plan §1.4: even width, scale 200
/// on Retina) and a `ResetGraphics` of that size.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_resize() {
    init_logging();
    let mut s = headless().await;

    for (geometry, expected, scale) in [
        (view(1600.0, 1000.0, 1.0), DesktopSize { width: 1600, height: 1000 }, 100),
        (view(1280.0, 800.0, 2.0), DesktopSize { width: 2560, height: 1600 }, 200),
        // An odd width is rounded down by g-r-d (1281 → 1280); an odd height is accepted.
        (view(1281.0, 801.0, 1.0), DesktopSize { width: 1280, height: 801 }, 100),
    ] {
        s.send(SessionCommand::Resize(geometry));
        let state = s
            .wait_state(
                "the resized desktop",
                RESIZE,
                move |st| matches!(st, SessionState::Connected { desktop, .. } if *desktop == expected),
            )
            .await;
        assert_eq!(state, SessionState::Connected { desktop: expected, scale });
        let output = Size::new(expected.width, expected.height);
        assert!(
            s.frames.calls().iter().any(|c| matches!(c, FrameSinkCall::Reset { output: o } if *o == output)),
            "ResetGraphics {output:?} reached the compositor"
        );
    }
    s.close().await;
}

/// While the tab is hidden no frame arrives; after allowing output again the server sends a
/// full frame of the changed desktop (plan §1.4: Refresh Rect is not supported).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_suppress_resume() {
    init_logging();
    let user = drift_e2e::headless_session_user();
    let mut s = headless().await;
    // Full-screen motion, so a visible session keeps receiving frames.
    host::kill_in_session(&user, "anim.py");
    host::spawn_in_session(&user, &format!("python3 /home/{user}/anim.py 60 full"));
    let _ = s.settle(Duration::from_secs(3)).await;
    let moving = s.presented_frames();
    assert!(moving > 0, "the animation produces frames while visible");

    s.send(SessionCommand::SetVisible(false));
    let _ = s.settle(Duration::from_secs(2)).await;
    let suppressed_at = s.presented_frames();
    let _ = s.settle(Duration::from_secs(3)).await;
    assert_eq!(s.presented_frames(), suppressed_at, "zero frames while Suppress Output is active");

    s.send(SessionCommand::SetVisible(true));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while s.presented_frames() == suppressed_at && tokio::time::Instant::now() < deadline {
        let _ = s.settle(Duration::from_millis(250)).await;
    }
    assert!(
        s.presented_frames() > suppressed_at,
        "the server sends a full frame of the current desktop after allow"
    );
    host::kill_in_session(&user, "anim.py");
    s.close().await;
}
