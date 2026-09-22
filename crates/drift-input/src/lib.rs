//! # drift-input
//!
//! Pure input translation from macOS events to [`drift_core::InputEvent`]s. No AppKit
//! types appear here: `drift-macos` extracts plain values (key codes, modifier flags,
//! deltas, points) and calls into this crate.
//!
//! | Module | Task |
//! |---|---|
//! | [`keymap`] | M2-1: `kVK_*` → set-1 scancode (+extended) for ANSI/ISO/JIS |
//! | [`modifiers`] | M2-1: `flagsChanged` diffing into left/right transitions, `CmdAs` |
//! | [`unicode`] | M2-2: "Type using Mac layout" routing |
//! | [`scroll`] | M2-3: precise-delta → fractional wheel-unit accumulator |
//! | [`coords`] | M2-3: `view_to_desktop` (1:1, fit/letterbox, Retina) |
//! | [`shortcuts`] | M2-4: `performKeyEquivalent:` allow-list |

pub mod coords;
pub mod keymap;
pub mod modifiers;
pub mod scroll;
pub mod shortcuts;
pub mod unicode;
