//! Modifier-flag diffing into key transitions. Owned by task **M2-1**.
//!
//! `flagsChanged:` only tells us the *new* `NSEvent.modifierFlags` plus the key code that
//! changed. The raw flags carry device-dependent bits (`NX_DEVICE*KEYMASK`) that distinguish
//! left from right. [`held_modifiers`] turns one flags word into the exact set of physically
//! held modifier keys; [`crate::keyboard::Keyboard`] diffs consecutive sets into scancode
//! presses and releases.

use crate::keymap::kvk;

/// Raw `NSEventModifierFlags` (`NSEvent.modifierFlags().0`), including the device-dependent
/// low bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModifierFlags(pub u64);

impl ModifierFlags {
    /// `NSEventModifierFlagCapsLock` (Caps Lock toggle state).
    pub const CAPS_LOCK: u64 = 1 << 16;
    /// `NSEventModifierFlagShift`.
    pub const SHIFT: u64 = 1 << 17;
    /// `NSEventModifierFlagControl`.
    pub const CONTROL: u64 = 1 << 18;
    /// `NSEventModifierFlagOption`.
    pub const OPTION: u64 = 1 << 19;
    /// `NSEventModifierFlagCommand`.
    pub const COMMAND: u64 = 1 << 20;
    /// `NSEventModifierFlagNumericPad`.
    pub const NUMERIC_PAD: u64 = 1 << 21;
    /// `NSEventModifierFlagHelp`.
    pub const HELP: u64 = 1 << 22;
    /// `NSEventModifierFlagFunction`.
    pub const FUNCTION: u64 = 1 << 23;

    /// `NX_DEVICELCTLKEYMASK`.
    pub const DEVICE_LEFT_CONTROL: u64 = 0x0000_0001;
    /// `NX_DEVICELSHIFTKEYMASK`.
    pub const DEVICE_LEFT_SHIFT: u64 = 0x0000_0002;
    /// `NX_DEVICERSHIFTKEYMASK`.
    pub const DEVICE_RIGHT_SHIFT: u64 = 0x0000_0004;
    /// `NX_DEVICELCMDKEYMASK`.
    pub const DEVICE_LEFT_COMMAND: u64 = 0x0000_0008;
    /// `NX_DEVICERCMDKEYMASK`.
    pub const DEVICE_RIGHT_COMMAND: u64 = 0x0000_0010;
    /// `NX_DEVICELALTKEYMASK`.
    pub const DEVICE_LEFT_OPTION: u64 = 0x0000_0020;
    /// `NX_DEVICERALTKEYMASK`.
    pub const DEVICE_RIGHT_OPTION: u64 = 0x0000_0040;
    /// `NX_DEVICERCTLKEYMASK`.
    pub const DEVICE_RIGHT_CONTROL: u64 = 0x0000_2000;

    /// `true` when every bit of `mask` is set.
    pub const fn contains(self, mask: u64) -> bool {
        self.0 & mask == mask
    }

    /// Caps Lock is toggled on.
    pub const fn caps_lock(self) -> bool {
        self.contains(Self::CAPS_LOCK)
    }

    /// Command (either side) is held.
    pub const fn command(self) -> bool {
        self.contains(Self::COMMAND)
    }

    /// Control (either side) is held.
    pub const fn control(self) -> bool {
        self.contains(Self::CONTROL)
    }

    /// Shift (either side) is held.
    pub const fn shift(self) -> bool {
        self.contains(Self::SHIFT)
    }

    /// Option (either side) is held.
    pub const fn option(self) -> bool {
        self.contains(Self::OPTION)
    }
}

/// One side-specific modifier key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModifierKey {
    /// Left Shift.
    LeftShift,
    /// Right Shift.
    RightShift,
    /// Left Control.
    LeftControl,
    /// Right Control.
    RightControl,
    /// Left Option (Alt).
    LeftOption,
    /// Right Option (AltGr).
    RightOption,
    /// Left Command.
    LeftCommand,
    /// Right Command.
    RightCommand,
}

impl ModifierKey {
    /// All modifier keys, in the order their transitions are emitted.
    pub const ALL: [Self; 8] = [
        Self::LeftShift,
        Self::RightShift,
        Self::LeftControl,
        Self::RightControl,
        Self::LeftOption,
        Self::RightOption,
        Self::LeftCommand,
        Self::RightCommand,
    ];

    /// The `kVK_*` code of this key.
    pub const fn kvk(self) -> u16 {
        match self {
            Self::LeftShift => kvk::SHIFT,
            Self::RightShift => kvk::RIGHT_SHIFT,
            Self::LeftControl => kvk::CONTROL,
            Self::RightControl => kvk::RIGHT_CONTROL,
            Self::LeftOption => kvk::OPTION,
            Self::RightOption => kvk::RIGHT_OPTION,
            Self::LeftCommand => kvk::COMMAND,
            Self::RightCommand => kvk::RIGHT_COMMAND,
        }
    }

    /// The modifier key for a `kVK_*` code, if it is one.
    pub fn from_kvk(code: u16) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.kvk() == code)
    }

    /// `true` for the two Command keys.
    pub const fn is_command(self) -> bool {
        matches!(self, Self::LeftCommand | Self::RightCommand)
    }

    const fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// A set of held [`ModifierKey`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModifierSet(u8);

impl ModifierSet {
    /// The empty set.
    pub const EMPTY: Self = Self(0);

    /// `true` when `key` is in the set.
    pub const fn contains(self, key: ModifierKey) -> bool {
        self.0 & key.bit() != 0
    }

    /// Adds `key`.
    pub fn insert(&mut self, key: ModifierKey) {
        self.0 |= key.bit();
    }

    /// Removes `key`.
    pub fn remove(&mut self, key: ModifierKey) {
        self.0 &= !key.bit();
    }

    /// `true` when no key is held.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Iterates the held keys in [`ModifierKey::ALL`] order.
    pub fn iter(self) -> impl Iterator<Item = ModifierKey> {
        ModifierKey::ALL.into_iter().filter(move |k| self.contains(*k))
    }
}

/// Computes the physically held modifier keys from one `modifierFlags` word.
///
/// * The device-independent bit (`SHIFT`, `CONTROL`, …) decides whether a modifier class is held
///   at all. When it is clear the class is released on both sides, whatever stale device bits say.
/// * When it is set, the device-dependent bits pick the side(s).
/// * Synthetic events may carry no device bits. Then the side comes from `changed_kvk` if that key
///   belongs to the class, else from `previous` (sides already held), else the left side.
pub fn held_modifiers(flags: ModifierFlags, changed_kvk: Option<u16>, previous: ModifierSet) -> ModifierSet {
    let _ = (flags, changed_kvk, previous);
    ModifierSet::EMPTY
}

#[cfg(test)]
#[path = "tests/modifiers.rs"]
mod tests;
