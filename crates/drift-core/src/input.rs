//! Wire-level input events (plan §3).
//!
//! Produced by `drift-input` (pure mapping) from `drift-macos` NSEvents and consumed by
//! the session actor, which encodes them as fast-path input PDUs.

use serde::{Deserialize, Serialize};

/// Pointer buttons supported by RDP fast-path input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum MouseButton {
    /// Primary button.
    Left,
    /// Secondary button.
    Right,
    /// Middle button / wheel click.
    Middle,
    /// Extended button 1 (back).
    X1,
    /// Extended button 2 (forward).
    X2,
}

/// One input event in remote desktop coordinates (plan §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum InputEvent {
    /// Set-1 scancode key event (positional; the remote XKB layout picks the character).
    Key {
        /// Scancode (without the `0xE0` prefix).
        scancode: u8,
        /// Extended (`0xE0`-prefixed) key.
        extended: bool,
        /// Press (`true`) or release.
        down: bool,
    },
    /// Unicode key event (layout-independent), one UTF-16 code unit.
    Unicode {
        /// UTF-16 code unit; surrogate pairs are sent as two events.
        ch: u16,
        /// Press (`true`) or release.
        down: bool,
    },
    /// Absolute pointer move in desktop pixels.
    MouseMove {
        /// X in desktop pixels.
        x: u16,
        /// Y in desktop pixels.
        y: u16,
    },
    /// Pointer button press/release at a desktop position.
    MouseButton {
        /// Which button.
        button: MouseButton,
        /// Press (`true`) or release.
        down: bool,
        /// X in desktop pixels.
        x: u16,
        /// Y in desktop pixels.
        y: u16,
    },
    /// Wheel rotation. Fractional-capable: any value in `-255..=255` (120 = one notch;
    /// positive = scroll up / right before g-r-d's HWHEEL sign inversion, see M2-3).
    Wheel {
        /// Horizontal (`PTR_FLAGS_HWHEEL`) instead of vertical.
        horizontal: bool,
        /// Wheel units, clamped to `-255..=255` by the producer.
        units: i16,
    },
    /// Synchronize lock-key toggle state.
    SyncToggles {
        /// Caps Lock on.
        caps: bool,
        /// Num Lock on.
        num: bool,
    },
    /// Release every key and button the remote believes is held (focus loss, reconnect).
    ReleaseAll,
}
