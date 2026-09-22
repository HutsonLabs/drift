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
//!
//! The desktop is always drawn with [`ScaleMode::Fit`]: with an adaptive desktop that is an
//! exact 1:1 mapping, and otherwise it is the aspect-preserving letterbox of M4-3.

use std::time::{Duration, Instant};

use drift_core::{ConnectMode, DisplayControlCaps, DisplayPrefs, MonitorLayout, ViewGeometry, desired_layout};
use drift_input::ScaleMode;

/// Trailing debounce window for geometry changes (plan M4-2).
pub const RESIZE_DEBOUNCE: Duration = Duration::from_millis(250);

/// Whether `mode` offers a Display Control channel (plan §1.2: not Desktop Sharing).
pub fn mode_has_display_control(mode: ConnectMode) -> bool {
    match mode {
        ConnectMode::RemoteLogin | ConnectMode::Headless => true,
        ConnectMode::DesktopSharing => false,
    }
}

/// How the app places the remote desktop in the view (see the module docs).
pub fn scale_mode(_mode: ConnectMode, _prefs: DisplayPrefs) -> ScaleMode {
    ScaleMode::Fit
}

/// The 250 ms trailing-debounce resize driver (see the module docs).
#[derive(Debug, Clone)]
pub struct ResizeDriver {
    mode: ConnectMode,
    prefs: DisplayPrefs,
    /// Latest geometry reported by the view.
    geometry: Option<ViewGeometry>,
    /// The server's capabilities, once its `DISPLAYCONTROL_CAPS_PDU` arrived on this leg.
    caps: Option<DisplayControlCaps>,
    /// What the server currently displays (connect size, or the last layout sent).
    current: Option<MonitorLayout>,
    /// End of the debounce window of an unsent geometry change.
    due: Option<Instant>,
}

impl ResizeDriver {
    /// A driver for a profile's mode and display preferences.
    pub fn new(mode: ConnectMode, prefs: DisplayPrefs) -> Self {
        Self { mode, prefs, geometry: None, caps: None, current: None, due: None }
    }

    /// `true` when this session may request layouts at all.
    fn adaptive(&self) -> bool {
        self.prefs.adaptive && mode_has_display_control(self.mode)
    }

    /// The layout of the current view geometry, with the server's caps when they are known.
    fn desired(&self) -> Option<MonitorLayout> {
        let geometry = self.geometry?;
        let caps = self.caps.unwrap_or_default();
        self.adaptive().then(|| desired_layout(geometry, self.prefs, &caps))
    }

    /// The layout to request at connect time (desktop size and scale of the Client Core Data),
    /// once the view geometry is known and the profile is adaptive.
    pub fn connect_layout(&self) -> Option<MonitorLayout> {
        self.desired()
    }

    /// The view geometry changed at `now`.
    pub fn on_geometry(&mut self, geometry: ViewGeometry, now: Instant) {
        if self.geometry == Some(geometry) {
            return;
        }
        self.geometry = Some(geometry);
        if self.adaptive() {
            self.due = Some(now + RESIZE_DEBOUNCE);
        }
    }

    /// A new leg was activated with `layout` on the server; Display Control is not ready yet.
    pub fn on_leg_activated(&mut self, layout: MonitorLayout) {
        self.caps = None;
        self.current = Some(layout);
    }

    /// The server's Display Control capabilities arrived on the current leg.
    pub fn on_display_control_ready(&mut self, caps: DisplayControlCaps, now: Instant) {
        self.caps = Some(caps);
        if self.adaptive() && self.geometry.is_some() && self.due.is_none() {
            // Catch up with a geometry that was reported while the channel was still opening.
            self.due = Some(now);
        }
    }

    /// When [`Self::poll`] should run next, if a layout is pending.
    pub fn deadline(&self) -> Option<Instant> {
        self.due
    }

    /// The layout to send now, if the debounce window has passed and it differs from the
    /// server's current layout. The returned layout becomes the current one.
    pub fn poll(&mut self, now: Instant) -> Option<MonitorLayout> {
        if self.due.is_some_and(|due| now < due) {
            return None;
        }
        // Without the server's capabilities the pending change has to wait.
        self.caps?;
        self.due = None;
        let desired = self.desired()?;
        if self.current == Some(desired) {
            return None;
        }
        self.current = Some(desired);
        Some(desired)
    }
}
