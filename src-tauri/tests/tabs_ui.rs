//! M6-2 Red: real Tauri session windows join one native tab group.
//!
//! The spike's approach on the real app: launch Drift (temporary config, in-memory secrets),
//! open two more tabs through the same code path as Cmd+T, and assert
//! `tabbedWindows.count == 3`; then send `newWindowForTab:` (the tab bar's "+") to a window and
//! assert a fourth tab appears.
//!
//! A Tauri event loop must own the process main thread, so this is a `harness = false` binary
//! that runs one test and exits. It needs a logged-in window server:
//! `cargo test -p drift-app --features macos-ui-tests --test tabs_ui`.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used)]

use std::sync::mpsc;
use std::time::{Duration, Instant};

use drift_app::RunOptions;
use drift_app::windows::{open_tab, tab_count, window_label};
use objc2::runtime::AnyObject;
use objc2::{msg_send, sel};
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

fn scenario(app: AppHandle) {
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            open_tab(&app);
            open_tab(&app);
            wait_for(&app, "Cmd+T ×2", 3);
            on_main(&app, |a| {
                let w = a.get_webview_window(&window_label(0)).unwrap();
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
