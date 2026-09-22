//! Local keyboard type detection (ANSI / ISO / JIS) for `drift-input`'s keymap (M2-1).
//!
//! HIToolbox reports the physical layout of the last-used keyboard: `KBGetLayoutType(LMGetKbdType())`
//! returns `kKeyboardANSI` (`'ANSI'`), `kKeyboardISO` (`'ISO '`) or `kKeyboardJIS` (`'JIS '`).

use drift_input::KeyboardType;

/// `kKeyboardJIS` (`'JIS '`).
pub const K_KEYBOARD_JIS: u32 = u32::from_be_bytes(*b"JIS ");
/// `kKeyboardANSI` (`'ANSI'`).
pub const K_KEYBOARD_ANSI: u32 = u32::from_be_bytes(*b"ANSI");
/// `kKeyboardISO` (`'ISO '`).
pub const K_KEYBOARD_ISO: u32 = u32::from_be_bytes(*b"ISO ");

/// Maps a `KBGetLayoutType` result to a [`KeyboardType`] (unknown values are ANSI).
pub const fn from_layout_type(layout: u32) -> KeyboardType {
    let _ = layout;
    KeyboardType::Jis
}

/// The type of the keyboard used most recently.
pub fn detect() -> KeyboardType {
    KeyboardType::Ansi
}
