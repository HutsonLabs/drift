//! Shortcut allow-list for `performKeyEquivalent:`. Owned by task **M2-4**.
//!
//! AppKit offers every Command/Control combo to `performKeyEquivalent:` before the menu. The
//! `RemoteView` claims (returns `YES` for, and sends to the remote) every combo **except** this
//! allow-list, which goes to Drift's menu:
//!
//! | Combo | Action |
//! |---|---|
//! | Cmd+T | new tab |
//! | Cmd+W | close tab |
//! | Cmd+Q | quit |
//! | Cmd+1 … Cmd+9 | select tab n |
//! | Cmd+Shift+[ / Cmd+Shift+] | previous / next tab |
//! | Cmd+` | cycle windows |
//!
//! When a combo goes to the menu, `drift-macos` calls
//! [`crate::keyboard::Keyboard::menu_shortcut_taken`] so the deferred Command press is dropped
//! and the remote never sees the combo.
//!
//! Matching follows AppKit menu matching: by `charactersIgnoringModifiers` (so Dvorak's Cmd+T is
//! the key labelled T), falling back to the ANSI key position when the layout produces no ASCII
//! character (Cyrillic, Greek, …). Tab digits also match by position (AZERTY digits need Shift).

use crate::keymap::kvk;
use crate::modifiers::ModifierFlags;

/// An allow-listed shortcut handled by Drift's menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuShortcut {
    /// Cmd+T.
    NewTab,
    /// Cmd+W.
    CloseTab,
    /// Cmd+Q.
    Quit,
    /// Cmd+1 … Cmd+9 (1-based).
    SelectTab(u8),
    /// Cmd+Shift+[.
    PreviousTab,
    /// Cmd+Shift+].
    NextTab,
    /// Cmd+`.
    CycleWindows,
}

/// Classifies a `performKeyEquivalent:` event.
///
/// Returns `Some` when the combo is allow-listed (return `NO` and let the menu handle it), `None`
/// when the remote gets it (return `YES` and route the event as a key press).
pub fn menu_shortcut(kvk: u16, chars_ignoring_modifiers: &str, flags: ModifierFlags) -> Option<MenuShortcut> {
    if !flags.command() || flags.control() || flags.option() {
        return None;
    }
    let mut chars = chars_ignoring_modifiers.chars();
    let typed = match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii() => Some(c.to_ascii_lowercase()),
        _ => None,
    };
    // Non-ASCII layouts (or no characters at all): match by ANSI position, like AppKit.
    let key = typed.or_else(|| ansi_position_char(kvk));
    if flags.shift() {
        return match key {
            Some('[' | '{') => Some(MenuShortcut::PreviousTab),
            Some(']' | '}') => Some(MenuShortcut::NextTab),
            _ => None,
        };
    }
    if let Some(n) = tab_digit(kvk) {
        return Some(MenuShortcut::SelectTab(n));
    }
    match key? {
        't' => Some(MenuShortcut::NewTab),
        'w' => Some(MenuShortcut::CloseTab),
        'q' => Some(MenuShortcut::Quit),
        '`' => Some(MenuShortcut::CycleWindows),
        // `c` is an ASCII digit 1..=9, so the subtraction cannot underflow.
        c @ '1'..='9' => Some(MenuShortcut::SelectTab(c as u8 - b'0')),
        _ => None,
    }
}

/// The US-ANSI character at an allow-listed key position.
fn ansi_position_char(code: u16) -> Option<char> {
    Some(match code {
        kvk::ANSI_T => 't',
        kvk::ANSI_W => 'w',
        kvk::ANSI_Q => 'q',
        kvk::ANSI_LEFT_BRACKET => '[',
        kvk::ANSI_RIGHT_BRACKET => ']',
        kvk::ANSI_GRAVE => '`',
        _ => return None,
    })
}

/// Tab number for the digit-row keys 1..9 (by position).
fn tab_digit(code: u16) -> Option<u8> {
    Some(match code {
        kvk::ANSI_1 => 1,
        kvk::ANSI_2 => 2,
        kvk::ANSI_3 => 3,
        kvk::ANSI_4 => 4,
        kvk::ANSI_5 => 5,
        kvk::ANSI_6 => 6,
        kvk::ANSI_7 => 7,
        kvk::ANSI_8 => 8,
        kvk::ANSI_9 => 9,
        _ => return None,
    })
}

#[cfg(test)]
#[path = "tests/shortcuts.rs"]
mod tests;
