//! UI-windows Red: AppKit pieces of the window model on a real window server — windows that
//! never tab, the Dock menu on the application delegate, and the confirmation sheet.
//!
//! Needs a logged-in window server, so it only runs with
//! `cargo test -p drift-macos --features macos-ui-tests --test windows_ui`. Without the feature
//! the binary lists no tests.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used)]

#[path = "support/harness.rs"]
mod harness;

use std::process::ExitCode;

#[cfg(feature = "macos-ui-tests")]
fn main() -> ExitCode {
    harness::run(&[
        ("session_windows_never_form_a_tab_group", ui::session_windows_never_form_a_tab_group),
        ("the_dock_menu_comes_from_the_provider", ui::the_dock_menu_comes_from_the_provider),
        ("a_confirmation_sheet_attaches_to_its_window", ui::a_confirmation_sheet_attaches_to_its_window),
    ])
}

#[cfg(not(feature = "macos-ui-tests"))]
fn main() -> ExitCode {
    harness::run(&[])
}

#[cfg(feature = "macos-ui-tests")]
mod ui {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use drift_macos::alert::{AlertText, confirm_sheet};
    use drift_macos::dock::{self, DockMenuItem};
    use drift_macos::window::disallow_tabbing;
    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, ProtocolObject};
    use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{
        NSAlertFirstButtonReturn, NSAlertSecondButtonReturn, NSApplication, NSApplicationActivationPolicy,
        NSApplicationDelegate, NSBackingStoreType, NSMenu, NSWindow, NSWindowStyleMask, NSWindowTabbingMode,
    };
    use objc2_foundation::{NSDate, NSObjectProtocol, NSPoint, NSRect, NSRunLoop, NSSize, NSString};

    define_class!(
        // Stands in for tao's application delegate (which has no `applicationDockMenu:`).
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "DriftTestAppDelegate"]
        struct TestDelegate;

        unsafe impl NSObjectProtocol for TestDelegate {}
        unsafe impl NSApplicationDelegate for TestDelegate {}
    );

    fn mtm() -> MainThreadMarker {
        let mtm = MainThreadMarker::new().expect("main thread");
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        mtm
    }

    fn window(mtm: MainThreadMarker, title: &str) -> Retained<NSWindow> {
        let rect = NSRect::new(NSPoint::new(120.0, 120.0), NSSize::new(480.0, 320.0));
        let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Resizable;
        // SAFETY: NSWindow's designated initializer.
        let win = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                rect,
                style,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: we keep the Retained.
        unsafe { win.setReleasedWhenClosed(false) };
        win.setTitle(&NSString::from_str(title));
        win
    }

    fn spin(until: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !until() && Instant::now() < deadline {
            let date = NSDate::dateWithTimeIntervalSinceNow(0.02);
            NSRunLoop::currentRunLoop().runUntilDate(&date);
        }
    }

    /// UI-windows decision 1: every window is its own window; AppKit never groups them, even
    /// with a shared tabbing identifier and "prefer tabs".
    pub fn session_windows_never_form_a_tab_group() {
        let mtm = mtm();
        let windows: Vec<_> = (0..3).map(|i| window(mtm, &format!("Drift window {i}"))).collect();
        for w in &windows {
            w.setTabbingIdentifier(&NSString::from_str("drift.test.shared"));
            disallow_tabbing(w);
            assert_eq!(w.tabbingMode(), NSWindowTabbingMode::Disallowed);
            w.makeKeyAndOrderFront(None);
        }
        spin(|| false);
        for (i, w) in windows.iter().enumerate() {
            assert!(w.tabbedWindows().is_none(), "window {i} is not tabbed");
            assert!(w.tabGroup().is_none_or(|g| g.windows().count() == 1), "window {i} is alone");
        }
        for w in &windows {
            w.close();
        }
    }

    /// UI-windows decision 10: `applicationDockMenu:` is added to the delegate's class and
    /// returns the provider's items; clicks come back with the item's id.
    pub fn the_dock_menu_comes_from_the_provider() {
        let mtm = mtm();
        let app = NSApplication::sharedApplication(mtm);
        // SAFETY: plain NSObject init of our delegate class.
        let delegate: Retained<TestDelegate> = unsafe { msg_send![TestDelegate::alloc(mtm), init] };
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

        let clicks = Rc::new(RefCell::new(Vec::<String>::new()));
        let log = clicks.clone();
        dock::install(
            mtm,
            || {
                vec![
                    DockMenuItem::Header("Sessions".into()),
                    DockMenuItem::Item {
                        id: "drift.dock.focus.session-3".into(),
                        title: "● Homelab".into(),
                        key: String::new(),
                    },
                    DockMenuItem::Separator,
                    DockMenuItem::Item {
                        id: "drift.dock.connections".into(),
                        title: "Connections".into(),
                        key: "0".into(),
                    },
                ]
            },
            move |id| log.borrow_mut().push(id.to_owned()),
        )
        .unwrap();

        // SAFETY: the method was just added with the `@@:@` signature AppKit calls it with.
        let menu: Option<Retained<NSMenu>> = unsafe { msg_send![&*delegate, applicationDockMenu: &*app] };
        let menu = menu.expect("a Dock menu");
        let items: Vec<_> = (0..menu.numberOfItems()).map(|i| menu.itemAtIndex(i).unwrap()).collect();
        let titles: Vec<String> = items.iter().map(|i| i.title().to_string()).collect();
        assert_eq!(titles, ["Sessions", "● Homelab", "", "Connections"]);
        assert!(!items[0].isEnabled(), "the header is disabled");
        assert!(items[2].isSeparatorItem());
        assert_eq!(items[3].keyEquivalent().to_string(), "0");

        menu.performActionForItemAtIndex(1);
        menu.performActionForItemAtIndex(3);
        assert_eq!(*clicks.borrow(), ["drift.dock.focus.session-3", "drift.dock.connections"]);
        app.setDelegate(None);
    }

    /// UI-windows decision 4: "Disconnect “<name>”?" is a sheet on the session window; its
    /// answer comes back through the callback.
    pub fn a_confirmation_sheet_attaches_to_its_window() {
        let mtm = mtm();
        let win = window(mtm, "Drift sheet");
        win.makeKeyAndOrderFront(None);
        let text = AlertText {
            message: "Disconnect “Homelab”?".into(),
            informative: "The remote session keeps running on the host.".into(),
            confirm: "Disconnect".into(),
            cancel: "Cancel".into(),
        };
        for (code, want) in [(NSAlertFirstButtonReturn, true), (NSAlertSecondButtonReturn, false)] {
            let answer = Rc::new(RefCell::new(None));
            let done = answer.clone();
            confirm_sheet(&win, &text, move |ok| *done.borrow_mut() = Some(ok));
            spin(|| win.attachedSheet().is_some());
            let sheet = win.attachedSheet().expect("a sheet on the window");
            win.endSheet_returnCode(&sheet, code);
            spin(|| answer.borrow().is_some());
            assert_eq!(*answer.borrow(), Some(want), "return code {code}");
        }
        win.close();
    }
}
