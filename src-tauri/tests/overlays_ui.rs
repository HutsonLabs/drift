//! M7-3 / M1 Red: a real session window can draw over the live picture.
//!
//! The pure rules live in `present`; this checks what AppKit actually does with them on a real
//! Tauri window:
//!
//! * the window is **not opaque** (`transparent(true)` plus Tauri's `macos-private-api`), so a
//!   transparent page shows the `RemoteView`'s last frame through it,
//! * the reconnect overlay leaves the web view filling the window and focused,
//! * the statistics HUD shrinks the web view to `present::hud_frame` and hands first responder
//!   back to the `RemoteView`, so everything outside that corner still goes to the remote
//!   desktop,
//! * the live screen hides the web view again.
//!
//! Like `tabs_ui` this needs the process main thread and a logged-in window server:
//! `cargo test -p drift-app --features macos-ui-tests --test overlays_ui`.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used)]

use std::sync::mpsc;
use std::time::Duration;

use drift_app::RunOptions;
use drift_app::present::{self, Hud};
use drift_app::view::{Screen, SessionView, StatsView};
use drift_app::windows::{apply_view_for_tests, window_label};
use drift_core::{ConnectMode, ConnectionProfile, DesktopSize, SessionState, Size};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{Message as _, msg_send};
use objc2_app_kit::{NSView, NSWindow};
use tauri::{AppHandle, Manager};

const NAME: &str = "overlays_are_drawn_over_the_live_picture";

fn on_main<T: Send + 'static>(app: &AppHandle, f: impl FnOnce(&AppHandle) -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    let a = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(f(&a));
    })
    .unwrap();
    rx.recv_timeout(Duration::from_secs(10)).expect("main thread responds")
}

/// `(content view, WKWebView, NSWindow)` of the first session window (main thread).
fn views(app: &AppHandle) -> (Retained<NSView>, Retained<NSView>, Retained<NSWindow>) {
    let w = app.get_webview_window(&window_label(0)).unwrap();
    let ptr = w.ns_view().unwrap().cast::<NSView>();
    // SAFETY: tao's live content `NSView*`; we are on the main thread and retain it.
    let content = unsafe { ptr.as_ref() }.unwrap().retain();
    let web = drift_macos::webview::find_webview(&content).expect("a WKWebView");
    let ptr = w.ns_window().unwrap().cast::<NSWindow>();
    // SAFETY: tao's live `NSWindow*`; we are on the main thread and retain it.
    let window = unsafe { ptr.as_ref() }.unwrap().retain();
    (content, web, window)
}

/// The class name of the window's first responder ("" when there is none).
fn first_responder(window: &NSWindow) -> String {
    let Some(responder) = window.firstResponder() else { return String::new() };
    let object: &AnyObject = responder.as_ref();
    // SAFETY: every object answers `class`; we are on the main thread.
    let class = unsafe {
        let c: &objc2::runtime::AnyClass = msg_send![object, class];
        c
    };
    class.name().to_string_lossy().into_owned()
}

fn view_for(screen: Screen) -> SessionView {
    let mut v = SessionView::new(&ConnectionProfile::new("Homelab", "10.1.2.40", ConnectMode::Headless));
    v.state = SessionState::Connected { desktop: DesktopSize::new(1280, 800), scale: 100 };
    v.screen = screen;
    v
}

fn scenario(app: AppHandle) {
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // 1. The window is non-opaque, so a transparent page shows the picture behind it.
            let opaque = on_main(&app, |a| views(a).2.isOpaque());
            assert!(!opaque, "the session window must not be opaque (M7-3 overlays)");

            // 2. Reconnect overlay: the web view fills the window and takes the keyboard.
            let view = view_for(Screen::Reconnecting);
            on_main(&app, {
                let view = view.clone();
                move |a| apply_view_for_tests(a, &window_label(0), &view)
            });
            let (hidden, frame, bounds, responder) = on_main(&app, |a| {
                let (content, web, window) = views(a);
                (web.isHidden(), web.frame(), content.bounds(), first_responder(&window))
            });
            assert!(!hidden, "the overlay is the web view; it must be visible");
            assert_eq!(
                (frame.size.width, frame.size.height),
                (bounds.size.width, bounds.size.height),
                "the overlay fills the window"
            );
            assert!(responder.contains("WebView"), "the overlay owns the keyboard, got {responder}");

            // 3. Statistics HUD: a corner panel, with the RemoteView back as first responder.
            let mut view = view_for(Screen::Live);
            view.show_stats = true;
            view.stats = Some(StatsView { fps_tenths: 589, ..Default::default() });
            on_main(&app, {
                let view = view.clone();
                move |a| apply_view_for_tests(a, &window_label(0), &view)
            });
            let (hidden, frame, bounds, responder) = on_main(&app, |a| {
                let (content, web, window) = views(a);
                (web.isHidden(), web.frame(), content.bounds(), first_responder(&window))
            });
            let want = present::hud_frame(
                Size::new(bounds.size.width, bounds.size.height),
                Hud::Stats,
                content_is_flipped(&app),
            );
            assert!(!hidden, "the HUD is the web view; it must be visible");
            assert_eq!((frame.size.width, frame.size.height), (want.width, want.height), "HUD size");
            assert!((frame.origin.x - want.x).abs() < 0.5, "HUD x: {frame:?} want {want:?}");
            assert!((frame.origin.y - want.y).abs() < 0.5, "HUD y: {frame:?} want {want:?}");
            assert!(frame.size.width < bounds.size.width / 2.0, "the HUD is a corner panel");
            assert_eq!(responder, "DriftRemoteView", "the picture keeps the keyboard under a HUD");

            // 4. Plain live picture: the web view is hidden again.
            on_main(&app, move |a| apply_view_for_tests(a, &window_label(0), &view_for(Screen::Live)));
            let hidden = on_main(&app, |a| views(a).1.isHidden());
            assert!(hidden, "no overlay: the web view is hidden");
        }));
        let ok = result.is_ok();
        println!("test {NAME} ... {}", if ok { "ok" } else { "FAILED" });
        let (status, passed, failed) = if ok { ("ok", 1, 0) } else { ("FAILED", 0, 1) };
        println!("\ntest result: {status}. {passed} passed; {failed} failed; 0 ignored");
        std::process::exit(if ok { 0 } else { 1 });
    });
}

/// Whether the web view's superview uses a flipped coordinate space.
fn content_is_flipped(app: &AppHandle) -> bool {
    on_main(app, |a| {
        let (_, web, _) = views(a);
        // SAFETY: `superview` returns the parent or nil; we are on the main thread.
        unsafe { web.superview() }.is_some_and(|parent| parent.isFlipped())
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--list") {
        if !args.iter().any(|a| a == "--ignored") {
            println!("{NAME}: test");
        }
        return;
    }
    let filters: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    if !filters.is_empty() && !filters.iter().any(|f| NAME.contains(f.as_str())) {
        println!("\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored");
        return;
    }
    println!("\nrunning 1 test");
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(60));
        println!("test {NAME} ... FAILED (timeout)");
        std::process::exit(2);
    });
    let dir = tempfile::tempdir().unwrap();
    let options = RunOptions {
        config_dir: Some(dir.path().to_path_buf()),
        memory_secrets: true,
        secrets: None,
        autoconnect: None,
        on_ready: Some(Box::new(scenario)),
    };
    drift_app::run_with(options);
}
