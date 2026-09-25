//! Tauri wrappers around [`crate::webview`] (feature `tauri`; plan §1.8, M1-6).
//!
//! All functions must run on the main thread (Tauri `run_on_main_thread` or a setup hook).
//! A webview is hidden with `tauri::Webview::hide()` — **not** `Window::hide()`, which hides
//! the whole window — and the `RemoteView` then becomes first responder.
//!
//! Session windows hold two webviews (the page and the title bar, task UI-windows), so Tauri no
//! longer treats them as single-webview `WebviewWindow`s: these helpers take the
//! [`tauri::Window`] and name the page's [`tauri::Webview`] explicitly.

use drift_core::KeyboardPrefs;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, Message as _};
use objc2_app_kit::{NSView, NSWindow};
use tauri::{Runtime, Webview, Window};

use crate::view::RemoteView;
use crate::webview::{self, AttachError};

/// A Tauri or view-hierarchy failure.
#[derive(Debug, thiserror::Error)]
pub enum GlueError {
    /// Tauri refused (window gone, wrong thread …).
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
    /// The view hierarchy was not as expected.
    #[error(transparent)]
    Attach(#[from] AttachError),
}

/// The window's `NSWindow` (main thread).
pub fn ns_window<R: Runtime>(window: &Window<R>) -> Result<Retained<NSWindow>, GlueError> {
    MainThreadMarker::new().ok_or(AttachError::NotMainThread)?;
    let ptr = window.ns_window()?.cast::<NSWindow>();
    // SAFETY: tao returns its live `NSWindow*`; we are on the main thread and retain it.
    unsafe { ptr.as_ref() }.map(|w| w.retain()).ok_or(GlueError::Attach(AttachError::NullView))
}

/// The window's content view (main thread); every webview and the `RemoteView` live in it.
pub fn content_view<R: Runtime>(window: &Window<R>) -> Result<Retained<NSView>, GlueError> {
    MainThreadMarker::new().ok_or(AttachError::NotMainThread)?;
    let ptr = window.ns_view()?.cast::<NSView>();
    // SAFETY: tao returns its live content `NSView*`; we are on the main thread and retain it.
    unsafe { ptr.as_ref() }.map(|v| v.retain()).ok_or(GlueError::Attach(AttachError::NullView))
}

/// Creates a `RemoteView` below the window's (only) `WKWebView` (sized to it, autoresizing) and
/// enables mouse-moved events for the window. Call it before adding further webviews; the
/// returned `WKWebView` is the page's.
pub fn attach<R: Runtime>(
    window: &Window<R>,
    prefs: KeyboardPrefs,
) -> Result<(Retained<RemoteView>, Retained<NSView>), GlueError> {
    let content = content_view(window)?;
    let web = webview::find_webview(&content).ok_or(AttachError::NoWebView)?;
    // SAFETY: `content` is a valid NSView and we are on the main thread (checked above).
    let view =
        unsafe { webview::attach_to_content_view(Retained::as_ptr(&content).cast_mut().cast(), prefs)? };
    ns_window(window)?.setAcceptsMouseMovedEvents(true);
    Ok((view, web))
}

/// Live picture: hides the page's webview and focuses the `RemoteView`.
pub fn show_remote<R: Runtime>(
    window: &Window<R>,
    page: &Webview<R>,
    view: &RemoteView,
) -> Result<(), GlueError> {
    page.hide()?;
    let win = ns_window(window)?;
    webview::focus_remote(&win, view);
    Ok(())
}

/// No live picture (form, prompt, overlay): shows the page's webview (`web` is its
/// `WKWebView`) and gives it keyboard focus. The `RemoteView` releases held keys when it loses
/// first responder.
pub fn show_webview<R: Runtime>(
    window: &Window<R>,
    page: &Webview<R>,
    web: &NSView,
) -> Result<(), GlueError> {
    page.show()?;
    let win = ns_window(window)?;
    webview::focus_webview(&win, web);
    Ok(())
}
