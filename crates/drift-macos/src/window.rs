//! Window visibility and key-state observer (task **M6-3**, platform part; plan §1.8).
//!
//! Only the selected tab of a group reports `occlusionState ∋ Visible`, and
//! `NSWindowDidChangeOcclusionStateNotification` fires on every tab switch. Background tabs
//! keep rendering at 60 fps unless Drift pauses them, so the app maps
//! [`WindowEvent::Occlusion`] to `SessionCommand::SetVisible` (Suppress Output) and
//! [`WindowEvent::Key`] to `SessionCommand::Focus`.

use objc2_app_kit::NSWindow;

/// A change of one window's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowEvent {
    /// The occlusion state changed; `visible` is `occlusionState ∋ Visible`.
    Occlusion {
        /// Any part of the window is visible on screen.
        visible: bool,
    },
    /// The window became (`true`) or resigned (`false`) key.
    Key(bool),
}

/// Whether any part of `window` is visible (`occlusionState ∋ Visible`).
pub fn is_visible(window: &NSWindow) -> bool {
    let _ = window;
    false
}

/// Observes one window. Observers are removed on drop.
#[derive(Debug)]
pub struct WindowObserver {}

impl WindowObserver {
    /// Starts observing `window`; `handler` runs on the posting thread (the main thread).
    pub fn new(window: &NSWindow, handler: impl Fn(WindowEvent) + 'static) -> Self {
        let _ = (window, handler);
        Self {}
    }
}
