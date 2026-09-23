//! Placing the `RemoteView` in a Tauri window (task **M1-6**, plan §1.8).
//!
//! `WebviewWindow::ns_view()` is the window's content view; wry's `WKWebView` subclass is a
//! descendant. The `RemoteView` is inserted **below** the web view in the same superview, sized
//! to it and autoresizing. While a picture is live the web view is hidden
//! (`tauri::Webview::hide()`, not `WebviewWindow::hide()`, which hides the window) and the
//! `RemoteView` becomes first responder; see `tauri_glue` (feature `tauri`) for the Tauri
//! wrappers.

use std::ffi::c_void;

use drift_core::KeyboardPrefs;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, Message as _};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSView, NSWindow, NSWindowOrderingMode};

use crate::view::RemoteView;

/// The view hierarchy was not as expected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AttachError {
    /// No `WKWebView` below the content view.
    #[error("no WKWebView found in the window's content view")]
    NoWebView,
    /// The reference view has no superview.
    #[error("the web view has no superview")]
    NoSuperview,
    /// Not called on the main thread.
    #[error("must be called on the main thread")]
    NotMainThread,
    /// A null view pointer.
    #[error("null view pointer")]
    NullView,
}

/// Whether `view`'s class is `class_name` or inherits from it.
fn is_kind_of(view: &NSView, class_name: &str) -> bool {
    let mut class = Some(view.class());
    while let Some(c) = class {
        if c.name().to_bytes() == class_name.as_bytes() {
            return true;
        }
        class = c.superclass();
    }
    false
}

/// Depth-first search for a descendant of `root` whose class is `class_name` or inherits from it.
pub fn find_subview_of_class(root: &NSView, class_name: &str) -> Option<Retained<NSView>> {
    for sub in root.subviews().iter() {
        if is_kind_of(&sub, class_name) {
            return Some(sub);
        }
        if let Some(found) = find_subview_of_class(&sub, class_name) {
            return Some(found);
        }
    }
    None
}

/// The window's first `WKWebView` (wry uses a subclass), depth first.
pub fn find_webview(root: &NSView) -> Option<Retained<NSView>> {
    find_subview_of_class(root, "WKWebView")
}

/// Every `WKWebView` below `root`, depth first (a session window has the page and, above it,
/// the tab strip).
pub fn find_webviews(root: &NSView) -> Vec<Retained<NSView>> {
    let mut found = Vec::new();
    for sub in root.subviews().iter() {
        if is_kind_of(&sub, "WKWebView") {
            found.push(sub);
        } else {
            found.extend(find_webviews(&sub));
        }
    }
    found
}

/// Inserts `view` directly below `reference` in `reference`'s superview, with the superview's
/// bounds and width/height autoresizing.
pub fn insert_below(reference: &NSView, view: &NSView) -> Result<(), AttachError> {
    // SAFETY: `superview` returns the (retained) parent or nil.
    let parent = unsafe { reference.superview() }.ok_or(AttachError::NoSuperview)?;
    view.setFrame(parent.bounds());
    view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    parent.addSubview_positioned_relativeTo(view, NSWindowOrderingMode::Below, Some(reference));
    Ok(())
}

/// Creates a `RemoteView` and inserts it below the web view found under `content_view`.
///
/// # Safety
/// `content_view` must be null or a valid `NSView*` (e.g. from `WebviewWindow::ns_view()`).
pub unsafe fn attach_to_content_view(
    content_view: *mut c_void,
    prefs: KeyboardPrefs,
) -> Result<Retained<RemoteView>, AttachError> {
    let mtm = MainThreadMarker::new().ok_or(AttachError::NotMainThread)?;
    // SAFETY: the caller guarantees a valid NSView pointer or null.
    let content = unsafe { content_view.cast::<NSView>().as_ref() }.ok_or(AttachError::NullView)?.retain();
    let web = find_webview(&content).ok_or(AttachError::NoWebView)?;
    let view = RemoteView::new(mtm, content.bounds(), prefs);
    insert_below(&web, &view)?;
    view.sync_drawable_size();
    Ok(view)
}

/// Makes `view` the first responder of `window` (after hiding the web view).
pub fn focus_remote(window: &NSWindow, view: &RemoteView) -> bool {
    window.makeFirstResponder(Some(view))
}

/// Gives keyboard focus back to the web view (after showing it).
pub fn focus_webview(window: &NSWindow, webview: &NSView) -> bool {
    window.makeFirstResponder(Some(webview))
}
