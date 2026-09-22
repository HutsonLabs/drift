//! Unicode typing-mode routing. Owned by task **M2-2**.
//!
//! With `KeyboardPrefs::type_with_mac_layout`, keys that produce text and are pressed without
//! Control/Command go through AppKit's text input system (`interpretKeyEvents:` →
//! `NSTextInputClient insertText:`), so dead keys and IMEs work, and the resulting text is sent
//! as layout-independent Unicode events. Chords (Control/Command held) and non-text keys (arrows,
//! Return, F-keys, …) always go as scancodes.

use drift_core::{InputEvent, KeyboardPrefs};

use crate::keymap::is_text_key;
use crate::modifiers::ModifierFlags;

/// Where a `keyDown:` goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyRoute {
    /// Positional scancode (the remote layout decides the character).
    Scancode,
    /// Hand the event to `interpretKeyEvents:`; the text arrives via `insertText:` and is sent
    /// with [`crate::keyboard::Keyboard::insert_text`]. Nothing is sent for the key itself.
    Text,
}

/// Routes one `keyDown:`.
///
/// * `type_with_mac_layout` off → always [`KeyRoute::Scancode`].
/// * Control or Command held → [`KeyRoute::Scancode`] (chords always use scancodes).
/// * An IME composition is in progress (`composing`, i.e. `hasMarkedText`) → [`KeyRoute::Text`],
///   so Return/arrows/Delete edit the composition instead of reaching the remote.
/// * Otherwise [`KeyRoute::Text`] for text keys ([`crate::keymap::is_text_key`]) and
///   [`KeyRoute::Scancode`] for everything else.
pub fn route_key_down(prefs: &KeyboardPrefs, kvk: u16, flags: ModifierFlags, composing: bool) -> KeyRoute {
    if !prefs.type_with_mac_layout || flags.command() || flags.control() {
        KeyRoute::Scancode
    } else if composing || is_text_key(kvk) {
        KeyRoute::Text
    } else {
        KeyRoute::Scancode
    }
}

/// Converts committed text into Unicode events: for every UTF-16 code unit, a press immediately
/// followed by its release (g-r-d de-duplicates pressed keysyms, so presses must not overlap).
/// Characters outside the BMP become two units (a surrogate pair). Control characters and the
/// AppKit function-key private-use range (`U+F700..=U+F8FF`) are dropped.
pub fn text_to_events(text: &str) -> Vec<InputEvent> {
    let mut events = Vec::with_capacity(text.len() * 2);
    let mut buf = [0u16; 2];
    for ch in text.chars().filter(|c| !c.is_control() && !('\u{F700}'..='\u{F8FF}').contains(c)) {
        for &unit in ch.encode_utf16(&mut buf).iter() {
            events.push(InputEvent::Unicode { ch: unit, down: true });
            events.push(InputEvent::Unicode { ch: unit, down: false });
        }
    }
    events
}

#[cfg(test)]
#[path = "tests/unicode.rs"]
mod tests;
