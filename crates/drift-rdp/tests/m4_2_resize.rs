//! M4-2 Red tests: the Display Control resize driver (plan §6 M4-2).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{Harness, PASS, USER, is_connected, options, profile, wait_log};
use drift_core::{
    ConnectMode, DesktopSize, DisplayControlCaps, DisplayPrefs, MonitorLayout, SessionState, Size,
    ViewGeometry, desired_layout,
};
use drift_input::ScaleMode;
use drift_rdp::resize::{RESIZE_DEBOUNCE, ResizeDriver, mode_has_display_control};
use drift_rdp::{SessionCapabilities, SessionCommand, SessionEvent, SessionSecrets};
use drift_testkit::{Channels, FakeServer, FrameSinkCall, LegScript, ManualClock, RecordedLayout, TestCert};

const WAIT: Duration = Duration::from_secs(20);
const CAPS: DisplayControlCaps = DisplayControlCaps {
    max_num_monitors: 1,
    max_monitor_area_factor_a: 8192,
    max_monitor_area_factor_b: 8192,
};

fn view(width: f64, height: f64, scale: f64) -> ViewGeometry {
    ViewGeometry { points: Size::new(width, height), backing_scale: scale }
}

fn layout(width: u32, height: u32, scale: u32) -> MonitorLayout {
    MonitorLayout {
        width,
        height,
        desktop_scale_factor: scale,
        device_scale_factor: drift_core::layout::device_scale_for(scale),
    }
}

// ---------------------------------------------------------------- pure driver

#[test]
fn only_desktop_sharing_lacks_display_control() {
    assert!(mode_has_display_control(ConnectMode::RemoteLogin));
    assert!(mode_has_display_control(ConnectMode::Headless));
    assert!(!mode_has_display_control(ConnectMode::DesktopSharing), "plan §1.2: no DISP channel");
}

#[test]
fn a_burst_of_geometry_changes_sends_one_layout() {
    let t0 = Instant::now();
    let mut d = ResizeDriver::new(ConnectMode::Headless, DisplayPrefs::default());
    d.on_leg_activated(layout(1280, 800, 100));
    d.on_display_control_ready(CAPS, t0);
    assert_eq!(d.poll(t0), None, "nothing to send before the view reports its geometry");

    for (i, size) in [(1600.0, 1000.0), (1500.0, 950.0), (1440.0, 900.0)].into_iter().enumerate() {
        d.on_geometry(view(size.0, size.1, 1.0), t0 + Duration::from_millis(50 * i as u64));
    }
    let last = t0 + Duration::from_millis(100);
    assert_eq!(d.deadline(), Some(last + RESIZE_DEBOUNCE));
    assert_eq!(d.poll(last + Duration::from_millis(249)), None, "still inside the debounce window");
    assert_eq!(d.poll(last + RESIZE_DEBOUNCE), Some(layout(1440, 900, 100)), "one layout for the burst");
    assert_eq!(d.poll(last + Duration::from_secs(5)), None, "nothing more to send");
}

#[test]
fn an_unchanged_layout_sends_nothing() {
    let t0 = Instant::now();
    let mut d = ResizeDriver::new(ConnectMode::Headless, DisplayPrefs::default());
    d.on_leg_activated(layout(2560, 1600, 200));
    d.on_display_control_ready(CAPS, t0);
    // The same geometry the server already has (Retina: points × 2, scale 200).
    d.on_geometry(view(1280.0, 800.0, 2.0), t0);
    assert_eq!(d.poll(t0 + RESIZE_DEBOUNCE), None);
    // A genuine change is still sent.
    d.on_geometry(view(1280.0, 900.0, 2.0), t0 + Duration::from_secs(1));
    assert_eq!(d.poll(t0 + Duration::from_secs(2)), Some(layout(2560, 1800, 200)));
}

#[test]
fn changes_are_held_until_display_control_is_ready() {
    let t0 = Instant::now();
    let mut d = ResizeDriver::new(ConnectMode::Headless, DisplayPrefs::default());
    d.on_leg_activated(layout(1280, 800, 100));
    d.on_geometry(view(1600.0, 1000.0, 1.0), t0);
    assert_eq!(d.poll(t0 + Duration::from_secs(5)), None, "no DISP capabilities yet");
    let t1 = t0 + Duration::from_secs(5);
    d.on_display_control_ready(CAPS, t1);
    assert_eq!(d.poll(t1), Some(layout(1600, 1000, 100)), "caught up as soon as the channel is ready");
}

#[test]
fn desktop_sharing_and_fixed_profiles_never_resize() {
    let t0 = Instant::now();
    let mut sharing = ResizeDriver::new(ConnectMode::DesktopSharing, DisplayPrefs::default());
    sharing.on_leg_activated(layout(1920, 1080, 100));
    sharing.on_geometry(view(1600.0, 1000.0, 1.0), t0);
    assert_eq!(sharing.deadline(), None, "no timer at all: Desktop Sharing scales to fit");
    assert_eq!(sharing.poll(t0 + Duration::from_secs(5)), None);
    assert_eq!(sharing.connect_layout(), None);

    let mut fixed = ResizeDriver::new(ConnectMode::Headless, DisplayPrefs { adaptive: false, retina: true });
    fixed.on_leg_activated(layout(1280, 800, 100));
    fixed.on_display_control_ready(CAPS, t0);
    fixed.on_geometry(view(1600.0, 1000.0, 1.0), t0);
    assert_eq!(fixed.poll(t0 + Duration::from_secs(5)), None, "a non-adaptive profile keeps its size");
}

#[test]
fn the_connect_layout_follows_the_view_geometry() {
    let t0 = Instant::now();
    let mut d = ResizeDriver::new(ConnectMode::Headless, DisplayPrefs::default());
    assert_eq!(d.connect_layout(), None, "no geometry yet: the caller uses its default");
    d.on_geometry(view(1280.0, 800.0, 2.0), t0);
    assert_eq!(
        d.connect_layout(),
        Some(desired_layout(view(1280.0, 800.0, 2.0), DisplayPrefs::default(), &CAPS)),
        "Retina: 2560×1600 at DesktopScaleFactor 200"
    );
    assert_eq!(d.connect_layout(), Some(layout(2560, 1600, 200)));
}

// ---------------------------------------------------------------- loopback

fn headless_leg(cert: &TestCert) -> LegScript {
    LegScript::nla(cert.clone(), USER, PASS).with_channels(Channels::all())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resize_burst_sends_one_layout_and_the_reset_updates_the_desktop() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![headless_leg(&cert)]).await.unwrap();
    let clock = ManualClock::new();
    let mut h = Harness::start_full(
        profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())),
        SessionSecrets::new(PASS),
        Arc::new(clock.clone()),
        options(),
    );
    h.wait_for("Connected", WAIT, is_connected).await;
    assert_eq!(
        h.wait_for("Capabilities", WAIT, |e| matches!(e, SessionEvent::Capabilities(_))).await,
        SessionEvent::Capabilities(SessionCapabilities {
            display_control: true,
            clipboard: true,
            scale_mode: ScaleMode::Fit,
        })
    );
    // A resize burst: three geometries within the debounce window.
    for (i, size) in [(1600.0, 1000.0), (1500.0, 950.0), (1440.0, 900.0)].into_iter().enumerate() {
        h.handle.send(SessionCommand::Resize(view(size.0, size.1, 1.0))).unwrap();
        clock.advance(Duration::from_millis(50));
        let _ = i;
    }
    let _ = h.drain_for(Duration::from_millis(200)).await;
    assert!(server.log().legs[0].monitor_layouts.is_empty(), "still debouncing");

    clock.advance(RESIZE_DEBOUNCE);
    let log = wait_log(&server, "one monitor layout", WAIT, |l| !l.legs[0].monitor_layouts.is_empty()).await;
    assert_eq!(
        log.legs[0].monitor_layouts,
        vec![RecordedLayout { width: 1440, height: 900, desktop_scale: 100 }],
        "exactly one layout for the burst"
    );

    // g-r-d answers every layout with ResetGraphics plus a new surface id (plan §1.4).
    let connected = h
        .wait_for("resized desktop", WAIT, |e| {
            matches!(
                e,
                SessionEvent::State(SessionState::Connected {
                    desktop: DesktopSize { width: 1440, height: 900 },
                    ..
                })
            )
        })
        .await;
    assert_eq!(
        connected,
        SessionEvent::State(SessionState::Connected {
            desktop: DesktopSize { width: 1440, height: 900 },
            scale: 100,
        })
    );
    let calls = h.frames.calls();
    assert!(
        calls.iter().any(|c| matches!(c, FrameSinkCall::Reset { output } if *output == Size::new(1440, 900))),
        "the compositor was reset to the new size: {calls:?}"
    );
    assert!(
        calls.iter().filter(|c| matches!(c, FrameSinkCall::CreateSurface { .. })).count() >= 2,
        "a new surface id after the reset: {calls:?}"
    );

    // The same geometry again changes nothing.
    h.handle.send(SessionCommand::Resize(view(1440.0, 900.0, 1.0))).unwrap();
    clock.advance(Duration::from_secs(1));
    let _ = h.drain_for(Duration::from_millis(400)).await;
    assert_eq!(server.log().legs[0].monitor_layouts.len(), 1, "an unchanged layout is not sent");
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_sharing_scales_to_fit_without_guessing() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![
        LegScript::nla(cert.clone(), USER, PASS).with_channels(Channels::desktop_sharing()),
    ])
    .await
    .unwrap();
    let clock = ManualClock::new();
    let mut h = Harness::start_full(
        profile(ConnectMode::DesktopSharing, server.port(), Some(cert.fingerprint())),
        SessionSecrets::new(PASS),
        Arc::new(clock.clone()),
        options(),
    );
    h.wait_for("Connected", WAIT, is_connected).await;
    let caps = h.wait_for("Capabilities", WAIT, |e| matches!(e, SessionEvent::Capabilities(_))).await;
    assert_eq!(
        caps,
        SessionEvent::Capabilities(SessionCapabilities {
            display_control: false,
            clipboard: true,
            scale_mode: ScaleMode::Fit,
        }),
        "no DISP channel in Desktop Sharing: Fit immediately, no timeout guessing"
    );
    h.handle.send(SessionCommand::Resize(view(1600.0, 1000.0, 1.0))).unwrap();
    clock.advance(Duration::from_secs(2));
    let _ = h.drain_for(Duration::from_millis(400)).await;
    assert!(server.log().legs[0].monitor_layouts.is_empty(), "no Display Control traffic");
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_first_leg_connects_with_the_view_geometry() {
    let cert = TestCert::generate("127.0.0.1");
    let server = FakeServer::start(vec![headless_leg(&cert)]).await.unwrap();
    let clock = ManualClock::new();
    let mut h = Harness::start_full(
        profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())),
        SessionSecrets::new(PASS),
        Arc::new(clock.clone()),
        options(),
    );
    // The app reports the geometry as soon as the tab exists, before the connection is up.
    h.handle.send(SessionCommand::Resize(view(1280.0, 800.0, 2.0))).unwrap();
    h.wait_for("Connected", WAIT, is_connected).await;
    let log = server.log();
    assert_eq!(log.legs[0].client_desktop, Some((2560, 1600)), "Retina geometry at connect time");
    assert_eq!(log.legs[0].client_desktop_scale, Some(200));
    h.close().await;
}
