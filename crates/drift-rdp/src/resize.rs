//! Display Control resize driver (task **M4-2**). Pure; the session actor drives it.
//!
//! The `RemoteView` reports its geometry on every `setFrameSize:` and
//! `viewDidChangeBackingProperties` ([`crate::SessionCommand::Resize`]). A live window resize
//! produces dozens of those per second, and every Display Control layout makes g-r-d rebuild
//! the graphics pipeline (`ResetGraphics` plus a new surface, plan §1.4), so the driver:
//!
//! - debounces with a **250 ms trailing** window on the injected [`drift_core::Clock`]: a burst
//!   of geometry changes produces one layout, 250 ms after the last change;
//! - sends a layout **only when it differs** from what the server already has (the connect-time
//!   size, or the last layout sent), using [`drift_core::desired_layout`];
//! - holds changes until the server's `DISPLAYCONTROL_CAPS_PDU` arrived, then catches up at once;
//! - never sends anything when the profile is not adaptive or the mode has no Display Control:
//!   **Desktop Sharing** offers no DISP channel (plan §1.2), so it goes straight to
//!   [`ScaleMode::Fit`] with no timeout guessing.

use std::time::{Duration, Instant};

use drift_core::{ConnectMode, DisplayControlCaps, DisplayPrefs, MonitorLayout, ViewGeometry};
use drift_input::ScaleMode;

/// Trailing debounce window for geometry changes (plan M4-2).
pub const RESIZE_DEBOUNCE: Duration = Duration::from_millis(250);

/// Whether `mode` offers a Display Control channel (plan §1.2: not Desktop Sharing).
pub fn mode_has_display_control(mode: ConnectMode) -> bool {
    todo!("M4-2: {mode:?}")
}

/// How the remote desktop is placed in the view for `mode` and `prefs` (see the module docs).
pub fn scale_mode(mode: ConnectMode, prefs: DisplayPrefs) -> ScaleMode {
    todo!("M4-2: {mode:?} {prefs:?}")
}

/// The 250 ms trailing-debounce resize driver (see the module docs).
#[derive(Debug, Clone)]
pub struct ResizeDriver {
    mode: ConnectMode,
    prefs: DisplayPrefs,
}

impl ResizeDriver {
    /// A driver for a profile's mode and display preferences.
    pub fn new(mode: ConnectMode, prefs: DisplayPrefs) -> Self {
        Self { mode, prefs }
    }

    /// The layout to request at connect time (desktop size and scale of the Client Core Data),
    /// once the view geometry is known and the profile is adaptive.
    pub fn connect_layout(&self) -> Option<MonitorLayout> {
        todo!("M4-2 {:?} {:?}", self.mode, self.prefs)
    }

    /// The view geometry changed at `now`.
    pub fn on_geometry(&mut self, geometry: ViewGeometry, now: Instant) {
        todo!("M4-2 {geometry:?} {now:?}")
    }

    /// A new leg was activated with `layout` on the server; Display Control is not ready yet.
    pub fn on_leg_activated(&mut self, layout: MonitorLayout) {
        todo!("M4-2 {layout:?}")
    }

    /// The server's Display Control capabilities arrived on the current leg.
    pub fn on_display_control_ready(&mut self, caps: DisplayControlCaps, now: Instant) {
        todo!("M4-2 {caps:?} {now:?}")
    }

    /// When [`Self::poll`] should run next, if a layout is pending.
    pub fn deadline(&self) -> Option<Instant> {
        todo!()
    }

    /// The layout to send now, if the debounce window has passed and it differs from the
    /// server's current layout. The returned layout becomes the current one.
    pub fn poll(&mut self, now: Instant) -> Option<MonitorLayout> {
        todo!("M4-2 {now:?}")
    }
}
