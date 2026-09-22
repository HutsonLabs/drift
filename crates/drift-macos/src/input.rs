//! Per-view input state behind `RemoteView` (tasks **M2-2**, **M2-4**; pure).
//!
//! [`InputController`] is the humble object's brain: `RemoteView` extracts plain values from
//! each `NSEvent` (key code, modifier flags, point in flipped view coordinates, deltas) and gets
//! back the exact [`InputEvent`]s to send. It combines `drift-input`'s keyboard translator,
//! scroll accumulator and viewport mapping with the pointer-button state, and makes the
//! `performKeyEquivalent:` decision ([`KeyEquivalent`]).

use drift_core::{DesktopSize, InputEvent, KeyboardPrefs, MouseButton, Point, ViewGeometry};
use drift_input::{KeyDown, KeyboardType, MenuShortcut, ModifierFlags, ScaleMode, ScrollConfig, ScrollDelta};

/// Outcome of `performKeyEquivalent:` for one key-down event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyEquivalent {
    /// Not ours: return `NO` (the view is not first responder, or no Command/Control is held,
    /// so AppKit delivers the event through `keyDown:`).
    Pass,
    /// An allow-listed Drift shortcut: return `NO` so the menu handles it; the remote never
    /// sees the combo.
    Menu(MenuShortcut),
    /// Claimed for the remote: return `YES` and send these events.
    Claim(Vec<InputEvent>),
}

/// Maps an `NSEvent.buttonNumber` to an RDP pointer button.
pub fn mouse_button(button_number: i64) -> Option<MouseButton> {
    let _ = button_number;
    None
}

/// Input state of one remote view.
#[derive(Debug, Clone)]
pub struct InputController {
    _prefs: KeyboardPrefs,
}

impl InputController {
    /// A controller for a profile's keyboard preferences, the local keyboard type and the scroll
    /// settings. No desktop is attached yet, so pointer events are dropped until
    /// [`InputController::set_desktop`].
    pub fn new(prefs: KeyboardPrefs, keyboard: KeyboardType, scroll: ScrollConfig) -> Self {
        let _ = (keyboard, scroll);
        Self { _prefs: prefs }
    }

    /// Changes the keyboard preferences (profile edited while connected).
    pub fn set_keyboard_prefs(&mut self, prefs: KeyboardPrefs) {
        let _ = prefs;
    }

    /// Changes the scroll conversion settings.
    pub fn set_scroll_config(&mut self, config: ScrollConfig) {
        let _ = config;
    }

    /// The view's size/backing scale changed.
    pub fn set_view_geometry(&mut self, view: ViewGeometry) {
        let _ = view;
    }

    /// The remote desktop size and how it is placed in the view (after activation / resize).
    pub fn set_desktop(&mut self, desktop: DesktopSize, mode: ScaleMode) {
        let _ = (desktop, mode);
    }

    /// Detaches the desktop (disconnected): pointer events are dropped.
    pub fn clear_desktop(&mut self) {}

    /// `keyDown:`.
    pub fn key_down(&mut self, kvk: u16, flags: ModifierFlags, is_repeat: bool, composing: bool) -> KeyDown {
        let _ = (kvk, flags, is_repeat, composing);
        KeyDown::Send(Vec::new())
    }

    /// Scancode fallback for `doCommandBySelector:` after [`KeyDown::InterpretText`].
    pub fn key_down_scancode(&mut self, kvk: u16, flags: ModifierFlags) -> Vec<InputEvent> {
        let _ = (kvk, flags);
        Vec::new()
    }

    /// `keyUp:`.
    pub fn key_up(&mut self, kvk: u16) -> Vec<InputEvent> {
        let _ = kvk;
        Vec::new()
    }

    /// `flagsChanged:`.
    pub fn flags_changed(&mut self, kvk: u16, flags: ModifierFlags) -> Vec<InputEvent> {
        let _ = (kvk, flags);
        Vec::new()
    }

    /// `insertText:` (typed text, dead-key and IME commits).
    pub fn insert_text(&mut self, text: &str) -> Vec<InputEvent> {
        let _ = text;
        Vec::new()
    }

    /// `performKeyEquivalent:` for a key-down event.
    pub fn key_equivalent(
        &mut self,
        is_first_responder: bool,
        kvk: u16,
        chars_ignoring_modifiers: &str,
        flags: ModifierFlags,
        is_repeat: bool,
    ) -> KeyEquivalent {
        let _ = (is_first_responder, kvk, chars_ignoring_modifiers, flags, is_repeat);
        KeyEquivalent::Pass
    }

    /// `mouseMoved:` / `*MouseDragged:` at a point in flipped view coordinates.
    pub fn mouse_move(&mut self, point: Point<f64>) -> Vec<InputEvent> {
        let _ = point;
        Vec::new()
    }

    /// `*MouseDown:` / `*MouseUp:` at a point in flipped view coordinates.
    pub fn mouse_button(&mut self, button: MouseButton, down: bool, point: Point<f64>) -> Vec<InputEvent> {
        let _ = (button, down, point);
        Vec::new()
    }

    /// `scrollWheel:`. `gesture_began` is `phase == NSEventPhaseBegan` (drops the remainder of
    /// the previous gesture).
    pub fn scroll(&mut self, delta: ScrollDelta, gesture_began: bool) -> Vec<InputEvent> {
        let _ = (delta, gesture_began);
        Vec::new()
    }

    /// Key focus gained (first responder in the key window) with the current modifier flags.
    pub fn focus_gained(&mut self, flags: ModifierFlags) -> Vec<InputEvent> {
        let _ = flags;
        Vec::new()
    }

    /// Key focus lost: releases held keys and pointer buttons.
    pub fn focus_lost(&mut self) -> Vec<InputEvent> {
        Vec::new()
    }

    /// "Send Ctrl+Alt+Del" menu item.
    pub fn ctrl_alt_del(&mut self) -> Vec<InputEvent> {
        Vec::new()
    }
}
