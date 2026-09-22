//! Remote monitor layout policy (task **M4-1**).
//!
//! [`desired_layout`] turns the local view geometry and the profile's [`DisplayPrefs`] into the
//! single-monitor layout Drift requests over the Display Control channel (MS-RDPEDISP
//! `DISPLAYCONTROL_MONITOR_LAYOUT`). It is pure and table/property tested; the resize driver
//! (M4-2) decides *when* to send it (debounce, adaptive on/off, DesktopSharing has no DISP).
//!
//! Policy (plan §6 M4-1, §1.4):
//! - **Retina on** and a HiDPI backing store (`backing_scale ≥ 1.5`): physical pixels
//!   (points × 2) with `DesktopScaleFactor = 200`, so GNOME renders at 2× (verified).
//! - Otherwise: one desktop pixel per point with `DesktopScaleFactor = 100`.
//! - Width is forced even (g-r-d does this anyway: 1281 → 1280), both dimensions are clamped to
//!   `[200, 8192]` and the area to the server's `MaxMonitorAreaFactorA × B × MaxNumMonitors`.
//! - `DeviceScaleFactor` is the member of {100, 140, 180} nearest to the desktop scale.

use crate::geometry::ViewGeometry;
use crate::profile::DisplayPrefs;

/// Smallest monitor width/height allowed by MS-RDPEDISP.
pub const MIN_MONITOR_DIMENSION: u32 = 200;
/// Largest monitor width/height allowed by MS-RDPEDISP.
pub const MAX_MONITOR_DIMENSION: u32 = 8192;
/// Valid `DeviceScaleFactor` values (MS-RDPEDISP 2.2.2.2.1).
pub const DEVICE_SCALE_FACTORS: [u32; 3] = [100, 140, 180];
/// Smallest valid `DesktopScaleFactor`.
pub const MIN_DESKTOP_SCALE: u32 = 100;
/// Largest valid `DesktopScaleFactor`.
pub const MAX_DESKTOP_SCALE: u32 = 500;
/// Backing scale at or above which the view counts as Retina.
const RETINA_BACKING_SCALE: f64 = 1.5;

/// The server's `DISPLAYCONTROL_CAPS_PDU` (MS-RDPEDISP 2.2.2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayControlCaps {
    /// `MaxNumMonitors`.
    pub max_num_monitors: u32,
    /// `MaxMonitorAreaFactorA`.
    pub max_monitor_area_factor_a: u32,
    /// `MaxMonitorAreaFactorB`.
    pub max_monitor_area_factor_b: u32,
}

impl DisplayControlCaps {
    /// Maximum total monitor area in pixels: `A × B × MaxNumMonitors` (saturating).
    pub fn max_area(&self) -> u64 {
        u64::from(self.max_monitor_area_factor_a)
            .saturating_mul(u64::from(self.max_monitor_area_factor_b))
            .saturating_mul(u64::from(self.max_num_monitors))
    }
}

impl Default for DisplayControlCaps {
    /// One monitor of up to 8192 × 8192 pixels.
    fn default() -> Self {
        Self {
            max_num_monitors: 1,
            max_monitor_area_factor_a: MAX_MONITOR_DIMENSION,
            max_monitor_area_factor_b: MAX_MONITOR_DIMENSION,
        }
    }
}

/// One `DISPLAYCONTROL_MONITOR_LAYOUT` entry (the primary monitor at the origin).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MonitorLayout {
    /// Width in desktop pixels (even, `200..=8192`).
    pub width: u32,
    /// Height in desktop pixels (`200..=8192`).
    pub height: u32,
    /// `DesktopScaleFactor` in percent (100 or 200).
    pub desktop_scale_factor: u32,
    /// `DeviceScaleFactor` in percent (one of [`DEVICE_SCALE_FACTORS`]).
    pub device_scale_factor: u32,
}

/// Computes the monitor layout to request for a view (see the module docs for the policy).
///
/// `prefs.adaptive` is not consulted here: whether to send a layout at all is the resize
/// driver's decision (M4-2). Degenerate geometry (zero, negative, NaN) yields the minimum size.
pub fn desired_layout(view: ViewGeometry, prefs: DisplayPrefs, caps: &DisplayControlCaps) -> MonitorLayout {
    let retina = prefs.retina && view.backing_scale >= RETINA_BACKING_SCALE;
    let (factor, desktop_scale_factor) = if retina { (2.0, 200) } else { (1.0, 100) };
    let (width, height) = fit_to_caps(
        to_pixels(view.points.width * factor),
        to_pixels(view.points.height * factor),
        caps.max_area(),
    );
    MonitorLayout {
        width,
        height,
        desktop_scale_factor,
        device_scale_factor: device_scale_for(desktop_scale_factor),
    }
}

/// The member of [`DEVICE_SCALE_FACTORS`] nearest to `desktop_scale` (ties go to the lower).
pub fn device_scale_for(desktop_scale: u32) -> u32 {
    let mut best = DEVICE_SCALE_FACTORS[0];
    for candidate in DEVICE_SCALE_FACTORS {
        if candidate.abs_diff(desktop_scale) < best.abs_diff(desktop_scale) {
            best = candidate;
        }
    }
    best
}

/// Rounds a non-negative length to whole pixels, mapping NaN/negative to 0 and saturating.
fn to_pixels(v: f64) -> u32 {
    let r = v.round();
    if r.is_nan() || r <= 0.0 {
        0
    } else if r >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        // `r` is finite and within `0..u32::MAX`, so the cast is exact.
        r as u32
    }
}

/// Clamps each dimension to `[200, 8192]`, forces the width even and shrinks the size
/// (keeping the aspect ratio as closely as integer pixels allow) until `w × h ≤ max_area`.
///
/// If `max_area` is smaller than 200 × 200 the minimum size is returned regardless: MS-RDPEDISP
/// forbids anything smaller, so such a server cannot be satisfied and the minimum is the least bad.
fn fit_to_caps(width: u32, height: u32, max_area: u64) -> (u32, u32) {
    let clamp = |v: u32| v.clamp(MIN_MONITOR_DIMENSION, MAX_MONITOR_DIMENSION);
    let even = |v: u32| v & !1;
    let (mut w, mut h) = (even(clamp(width)), clamp(height));
    let area = |w: u32, h: u32| u64::from(w) * u64::from(h);
    if area(w, h) <= max_area {
        return (w, h);
    }
    let ratio = (max_area as f64 / area(w, h) as f64).sqrt();
    w = even(clamp(to_pixels((f64::from(w) * ratio).floor())));
    h = clamp(to_pixels((f64::from(h) * ratio).floor()));
    // Correct for floating-point rounding: shrink the larger side until the area fits.
    while area(w, h) > max_area && (w > MIN_MONITOR_DIMENSION || h > MIN_MONITOR_DIMENSION) {
        if (w >= h && w > MIN_MONITOR_DIMENSION) || h <= MIN_MONITOR_DIMENSION {
            w -= 2;
        } else {
            h -= 1;
        }
    }
    (w, h)
}
