use super::*;
use crate::keymap::kvk;
use MenuShortcut::*;

use crate::modifiers::ModifierFlags as F;

const CMD: u64 = F::COMMAND | F::DEVICE_LEFT_COMMAND;
const SHIFT: u64 = F::SHIFT | F::DEVICE_LEFT_SHIFT;
const CTRL: u64 = F::CONTROL | F::DEVICE_LEFT_CONTROL;
const OPT: u64 = F::OPTION | F::DEVICE_LEFT_OPTION;

#[test]
fn allow_list_goes_to_the_menu() {
    let cases: &[(u16, &str, u64, MenuShortcut)] = &[
        (kvk::ANSI_T, "t", CMD, NewTab),
        (kvk::ANSI_W, "w", CMD, CloseTab),
        (kvk::ANSI_Q, "q", CMD, Quit),
        (kvk::ANSI_1, "1", CMD, SelectTab(1)),
        (kvk::ANSI_2, "2", CMD, SelectTab(2)),
        (kvk::ANSI_3, "3", CMD, SelectTab(3)),
        (kvk::ANSI_4, "4", CMD, SelectTab(4)),
        (kvk::ANSI_5, "5", CMD, SelectTab(5)),
        (kvk::ANSI_6, "6", CMD, SelectTab(6)),
        (kvk::ANSI_7, "7", CMD, SelectTab(7)),
        (kvk::ANSI_8, "8", CMD, SelectTab(8)),
        (kvk::ANSI_9, "9", CMD, SelectTab(9)),
        // charactersIgnoringModifiers keeps Shift: Cmd+Shift+[ reports "{"
        (kvk::ANSI_LEFT_BRACKET, "{", CMD | SHIFT, PreviousTab),
        (kvk::ANSI_RIGHT_BRACKET, "}", CMD | SHIFT, NextTab),
        (kvk::ANSI_LEFT_BRACKET, "[", CMD | SHIFT, PreviousTab),
        (kvk::ANSI_RIGHT_BRACKET, "]", CMD | SHIFT, NextTab),
        (kvk::ANSI_GRAVE, "`", CMD, CycleWindows),
        // Caps Lock, fn and keypad bits are ignored
        (kvk::ANSI_T, "T", CMD | F::CAPS_LOCK, NewTab),
        (kvk::ANSI_KEYPAD_1, "1", CMD | F::NUMERIC_PAD, SelectTab(1)),
        // right Command works too
        (kvk::ANSI_W, "w", F::COMMAND | F::DEVICE_RIGHT_COMMAND, CloseTab),
        // Dvorak: the key labelled T sits at the ANSI K position
        (kvk::ANSI_K, "t", CMD, NewTab),
        // Russian: non-ASCII → match by ANSI position
        (kvk::ANSI_T, "е", CMD, NewTab),
        (kvk::ANSI_Q, "й", CMD, Quit),
        // AZERTY: digits by position
        (kvk::ANSI_1, "&", CMD, SelectTab(1)),
    ];
    for (code, chars, flags, expected) in cases {
        assert_eq!(
            menu_shortcut(*code, chars, F(*flags)),
            Some(*expected),
            "kvk 0x{code:02X} {chars:?} {flags:#x}"
        );
    }
}

#[test]
fn everything_else_goes_to_the_remote() {
    let cases: &[(u16, &str, u64)] = &[
        (kvk::ANSI_C, "c", CMD),
        (kvk::ANSI_V, "v", CMD),
        (kvk::ANSI_A, "a", CMD),
        (kvk::ANSI_0, "0", CMD),
        (kvk::TAB, "\t", CMD),
        (kvk::TAB, "\t", CTRL),
        (kvk::ANSI_T, "t", CTRL),
        (kvk::ANSI_T, "T", CMD | SHIFT),
        (kvk::ANSI_T, "t", CMD | OPT),
        (kvk::ANSI_Q, "q", CMD | CTRL),
        (kvk::ANSI_W, "w", CMD | CTRL),
        (kvk::ANSI_1, "!", CMD | SHIFT),
        (kvk::ANSI_1, "1", CMD | OPT),
        (kvk::ANSI_LEFT_BRACKET, "[", CMD),
        (kvk::ANSI_RIGHT_BRACKET, "]", CMD),
        (kvk::ANSI_LEFT_BRACKET, "{", CMD | SHIFT | OPT),
        (kvk::ANSI_GRAVE, "~", CMD | SHIFT),
        (kvk::ANSI_T, "t", 0),
        (kvk::ANSI_T, "t", SHIFT),
        // Dvorak: the ANSI T position produces "y"
        (kvk::ANSI_T, "y", CMD),
        (kvk::ANSI_T, "", CTRL),
    ];
    for (code, chars, flags) in cases {
        assert_eq!(menu_shortcut(*code, chars, F(*flags)), None, "kvk 0x{code:02X} {chars:?} {flags:#x}");
    }
}

#[test]
fn empty_characters_fall_back_to_position() {
    assert_eq!(menu_shortcut(kvk::ANSI_W, "", F(CMD)), Some(CloseTab));
    assert_eq!(menu_shortcut(kvk::ANSI_RIGHT_BRACKET, "", F(CMD | SHIFT)), Some(NextTab));
    assert_eq!(menu_shortcut(kvk::ANSI_GRAVE, "", F(CMD)), Some(CycleWindows));
    assert_eq!(menu_shortcut(kvk::ANSI_C, "", F(CMD)), None);
}
