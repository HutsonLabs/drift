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
    match layout {
        K_KEYBOARD_ISO => KeyboardType::Iso,
        K_KEYBOARD_JIS => KeyboardType::Jis,
        _ => KeyboardType::Ansi,
    }
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    /// `PhysicalKeyboardLayoutType KBGetLayoutType(SInt16 iKeyboardType)` (HIToolbox).
    fn KBGetLayoutType(keyboard_type: i16) -> u32;
    /// `UInt8 LMGetKbdType(void)` (HIToolbox).
    fn LMGetKbdType() -> u8;
}

/// The type of the keyboard used most recently.
pub fn detect() -> KeyboardType {
    // SAFETY: both HIToolbox functions take/return plain integers and have no preconditions.
    let layout = unsafe { KBGetLayoutType(i16::from(LMGetKbdType())) };
    from_layout_type(layout)
}
