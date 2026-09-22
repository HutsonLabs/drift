//! Keyboard type detection mapping.
#![allow(missing_docs)]

use drift_input::KeyboardType;
use drift_macos::keyboard_type::{K_KEYBOARD_ANSI, K_KEYBOARD_ISO, K_KEYBOARD_JIS, detect, from_layout_type};

#[test]
fn four_char_codes() {
    assert_eq!(K_KEYBOARD_ANSI, 0x414E_5349);
    assert_eq!(K_KEYBOARD_ISO, 0x4953_4F20);
    assert_eq!(K_KEYBOARD_JIS, 0x4A49_5320);
}

#[test]
fn layout_type_table() {
    assert_eq!(from_layout_type(K_KEYBOARD_ANSI), KeyboardType::Ansi);
    assert_eq!(from_layout_type(K_KEYBOARD_ISO), KeyboardType::Iso);
    assert_eq!(from_layout_type(K_KEYBOARD_JIS), KeyboardType::Jis);
    assert_eq!(from_layout_type(0), KeyboardType::Ansi);
    assert_eq!(from_layout_type(u32::MAX), KeyboardType::Ansi);
}

#[test]
fn detect_returns_a_type_without_crashing() {
    let t = detect();
    assert!(matches!(t, KeyboardType::Ansi | KeyboardType::Iso | KeyboardType::Jis));
}
