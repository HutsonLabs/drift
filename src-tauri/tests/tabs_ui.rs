//! M6-2 Red: real Tauri session windows join one native tab group.
//!
//! The spike's approach on the real app: launch Drift (temporary config, in-memory secrets),
//! open two more tabs through the same code path as Cmd+T, and assert
//! `tabbedWindows.count == 3`; then send `newWindowForTab:` to a window and assert a fourth tab
//! appears.
//!
//! UI-tabs: AppKit's own tab bar is hidden (only the title bar row is left), every window has
//! a strip webview whose model lists the group's tabs in order, the traffic lights sit centred
//! in the 46-point strip, and selecting a tab through the strip's path selects that window.
//!
//! A Tauri event loop must own the process main thread, so this is a `harness = false` binary
//! that runs one test and exits. It needs a logged-in window server:
//! `cargo test -p drift-app --features macos-ui-tests --test tabs_ui`.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used)]

use std::sync::mpsc;
use std::time::{Duration, Instant};

use drift_app::RunOptions;
use drift_app::strip::{CONNECTIONS_TITLE, STRIP_HEIGHT, TabKind, strip_label};
use drift_app::windows::{open_tab, select_tab, tab_count, tab_strip, window_label};
use objc2::runtime::AnyObject;
use objc2::{msg_send, sel};
use objc2_app_kit::{NSWindow, NSWindowButton};
use tauri::{AppHandle, Manager};

const NAME: &str = "session_windows_join_one_native_tab_group";

fn on_main<T: Send + 'static>(app: &AppHandle, f: impl FnOnce(&AppHandle) -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    let a = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(f(&a));
    })
    .unwrap();
    rx.recv_timeout(Duration::from_secs(10)).expect("main thread responds")
}

fn wait_for(app: &AppHandle, what: &str, want: usize) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let n = on_main(app, |a| tab_count(a, &window_label(0)));
        if n == Some(want) {
            println!("{what}: tabbedWindows.count == {want}");
            return;
        }
        assert!(Instant::now() < deadline, "{what}: tab count {n:?}, want {want}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The session window `label`'s `NSWindow` (main thread).
fn ns_window(app: &AppHandle, label: &str) -> objc2::rc::Retained<NSWindow> {
    use objc2::Message as _;
    let w = app.get_webview(label).unwrap().window();
    let ptr = w.ns_window().unwrap().cast::<NSWindow>();
    // SAFETY: tao's live NSWindow; we are on the main thread and retain it.
    unsafe { ptr.as_ref() }.unwrap().retain()
}

/// Waits until every tab's strip lists `want` tabs.
fn wait_for_strips(app: &AppHandle, want: usize) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let counts: Vec<Option<usize>> = on_main(app, move |a| {
            (0..want as u64).map(|n| tab_strip(a, &window_label(n)).map(|s| s.tabs.len())).collect()
        });
        if counts.iter().all(|c| *c == Some(want)) {
            return;
        }
        assert!(Instant::now() < deadline, "strips: {counts:?}, want {want} tabs each");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn check_strips_and_title_bar(app: &AppHandle) {
    wait_for_strips(app, 3);
    on_main(app, |a| {
        let labels: Vec<String> = (0..3).map(window_label).collect();
        for label in &labels {
            let ns = ns_window(a, label);
            let content = ns.contentView().unwrap();
            let covered = content.bounds().size.height - ns.contentLayoutRect().size.height;
            assert!(covered < 40.0, "{label}: AppKit's tab bar is hidden, {covered} pt covered");
            assert!(a.get_webview(&strip_label(label)).is_some(), "{label} has a strip webview");
            let strip = tab_strip(a, label).unwrap();
            let ids: Vec<&str> = strip.tabs.iter().map(|t| t.id.as_str()).collect();
            assert_eq!(ids, labels.iter().map(String::as_str).collect::<Vec<_>>(), "{label}: group order");
            assert_eq!(strip.active, *label);
            assert!(strip.tabs.iter().all(|t| t.kind == TabKind::Manager && t.title == CONNECTIONS_TITLE));
        }
        // The traffic lights sit centred in the strip, beside the tabs (mockup: 18 pt / 17 pt).
        let key = ns_window(a, &labels[2]);
        let close = key.standardWindowButton(NSWindowButton::CloseButton).unwrap();
        let rect = close.convertRect_toView(close.bounds(), None);
        let height = key.contentView().unwrap().bounds().size.height;
        let centre = height - rect.origin.y - rect.size.height / 2.0;
        assert!((centre - STRIP_HEIGHT / 2.0).abs() <= 3.0, "close button centre {centre} pt from the top");
        assert!((rect.origin.x - 18.0).abs() <= 3.0, "close button at x {}", rect.origin.x);
    });
    // Clicking a tab in the strip selects that window of the group.
    on_main(app, |a| select_tab(a, &window_label(0)).unwrap());
    let selected = on_main(app, |a| {
        let ns = ns_window(a, &window_label(0));
        let group = ns.tabGroup().unwrap();
        let sel = group.selectedWindow().unwrap();
        std::ptr::eq(&*sel, &*ns)
    });
    assert!(selected, "select_tab selects the window in its tab group");
    println!("strip: 3 tabs in group order, native tab bar hidden, traffic lights in the strip");
}

fn scenario(app: AppHandle) {
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            open_tab(&app);
            open_tab(&app);
            wait_for(&app, "Cmd+T ×2", 3);
            check_strips_and_title_bar(&app);
            on_main(&app, |a| {
                let w = a.get_webview(&window_label(0)).unwrap().window();
                let ns = w.ns_window().unwrap().cast::<AnyObject>();
                // SAFETY: tao's live NSWindow on the main thread; `newWindowForTab:` was installed
                // on its class by Drift (the tab bar's "+" sends exactly this message).
                unsafe {
                    let responds: bool = msg_send![&*ns, respondsToSelector: sel!(newWindowForTab:)];
                    assert!(responds, "newWindowForTab: installed on TaoWindow");
                    let _: () = msg_send![&*ns, newWindowForTab: std::ptr::null::<AnyObject>()];
                }
            });
            wait_for(&app, "newWindowForTab:", 4);
        }));
        let ok = result.is_ok();
        println!("test {NAME} ... {}", if ok { "ok" } else { "FAILED" });
        let (status, passed, failed) = if ok { ("ok", 1, 0) } else { ("FAILED", 0, 1) };
        println!("\ntest result: {status}. {passed} passed; {failed} failed; 0 ignored");
        std::process::exit(if ok { 0 } else { 1 });
    });
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
    // `..Default::default()` on purpose: a new `RunOptions` field must not break this test
    // binary, which only `cargo xtask ci`'s `--all-features` lint pass compiles.
    let options = RunOptions {
        config_dir: Some(dir.path().to_path_buf()),
        memory_secrets: true,
        on_ready: Some(Box::new(scenario)),
        ..Default::default()
    };
    drift_app::run_with(options);
}
