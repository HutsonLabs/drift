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
    let _ = (kvk, chars_ignoring_modifiers, flags);
    None
}

#[cfg(test)]
#[path = "tests/shortcuts.rs"]
mod tests;
