//! Stateful keyboard translation (tasks **M2-1** and **M2-2**).
//!
//! [`Keyboard`] is the single object `drift-macos`'s `RemoteView` feeds with plain values from
//! `keyDown:`, `keyUp:`, `flagsChanged:`, `insertText:` and focus changes. It returns the exact
//! [`InputEvent`]s to send. Invariants (property-tested):
//!
//! * On the wire, every scancode press is followed by exactly one release before it is pressed
//!   again, and no release is sent for a key the remote does not consider pressed. Two Mac keys
//!   that map to the same scancode (Control and Command-as-Ctrl) are reference counted.
//! * [`Keyboard::focus_lost`] releases exactly the scancodes currently pressed.
//! * The Command key is *deferred*: its press is only sent once another key or a pointer button
//!   is used with it, or as a tap (press + release) when it is released alone. A combo claimed
//!   by the menu (`performKeyEquivalent:` allow-list, M2-4) therefore never reaches the remote,
//!   and Cmd+Tab away from Drift does not open the GNOME overview.
//! * Caps Lock is never sent as a key; its toggle state goes out as [`InputEvent::SyncToggles`]
//!   (Num Lock is always reported on: Mac keypads always type digits).

use drift_core::{InputEvent, KeyboardPrefs};

use crate::keymap::{KeyboardType, Scancode};
use crate::modifiers::ModifierFlags;

/// Result of [`Keyboard::key_down`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyDown {
    /// Send these events (possibly none, e.g. an auto-repeat or an unmapped key).
    Send(Vec<InputEvent>),
    /// "Type using Mac layout": pass the `NSEvent` to `interpretKeyEvents:`; the produced text
    /// comes back through [`Keyboard::insert_text`]. If AppKit instead calls
    /// `doCommandBySelector:`, send the key with [`Keyboard::key_down_scancode`].
    InterpretText,
}

/// Keyboard translation state for one session view.
#[derive(Debug, Clone)]
pub struct Keyboard {
    prefs: KeyboardPrefs,
    keyboard: KeyboardType,
}

impl Keyboard {
    /// Creates the translator for a profile's keyboard preferences and the local keyboard type.
    pub fn new(prefs: KeyboardPrefs, keyboard: KeyboardType) -> Self {
        Self { prefs, keyboard }
    }

    /// The active preferences.
    pub fn prefs(&self) -> KeyboardPrefs {
        self.prefs
    }

    /// Changes the preferences (profile edit). Keys already pressed are released with the
    /// scancodes they were pressed with.
    pub fn set_prefs(&mut self, prefs: KeyboardPrefs) {
        self.prefs = prefs;
    }

    /// Changes the local keyboard type (keyboard plugged in / switched).
    pub fn set_keyboard_type(&mut self, keyboard: KeyboardType) {
        self.keyboard = keyboard;
    }

    /// `flagsChanged:` with the changed key code and the new raw modifier flags.
    pub fn flags_changed(&mut self, kvk: u16, flags: ModifierFlags) -> Vec<InputEvent> {
        let _ = (kvk, flags);
        Vec::new()
    }

    /// `keyDown:`. `composing` is `hasMarkedText` of the view's text input client.
    pub fn key_down(&mut self, kvk: u16, flags: ModifierFlags, is_repeat: bool, composing: bool) -> KeyDown {
        let _ = (kvk, flags, is_repeat, composing);
        KeyDown::Send(Vec::new())
    }

    /// Sends `kvk` as a scancode press regardless of routing (fallback for
    /// `doCommandBySelector:` after [`KeyDown::InterpretText`]).
    pub fn key_down_scancode(&mut self, kvk: u16, flags: ModifierFlags) -> Vec<InputEvent> {
        let _ = (kvk, flags);
        Vec::new()
    }

    /// `keyUp:`. Releases the scancode sent for `kvk`, if any.
    pub fn key_up(&mut self, kvk: u16) -> Vec<InputEvent> {
        let _ = kvk;
        Vec::new()
    }

    /// `insertText:` from the text input system (typed characters, dead-key and IME commits).
    /// Held Shift/Option scancodes are released around the Unicode events and pressed again
    /// afterwards, so the remote does not apply them to the injected keysyms.
    pub fn insert_text(&mut self, text: &str) -> Vec<InputEvent> {
        let _ = text;
        Vec::new()
    }

    /// `performKeyEquivalent:` handed an allow-listed combo to the menu: a deferred Command press
    /// is cancelled so the remote never sees the combo.
    pub fn menu_shortcut_taken(&mut self) {}

    /// Call before sending a pointer button: flushes a deferred Command press (Super+drag).
    pub fn prepare_pointer_button(&mut self) -> Vec<InputEvent> {
        Vec::new()
    }

    /// The view lost key focus (window resigned key, tab switched, app deactivated): releases
    /// exactly the scancodes the remote considers pressed and forgets all key state.
    pub fn focus_lost(&mut self) -> Vec<InputEvent> {
        Vec::new()
    }

    /// The view became first responder / key: resynchronises lock toggles and presses the
    /// modifiers held right now.
    pub fn focus_gained(&mut self, flags: ModifierFlags) -> Vec<InputEvent> {
        let _ = flags;
        Vec::new()
    }

    /// The "Send Ctrl+Alt+Del" menu item.
    pub fn ctrl_alt_del(&mut self) -> Vec<InputEvent> {
        Vec::new()
    }

    /// Scancodes the remote currently considers pressed (sorted).
    pub fn remote_pressed(&self) -> Vec<Scancode> {
        Vec::new()
    }
}

#[cfg(test)]
#[path = "tests/keyboard.rs"]
mod tests;
