//! M2-2 routing table: `é` via dead key, `ß`, `Ctrl+C`, `Cmd+V`, emoji.

use super::*;
use crate::keyboard::{KeyDown, Keyboard};
use crate::keymap::{KeyboardType, kvk};
use drift_core::CmdAs;

use crate::modifiers::ModifierFlags as F;

const L_OPT: u64 = F::OPTION | F::DEVICE_LEFT_OPTION;
const L_CTRL: u64 = F::CONTROL | F::DEVICE_LEFT_CONTROL;
const L_CMD: u64 = F::COMMAND | F::DEVICE_LEFT_COMMAND;
const L_SHIFT: u64 = F::SHIFT | F::DEVICE_LEFT_SHIFT;

fn mac() -> KeyboardPrefs {
    KeyboardPrefs { cmd_as: CmdAs::Super, type_with_mac_layout: true }
}

fn uni(ch: u16) -> [InputEvent; 2] {
    [InputEvent::Unicode { ch, down: true }, InputEvent::Unicode { ch, down: false }]
}

fn key(code: u8, extended: bool, down: bool) -> InputEvent {
    InputEvent::Key { scancode: code, extended, down }
}

#[test]
fn routing_table() {
    let off = KeyboardPrefs::default();
    let cases: &[(&KeyboardPrefs, u16, u64, bool, KeyRoute)] = &[
        // scancode mode: everything positional
        (&off, kvk::ANSI_A, 0, false, KeyRoute::Scancode),
        (&off, kvk::ANSI_E, L_OPT, true, KeyRoute::Scancode),
        // mac layout: printable keys without Ctrl/Cmd → text system
        (&mac(), kvk::ANSI_A, 0, false, KeyRoute::Text),
        (&mac(), kvk::ANSI_A, L_SHIFT, false, KeyRoute::Text),
        (&mac(), kvk::ANSI_E, L_OPT, false, KeyRoute::Text), // dead key ´
        (&mac(), kvk::ANSI_S, L_OPT, false, KeyRoute::Text), // ß
        (&mac(), kvk::SPACE, 0, false, KeyRoute::Text),
        (&mac(), kvk::ANSI_KEYPAD_7, F::NUMERIC_PAD, false, KeyRoute::Text),
        (&mac(), kvk::ISO_SECTION, 0, false, KeyRoute::Text),
        // chords always scancodes
        (&mac(), kvk::ANSI_C, L_CTRL, false, KeyRoute::Scancode),
        (&mac(), kvk::ANSI_V, L_CMD, false, KeyRoute::Scancode),
        (&mac(), kvk::ANSI_V, L_CMD, true, KeyRoute::Scancode),
        // non-text keys are scancodes …
        (&mac(), kvk::RETURN, 0, false, KeyRoute::Scancode),
        (&mac(), kvk::LEFT_ARROW, 0, false, KeyRoute::Scancode),
        (&mac(), kvk::DELETE, 0, false, KeyRoute::Scancode),
        (&mac(), kvk::F5, 0, false, KeyRoute::Scancode),
        (&mac(), kvk::ESCAPE, 0, false, KeyRoute::Scancode),
        // … unless an IME composition is being edited
        (&mac(), kvk::RETURN, 0, true, KeyRoute::Text),
        (&mac(), kvk::DELETE, 0, true, KeyRoute::Text),
        (&mac(), kvk::JIS_KANA, 0, true, KeyRoute::Text),
    ];
    for (prefs, code, flags, composing, expected) in cases {
        assert_eq!(
            route_key_down(prefs, *code, F(*flags), *composing),
            *expected,
            "kvk 0x{code:02X} flags {flags:#x} composing {composing}"
        );
    }
}

#[test]
fn text_to_events_pairs_each_utf16_unit() {
    assert_eq!(text_to_events("a"), uni(0x61).to_vec());
    assert_eq!(text_to_events("ß"), uni(0xDF).to_vec());
    // emoji: surrogate pair sent as two UTF-16 units
    let mut want = uni(0xD83D).to_vec();
    want.extend(uni(0xDE00));
    assert_eq!(text_to_events("😀"), want);
    // `ll`: press/release per unit, never two overlapping presses
    let mut want = uni(0x6C).to_vec();
    want.extend(uni(0x6C));
    assert_eq!(text_to_events("ll"), want);
    // control characters and AppKit function-key code points are dropped
    assert_eq!(text_to_events("\r\t\u{7f}\u{F700}\u{F8FF}"), vec![]);
    assert_eq!(text_to_events(""), vec![]);
}

#[test]
fn e_acute_via_dead_key() {
    let mut k = Keyboard::new(mac(), KeyboardType::Ansi);
    // Option down → Alt down on the remote (modifiers stay positional)
    assert_eq!(k.flags_changed(kvk::OPTION, F(L_OPT)), vec![key(0x38, false, true)]);
    // Option+E → text system (produces marked text ´, nothing sent)
    assert_eq!(k.key_down(kvk::ANSI_E, F(L_OPT), false, false), KeyDown::InterpretText);
    assert_eq!(k.key_up(kvk::ANSI_E), vec![]);
    assert_eq!(k.flags_changed(kvk::OPTION, F(0)), vec![key(0x38, false, false)]);
    // E while composing → text system, which commits é
    assert_eq!(k.key_down(kvk::ANSI_E, F(0), false, true), KeyDown::InterpretText);
    assert_eq!(k.insert_text("é"), uni(0xE9).to_vec());
    assert_eq!(k.key_up(kvk::ANSI_E), vec![]);
    assert!(k.remote_pressed().is_empty());
}

#[test]
fn sharp_s_releases_option_around_the_unicode_event() {
    let mut k = Keyboard::new(mac(), KeyboardType::Ansi);
    k.flags_changed(kvk::OPTION, F(L_OPT));
    assert_eq!(k.key_down(kvk::ANSI_S, F(L_OPT), false, false), KeyDown::InterpretText);
    let mut want = vec![key(0x38, false, false)];
    want.extend(uni(0xDF));
    want.push(key(0x38, false, true));
    assert_eq!(k.insert_text("ß"), want);
    assert_eq!(k.key_up(kvk::ANSI_S), vec![]);
}

#[test]
fn shift_is_released_around_unicode_but_control_is_not_touched() {
    let mut k = Keyboard::new(mac(), KeyboardType::Ansi);
    k.flags_changed(kvk::SHIFT, F(L_SHIFT));
    let mut want = vec![key(0x2A, false, false)];
    want.extend(uni(u16::from(b'A')));
    want.push(key(0x2A, false, true));
    assert_eq!(k.insert_text("A"), want);
}

#[test]
fn ctrl_c_is_a_scancode_chord() {
    let mut k = Keyboard::new(mac(), KeyboardType::Ansi);
    assert_eq!(k.flags_changed(kvk::CONTROL, F(L_CTRL)), vec![key(0x1D, false, true)]);
    assert_eq!(k.key_down(kvk::ANSI_C, F(L_CTRL), false, false), KeyDown::Send(vec![key(0x2E, false, true)]));
    assert_eq!(k.key_up(kvk::ANSI_C), vec![key(0x2E, false, false)]);
}

#[test]
fn cmd_v_is_a_scancode_chord() {
    let mut k = Keyboard::new(mac(), KeyboardType::Ansi);
    assert_eq!(k.flags_changed(kvk::COMMAND, F(L_CMD)), vec![]);
    assert_eq!(
        k.key_down(kvk::ANSI_V, F(L_CMD), false, false),
        KeyDown::Send(vec![key(0x5B, true, true), key(0x2F, false, true)])
    );
}

#[test]
fn emoji_via_insert_text() {
    let mut k = Keyboard::new(mac(), KeyboardType::Ansi);
    let mut want = uni(0xD83D).to_vec();
    want.extend(uni(0xDE00));
    assert_eq!(k.insert_text("😀"), want);
}

#[test]
fn doc_command_fallback_sends_scancode() {
    let mut k = Keyboard::new(mac(), KeyboardType::Ansi);
    assert_eq!(k.key_down(kvk::ANSI_A, F(0), false, false), KeyDown::InterpretText);
    assert_eq!(k.key_down_scancode(kvk::ANSI_A, F(0)), vec![key(0x1E, false, true)]);
    assert_eq!(k.key_up(kvk::ANSI_A), vec![key(0x1E, false, false)]);
}
