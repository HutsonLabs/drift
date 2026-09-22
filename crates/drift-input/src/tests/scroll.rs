use super::*;
use proptest::prelude::*;

fn v(units: i16) -> InputEvent {
    InputEvent::Wheel { horizontal: false, units }
}
fn h(units: i16) -> InputEvent {
    InputEvent::Wheel { horizontal: true, units }
}

#[test]
fn default_is_two_units_per_point() {
    let mut a = ScrollAccumulator::default();
    assert_eq!(a.config(), ScrollConfig { units_per_point: 2.0, reverse: false });
    assert_eq!(a.push(ScrollDelta::Precise { dx: 0.0, dy: 6.0 }), vec![v(12)]);
    assert_eq!(a.push(ScrollDelta::Precise { dx: 0.0, dy: -1.5 }), vec![v(-3)]);
}

#[test]
fn fractions_are_carried() {
    let mut a = ScrollAccumulator::default();
    assert_eq!(a.push(ScrollDelta::Precise { dx: 0.0, dy: 0.2 }), vec![]);
    assert_eq!(a.residual(), (0.4, 0.0));
    assert_eq!(a.push(ScrollDelta::Precise { dx: 0.0, dy: 0.3 }), vec![v(1)]);
    assert!(a.residual().0.abs() < 1e-9);
    a.push(ScrollDelta::Precise { dx: 0.0, dy: 0.25 });
    a.reset();
    assert_eq!(a.residual(), (0.0, 0.0));
}

#[test]
fn configurable_ratio() {
    let mut a = ScrollAccumulator::new(ScrollConfig { units_per_point: 3.0, reverse: false });
    assert_eq!(a.push(ScrollDelta::Precise { dx: 0.0, dy: 4.0 }), vec![v(12)]);
}

#[test]
fn horizontal_is_negated_to_rdp_convention_and_emitted_after_vertical() {
    let mut a = ScrollAccumulator::default();
    // AppKit +dx = towards the left → RDP HWHEEL negative
    assert_eq!(a.push(ScrollDelta::Precise { dx: 5.0, dy: 2.0 }), vec![v(4), h(-10)]);
    assert_eq!(a.push(ScrollDelta::Precise { dx: -1.0, dy: 0.0 }), vec![h(2)]);
}

#[test]
fn natural_scrolling_is_respected_and_reverse_flips() {
    // With natural scrolling, a two-finger swipe *up* arrives as negative dy (the system already
    // inverted it) and must scroll the remote content the same way a Mac app would: down.
    let mut a = ScrollAccumulator::default();
    assert_eq!(a.push(ScrollDelta::Precise { dx: 0.0, dy: -10.0 }), vec![v(-20)]);
    let mut r = ScrollAccumulator::new(ScrollConfig { units_per_point: 2.0, reverse: true });
    assert_eq!(r.push(ScrollDelta::Precise { dx: 3.0, dy: -10.0 }), vec![v(20), h(6)]);
}

#[test]
fn clamp_and_split_at_255() {
    let mut a = ScrollAccumulator::default();
    assert_eq!(a.push(ScrollDelta::Precise { dx: 0.0, dy: 300.0 }), vec![v(255), v(255), v(90)]);
    assert_eq!(a.push(ScrollDelta::Precise { dx: 0.0, dy: -127.5 }), vec![v(-255)]);
    assert_eq!(a.push(ScrollDelta::Precise { dx: -128.0, dy: 0.0 }), vec![h(255), h(1)]);
    // absurd deltas are bounded, never overflow
    let ev = a.push(ScrollDelta::Precise { dx: 0.0, dy: f64::MAX });
    assert!(!ev.is_empty() && ev.len() <= 1024);
    let ev = a.push(ScrollDelta::Precise { dx: f64::NAN, dy: f64::INFINITY });
    assert!(ev.len() <= 1024);
    let (rv, rh) = a.residual();
    assert!(rv.is_finite() && rh.is_finite());
}

#[test]
fn lines_are_120_units_per_notch() {
    let mut a = ScrollAccumulator::default();
    assert_eq!(a.push(ScrollDelta::Lines { dx: 0.0, dy: 1.0 }), vec![v(120)]);
    assert_eq!(a.push(ScrollDelta::Lines { dx: 0.0, dy: -3.0 }), vec![v(-120), v(-120), v(-120)]);
    assert_eq!(a.push(ScrollDelta::Lines { dx: 1.0, dy: 0.0 }), vec![h(-120)]);
    assert_eq!(a.push(ScrollDelta::Lines { dx: 0.0, dy: 0.5 }), vec![v(60)]);
}

#[test]
fn zero_delta_sends_nothing() {
    let mut a = ScrollAccumulator::default();
    assert_eq!(a.push(ScrollDelta::Precise { dx: 0.0, dy: 0.0 }), vec![]);
    assert_eq!(a.push(ScrollDelta::Lines { dx: -0.0, dy: 0.0 }), vec![]);
}

fn sum(events: &[InputEvent], horizontal_axis: bool) -> i64 {
    events
        .iter()
        .map(|e| match *e {
            InputEvent::Wheel { horizontal, units } if horizontal == horizontal_axis => i64::from(units),
            _ => 0,
        })
        .sum()
}

proptest! {
    /// Deltas that are exact binary fractions (as trackpads report: multiples of 1/64 pt)
    /// accumulate exactly: emitted + residual == total, with no drift at all.
    #[test]
    fn emitted_units_equal_accumulated_units_no_drift(
        ratio in proptest::sample::select(vec![0.5, 1.0, 2.0, 3.0, 4.0]),
        deltas in proptest::collection::vec((-6400i32..6400, -6400i32..6400), 0..300),
    ) {
        let mut a = ScrollAccumulator::new(ScrollConfig { units_per_point: ratio, reverse: false });
        let (mut total_v, mut total_h) = (0.0f64, 0.0f64);
        let (mut sent_v, mut sent_h) = (0i64, 0i64);
        for (dx, dy) in deltas {
            let (dx, dy) = (f64::from(dx) / 64.0, f64::from(dy) / 64.0);
            total_v += dy * ratio;
            total_h -= dx * ratio;
            let ev = a.push(ScrollDelta::Precise { dx, dy });
            for e in &ev {
                match *e {
                    InputEvent::Wheel { units, .. } => prop_assert!(units != 0 && units.abs() <= MAX_UNITS_PER_EVENT),
                    ref other => prop_assert!(false, "unexpected {other:?}"),
                }
            }
            sent_v += sum(&ev, false);
            sent_h += sum(&ev, true);
            let (rv, rh) = a.residual();
            prop_assert!(rv.abs() < 1.0 && rh.abs() < 1.0);
            prop_assert_eq!(sent_v as f64 + rv, total_v);
            prop_assert_eq!(sent_h as f64 + rh, total_h);
        }
    }

    /// Arbitrary real deltas never drift by a whole unit.
    #[test]
    fn arbitrary_deltas_stay_within_one_unit(
        deltas in proptest::collection::vec((-500.0f64..500.0, -500.0f64..500.0), 0..200),
    ) {
        let mut a = ScrollAccumulator::default();
        let (mut total_v, mut sent_v) = (0.0f64, 0i64);
        for (dx, dy) in deltas {
            total_v += dy * 2.0;
            sent_v += sum(&a.push(ScrollDelta::Precise { dx, dy }), false);
        }
        prop_assert!((total_v - sent_v as f64).abs() < 1.0 + 1e-6);
    }
}
