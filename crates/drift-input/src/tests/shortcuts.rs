use super::*;
use crate::keymap::kvk;
use MenuShortcut::*;

use crate::modifiers::ModifierFlags as F;

const CMD: u64 = F::COMMAND | F::DEVICE_LEFT_COMMAND;
const SHIFT: u64 = F::SHIFT | F::DEVICE_LEFT_SHIFT;
const CTRL: u64 = F::CONTROL | F::DEVICE_LEFT_CONTROL;
const OPT: u64 = F::OPTION | F::DEVICE_LEFT_OPTION;

/// UI-windows decision 9: the Command combos Drift's menus use while the remote desktop has
/// the keyboard.
#[test]
fn allow_list_goes_to_the_menu() {
    let cases: &[(u16, &str, u64, MenuShortcut)] = &[
        (kvk::ANSI_N, "n", CMD, NewConnection),
        (kvk::ANSI_0, "0", CMD, ShowConnections),
        (kvk::ANSI_E, "e", CMD, EditConnection),
        // charactersIgnoringModifiers keeps Shift: Shift+Cmd+D reports "D"
        (kvk::ANSI_D, "D", CMD | SHIFT, Disconnect),
        (kvk::ANSI_D, "d", CMD | SHIFT, Disconnect),
        (kvk::ANSI_W, "w", CMD, CloseWindow),
        (kvk::ANSI_Q, "q", CMD, Quit),
        (kvk::ANSI_1, "1", CMD, SelectSession(1)),
        (kvk::ANSI_2, "2", CMD, SelectSession(2)),
        (kvk::ANSI_3, "3", CMD, SelectSession(3)),
        (kvk::ANSI_4, "4", CMD, SelectSession(4)),
        (kvk::ANSI_5, "5", CMD, SelectSession(5)),
        (kvk::ANSI_6, "6", CMD, SelectSession(6)),
        (kvk::ANSI_7, "7", CMD, SelectSession(7)),
        (kvk::ANSI_8, "8", CMD, SelectSession(8)),
        (kvk::ANSI_9, "9", CMD, SelectSession(9)),
        (kvk::ANSI_GRAVE, "`", CMD, CycleWindows),
        // Caps Lock, fn and keypad bits are ignored
        (kvk::ANSI_N, "N", CMD | F::CAPS_LOCK, NewConnection),
        (kvk::ANSI_KEYPAD_1, "1", CMD | F::NUMERIC_PAD, SelectSession(1)),
        // right Command works too
        (kvk::ANSI_W, "w", F::COMMAND | F::DEVICE_RIGHT_COMMAND, CloseWindow),
        // Dvorak: the key labelled E sits at the ANSI D position, N at ANSI L
        (kvk::ANSI_D, "e", CMD, EditConnection),
        (kvk::ANSI_L, "n", CMD, NewConnection),
        // Russian: non-ASCII → match by ANSI position
        (kvk::ANSI_N, "т", CMD, NewConnection),
        (kvk::ANSI_E, "у", CMD, EditConnection),
        (kvk::ANSI_D, "В", CMD | SHIFT, Disconnect),
        (kvk::ANSI_Q, "й", CMD, Quit),
        // AZERTY: digits by position (Cmd+0 is "à" there)
        (kvk::ANSI_1, "&", CMD, SelectSession(1)),
        (kvk::ANSI_0, "à", CMD, ShowConnections),
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
        (kvk::TAB, "\t", CMD),
        (kvk::TAB, "\t", CTRL),
        // UI-windows: the tab shortcuts now belong to the remote desktop.
        (kvk::ANSI_T, "t", CMD),
        (kvk::ANSI_LEFT_BRACKET, "{", CMD | SHIFT),
        (kvk::ANSI_RIGHT_BRACKET, "}", CMD | SHIFT),
        (kvk::ANSI_LEFT_BRACKET, "[", CMD),
        (kvk::ANSI_RIGHT_BRACKET, "]", CMD),
        // Cmd+D without Shift, Shift+Cmd+N/E/0/W and Option or Control variants
        (kvk::ANSI_D, "d", CMD),
        (kvk::ANSI_N, "N", CMD | SHIFT),
        (kvk::ANSI_E, "E", CMD | SHIFT),
        (kvk::ANSI_0, ")", CMD | SHIFT),
        (kvk::ANSI_W, "W", CMD | SHIFT),
        (kvk::ANSI_D, "d", CMD | SHIFT | OPT),
        (kvk::ANSI_N, "n", CTRL),
        (kvk::ANSI_N, "n", CMD | OPT),
        (kvk::ANSI_Q, "q", CMD | CTRL),
        (kvk::ANSI_W, "w", CMD | CTRL),
        (kvk::ANSI_1, "!", CMD | SHIFT),
        (kvk::ANSI_1, "1", CMD | OPT),
        (kvk::ANSI_GRAVE, "~", CMD | SHIFT),
        (kvk::ANSI_N, "n", 0),
        (kvk::ANSI_N, "n", SHIFT),
        // Dvorak: the ANSI N position produces "b"
        (kvk::ANSI_N, "b", CMD),
        (kvk::ANSI_N, "", CTRL),
    ];
    for (code, chars, flags) in cases {
        assert_eq!(menu_shortcut(*code, chars, F(*flags)), None, "kvk 0x{code:02X} {chars:?} {flags:#x}");
    }
}

#[test]
fn empty_characters_fall_back_to_position() {
    assert_eq!(menu_shortcut(kvk::ANSI_W, "", F(CMD)), Some(CloseWindow));
    assert_eq!(menu_shortcut(kvk::ANSI_D, "", F(CMD | SHIFT)), Some(Disconnect));
    assert_eq!(menu_shortcut(kvk::ANSI_0, "", F(CMD)), Some(ShowConnections));
    assert_eq!(menu_shortcut(kvk::ANSI_GRAVE, "", F(CMD)), Some(CycleWindows));
    assert_eq!(menu_shortcut(kvk::ANSI_T, "", F(CMD)), None);
    assert_eq!(menu_shortcut(kvk::ANSI_C, "", F(CMD)), None);
}
