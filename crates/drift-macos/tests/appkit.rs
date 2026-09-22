//! AppKit smoke tests for the humble objects (M1-6, M2-2, M2-4, M2-5, M6-3), run on the main
//! thread by `support/harness.rs`. They use off-screen, never-shown windows, so they need a
//! window-server session (any logged-in Mac or GitHub's macOS runners) but no user interaction.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used)]

#[path = "support/harness.rs"]
mod harness;

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;

use drift_core::{CmdAs, InputEvent, KeyboardPrefs, MouseButton, Size, ViewGeometry};
use drift_input::ScaleMode;
use drift_macos::cursor::{CursorShape, PointerDecoder, PointerEvent};
use drift_macos::view::{RemoteView, RemoteViewHandler};
use drift_macos::webview::{find_subview_of_class, find_webview, focus_remote, insert_below};
use drift_macos::window::{WindowEvent, WindowObserver, is_visible};
use drift_testkit::fixtures;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSAccessibilityImageRole, NSApplication, NSApplicationActivationPolicy,
    NSBackingStoreType, NSButton, NSEvent,
    NSEventModifierFlags, NSEventType, NSResponder, NSTextInputClient, NSView, NSWindow,
    NSWindowDidChangeOcclusionStateNotification, NSWindowDidResignKeyNotification, NSWindowStyleMask,
};
use objc2_foundation::{NSNotificationCenter, NSPoint, NSRange, NSRect, NSSize, NSString};

fn main() -> ExitCode {
    harness::run(&[
        ("remote_view_is_layer_hosting_metal", remote_view_is_layer_hosting_metal),
        ("drawable_size_tracks_frame_and_backing_scale", drawable_size_tracks_frame_and_backing_scale),
        ("geometry_changes_reach_the_handler", geometry_changes_reach_the_handler),
        ("key_down_and_up_reach_the_handler_as_scancodes", key_down_and_up_reach_the_handler_as_scancodes),
        (
            "perform_key_equivalent_claims_all_but_the_allow_list",
            perform_key_equivalent_claims_all_but_the_allow_list,
        ),
        (
            "perform_key_equivalent_ignores_views_that_are_not_first_responder",
            perform_key_equivalent_ignores_views_that_are_not_first_responder,
        ),
        ("insert_text_sends_unicode_in_mac_layout_mode", insert_text_sends_unicode_in_mac_layout_mode),
        ("marked_text_state_for_ime", marked_text_state_for_ime),
        (
            "mouse_down_maps_flipped_view_points_to_desktop_pixels",
            mouse_down_maps_flipped_view_points_to_desktop_pixels,
        ),
        ("resigning_first_responder_releases_held_keys", resigning_first_responder_releases_held_keys),
        (
            "cursor_from_fixture_pointer_has_point_size_and_hotspot",
            cursor_from_fixture_pointer_has_point_size_and_hotspot,
        ),
        ("hidden_and_default_cursor_shapes", hidden_and_default_cursor_shapes),
        (
            "find_subview_of_class_walks_the_hierarchy_and_superclasses",
            find_subview_of_class_walks_the_hierarchy_and_superclasses,
        ),
        (
            "insert_below_places_the_view_under_the_reference",
            insert_below_places_the_view_under_the_reference,
        ),
        ("focus_remote_makes_the_view_first_responder", focus_remote_makes_the_view_first_responder),
        (
            "window_observer_reports_occlusion_and_key_changes",
            window_observer_reports_occlusion_and_key_changes,
        ),
        ("remote_view_is_an_accessibility_element_with_a_role", remote_view_is_an_accessibility_element_with_a_role),
        ("remote_view_accessibility_label_follows_the_session", remote_view_accessibility_label_follows_the_session),
    ])
}

// ---------------------------------------------------------------------------------------
// Fixtures

fn mtm() -> MainThreadMarker {
    let mtm = MainThreadMarker::new().expect("harness runs tests on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    mtm
}

fn window(mtm: MainThreadMarker, w: f64, h: f64) -> Retained<NSWindow> {
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h));
    // SAFETY: designated initializer with valid arguments; released by us (not on close).
    let win = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect,
            NSWindowStyleMask::Titled | NSWindowStyleMask::Resizable,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: we keep the Retained; AppKit must not release it again on close.
    unsafe { win.setReleasedWhenClosed(false) };
    win
}

#[derive(Default)]
struct Recorder {
    input: RefCell<Vec<InputEvent>>,
    geometry: RefCell<Vec<ViewGeometry>>,
    focus: RefCell<Vec<bool>>,
}

impl RemoteViewHandler for Recorder {
    fn input(&self, events: &[InputEvent]) {
        self.input.borrow_mut().extend_from_slice(events);
    }
    fn geometry_changed(&self, geometry: ViewGeometry) {
        self.geometry.borrow_mut().push(geometry);
    }
    fn focus_changed(&self, focused: bool) {
        self.focus.borrow_mut().push(focused);
    }
}

impl Recorder {
    fn take(&self) -> Vec<InputEvent> {
        std::mem::take(&mut *self.input.borrow_mut())
    }
}

struct Rig {
    window: Retained<NSWindow>,
    view: Retained<RemoteView>,
    rec: Rc<Recorder>,
}

fn rig_with(prefs: KeyboardPrefs) -> Rig {
    let mtm = mtm();
    let window = window(mtm, 400.0, 300.0);
    let content = window.contentView().unwrap();
    let view = RemoteView::new(mtm, content.bounds(), prefs);
    content.addSubview(&view);
    let rec = Rc::new(Recorder::default());
    view.set_handler(rec.clone());
    assert!(window.makeFirstResponder(Some(&view)), "RemoteView accepts first responder");
    rec.take();
    Rig { window, view, rec }
}

fn rig() -> Rig {
    rig_with(KeyboardPrefs::default())
}

fn key_event(
    win: &NSWindow,
    down: bool,
    key_code: u16,
    chars: &str,
    mods: NSEventModifierFlags,
) -> Retained<NSEvent> {
    NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
        if down { NSEventType::KeyDown } else { NSEventType::KeyUp },
        NSPoint::new(0.0, 0.0),
        mods,
        0.0,
        win.windowNumber(),
        None,
        &NSString::from_str(chars),
        &NSString::from_str(chars),
        false,
        key_code,
    )
    .unwrap()
}

fn mouse_event(win: &NSWindow, ty: NSEventType, p: NSPoint) -> Retained<NSEvent> {
    NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        ty,
        p,
        NSEventModifierFlags::empty(),
        0.0,
        win.windowNumber(),
        None,
        0,
        1,
        1.0,
    )
    .unwrap()
}

fn key(scancode: u8, extended: bool, down: bool) -> InputEvent {
    InputEvent::Key { scancode, extended, down }
}

fn responder(view: &RemoteView) -> &NSResponder {
    view
}

// ---------------------------------------------------------------------------------------
// RemoteView

fn remote_view_is_layer_hosting_metal() {
    let r = rig();
    let layer = r.view.layer().expect("layer-hosting view has a layer");
    let metal: &objc2::runtime::AnyObject = &layer;
    assert!(metal.class().name().to_str().unwrap().contains("CAMetalLayer"), "{:?}", metal.class().name());
    assert!(r.view.wantsLayer());
    assert!(r.view.isFlipped(), "top-left origin for pointer mapping");
    assert!(r.view.acceptsFirstResponder());
    assert!(r.view.isOpaque());
}

fn drawable_size_tracks_frame_and_backing_scale() {
    let r = rig();
    let scale = r.window.backingScaleFactor();
    r.view.setFrameSize(NSSize::new(321.0, 123.0));
    let layer = r.view.metal_layer();
    assert_eq!(layer.contentsScale(), scale);
    let ds = layer.drawableSize();
    assert_eq!((ds.width, ds.height), (321.0 * scale, 123.0 * scale));
    assert_eq!(r.view.geometry(), ViewGeometry { points: Size::new(321.0, 123.0), backing_scale: scale });
    r.view.viewDidChangeBackingProperties();
    let ds = layer.drawableSize();
    assert_eq!((ds.width, ds.height), (321.0 * scale, 123.0 * scale));
}

fn geometry_changes_reach_the_handler() {
    let r = rig();
    r.rec.geometry.borrow_mut().clear();
    r.view.setFrameSize(NSSize::new(200.0, 100.0));
    let g = r.rec.geometry.borrow();
    assert_eq!(g.last().map(|g| g.points), Some(Size::new(200.0, 100.0)), "{g:?}");
}

fn key_down_and_up_reach_the_handler_as_scancodes() {
    let r = rig();
    responder(&r.view).keyDown(&key_event(&r.window, true, 0, "a", NSEventModifierFlags::empty()));
    responder(&r.view).keyUp(&key_event(&r.window, false, 0, "a", NSEventModifierFlags::empty()));
    assert_eq!(r.rec.take(), vec![key(0x1E, false, true), key(0x1E, false, false)]);
}

fn perform_key_equivalent_claims_all_but_the_allow_list() {
    let r = rig();
    // Cmd+T is Drift's "New Tab": not claimed, nothing sent.
    let cmd_t = key_event(&r.window, true, 17, "t", NSEventModifierFlags::Command);
    assert!(!r.view.performKeyEquivalent(&cmd_t), "Cmd+T goes to the menu");
    assert_eq!(r.rec.take(), vec![]);
    // Cmd+K is claimed and sent as Super+K.
    let cmd_k = key_event(&r.window, true, 40, "k", NSEventModifierFlags::Command);
    assert!(r.view.performKeyEquivalent(&cmd_k), "Cmd+K is claimed for the remote");
    let sent = r.rec.take();
    assert!(sent.contains(&key(0x25, false, true)), "{sent:?}");
    assert!(sent.contains(&key(0x5B, true, true)), "{sent:?}");
    // Ctrl+Tab is claimed (AppKit would otherwise use it for key-view looping).
    let ctrl_tab = key_event(&r.window, true, 48, "\t", NSEventModifierFlags::Control);
    assert!(r.view.performKeyEquivalent(&ctrl_tab));
    assert!(r.rec.take().contains(&key(0x0F, false, true)));
}

fn perform_key_equivalent_ignores_views_that_are_not_first_responder() {
    let r = rig();
    assert!(r.window.makeFirstResponder(None));
    r.rec.take();
    let cmd_c = key_event(&r.window, true, 8, "c", NSEventModifierFlags::Command);
    assert!(!r.view.performKeyEquivalent(&cmd_c));
    assert_eq!(r.rec.take(), vec![]);
}

fn insert_text_sends_unicode_in_mac_layout_mode() {
    let r = rig_with(KeyboardPrefs { cmd_as: CmdAs::Super, type_with_mac_layout: true });
    let text = NSString::from_str("ß✓");
    // SAFETY: `insertText:replacementRange:` accepts an NSString.
    unsafe { r.view.insertText_replacementRange(&text, NSRange::new(usize::MAX >> 1, 0)) };
    assert_eq!(
        r.rec.take(),
        vec![
            InputEvent::Unicode { ch: 0xDF, down: true },
            InputEvent::Unicode { ch: 0xDF, down: false },
            InputEvent::Unicode { ch: 0x2713, down: true },
            InputEvent::Unicode { ch: 0x2713, down: false },
        ]
    );
}

fn marked_text_state_for_ime() {
    let r = rig_with(KeyboardPrefs { cmd_as: CmdAs::Super, type_with_mac_layout: true });
    assert!(!r.view.hasMarkedText());
    let marked = NSString::from_str("に");
    // SAFETY: NSString argument, valid ranges.
    unsafe {
        r.view.setMarkedText_selectedRange_replacementRange(
            &marked,
            NSRange::new(1, 0),
            NSRange::new(usize::MAX >> 1, 0),
        )
    };
    assert!(r.view.hasMarkedText());
    assert_eq!(r.view.markedRange().length, 1);
    assert_eq!(r.rec.take(), vec![], "composition is local until committed");
    let commit = NSString::from_str("日");
    // SAFETY: as above.
    unsafe { r.view.insertText_replacementRange(&commit, NSRange::new(usize::MAX >> 1, 0)) };
    assert!(!r.view.hasMarkedText(), "commit ends the composition");
    assert_eq!(
        r.rec.take(),
        vec![InputEvent::Unicode { ch: 0x65E5, down: true }, InputEvent::Unicode { ch: 0x65E5, down: false }]
    );
}

fn mouse_down_maps_flipped_view_points_to_desktop_pixels() {
    let r = rig();
    r.view.setFrameSize(NSSize::new(400.0, 300.0));
    r.view.setFrameOrigin(NSPoint::new(0.0, 0.0));
    r.view.set_desktop(Size::new(800, 600), ScaleMode::Fit);
    // Window coordinates are bottom-left based: y = 300 - 10 → view y 10 (flipped).
    let down = mouse_event(&r.window, NSEventType::LeftMouseDown, NSPoint::new(20.0, 290.0));
    responder(&r.view).mouseDown(&down);
    let up = mouse_event(&r.window, NSEventType::LeftMouseUp, NSPoint::new(20.0, 290.0));
    responder(&r.view).mouseUp(&up);
    let sent = r.rec.take();
    assert!(
        sent.contains(&InputEvent::MouseButton { button: MouseButton::Left, down: true, x: 40, y: 20 }),
        "{sent:?}"
    );
    assert!(sent.contains(&InputEvent::MouseButton { button: MouseButton::Left, down: false, x: 40, y: 20 }));
}

fn resigning_first_responder_releases_held_keys() {
    let r = rig();
    responder(&r.view).keyDown(&key_event(&r.window, true, 0, "a", NSEventModifierFlags::empty()));
    r.rec.take();
    r.rec.focus.borrow_mut().clear();
    assert!(r.window.makeFirstResponder(None));
    assert_eq!(r.rec.take(), vec![key(0x1E, false, false)]);
    assert_eq!(*r.rec.focus.borrow(), vec![false]);
}

// ---------------------------------------------------------------------------------------
// Cursor

fn fixture_cursor(rel: &str) -> CursorShape {
    let mut dec = PointerDecoder::default();
    let mut shape = None;
    for rec in fixtures::records(rel) {
        for ev in dec.decode_output_pdu(&rec).unwrap() {
            if let PointerEvent::Shape(s @ CursorShape::Image(_)) = ev {
                shape = Some(s);
            }
        }
    }
    shape.expect("fixture has a pointer image")
}

fn cursor_from_fixture_pointer_has_point_size_and_hotspot() {
    let r = rig();
    for (rel, scale) in
        [("pdus/fastpath_pointer_scale100.rec", 100), ("pdus/fastpath_pointer_scale200.rec", 200)]
    {
        r.view.set_cursor_shape(&fixture_cursor(rel), scale);
        let cursor = r.view.current_cursor();
        let size = cursor.image().size();
        assert_eq!((size.width, size.height), (43.0, 43.0), "{rel}");
        let hs = cursor.hotSpot();
        assert_eq!((hs.x, hs.y), (5.0, 5.0), "{rel}");
    }
}

fn hidden_and_default_cursor_shapes() {
    let r = rig();
    r.view.set_cursor_shape(&CursorShape::Hidden, 100);
    let hidden = r.view.current_cursor();
    assert_ne!(Retained::as_ptr(&hidden), Retained::as_ptr(&objc2_app_kit::NSCursor::arrowCursor()));
    r.view.set_cursor_shape(&CursorShape::Default, 100);
    assert_eq!(
        Retained::as_ptr(&r.view.current_cursor()),
        Retained::as_ptr(&objc2_app_kit::NSCursor::arrowCursor())
    );
}

// ---------------------------------------------------------------------------------------
// Web view helpers

fn find_subview_of_class_walks_the_hierarchy_and_superclasses() {
    let mtm = mtm();
    let root = NSView::new(mtm);
    let mid = NSView::new(mtm);
    let button = NSButton::new(mtm);
    root.addSubview(&mid);
    mid.addSubview(&button);
    let found = find_subview_of_class(&root, "NSControl").expect("NSButton is an NSControl");
    assert_eq!(Retained::as_ptr(&found).cast::<()>(), Retained::as_ptr(&button).cast::<()>());
    assert!(find_subview_of_class(&root, "NSTextView").is_none());
    assert!(find_webview(&root).is_none());
}

fn insert_below_places_the_view_under_the_reference() {
    let mtm = mtm();
    let parent = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(300.0, 200.0)),
    );
    let web = NSButton::new(mtm);
    parent.addSubview(&web);
    let remote = RemoteView::new(mtm, NSRect::ZERO, KeyboardPrefs::default());
    insert_below(&web, &remote).unwrap();
    let subs = parent.subviews();
    assert_eq!(subs.count(), 2);
    assert_eq!(
        Retained::as_ptr(&subs.objectAtIndex(0)).cast::<()>(),
        Retained::as_ptr(&remote).cast::<()>(),
        "below"
    );
    assert_eq!(remote.frame().size, NSSize::new(300.0, 200.0));
    let orphan = NSButton::new(mtm);
    assert!(insert_below(&orphan, &RemoteView::new(mtm, NSRect::ZERO, KeyboardPrefs::default())).is_err());
}

fn focus_remote_makes_the_view_first_responder() {
    let r = rig();
    assert!(r.window.makeFirstResponder(None));
    assert!(focus_remote(&r.window, &r.view));
    let fr = r.window.firstResponder().unwrap();
    assert_eq!(Retained::as_ptr(&fr).cast::<()>(), Retained::as_ptr(&r.view).cast::<()>());
}

// ---------------------------------------------------------------------------------------
// Window observer

fn window_observer_reports_occlusion_and_key_changes() {
    let mtm = mtm();
    let win = window(mtm, 100.0, 100.0);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    let observer = WindowObserver::new(&win, move |e| s.borrow_mut().push(e));
    let center = NSNotificationCenter::defaultCenter();
    // SAFETY: posting AppKit notification names for our own window.
    unsafe {
        center.postNotificationName_object(NSWindowDidChangeOcclusionStateNotification, Some(&win));
        center.postNotificationName_object(NSWindowDidResignKeyNotification, Some(&win));
    }
    assert_eq!(
        *seen.borrow(),
        vec![WindowEvent::Occlusion { visible: is_visible(&win) }, WindowEvent::Key(false)]
    );
    // Another window's notifications are ignored.
    let other = window(mtm, 100.0, 100.0);
    // SAFETY: as above.
    unsafe { center.postNotificationName_object(NSWindowDidChangeOcclusionStateNotification, Some(&other)) };
    assert_eq!(seen.borrow().len(), 2);
    drop(observer);
    // SAFETY: as above.
    unsafe { center.postNotificationName_object(NSWindowDidResignKeyNotification, Some(&win)) };
    assert_eq!(seen.borrow().len(), 2, "removed on drop");
}

// ---------------------------------------------------------------------------------------
// Accessibility (M9-4)

/// VoiceOver must find the live picture: an element with an image role, a role description
/// and help text, not an anonymous `NSView` that reads as "group".
fn remote_view_is_an_accessibility_element_with_a_role() {
    let r = rig();
    let view: &NSView = &r.view;
    assert!(view.isAccessibilityElement(), "the picture is one accessibility element");
    let role = view.accessibilityRole().expect("an accessibility role");
    assert_eq!(&*role, unsafe { NSAccessibilityImageRole });
    let described = view.accessibilityRoleDescription().expect("a role description");
    assert_eq!(described.to_string(), "remote desktop");
    let help = view.accessibilityHelp().expect("help text").to_string();
    assert!(help.contains("remote"), "{help}");
    // The Metal layer is the picture; VoiceOver must not walk into it.
    assert_eq!(view.accessibilityChildren().map_or(0, |c| c.len()), 0);
}

/// The label names the connection and the desktop, so VoiceOver announces which tab this is.
fn remote_view_accessibility_label_follows_the_session() {
    let r = rig();
    let view: &NSView = &r.view;
    assert_eq!(
        view.accessibilityLabel().expect("a default label").to_string(),
        "Remote desktop",
        "there is always a label, even before a session connects"
    );
    r.view.set_accessibility_label("Homelab — remote desktop, 1280 by 800 pixels");
    assert_eq!(
        view.accessibilityLabel().expect("a label").to_string(),
        "Homelab — remote desktop, 1280 by 800 pixels"
    );
}
