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

use std::collections::BTreeMap;

use drift_core::{InputEvent, KeyboardPrefs};

use crate::keymap::{KeyboardType, Scancode, is_modifier, kvk, scancode_for};
use crate::modifiers::{ModifierFlags, ModifierKey, ModifierSet, held_modifiers};
use crate::unicode::{KeyRoute, route_key_down, text_to_events};

/// Scancodes released around Unicode events: Shift, Alt and AltGr (Control is left alone; a
/// Control chord never produces text).
const TEXT_LEVEL_MODIFIERS: [Scancode; 4] =
    [Scancode::new(0x2A), Scancode::new(0x36), Scancode::new(0x38), Scancode::ext(0x38)];

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
    /// Modifier keys physically held, per the last flags seen.
    mods: ModifierSet,
    /// Scancode each held modifier was pressed with (indexed by `ModifierKey as usize`).
    mod_sent: [Option<Scancode>; 8],
    /// Command keys held but not yet sent (deferred).
    cmd_pending: ModifierSet,
    /// Pending Command keys whose combo went to the menu: no tap on release.
    cmd_consumed: ModifierSet,
    /// Non-modifier keys pressed as scancodes, by `kVK_*`.
    held_keys: BTreeMap<u16, Scancode>,
    /// Wire state: scancodes the remote considers pressed, with a reference count.
    pressed: BTreeMap<Scancode, u8>,
    /// Last Caps Lock state reported to the remote.
    caps: Option<bool>,
}

impl Keyboard {
    /// Creates the translator for a profile's keyboard preferences and the local keyboard type.
    pub fn new(prefs: KeyboardPrefs, keyboard: KeyboardType) -> Self {
        Self {
            prefs,
            keyboard,
            mods: ModifierSet::EMPTY,
            mod_sent: [None; 8],
            cmd_pending: ModifierSet::EMPTY,
            cmd_consumed: ModifierSet::EMPTY,
            held_keys: BTreeMap::new(),
            pressed: BTreeMap::new(),
            caps: None,
        }
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
        let mut out = Vec::new();
        self.sync_caps(flags, kvk == kvk::CAPS_LOCK, &mut out);
        self.apply_modifiers(flags, Some(kvk), &mut out);
        out
    }

    /// `keyDown:`. `composing` is `hasMarkedText` of the view's text input client.
    ///
    /// Auto-repeats are never re-sent as scancodes: g-r-d ignores repeated presses and the remote
    /// (Wayland clients) repeats held keys itself. Text-routed repeats come back through
    /// `insertText:` and are typed again.
    pub fn key_down(&mut self, kvk: u16, flags: ModifierFlags, is_repeat: bool, composing: bool) -> KeyDown {
        match route_key_down(&self.prefs, kvk, flags, composing) {
            KeyRoute::Text => KeyDown::InterpretText,
            KeyRoute::Scancode if is_repeat => KeyDown::Send(Vec::new()),
            KeyRoute::Scancode => KeyDown::Send(self.key_down_scancode(kvk, flags)),
        }
    }

    /// Sends `kvk` as a scancode press regardless of routing (fallback for
    /// `doCommandBySelector:` after [`KeyDown::InterpretText`]).
    ///
    /// Modifier changes missed while the view was not focused are applied first.
    pub fn key_down_scancode(&mut self, kvk: u16, flags: ModifierFlags) -> Vec<InputEvent> {
        let mut out = Vec::new();
        self.sync_caps(flags, false, &mut out);
        self.apply_modifiers(flags, None, &mut out);
        if is_modifier(kvk) || self.held_keys.contains_key(&kvk) {
            return out;
        }
        if let Some(sc) = scancode_for(kvk, self.keyboard, self.prefs.cmd_as) {
            self.flush_command(&mut out);
            self.press(sc, &mut out);
            self.held_keys.insert(kvk, sc);
        }
        out
    }

    /// `keyUp:`. Releases the scancode sent for `kvk`, if any.
    pub fn key_up(&mut self, kvk: u16) -> Vec<InputEvent> {
        let mut out = Vec::new();
        if let Some(sc) = self.held_keys.remove(&kvk) {
            self.release(sc, &mut out);
        }
        out
    }

    /// `insertText:` from the text input system (typed characters, dead-key and IME commits).
    /// Held Shift/Option scancodes are released around the Unicode events and pressed again
    /// afterwards, so the remote does not apply them to the injected keysyms.
    pub fn insert_text(&mut self, text: &str) -> Vec<InputEvent> {
        let text_events = text_to_events(text);
        if text_events.is_empty() {
            return text_events;
        }
        let lifted: Vec<Scancode> =
            TEXT_LEVEL_MODIFIERS.into_iter().filter(|sc| self.pressed.contains_key(sc)).collect();
        let mut out: Vec<InputEvent> = lifted.iter().map(|sc| sc.event(false)).collect();
        out.extend(text_events);
        out.extend(lifted.iter().map(|sc| sc.event(true)));
        out
    }

    /// `performKeyEquivalent:` handed an allow-listed combo to the menu: a deferred Command press
    /// is cancelled so the remote never sees the combo.
    pub fn menu_shortcut_taken(&mut self) {
        self.cmd_consumed = self.cmd_pending;
    }

    /// Call before sending a pointer button: flushes a deferred Command press (Super+drag).
    pub fn prepare_pointer_button(&mut self) -> Vec<InputEvent> {
        let mut out = Vec::new();
        self.flush_command(&mut out);
        out
    }

    /// The view lost key focus (window resigned key, app deactivated): releases
    /// exactly the scancodes the remote considers pressed and forgets all key state. A deferred
    /// Command press is dropped without being sent.
    pub fn focus_lost(&mut self) -> Vec<InputEvent> {
        let out = self.pressed.keys().map(|sc| sc.event(false)).collect();
        self.pressed.clear();
        self.held_keys.clear();
        self.mods = ModifierSet::EMPTY;
        self.mod_sent = [None; 8];
        self.cmd_pending = ModifierSet::EMPTY;
        self.cmd_consumed = ModifierSet::EMPTY;
        out
    }

    /// The view became first responder / key: resynchronises lock toggles and presses the
    /// modifiers held right now.
    pub fn focus_gained(&mut self, flags: ModifierFlags) -> Vec<InputEvent> {
        let mut out = Vec::new();
        self.sync_caps(flags, true, &mut out);
        self.apply_modifiers(flags, None, &mut out);
        out
    }

    /// The "Send Ctrl+Alt+Del" menu item.
    pub fn ctrl_alt_del(&mut self) -> Vec<InputEvent> {
        let seq = [Scancode::new(0x1D), Scancode::new(0x38), Scancode::ext(0x53)];
        let mut out = Vec::new();
        for sc in seq {
            self.press(sc, &mut out);
        }
        for sc in seq.into_iter().rev() {
            self.release(sc, &mut out);
        }
        out
    }

    /// Scancodes the remote currently considers pressed (sorted).
    pub fn remote_pressed(&self) -> Vec<Scancode> {
        self.pressed.keys().copied().collect()
    }

    // --- internals ---------------------------------------------------------------------------

    /// Emits `SyncToggles` when the Caps Lock state changed (or always with `force`).
    fn sync_caps(&mut self, flags: ModifierFlags, force: bool, out: &mut Vec<InputEvent>) {
        let caps = flags.caps_lock();
        let changed = self.caps.is_some_and(|c| c != caps);
        if force || changed {
            // Num Lock stays on: Mac keypads always type digits.
            out.push(InputEvent::SyncToggles { caps, num: true });
        }
        self.caps = Some(caps);
    }

    /// Diffs the held modifier set; releases first (last class first), then presses.
    fn apply_modifiers(&mut self, flags: ModifierFlags, changed: Option<u16>, out: &mut Vec<InputEvent>) {
        let new = held_modifiers(flags, changed, self.mods);
        for key in ModifierKey::ALL.into_iter().rev() {
            if self.mods.contains(key) && !new.contains(key) {
                self.release_modifier(key, out);
            }
        }
        for key in ModifierKey::ALL {
            if !self.mods.contains(key) && new.contains(key) {
                self.press_modifier(key, out);
            }
        }
        self.mods = new;
    }

    fn press_modifier(&mut self, key: ModifierKey, out: &mut Vec<InputEvent>) {
        if key.is_command() {
            self.cmd_pending.insert(key);
            self.cmd_consumed.remove(key);
        } else if let Some(sc) = scancode_for(key.kvk(), self.keyboard, self.prefs.cmd_as) {
            self.press(sc, out);
            self.mod_sent[key as usize] = Some(sc);
        }
    }

    fn release_modifier(&mut self, key: ModifierKey, out: &mut Vec<InputEvent>) {
        if self.cmd_pending.contains(key) {
            self.cmd_pending.remove(key);
            let consumed = self.cmd_consumed.contains(key);
            self.cmd_consumed.remove(key);
            if !consumed && let Some(sc) = scancode_for(key.kvk(), self.keyboard, self.prefs.cmd_as) {
                // A lone tap (e.g. Super alone opens the GNOME overview).
                self.press(sc, out);
                self.release(sc, out);
            }
        } else if let Some(sc) = self.mod_sent[key as usize].take() {
            self.release(sc, out);
        }
    }

    /// Sends any deferred Command press.
    fn flush_command(&mut self, out: &mut Vec<InputEvent>) {
        for key in self.cmd_pending.iter() {
            if let Some(sc) = scancode_for(key.kvk(), self.keyboard, self.prefs.cmd_as) {
                self.press(sc, out);
                self.mod_sent[key as usize] = Some(sc);
            }
        }
        self.cmd_pending = ModifierSet::EMPTY;
        self.cmd_consumed = ModifierSet::EMPTY;
    }

    fn press(&mut self, sc: Scancode, out: &mut Vec<InputEvent>) {
        let count = self.pressed.entry(sc).or_insert(0);
        if *count == 0 {
            out.push(sc.event(true));
        }
        *count = count.saturating_add(1);
    }

    fn release(&mut self, sc: Scancode, out: &mut Vec<InputEvent>) {
        if let Some(count) = self.pressed.get_mut(&sc) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.pressed.remove(&sc);
                out.push(sc.event(false));
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/keyboard.rs"]
mod tests;
