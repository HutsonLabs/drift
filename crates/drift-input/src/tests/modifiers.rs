use super::*;
use ModifierKey::*;

use crate::modifiers::ModifierFlags as F;

fn set(keys: &[ModifierKey]) -> ModifierSet {
    let mut s = ModifierSet::EMPTY;
    for k in keys {
        s.insert(*k);
    }
    s
}

#[test]
fn device_bits_pick_the_side() {
    let cases: &[(u64, &[ModifierKey])] = &[
        (0, &[]),
        (F::SHIFT | F::DEVICE_LEFT_SHIFT, &[LeftShift]),
        (F::SHIFT | F::DEVICE_RIGHT_SHIFT, &[RightShift]),
        (F::SHIFT | F::DEVICE_LEFT_SHIFT | F::DEVICE_RIGHT_SHIFT, &[LeftShift, RightShift]),
        (F::CONTROL | F::DEVICE_LEFT_CONTROL, &[LeftControl]),
        (F::CONTROL | F::DEVICE_RIGHT_CONTROL, &[RightControl]),
        (F::OPTION | F::DEVICE_LEFT_OPTION, &[LeftOption]),
        (F::OPTION | F::DEVICE_RIGHT_OPTION, &[RightOption]),
        (F::COMMAND | F::DEVICE_LEFT_COMMAND, &[LeftCommand]),
        (F::COMMAND | F::DEVICE_RIGHT_COMMAND, &[RightCommand]),
        (F::COMMAND | F::SHIFT | F::DEVICE_LEFT_COMMAND | F::DEVICE_RIGHT_SHIFT, &[RightShift, LeftCommand]),
        // caps/fn/numpad bits never make a modifier key "held"
        (F::CAPS_LOCK | F::FUNCTION | F::NUMERIC_PAD, &[]),
    ];
    for (flags, expected) in cases {
        assert_eq!(held_modifiers(F(*flags), None, ModifierSet::EMPTY), set(expected), "flags {flags:#x}");
    }
}

#[test]
fn independent_bit_clear_releases_despite_stale_device_bits() {
    let flags = F(F::DEVICE_LEFT_SHIFT | F::DEVICE_LEFT_COMMAND);
    assert_eq!(held_modifiers(flags, None, set(&[LeftShift, LeftCommand])), ModifierSet::EMPTY);
}

#[test]
fn synthetic_flags_without_device_bits() {
    // side from the changed key
    assert_eq!(held_modifiers(F(F::SHIFT), Some(kvk::RIGHT_SHIFT), ModifierSet::EMPTY), set(&[RightShift]));
    assert_eq!(
        held_modifiers(F(F::COMMAND), Some(kvk::RIGHT_COMMAND), ModifierSet::EMPTY),
        set(&[RightCommand])
    );
    // otherwise keep what was held
    assert_eq!(held_modifiers(F(F::OPTION), Some(kvk::SHIFT), set(&[RightOption])), set(&[RightOption]));
    // otherwise left
    assert_eq!(held_modifiers(F(F::CONTROL), None, ModifierSet::EMPTY), set(&[LeftControl]));
    assert_eq!(held_modifiers(F(F::CONTROL), Some(kvk::ANSI_A), ModifierSet::EMPTY), set(&[LeftControl]));
}

#[test]
fn modifier_key_round_trip() {
    for k in ModifierKey::ALL {
        assert_eq!(ModifierKey::from_kvk(k.kvk()), Some(k));
        assert_eq!(k.is_command(), matches!(k, LeftCommand | RightCommand));
    }
    assert_eq!(ModifierKey::from_kvk(kvk::CAPS_LOCK), None);
    assert_eq!(ModifierKey::from_kvk(kvk::ANSI_A), None);
}

#[test]
fn modifier_set_ops() {
    let mut s = ModifierSet::EMPTY;
    assert!(s.is_empty());
    s.insert(RightOption);
    s.insert(LeftShift);
    assert!(s.contains(RightOption) && !s.contains(LeftOption));
    assert_eq!(s.iter().collect::<Vec<_>>(), vec![LeftShift, RightOption]);
    s.remove(LeftShift);
    assert_eq!(s.iter().collect::<Vec<_>>(), vec![RightOption]);
}

#[test]
fn flag_accessors() {
    let f = F(F::CAPS_LOCK | F::SHIFT | F::CONTROL | F::OPTION | F::COMMAND);
    assert!(f.caps_lock() && f.shift() && f.control() && f.option() && f.command());
    let f = F(0);
    assert!(!f.caps_lock() && !f.shift() && !f.control() && !f.option() && !f.command());
}
