//! Window visibility and key-state observer (task **M6-3**, platform part; plan §1.8).
//!
//! Only the selected tab of a group reports `occlusionState ∋ Visible`, and
//! `NSWindowDidChangeOcclusionStateNotification` fires on every tab switch. Background tabs
//! keep rendering at 60 fps unless Drift pauses them, so the app maps
//! [`WindowEvent::Occlusion`] to `SessionCommand::SetVisible` (Suppress Output) and
//! [`WindowEvent::Key`] to `SessionCommand::Focus`.

use std::ptr::NonNull;
use std::rc::Rc;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSWindow, NSWindowDidBecomeKeyNotification, NSWindowDidChangeOcclusionStateNotification,
    NSWindowDidResignKeyNotification, NSWindowOcclusionState,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSNotificationName, NSObjectProtocol};

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
    window.occlusionState().contains(NSWindowOcclusionState::Visible)
}

/// Observes one window. Observers are removed on drop.
#[derive(Debug)]
pub struct WindowObserver {
    center: Retained<NSNotificationCenter>,
    tokens: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

impl WindowObserver {
    /// Starts observing `window`; `handler` runs on the posting thread (the main thread).
    pub fn new(window: &NSWindow, handler: impl Fn(WindowEvent) + 'static) -> Self {
        let handler: Rc<dyn Fn(WindowEvent)> = Rc::new(handler);
        let center = NSNotificationCenter::defaultCenter();
        let mut tokens = Vec::new();
        let mut observe = |name: &NSNotificationName, map: fn(&NSNotification) -> Option<WindowEvent>| {
            let h = handler.clone();
            let block = RcBlock::new(move |note: NonNull<NSNotification>| {
                // SAFETY: the center passes a valid notification for the duration of the call.
                if let Some(event) = map(unsafe { note.as_ref() }) {
                    h(event);
                }
            });
            // SAFETY: observing a constant AppKit name for this window only, delivered
            // synchronously on the posting thread; removed in `Drop`.
            let token = unsafe {
                center.addObserverForName_object_queue_usingBlock(Some(name), Some(window), None, &block)
            };
            tokens.push(token);
        };
        // SAFETY: reading an immutable AppKit notification-name constant.
        let occlusion = unsafe { NSWindowDidChangeOcclusionStateNotification };
        // SAFETY: as above.
        let became_key = unsafe { NSWindowDidBecomeKeyNotification };
        // SAFETY: as above.
        let resigned_key = unsafe { NSWindowDidResignKeyNotification };
        observe(occlusion, |note| {
            let window = note.object()?.downcast::<NSWindow>().ok()?;
            Some(WindowEvent::Occlusion { visible: is_visible(&window) })
        });
        observe(became_key, |_| Some(WindowEvent::Key(true)));
        observe(resigned_key, |_| Some(WindowEvent::Key(false)));
        Self { center, tokens }
    }
}

impl Drop for WindowObserver {
    fn drop(&mut self) {
        for token in &self.tokens {
            // SAFETY: removing a token we registered.
            unsafe { self.center.removeObserver(token.as_ref()) };
        }
    }
}
