//! # drift-core
//!
//! Pure (no I/O, no FFI) model shared by every Drift crate. This crate holds the
//! **core contracts** fixed by plan §3 (task M0-5):
//!
//! - [`profile`]: [`ConnectMode`], [`ConnectionProfile`], per-profile preferences and
//!   [`CertFingerprint`] (formatted exactly like `grdctl status`).
//! - [`state`]: the [`SessionState`] finite-state machine and [`DisconnectReason`].
//! - [`input`]: the wire-level [`InputEvent`] produced by `drift-input` / `drift-macos`
//!   and consumed by the session actor.
//! - [`geometry`]: [`Size`], [`Point`], [`Rect`], [`Bgra`] and [`ViewGeometry`].
//! - [`video`]: the opaque [`Nv12Frame`] handle passed from `drift-video` through
//!   `drift-gfx` to `drift-render` (see `docs/adr/M0-5-nv12-frame-location.md`).
//! - [`clock`]: the [`Clock`] abstraction (tests use `drift_testkit::ManualClock`).
//!
//! Modules owned by later tasks (`reconnect` for M7-1, `layout` for M4-1) are
//! present as documented stubs so that parallel work does not collide.

pub mod clock;
pub mod geometry;
pub mod input;
pub mod layout;
pub mod profile;
pub mod reconnect;
pub mod state;
pub mod video;

pub use clock::{Clock, SystemClock};
pub use geometry::{Bgra, DesktopSize, Point, Rect, Size, ViewGeometry};
pub use input::{InputEvent, MouseButton};
pub use profile::{
    CertFingerprint, ClipboardPrefs, CmdAs, ConnectMode, ConnectionProfile, DisplayPrefs, KeyboardPrefs,
    ProfileError,
};
pub use state::{ConnectStage, DisconnectReason, InvalidTransition, SessionState};
pub use video::{Nv12Frame, Nv12Planes, Nv12Source};
