//! Scroll accumulator producing fractional wheel units. Owned by task **M2-3**.
//!
//! g-r-d turns every wheel event into `value / 120 × 10 px` of smooth scrolling, fractional values
//! included (plan §1.5: 30 × +12 scrolls exactly as far as 3 × +120). So trackpad deltas are
//! converted to small wheel-unit events rather than whole notches.
//!
//! Sign conventions (RDP): vertical `+` = scroll up (towards the top of the document), horizontal
//! `+` = scroll right. AppKit's `scrollingDeltaX/Y` already include the user's *natural scrolling*
//! preference (`isDirectionInvertedFromDevice`), and a positive delta means "towards the top /
//! left", so vertical passes through and horizontal is negated. The HWHEEL sign as g-r-d applies it
//! is locked in by the `e2e_hscroll` test.

use drift_core::InputEvent;

/// Largest magnitude of one wheel event (the fast-path rotation field is 9 bits signed).
pub const MAX_UNITS_PER_EVENT: i16 = 255;

/// One wheel notch.
pub const UNITS_PER_NOTCH: i16 = 120;

/// Scroll conversion settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollConfig {
    /// Wheel units per point of precise (trackpad / Magic Mouse) delta. Default 2.
    pub units_per_point: f64,
    /// Reverse both axes on top of the system's natural-scrolling setting. Default off.
    pub reverse: bool,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self { units_per_point: 2.0, reverse: false }
    }
}

/// One `scrollWheel:` event's deltas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollDelta {
    /// `hasPreciseScrollingDeltas == true`: `scrollingDeltaX/Y` in points.
    Precise {
        /// `scrollingDeltaX`.
        dx: f64,
        /// `scrollingDeltaY`.
        dy: f64,
    },
    /// Line-based mouse wheel: `scrollingDeltaX/Y` in lines (one notch ≈ 1 line). Each line is
    /// ±120 units, sent as events of at most one notch.
    Lines {
        /// `scrollingDeltaX`.
        dx: f64,
        /// `scrollingDeltaY`.
        dy: f64,
    },
}

/// Accumulates scroll deltas and emits whole wheel units, carrying the fractional remainder so
/// that nothing is lost or invented over a gesture.
#[derive(Debug, Clone, Default)]
pub struct ScrollAccumulator {
    config: ScrollConfig,
}

impl ScrollAccumulator {
    /// Creates an accumulator.
    pub fn new(config: ScrollConfig) -> Self {
        Self { config }
    }

    /// The active configuration.
    pub fn config(&self) -> ScrollConfig {
        self.config
    }

    /// Adds one event's deltas and returns the wheel events to send (vertical first).
    pub fn push(&mut self, delta: ScrollDelta) -> Vec<InputEvent> {
        let _ = delta;
        Vec::new()
    }

    /// Fractional units not yet sent, `(vertical, horizontal)`, each in `(-1, 1)`.
    pub fn residual(&self) -> (f64, f64) {
        (0.0, 0.0)
    }

    /// Drops the remainder (e.g. when a new gesture starts in the opposite direction).
    pub fn reset(&mut self) {}
}

#[cfg(test)]
#[path = "tests/scroll.rs"]
mod tests;
