//! M6-2 window-group integration test (the spike's approach: open 3 windows, assert
//! `tabbedWindows.count == 3`), plus `newWindowForTab:` installation on the real window class
//! even when key-value observing has isa-swizzled the window.
//!
//! Needs a window server with window tabbing, so it only runs with
//! `cargo test -p drift-macos --features macos-ui-tests` (plan M6-2). Without the feature the
//! binary lists no tests.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used)]

#[path = "support/harness.rs"]
mod harness;

use std::process::ExitCode;

#[cfg(feature = "macos-ui-tests")]
fn main() -> ExitCode {
    harness::run(&[
        ("three_windows_form_one_tab_group", ui::three_windows_form_one_tab_group),
        (
            "new_window_for_tab_is_installed_on_the_real_class",
            ui::new_window_for_tab_is_installed_on_the_real_class,
        ),
    ])
}

#[cfg(not(feature = "macos-ui-tests"))]
fn main() -> ExitCode {
    harness::run(&[])
}

#[cfg(feature = "macos-ui-tests")]
mod ui {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSWindow, NSWindowStyleMask,
        NSWindowTabbingMode,
    };
    use objc2_foundation::{
        NSKeyValueObservingOptions, NSObjectNSKeyValueObserverRegistration, NSPoint, NSRect, NSSize, NSString,
    };

    use drift_macos::tabs::{
        TABBING_IDENTIFIER, add_tab, install_new_window_for_tab, prepare_for_tabs, select_tab, tab_count,
    };

    define_class!(
        // Stands in for tao's `TaoWindow` (an NSWindow subclass without `newWindowForTab:`).
        #[unsafe(super(NSWindow))]
        #[thread_kind = MainThreadOnly]
        #[name = "DriftTestTaoWindow"]
        struct TestTaoWindow;
    );

    fn mtm() -> MainThreadMarker {
        let mtm = MainThreadMarker::new().expect("main thread");
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        mtm
    }

    fn tao_window(mtm: MainThreadMarker, title: &str) -> Retained<NSWindow> {
        let rect = NSRect::new(NSPoint::new(100.0, 100.0), NSSize::new(320.0, 200.0));
        // SAFETY: NSWindow's designated initializer on our subclass.
        let win: Retained<TestTaoWindow> = unsafe {
            msg_send![
                TestTaoWindow::alloc(mtm),
                initWithContentRect: rect,
                styleMask: NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
                backing: NSBackingStoreType::Buffered,
                defer: false
            ]
        };
        let win: Retained<NSWindow> = Retained::into_super(win);
        // SAFETY: we keep the Retained.
        unsafe { win.setReleasedWhenClosed(false) };
        win.setTitle(&NSString::from_str(title));
        win
    }

    pub fn three_windows_form_one_tab_group() {
        let mtm = mtm();
        // Created hidden, joined to the group, then shown (M6-2).
        let windows: Vec<_> = (0..3).map(|i| tao_window(mtm, &format!("Drift test tab {i}"))).collect();
        for w in &windows {
            prepare_for_tabs(w);
            assert_eq!(w.tabbingMode(), NSWindowTabbingMode::Preferred);
            assert_eq!(w.tabbingIdentifier().to_string(), TABBING_IDENTIFIER);
        }
        windows[0].orderFront(None);
        for w in &windows[1..] {
            add_tab(&windows[0], w);
        }
        for (i, w) in windows.iter().enumerate() {
            assert_eq!(tab_count(w), 3, "window {i}");
        }
        assert!(select_tab(&windows[0], 2));
        assert!(!select_tab(&windows[0], 3), "out of range");
        let group = windows[0].tabGroup().unwrap();
        let selected = group.selectedWindow().unwrap();
        assert_eq!(Retained::as_ptr(&selected), Retained::as_ptr(&group.windows().objectAtIndex(2)));
        for w in &windows {
            w.close();
        }
    }

    static NEW_TAB_CALLS: AtomicUsize = AtomicUsize::new(0);

    pub fn new_window_for_tab_is_installed_on_the_real_class() {
        let mtm = mtm();
        let win = tao_window(mtm, "Drift test KVO");
        // Key-value observing isa-swizzles the window into an NSKVONotifying_ subclass, as tao's
        // windows are at runtime.
        let observer = NSObject::new();
        let key = NSString::from_str("title");
        // SAFETY: the observer outlives the observation, which is removed below.
        unsafe {
            win.addObserver_forKeyPath_options_context(
                &observer,
                &key,
                NSKeyValueObservingOptions::New,
                std::ptr::null_mut(),
            )
        };
        let isa = win.class().name().to_string_lossy().into_owned();
        assert!(isa.starts_with("NSKVONotifying_"), "expected a KVO subclass, got {isa}");

        install_new_window_for_tab(&win, || {
            NEW_TAB_CALLS.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
        // Idempotent.
        install_new_window_for_tab(&win, || {}).unwrap();

        let real = objc2::runtime::AnyClass::get(c"DriftTestTaoWindow").unwrap();
        assert!(real.instance_method(sel!(newWindowForTab:)).is_some(), "added to the real class");

        // SAFETY: `tryToPerform:with:` dispatches `newWindowForTab:` with a nil sender.
        let performed = unsafe { win.tryToPerform_with(sel!(newWindowForTab:), None::<&AnyObject>) };
        assert!(performed);
        assert_eq!(NEW_TAB_CALLS.load(Ordering::SeqCst), 1);

        // SAFETY: removing the observation added above.
        unsafe { win.removeObserver_forKeyPath(&observer, &key) };
        // Another instance of the class responds too.
        let other = tao_window(mtm, "Drift test plain");
        // SAFETY: as above.
        assert!(unsafe { other.tryToPerform_with(sel!(newWindowForTab:), None::<&AnyObject>) });
        assert_eq!(NEW_TAB_CALLS.load(Ordering::SeqCst), 2);
    }
}
