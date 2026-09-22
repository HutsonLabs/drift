//! Tauri wrappers around [`crate::webview`] (feature `tauri`; plan §1.8, M1-6).
//!
//! All functions must run on the main thread (Tauri `run_on_main_thread` or a setup hook).
//! The webview is hidden with `tauri::Webview::hide()` — **not** `WebviewWindow::hide()`,
//! which hides the whole window — and the `RemoteView` then becomes first responder.

use drift_core::KeyboardPrefs;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, Message as _};
use objc2_app_kit::{NSView, NSWindow};
use tauri::{Runtime, Webview, WebviewWindow};

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

fn ns_window<R: Runtime>(window: &WebviewWindow<R>) -> Result<Retained<NSWindow>, GlueError> {
    MainThreadMarker::new().ok_or(AttachError::NotMainThread)?;
    let ptr = window.ns_window()?.cast::<NSWindow>();
    // SAFETY: tao returns its live `NSWindow*`; we are on the main thread and retain it.
    unsafe { ptr.as_ref() }.map(|w| w.retain()).ok_or(GlueError::Attach(AttachError::NullView))
}

fn content_view<R: Runtime>(window: &WebviewWindow<R>) -> Result<Retained<NSView>, GlueError> {
    MainThreadMarker::new().ok_or(AttachError::NotMainThread)?;
    let ptr = window.ns_view()?.cast::<NSView>();
    // SAFETY: tao returns its live content `NSView*`; we are on the main thread and retain it.
    unsafe { ptr.as_ref() }.map(|v| v.retain()).ok_or(GlueError::Attach(AttachError::NullView))
}

/// Creates a `RemoteView` below the window's `WKWebView` (sized to it, autoresizing) and
/// enables mouse-moved events for the window.
pub fn attach<R: Runtime>(
    window: &WebviewWindow<R>,
    prefs: KeyboardPrefs,
) -> Result<Retained<RemoteView>, GlueError> {
    let content = content_view(window)?;
    // SAFETY: `content` is a valid NSView and we are on the main thread (checked above).
    let view =
        unsafe { webview::attach_to_content_view(Retained::as_ptr(&content).cast_mut().cast(), prefs)? };
    ns_window(window)?.setAcceptsMouseMovedEvents(true);
    Ok(view)
}

/// Live picture: hides the webview and focuses the `RemoteView`.
pub fn show_remote<R: Runtime>(window: &WebviewWindow<R>, view: &RemoteView) -> Result<(), GlueError> {
    let wv: &Webview<R> = window.as_ref();
    wv.hide()?;
    let win = ns_window(window)?;
    webview::focus_remote(&win, view);
    Ok(())
}

/// No live picture (form, prompt, overlay): shows the webview and gives it keyboard focus.
/// The `RemoteView` releases held keys when it loses first responder.
pub fn show_webview<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), GlueError> {
    let wv: &Webview<R> = window.as_ref();
    wv.show()?;
    let content = content_view(window)?;
    if let Some(web) = webview::find_webview(&content) {
        let win = ns_window(window)?;
        webview::focus_webview(&win, &web);
    }
    Ok(())
}
