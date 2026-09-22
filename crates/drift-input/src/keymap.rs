//! `kVK_*` → set-1 scancode mapping. Owned by task **M2-1**.
//!
//! macOS reports *virtual key codes* (`NSEvent.keyCode`, the Carbon `kVK_*` constants from
//! `HIToolbox/Events.h`). They are positional, like PC scancodes, so the mapping is a fixed
//! table. The remote XKB layout then decides which character a scancode produces (plan §1.5).
//!
//! Two keyboard-type specifics (see `docs/adr/M2-1-keyboard-translation.md`):
//! - **ISO**: macOS reports the key left of `1` (`§`/`^`) as `kVK_ISO_Section` and the key right
//!   of left Shift (`<`/`` ` ``) as `kVK_ANSI_Grave`. On ANSI/JIS keyboards `kVK_ANSI_Grave` is the
//!   key left of `1`. [`scancode_for`] undoes the swap so the *physical position* is preserved.
//! - **JIS**: `¥`, `_`/`ろ`, keypad `,`, `英数` and `かな` map to the PC Japanese keys.

use drift_core::{CmdAs, InputEvent};

/// Carbon virtual key codes (`kVK_*` from `HIToolbox/Events.h`), named without the `kVK_`
/// prefix.
#[allow(missing_docs)]
pub mod kvk {
    // --- ANSI (layout-dependent, named for the US-ANSI legend) ---
    pub const ANSI_A: u16 = 0x00;
    pub const ANSI_S: u16 = 0x01;
    pub const ANSI_D: u16 = 0x02;
    pub const ANSI_F: u16 = 0x03;
    pub const ANSI_H: u16 = 0x04;
    pub const ANSI_G: u16 = 0x05;
    pub const ANSI_Z: u16 = 0x06;
    pub const ANSI_X: u16 = 0x07;
    pub const ANSI_C: u16 = 0x08;
    pub const ANSI_V: u16 = 0x09;
    pub const ANSI_B: u16 = 0x0B;
    pub const ANSI_Q: u16 = 0x0C;
    pub const ANSI_W: u16 = 0x0D;
    pub const ANSI_E: u16 = 0x0E;
    pub const ANSI_R: u16 = 0x0F;
    pub const ANSI_Y: u16 = 0x10;
    pub const ANSI_T: u16 = 0x11;
    pub const ANSI_1: u16 = 0x12;
    pub const ANSI_2: u16 = 0x13;
    pub const ANSI_3: u16 = 0x14;
    pub const ANSI_4: u16 = 0x15;
    pub const ANSI_6: u16 = 0x16;
    pub const ANSI_5: u16 = 0x17;
    pub const ANSI_EQUAL: u16 = 0x18;
    pub const ANSI_9: u16 = 0x19;
    pub const ANSI_7: u16 = 0x1A;
    pub const ANSI_MINUS: u16 = 0x1B;
    pub const ANSI_8: u16 = 0x1C;
    pub const ANSI_0: u16 = 0x1D;
    pub const ANSI_RIGHT_BRACKET: u16 = 0x1E;
    pub const ANSI_O: u16 = 0x1F;
    pub const ANSI_U: u16 = 0x20;
    pub const ANSI_LEFT_BRACKET: u16 = 0x21;
    pub const ANSI_I: u16 = 0x22;
    pub const ANSI_P: u16 = 0x23;
    pub const ANSI_L: u16 = 0x25;
    pub const ANSI_J: u16 = 0x26;
    pub const ANSI_QUOTE: u16 = 0x27;
    pub const ANSI_K: u16 = 0x28;
    pub const ANSI_SEMICOLON: u16 = 0x29;
    pub const ANSI_BACKSLASH: u16 = 0x2A;
    pub const ANSI_COMMA: u16 = 0x2B;
    pub const ANSI_SLASH: u16 = 0x2C;
    pub const ANSI_N: u16 = 0x2D;
    pub const ANSI_M: u16 = 0x2E;
    pub const ANSI_PERIOD: u16 = 0x2F;
    pub const ANSI_GRAVE: u16 = 0x32;
    pub const ANSI_KEYPAD_DECIMAL: u16 = 0x41;
    pub const ANSI_KEYPAD_MULTIPLY: u16 = 0x43;
    pub const ANSI_KEYPAD_PLUS: u16 = 0x45;
    pub const ANSI_KEYPAD_CLEAR: u16 = 0x47;
    pub const ANSI_KEYPAD_DIVIDE: u16 = 0x4B;
    pub const ANSI_KEYPAD_ENTER: u16 = 0x4C;
    pub const ANSI_KEYPAD_MINUS: u16 = 0x4E;
    pub const ANSI_KEYPAD_EQUALS: u16 = 0x51;
    pub const ANSI_KEYPAD_0: u16 = 0x52;
    pub const ANSI_KEYPAD_1: u16 = 0x53;
    pub const ANSI_KEYPAD_2: u16 = 0x54;
    pub const ANSI_KEYPAD_3: u16 = 0x55;
    pub const ANSI_KEYPAD_4: u16 = 0x56;
    pub const ANSI_KEYPAD_5: u16 = 0x57;
    pub const ANSI_KEYPAD_6: u16 = 0x58;
    pub const ANSI_KEYPAD_7: u16 = 0x59;
    pub const ANSI_KEYPAD_8: u16 = 0x5B;
    pub const ANSI_KEYPAD_9: u16 = 0x5C;

    // --- layout-independent ---
    pub const RETURN: u16 = 0x24;
    pub const TAB: u16 = 0x30;
    pub const SPACE: u16 = 0x31;
    pub const DELETE: u16 = 0x33;
    pub const ESCAPE: u16 = 0x35;
    pub const RIGHT_COMMAND: u16 = 0x36;
    pub const COMMAND: u16 = 0x37;
    pub const SHIFT: u16 = 0x38;
    pub const CAPS_LOCK: u16 = 0x39;
    pub const OPTION: u16 = 0x3A;
    pub const CONTROL: u16 = 0x3B;
    pub const RIGHT_SHIFT: u16 = 0x3C;
    pub const RIGHT_OPTION: u16 = 0x3D;
    pub const RIGHT_CONTROL: u16 = 0x3E;
    pub const FUNCTION: u16 = 0x3F;
    pub const F17: u16 = 0x40;
    pub const VOLUME_UP: u16 = 0x48;
    pub const VOLUME_DOWN: u16 = 0x49;
    pub const MUTE: u16 = 0x4A;
    pub const F18: u16 = 0x4F;
    pub const F19: u16 = 0x50;
    pub const F20: u16 = 0x5A;
    pub const F5: u16 = 0x60;
    pub const F6: u16 = 0x61;
    pub const F7: u16 = 0x62;
    pub const F3: u16 = 0x63;
    pub const F8: u16 = 0x64;
    pub const F9: u16 = 0x65;
    pub const F11: u16 = 0x67;
    pub const F13: u16 = 0x69;
    pub const F16: u16 = 0x6A;
    pub const F14: u16 = 0x6B;
    pub const F10: u16 = 0x6D;
    pub const CONTEXTUAL_MENU: u16 = 0x6E;
    pub const F12: u16 = 0x6F;
    pub const F15: u16 = 0x71;
    pub const HELP: u16 = 0x72;
    pub const HOME: u16 = 0x73;
    pub const PAGE_UP: u16 = 0x74;
    pub const FORWARD_DELETE: u16 = 0x75;
    pub const F4: u16 = 0x76;
    pub const END: u16 = 0x77;
    pub const F2: u16 = 0x78;
    pub const PAGE_DOWN: u16 = 0x79;
    pub const F1: u16 = 0x7A;
    pub const LEFT_ARROW: u16 = 0x7B;
    pub const RIGHT_ARROW: u16 = 0x7C;
    pub const DOWN_ARROW: u16 = 0x7D;
    pub const UP_ARROW: u16 = 0x7E;

    // --- ISO ---
    pub const ISO_SECTION: u16 = 0x0A;

    // --- JIS ---
    pub const JIS_YEN: u16 = 0x5D;
    pub const JIS_UNDERSCORE: u16 = 0x5E;
    pub const JIS_KEYPAD_COMMA: u16 = 0x5F;
    pub const JIS_EISU: u16 = 0x66;
    pub const JIS_KANA: u16 = 0x68;
}

/// Every `kVK_*` constant with its Carbon name, for exhaustive table tests and diagnostics.
pub const ALL_KVK: &[(&str, u16)] = &[
    ("kVK_ANSI_A", kvk::ANSI_A),
    ("kVK_ANSI_S", kvk::ANSI_S),
    ("kVK_ANSI_D", kvk::ANSI_D),
    ("kVK_ANSI_F", kvk::ANSI_F),
    ("kVK_ANSI_H", kvk::ANSI_H),
    ("kVK_ANSI_G", kvk::ANSI_G),
    ("kVK_ANSI_Z", kvk::ANSI_Z),
    ("kVK_ANSI_X", kvk::ANSI_X),
    ("kVK_ANSI_C", kvk::ANSI_C),
    ("kVK_ANSI_V", kvk::ANSI_V),
    ("kVK_ANSI_B", kvk::ANSI_B),
    ("kVK_ANSI_Q", kvk::ANSI_Q),
    ("kVK_ANSI_W", kvk::ANSI_W),
    ("kVK_ANSI_E", kvk::ANSI_E),
    ("kVK_ANSI_R", kvk::ANSI_R),
    ("kVK_ANSI_Y", kvk::ANSI_Y),
    ("kVK_ANSI_T", kvk::ANSI_T),
    ("kVK_ANSI_1", kvk::ANSI_1),
    ("kVK_ANSI_2", kvk::ANSI_2),
    ("kVK_ANSI_3", kvk::ANSI_3),
    ("kVK_ANSI_4", kvk::ANSI_4),
    ("kVK_ANSI_6", kvk::ANSI_6),
    ("kVK_ANSI_5", kvk::ANSI_5),
    ("kVK_ANSI_Equal", kvk::ANSI_EQUAL),
    ("kVK_ANSI_9", kvk::ANSI_9),
    ("kVK_ANSI_7", kvk::ANSI_7),
    ("kVK_ANSI_Minus", kvk::ANSI_MINUS),
    ("kVK_ANSI_8", kvk::ANSI_8),
    ("kVK_ANSI_0", kvk::ANSI_0),
    ("kVK_ANSI_RightBracket", kvk::ANSI_RIGHT_BRACKET),
    ("kVK_ANSI_O", kvk::ANSI_O),
    ("kVK_ANSI_U", kvk::ANSI_U),
    ("kVK_ANSI_LeftBracket", kvk::ANSI_LEFT_BRACKET),
    ("kVK_ANSI_I", kvk::ANSI_I),
    ("kVK_ANSI_P", kvk::ANSI_P),
    ("kVK_ANSI_L", kvk::ANSI_L),
    ("kVK_ANSI_J", kvk::ANSI_J),
    ("kVK_ANSI_Quote", kvk::ANSI_QUOTE),
    ("kVK_ANSI_K", kvk::ANSI_K),
    ("kVK_ANSI_Semicolon", kvk::ANSI_SEMICOLON),
    ("kVK_ANSI_Backslash", kvk::ANSI_BACKSLASH),
    ("kVK_ANSI_Comma", kvk::ANSI_COMMA),
    ("kVK_ANSI_Slash", kvk::ANSI_SLASH),
    ("kVK_ANSI_N", kvk::ANSI_N),
    ("kVK_ANSI_M", kvk::ANSI_M),
    ("kVK_ANSI_Period", kvk::ANSI_PERIOD),
    ("kVK_ANSI_Grave", kvk::ANSI_GRAVE),
    ("kVK_ANSI_KeypadDecimal", kvk::ANSI_KEYPAD_DECIMAL),
    ("kVK_ANSI_KeypadMultiply", kvk::ANSI_KEYPAD_MULTIPLY),
    ("kVK_ANSI_KeypadPlus", kvk::ANSI_KEYPAD_PLUS),
    ("kVK_ANSI_KeypadClear", kvk::ANSI_KEYPAD_CLEAR),
    ("kVK_ANSI_KeypadDivide", kvk::ANSI_KEYPAD_DIVIDE),
    ("kVK_ANSI_KeypadEnter", kvk::ANSI_KEYPAD_ENTER),
    ("kVK_ANSI_KeypadMinus", kvk::ANSI_KEYPAD_MINUS),
    ("kVK_ANSI_KeypadEquals", kvk::ANSI_KEYPAD_EQUALS),
    ("kVK_ANSI_Keypad0", kvk::ANSI_KEYPAD_0),
    ("kVK_ANSI_Keypad1", kvk::ANSI_KEYPAD_1),
    ("kVK_ANSI_Keypad2", kvk::ANSI_KEYPAD_2),
    ("kVK_ANSI_Keypad3", kvk::ANSI_KEYPAD_3),
    ("kVK_ANSI_Keypad4", kvk::ANSI_KEYPAD_4),
    ("kVK_ANSI_Keypad5", kvk::ANSI_KEYPAD_5),
    ("kVK_ANSI_Keypad6", kvk::ANSI_KEYPAD_6),
    ("kVK_ANSI_Keypad7", kvk::ANSI_KEYPAD_7),
    ("kVK_ANSI_Keypad8", kvk::ANSI_KEYPAD_8),
    ("kVK_ANSI_Keypad9", kvk::ANSI_KEYPAD_9),
    ("kVK_Return", kvk::RETURN),
    ("kVK_Tab", kvk::TAB),
    ("kVK_Space", kvk::SPACE),
    ("kVK_Delete", kvk::DELETE),
    ("kVK_Escape", kvk::ESCAPE),
    ("kVK_RightCommand", kvk::RIGHT_COMMAND),
    ("kVK_Command", kvk::COMMAND),
    ("kVK_Shift", kvk::SHIFT),
    ("kVK_CapsLock", kvk::CAPS_LOCK),
    ("kVK_Option", kvk::OPTION),
    ("kVK_Control", kvk::CONTROL),
    ("kVK_RightShift", kvk::RIGHT_SHIFT),
    ("kVK_RightOption", kvk::RIGHT_OPTION),
    ("kVK_RightControl", kvk::RIGHT_CONTROL),
    ("kVK_Function", kvk::FUNCTION),
    ("kVK_F17", kvk::F17),
    ("kVK_VolumeUp", kvk::VOLUME_UP),
    ("kVK_VolumeDown", kvk::VOLUME_DOWN),
    ("kVK_Mute", kvk::MUTE),
    ("kVK_F18", kvk::F18),
    ("kVK_F19", kvk::F19),
    ("kVK_F20", kvk::F20),
    ("kVK_F5", kvk::F5),
    ("kVK_F6", kvk::F6),
    ("kVK_F7", kvk::F7),
    ("kVK_F3", kvk::F3),
    ("kVK_F8", kvk::F8),
    ("kVK_F9", kvk::F9),
    ("kVK_F11", kvk::F11),
    ("kVK_F13", kvk::F13),
    ("kVK_F16", kvk::F16),
    ("kVK_F14", kvk::F14),
    ("kVK_F10", kvk::F10),
    ("kVK_ContextualMenu", kvk::CONTEXTUAL_MENU),
    ("kVK_F12", kvk::F12),
    ("kVK_F15", kvk::F15),
    ("kVK_Help", kvk::HELP),
    ("kVK_Home", kvk::HOME),
    ("kVK_PageUp", kvk::PAGE_UP),
    ("kVK_ForwardDelete", kvk::FORWARD_DELETE),
    ("kVK_F4", kvk::F4),
    ("kVK_End", kvk::END),
    ("kVK_F2", kvk::F2),
    ("kVK_PageDown", kvk::PAGE_DOWN),
    ("kVK_F1", kvk::F1),
    ("kVK_LeftArrow", kvk::LEFT_ARROW),
    ("kVK_RightArrow", kvk::RIGHT_ARROW),
    ("kVK_DownArrow", kvk::DOWN_ARROW),
    ("kVK_UpArrow", kvk::UP_ARROW),
    ("kVK_ISO_Section", kvk::ISO_SECTION),
    ("kVK_JIS_Yen", kvk::JIS_YEN),
    ("kVK_JIS_Underscore", kvk::JIS_UNDERSCORE),
    ("kVK_JIS_KeypadComma", kvk::JIS_KEYPAD_COMMA),
    ("kVK_JIS_Eisu", kvk::JIS_EISU),
    ("kVK_JIS_Kana", kvk::JIS_KANA),
];

/// A PC/AT set-1 scancode as carried by RDP fast-path keyboard events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Scancode {
    /// Make code without the `0xE0` prefix.
    pub code: u8,
    /// `0xE0`-prefixed ("extended") key.
    pub extended: bool,
}

impl Scancode {
    /// A non-extended scancode.
    pub const fn new(code: u8) -> Self {
        Self { code, extended: false }
    }

    /// An extended (`0xE0`) scancode.
    pub const fn ext(code: u8) -> Self {
        Self { code, extended: true }
    }

    /// The press/release [`InputEvent`] for this scancode.
    pub const fn event(self, down: bool) -> InputEvent {
        InputEvent::Key { scancode: self.code, extended: self.extended, down }
    }
}

/// Physical keyboard type, as reported by `KBGetLayoutType(LMGetKbdType())`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum KeyboardType {
    /// US-style ANSI keyboard.
    #[default]
    Ansi,
    /// European ISO keyboard (extra key right of left Shift, `§` left of `1`).
    Iso,
    /// Japanese JIS keyboard.
    Jis,
}

impl KeyboardType {
    /// `kKeyboardJIS` (`'JIS '`).
    pub const LAYOUT_JIS: u32 = u32::from_be_bytes(*b"JIS ");
    /// `kKeyboardANSI` (`'ANSI'`).
    pub const LAYOUT_ANSI: u32 = u32::from_be_bytes(*b"ANSI");
    /// `kKeyboardISO` (`'ISO '`).
    pub const LAYOUT_ISO: u32 = u32::from_be_bytes(*b"ISO ");

    /// Maps the `PhysicalKeyboardLayoutType` returned by `KBGetLayoutType`; unknown values
    /// are treated as ANSI.
    pub const fn from_layout_type(layout_type: u32) -> Self {
        let _ = layout_type;
        Self::Ansi
    }
}

/// Maps a `kVK_*` code to its set-1 scancode.
///
/// Returns `None` for keys with no PC equivalent (`fn`, keypad Clear) and unknown codes.
/// The Command keys map according to `cmd_as`: `Super` → left/right Windows key (`0x5B`/`0x5C`
/// extended), `Ctrl` → left/right Control (`0x1D` / `0x1D` extended).
pub fn scancode_for(kvk: u16, keyboard: KeyboardType, cmd_as: CmdAs) -> Option<Scancode> {
    let _ = (kvk, keyboard, cmd_as);
    None
}

/// `true` for the modifier keys that arrive through `flagsChanged:` (Shift, Control, Option,
/// Command on either side, Caps Lock and `fn`).
pub const fn is_modifier(kvk: u16) -> bool {
    let _ = kvk;
    false
}

/// `true` for keys whose job is to produce text (letters, digits, punctuation, Space, keypad
/// digits/operators and the ISO/JIS character keys). Used by the "Type using Mac layout"
/// routing: everything else (arrows, Return, Tab, Delete, F-keys, …) always goes as a scancode.
pub const fn is_text_key(kvk: u16) -> bool {
    let _ = kvk;
    false
}

#[cfg(test)]
#[path = "tests/keymap.rs"]
mod tests;
