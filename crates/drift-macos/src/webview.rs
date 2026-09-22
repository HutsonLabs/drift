//! Placing the `RemoteView` in a Tauri window (task **M1-6**, plan §1.8).
//!
//! `WebviewWindow::ns_view()` is the window's content view; wry's `WKWebView` subclass is a
//! descendant. The `RemoteView` is inserted **below** the web view in the same superview, sized
//! to it and autoresizing. While a picture is live the web view is hidden
//! (`tauri::Webview::hide()`, not `WebviewWindow::hide()`, which hides the window) and the
//! `RemoteView` becomes first responder; see [`crate::tauri_glue`] for the Tauri wrappers.

use std::ffi::c_void;

use drift_core::KeyboardPrefs;
use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWindow};

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

/// Depth-first search for a descendant of `root` whose class is `class_name` or inherits from it.
pub fn find_subview_of_class(root: &NSView, class_name: &str) -> Option<Retained<NSView>> {
    let _ = (root, class_name);
    None
}

/// The window's `WKWebView` (wry uses a subclass).
pub fn find_webview(root: &NSView) -> Option<Retained<NSView>> {
    find_subview_of_class(root, "WKWebView")
}

/// Inserts `view` directly below `reference` in `reference`'s superview, with the superview's
/// bounds and width/height autoresizing.
pub fn insert_below(reference: &NSView, view: &NSView) -> Result<(), AttachError> {
    let _ = (reference, view);
    Err(AttachError::NoSuperview)
}

/// Creates a `RemoteView` and inserts it below the web view found under `content_view`.
///
/// # Safety
/// `content_view` must be null or a valid `NSView*` (e.g. from `WebviewWindow::ns_view()`),
/// and this must run on the main thread.
pub unsafe fn attach_to_content_view(
    content_view: *mut c_void,
    prefs: KeyboardPrefs,
) -> Result<Retained<RemoteView>, AttachError> {
    let _ = (content_view, prefs);
    Err(AttachError::NullView)
}

/// Makes `view` the first responder of `window` (after hiding the web view).
pub fn focus_remote(window: &NSWindow, view: &RemoteView) -> bool {
    let _ = (window, view);
    false
}

/// Gives keyboard focus back to the web view (after showing it).
pub fn focus_webview(window: &NSWindow, webview: &NSView) -> bool {
    let _ = (window, webview);
    false
}
