use super::*;
use crate::keymap::kvk;
use drift_core::CmdAs;
use proptest::prelude::*;
use std::collections::BTreeSet;

use crate::modifiers::ModifierFlags as F;

const L_SHIFT: u64 = F::SHIFT | F::DEVICE_LEFT_SHIFT;
const R_SHIFT: u64 = F::SHIFT | F::DEVICE_RIGHT_SHIFT;
const L_CTRL: u64 = F::CONTROL | F::DEVICE_LEFT_CONTROL;
const L_CMD: u64 = F::COMMAND | F::DEVICE_LEFT_COMMAND;
const R_CMD: u64 = F::COMMAND | F::DEVICE_RIGHT_COMMAND;

fn kb() -> Keyboard {
    Keyboard::new(KeyboardPrefs::default(), KeyboardType::Ansi)
}

fn kb_with(cmd_as: CmdAs, mac_layout: bool) -> Keyboard {
    Keyboard::new(KeyboardPrefs { cmd_as, type_with_mac_layout: mac_layout }, KeyboardType::Ansi)
}

fn d(code: u8) -> InputEvent {
    InputEvent::Key { scancode: code, extended: false, down: true }
}
fn u(code: u8) -> InputEvent {
    InputEvent::Key { scancode: code, extended: false, down: false }
}
fn de(code: u8) -> InputEvent {
    InputEvent::Key { scancode: code, extended: true, down: true }
}
fn ue(code: u8) -> InputEvent {
    InputEvent::Key { scancode: code, extended: true, down: false }
}
fn send(k: &mut Keyboard, code: u16, flags: u64) -> Vec<InputEvent> {
    match k.key_down(code, F(flags), false, false) {
        KeyDown::Send(ev) => ev,
        KeyDown::InterpretText => panic!("expected scancode route"),
    }
}

#[test]
fn flags_changed_diffs_left_and_right_shift() {
    let mut k = kb();
    assert_eq!(k.flags_changed(kvk::SHIFT, F(L_SHIFT)), vec![d(0x2A)]);
    assert_eq!(k.flags_changed(kvk::RIGHT_SHIFT, F(L_SHIFT | R_SHIFT)), vec![d(0x36)]);
    assert_eq!(k.flags_changed(kvk::SHIFT, F(R_SHIFT)), vec![u(0x2A)]);
    assert_eq!(k.flags_changed(kvk::RIGHT_SHIFT, F(0)), vec![u(0x36)]);
    assert!(k.remote_pressed().is_empty());
}

#[test]
fn right_option_is_extended_alt_and_right_control_extended_ctrl() {
    let mut k = kb();
    assert_eq!(k.flags_changed(kvk::RIGHT_OPTION, F(F::OPTION | F::DEVICE_RIGHT_OPTION)), vec![de(0x38)]);
    assert_eq!(
        k.flags_changed(
            kvk::RIGHT_CONTROL,
            F(F::OPTION | F::DEVICE_RIGHT_OPTION | F::CONTROL | F::DEVICE_RIGHT_CONTROL)
        ),
        vec![de(0x1D)]
    );
    assert_eq!(k.flags_changed(kvk::RIGHT_OPTION, F(0)), vec![ue(0x38), ue(0x1D)]);
}

#[test]
fn command_is_super_and_deferred_until_used() {
    let mut k = kb();
    assert_eq!(k.flags_changed(kvk::COMMAND, F(L_CMD)), vec![]);
    assert_eq!(send(&mut k, kvk::ANSI_C, L_CMD), vec![de(0x5B), d(0x2E)]);
    assert_eq!(k.key_up(kvk::ANSI_C), vec![u(0x2E)]);
    assert_eq!(k.flags_changed(kvk::COMMAND, F(0)), vec![ue(0x5B)]);
}

#[test]
fn right_command_is_right_super() {
    let mut k = kb();
    assert_eq!(k.flags_changed(kvk::RIGHT_COMMAND, F(R_CMD)), vec![]);
    assert_eq!(send(&mut k, kvk::ANSI_L, R_CMD), vec![de(0x5C), d(0x26)]);
}

#[test]
fn command_tap_alone_is_sent_as_press_and_release() {
    let mut k = kb();
    assert_eq!(k.flags_changed(kvk::COMMAND, F(L_CMD)), vec![]);
    assert_eq!(k.flags_changed(kvk::COMMAND, F(0)), vec![de(0x5B), ue(0x5B)]);
}

#[test]
fn menu_combo_never_reaches_remote() {
    let mut k = kb();
    assert_eq!(k.flags_changed(kvk::COMMAND, F(L_CMD)), vec![]);
    k.menu_shortcut_taken(); // Cmd+T went to the menu
    assert_eq!(k.key_up(kvk::ANSI_T), vec![]);
    assert_eq!(k.flags_changed(kvk::COMMAND, F(0)), vec![]);
    // a later combo in the same Cmd hold after a menu shortcut still works
    assert_eq!(k.flags_changed(kvk::COMMAND, F(L_CMD)), vec![]);
    k.menu_shortcut_taken();
    assert_eq!(send(&mut k, kvk::ANSI_K, L_CMD), vec![de(0x5B), d(0x25)]);
}

#[test]
fn pointer_button_flushes_deferred_command() {
    let mut k = kb();
    assert_eq!(k.flags_changed(kvk::COMMAND, F(L_CMD)), vec![]);
    assert_eq!(k.prepare_pointer_button(), vec![de(0x5B)]);
    assert_eq!(k.prepare_pointer_button(), vec![]);
    assert_eq!(k.flags_changed(kvk::COMMAND, F(0)), vec![ue(0x5B)]);
}

#[test]
fn cmd_as_ctrl_sends_control_and_refcounts_with_physical_control() {
    let mut k = kb_with(CmdAs::Ctrl, false);
    assert_eq!(k.flags_changed(kvk::CONTROL, F(L_CTRL)), vec![d(0x1D)]);
    assert_eq!(k.flags_changed(kvk::COMMAND, F(L_CTRL | L_CMD)), vec![]);
    assert_eq!(send(&mut k, kvk::ANSI_C, L_CTRL | L_CMD), vec![d(0x2E)]);
    assert_eq!(k.key_up(kvk::ANSI_C), vec![u(0x2E)]);
    // physical Control released while Cmd-as-Ctrl is still held: remote keeps Ctrl down
    assert_eq!(k.flags_changed(kvk::CONTROL, F(L_CMD)), vec![]);
    assert_eq!(k.flags_changed(kvk::COMMAND, F(0)), vec![u(0x1D)]);
    assert!(k.remote_pressed().is_empty());
}

#[test]
fn cmd_as_ctrl_combo() {
    let mut k = kb_with(CmdAs::Ctrl, false);
    assert_eq!(k.flags_changed(kvk::COMMAND, F(L_CMD)), vec![]);
    assert_eq!(send(&mut k, kvk::ANSI_V, L_CMD), vec![d(0x1D), d(0x2F)]);
}

#[test]
fn autorepeat_is_not_resent() {
    let mut k = kb();
    assert_eq!(send(&mut k, kvk::ANSI_A, 0), vec![d(0x1E)]);
    assert_eq!(k.key_down(kvk::ANSI_A, F(0), true, false), KeyDown::Send(vec![]));
    // a missed keyUp does not double-press either
    assert_eq!(send(&mut k, kvk::ANSI_A, 0), vec![]);
    assert_eq!(k.key_up(kvk::ANSI_A), vec![u(0x1E)]);
    assert_eq!(k.key_up(kvk::ANSI_A), vec![]);
}

#[test]
fn extended_keys_and_unmapped_keys() {
    let mut k = kb();
    assert_eq!(send(&mut k, kvk::LEFT_ARROW, 0), vec![de(0x4B)]);
    assert_eq!(k.key_up(kvk::LEFT_ARROW), vec![ue(0x4B)]);
    assert_eq!(send(&mut k, kvk::ANSI_KEYPAD_CLEAR, 0), vec![]);
    assert_eq!(k.key_up(kvk::ANSI_KEYPAD_CLEAR), vec![]);
    // modifier / caps codes never come through key_down as scancodes
    assert_eq!(send(&mut k, kvk::CAPS_LOCK, 0), vec![]);
}

#[test]
fn key_down_resyncs_missed_modifier_changes() {
    let mut k = kb();
    // Shift was pressed while another window had focus
    assert_eq!(send(&mut k, kvk::ANSI_A, L_SHIFT), vec![d(0x2A), d(0x1E)]);
}

#[test]
fn focus_loss_releases_exactly_the_held_keys() {
    let mut k = kb();
    k.flags_changed(kvk::SHIFT, F(L_SHIFT));
    send(&mut k, kvk::ANSI_A, L_SHIFT);
    send(&mut k, kvk::ANSI_B, L_SHIFT);
    send(&mut k, kvk::UP_ARROW, L_SHIFT);
    k.key_up(kvk::ANSI_A);
    let ev = k.focus_lost();
    let mut got: Vec<_> = ev.clone();
    got.sort_by_key(|e| format!("{e:?}"));
    let mut want = vec![u(0x2A), u(0x30), ue(0x48)];
    want.sort_by_key(|e| format!("{e:?}"));
    assert_eq!(got, want);
    assert!(k.remote_pressed().is_empty());
    // the keys' later keyUps (delivered elsewhere or never) send nothing
    assert_eq!(k.key_up(kvk::ANSI_B), vec![]);
    assert_eq!(k.focus_lost(), vec![]);
}

#[test]
fn focus_loss_drops_deferred_command_silently() {
    let mut k = kb();
    k.flags_changed(kvk::COMMAND, F(L_CMD)); // Cmd+Tab: the Tab goes to macOS
    assert_eq!(k.focus_lost(), vec![]);
    // on return, Cmd is up
    assert_eq!(k.focus_gained(F(0)), vec![InputEvent::SyncToggles { caps: false, num: true }]);
}

#[test]
fn focus_gained_syncs_toggles_and_presses_held_modifiers() {
    let mut k = kb();
    assert_eq!(
        k.focus_gained(F(F::CAPS_LOCK | L_SHIFT)),
        vec![InputEvent::SyncToggles { caps: true, num: true }, d(0x2A)]
    );
}

#[test]
fn caps_lock_produces_sync_toggles_not_keys() {
    let mut k = kb();
    assert_eq!(
        k.flags_changed(kvk::CAPS_LOCK, F(F::CAPS_LOCK)),
        vec![InputEvent::SyncToggles { caps: true, num: true }]
    );
    // unrelated flag changes keep the toggle state and do not resend it
    assert_eq!(k.flags_changed(kvk::SHIFT, F(F::CAPS_LOCK | L_SHIFT)), vec![d(0x2A)]);
    assert_eq!(k.flags_changed(kvk::SHIFT, F(F::CAPS_LOCK)), vec![u(0x2A)]);
    assert_eq!(
        k.flags_changed(kvk::CAPS_LOCK, F(0)),
        vec![InputEvent::SyncToggles { caps: false, num: true }]
    );
    assert!(k.remote_pressed().is_empty());
}

#[test]
fn ctrl_alt_del_sequence() {
    let mut k = kb();
    assert_eq!(k.ctrl_alt_del(), vec![d(0x1D), d(0x38), de(0x53), ue(0x53), u(0x38), u(0x1D)]);
    // with Control already held, Control stays pressed afterwards
    k.flags_changed(kvk::CONTROL, F(L_CTRL));
    assert_eq!(k.ctrl_alt_del(), vec![d(0x38), de(0x53), ue(0x53), u(0x38)]);
    assert_eq!(k.remote_pressed(), vec![Scancode::new(0x1D)]);
}

#[test]
fn prefs_and_keyboard_type_can_change() {
    let mut k = kb();
    let p = KeyboardPrefs { cmd_as: CmdAs::Ctrl, type_with_mac_layout: true };
    k.set_prefs(p);
    assert_eq!(k.prefs(), p);
    k.set_keyboard_type(KeyboardType::Iso);
    assert_eq!(k.key_down(kvk::ANSI_GRAVE, F(0), false, false), KeyDown::InterpretText);
    assert_eq!(k.key_down_scancode(kvk::ANSI_GRAVE, F(0)), vec![d(0x56)]);
}

#[test]
fn key_held_across_cmd_as_change_releases_original_scancode() {
    let mut k = kb();
    k.flags_changed(kvk::COMMAND, F(L_CMD));
    send(&mut k, kvk::ANSI_A, L_CMD);
    k.set_prefs(KeyboardPrefs { cmd_as: CmdAs::Ctrl, type_with_mac_layout: false });
    k.key_up(kvk::ANSI_A);
    assert_eq!(k.flags_changed(kvk::COMMAND, F(0)), vec![ue(0x5B)]);
}

// ---------------------------------------------------------------------------------------------
// Property: wire-level balance for any sequence of keyboard callbacks.

#[derive(Debug, Clone)]
enum Op {
    Flags(u16, u64),
    Down(u16, u64, bool, bool),
    Up(u16),
    Text(String),
    Menu,
    Pointer,
    FocusLost,
    FocusGained(u64),
    Cad,
}

fn flag_bits() -> impl Strategy<Value = u64> {
    let bits = [
        F::CAPS_LOCK,
        F::SHIFT,
        F::CONTROL,
        F::OPTION,
        F::COMMAND,
        F::FUNCTION,
        F::DEVICE_LEFT_CONTROL,
        F::DEVICE_LEFT_SHIFT,
        F::DEVICE_RIGHT_SHIFT,
        F::DEVICE_LEFT_COMMAND,
        F::DEVICE_RIGHT_COMMAND,
        F::DEVICE_LEFT_OPTION,
        F::DEVICE_RIGHT_OPTION,
        F::DEVICE_RIGHT_CONTROL,
    ];
    proptest::collection::vec(any::<bool>(), bits.len())
        .prop_map(move |on| bits.iter().zip(on).filter(|(_, o)| *o).fold(0, |a, (b, _)| a | b))
}

fn any_kvk() -> impl Strategy<Value = u16> {
    prop_oneof![
        proptest::sample::select(crate::keymap::ALL_KVK.iter().map(|(_, c)| *c).collect::<Vec<_>>()),
        0u16..0x80,
    ]
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (any_kvk(), flag_bits()).prop_map(|(k, f)| Op::Flags(k, f)),
        4 => (any_kvk(), flag_bits(), any::<bool>(), any::<bool>()).prop_map(|(k, f, r, c)| Op::Down(k, f, r, c)),
        3 => any_kvk().prop_map(Op::Up),
        1 => "\\PC{0,3}".prop_map(Op::Text),
        1 => Just(Op::Menu),
        1 => Just(Op::Pointer),
        1 => Just(Op::FocusLost),
        1 => flag_bits().prop_map(Op::FocusGained),
        1 => Just(Op::Cad),
    ]
}

fn check_wire(pressed: &mut BTreeSet<(u8, bool)>, events: &[InputEvent]) -> Result<(), TestCaseError> {
    let mut i = 0;
    while i < events.len() {
        match events[i] {
            InputEvent::Key { scancode, extended, down: true } => {
                prop_assert!(pressed.insert((scancode, extended)), "double press {scancode:#x}/{extended}");
            }
            InputEvent::Key { scancode, extended, down: false } => {
                prop_assert!(
                    pressed.remove(&(scancode, extended)),
                    "release of unpressed {scancode:#x}/{extended}"
                );
            }
            InputEvent::Unicode { ch, down: true } => {
                prop_assert_eq!(events.get(i + 1), Some(&InputEvent::Unicode { ch, down: false }));
                i += 1;
            }
            InputEvent::Unicode { down: false, .. } => prop_assert!(false, "unpaired unicode release"),
            InputEvent::SyncToggles { num, .. } => prop_assert!(num),
            other => prop_assert!(false, "unexpected {other:?}"),
        }
        i += 1;
    }
    Ok(())
}

proptest! {
    #[test]
    fn down_up_balanced_for_any_sequence(
        cmd_ctrl in any::<bool>(),
        mac_layout in any::<bool>(),
        ops in proptest::collection::vec(op(), 0..60),
    ) {
        let mut k = kb_with(if cmd_ctrl { CmdAs::Ctrl } else { CmdAs::Super }, mac_layout);
        let mut pressed = BTreeSet::new();
        for op in ops {
            let ev = match op {
                Op::Flags(c, f) => k.flags_changed(c, F(f)),
                Op::Down(c, f, r, comp) => match k.key_down(c, F(f), r, comp) {
                    KeyDown::Send(ev) => ev,
                    KeyDown::InterpretText => k.key_down_scancode(c, F(f)),
                },
                Op::Up(c) => k.key_up(c),
                Op::Text(t) => k.insert_text(&t),
                Op::Menu => { k.menu_shortcut_taken(); vec![] }
                Op::Pointer => k.prepare_pointer_button(),
                Op::FocusLost => k.focus_lost(),
                Op::FocusGained(f) => k.focus_gained(F(f)),
                Op::Cad => k.ctrl_alt_del(),
            };
            check_wire(&mut pressed, &ev)?;
            let model: Vec<Scancode> = pressed.iter().map(|&(c, e)| Scancode { code: c, extended: e }).collect();
            prop_assert_eq!(k.remote_pressed(), model);
        }
        // releasing every modifier balances everything that came from flagsChanged …
        let ev = k.flags_changed(kvk::SHIFT, F(0));
        check_wire(&mut pressed, &ev)?;
        // … and focus loss releases the rest, exactly.
        let ev = k.focus_lost();
        prop_assert_eq!(ev.len(), pressed.len());
        check_wire(&mut pressed, &ev)?;
        prop_assert!(pressed.is_empty());
    }
}
