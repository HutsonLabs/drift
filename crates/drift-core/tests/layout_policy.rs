//! M4-1 Red: `desired_layout` table tests and an MS-RDPEDISP property test.

use drift_core::layout::{
    DEVICE_SCALE_FACTORS, DisplayControlCaps, MonitorLayout, desired_layout, device_scale_for,
};
use drift_core::{DisplayPrefs, Size, ViewGeometry};
use proptest::prelude::*;

fn view(w: f64, h: f64, scale: f64) -> ViewGeometry {
    ViewGeometry { points: Size::new(w, h), backing_scale: scale }
}

const RETINA_ON: DisplayPrefs = DisplayPrefs { adaptive: true, retina: true };
const RETINA_OFF: DisplayPrefs = DisplayPrefs { adaptive: true, retina: false };

fn layout(w: u32, h: u32, desktop: u32, device: u32) -> MonitorLayout {
    MonitorLayout { width: w, height: h, desktop_scale_factor: desktop, device_scale_factor: device }
}

#[test]
fn layout_table() {
    let caps = DisplayControlCaps::default();
    #[rustfmt::skip]
    let table: &[(&str, ViewGeometry, DisplayPrefs, MonitorLayout)] = &[
        ("MBA 13\" at 2x, retina on",   view(1470.0, 956.0, 2.0), RETINA_ON,  layout(2940, 1912, 200, 180)),
        ("MBA 13\" at 2x, retina off",  view(1470.0, 956.0, 2.0), RETINA_OFF, layout(1470, 956, 100, 100)),
        ("external 1x, retina on",      view(1920.0, 1080.0, 1.0), RETINA_ON, layout(1920, 1080, 100, 100)),
        ("external 1x, retina off",     view(1920.0, 1080.0, 1.0), RETINA_OFF, layout(1920, 1080, 100, 100)),
        ("odd 1281x801 at 1x",          view(1281.0, 801.0, 1.0), RETINA_OFF, layout(1280, 801, 100, 100)),
        ("odd half points at 2x",       view(640.5, 400.5, 2.0), RETINA_ON,   layout(1280, 801, 200, 180)),
        ("fractional points round",     view(1280.4, 799.6, 1.0), RETINA_OFF, layout(1280, 800, 100, 100)),
        ("tiny window",                 view(120.0, 80.0, 1.0), RETINA_OFF,   layout(200, 200, 100, 100)),
        ("tiny window at 2x",           view(60.0, 90.0, 2.0), RETINA_ON,     layout(200, 200, 200, 180)),
        ("6K XDR at 2x",                view(3008.0, 1692.0, 2.0), RETINA_ON, layout(6016, 3384, 200, 180)),
        ("6K XDR at 1x (clamped)",      view(6016.0, 3384.0, 1.0), RETINA_ON, layout(6016, 3384, 100, 100)),
        ("huge clamps to 8192",         view(9000.0, 9000.0, 1.0), RETINA_OFF, layout(8192, 8192, 100, 100)),
        ("huge at 2x clamps to 8192",   view(5000.0, 3000.0, 2.0), RETINA_ON, layout(8192, 6000, 200, 180)),
        ("degenerate NaN/negative",     view(f64::NAN, -5.0, 2.0), RETINA_ON, layout(200, 200, 200, 180)),
        ("zero backing scale",          view(1280.0, 800.0, 0.0), RETINA_ON,  layout(1280, 800, 100, 100)),
    ];
    for (name, v, prefs, expected) in table {
        assert_eq!(desired_layout(*v, *prefs, &caps), *expected, "{name}");
    }
}

#[test]
fn adaptive_flag_does_not_change_the_layout() {
    let caps = DisplayControlCaps::default();
    let v = view(1470.0, 956.0, 2.0);
    let off = DisplayPrefs { adaptive: false, retina: true };
    assert_eq!(desired_layout(v, off, &caps), desired_layout(v, RETINA_ON, &caps));
}

#[test]
fn server_max_area_is_respected_and_aspect_roughly_kept() {
    // A server that only allows 1920x1080 worth of pixels.
    let caps = DisplayControlCaps {
        max_num_monitors: 1,
        max_monitor_area_factor_a: 1920,
        max_monitor_area_factor_b: 1080,
    };
    let l = desired_layout(view(1470.0, 956.0, 2.0), RETINA_ON, &caps);
    assert!(u64::from(l.width) * u64::from(l.height) <= 1920 * 1080, "{l:?}");
    assert_eq!(l.width % 2, 0);
    assert_eq!(l.desktop_scale_factor, 200);
    let aspect = f64::from(l.width) / f64::from(l.height);
    assert!((aspect - 2940.0 / 1912.0).abs() < 0.01, "{l:?}");
    // Close to the maximum (no over-shrinking).
    assert!(u64::from(l.width) * u64::from(l.height) > 1920 * 1080 * 98 / 100, "{l:?}");
}

#[test]
fn max_area_accounts_for_monitor_count() {
    let caps = DisplayControlCaps {
        max_num_monitors: 4,
        max_monitor_area_factor_a: 1000,
        max_monitor_area_factor_b: 1000,
    };
    assert_eq!(caps.max_area(), 4_000_000);
    let caps = DisplayControlCaps {
        max_num_monitors: u32::MAX,
        max_monitor_area_factor_a: u32::MAX,
        max_monitor_area_factor_b: u32::MAX,
    };
    assert_eq!(caps.max_area(), u64::MAX);
}

#[test]
fn device_scale_is_nearest_valid_value() {
    assert_eq!(device_scale_for(100), 100);
    assert_eq!(device_scale_for(119), 100);
    assert_eq!(device_scale_for(120), 100); // tie goes low
    assert_eq!(device_scale_for(121), 140);
    assert_eq!(device_scale_for(160), 140);
    assert_eq!(device_scale_for(161), 180);
    assert_eq!(device_scale_for(200), 180);
    assert_eq!(device_scale_for(500), 180);
}

/// Independent MS-RDPEDISP 2.2.2.2.1 check for a single primary monitor.
fn satisfies_ms_rdpedisp(l: &MonitorLayout, caps: &DisplayControlCaps) -> Result<(), String> {
    if !(200..=8192).contains(&l.width) || l.width % 2 != 0 {
        return Err(format!("width {} must be even and within 200..=8192", l.width));
    }
    if !(200..=8192).contains(&l.height) {
        return Err(format!("height {} must be within 200..=8192", l.height));
    }
    if !(100..=500).contains(&l.desktop_scale_factor) {
        return Err(format!("desktop scale {}", l.desktop_scale_factor));
    }
    if !DEVICE_SCALE_FACTORS.contains(&l.device_scale_factor) {
        return Err(format!("device scale {}", l.device_scale_factor));
    }
    if u64::from(l.width) * u64::from(l.height) > caps.max_area() {
        return Err(format!("area {}x{} exceeds {}", l.width, l.height, caps.max_area()));
    }
    Ok(())
}

fn any_caps() -> impl Strategy<Value = DisplayControlCaps> {
    // Servers that can at least fit one minimum-size monitor.
    (1u32..=16, 200u32..=16384, 200u32..=16384).prop_map(|(n, a, b)| DisplayControlCaps {
        max_num_monitors: n,
        max_monitor_area_factor_a: a,
        max_monitor_area_factor_b: b,
    })
}

fn any_length() -> impl Strategy<Value = f64> {
    prop_oneof![
        -10.0f64..20000.0,
        Just(0.0),
        Just(f64::NAN),
        Just(f64::INFINITY),
        Just(f64::NEG_INFINITY),
        Just(1e300),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    #[test]
    fn result_satisfies_ms_rdpedisp(
        w in any_length(),
        h in any_length(),
        backing in prop_oneof![Just(1.0), Just(2.0), 0.0f64..4.0],
        retina in any::<bool>(),
        adaptive in any::<bool>(),
        caps in any_caps(),
    ) {
        let l = desired_layout(view(w, h, backing), DisplayPrefs { adaptive, retina }, &caps);
        if let Err(e) = satisfies_ms_rdpedisp(&l, &caps) {
            prop_assert!(false, "{e}: {l:?} for {w}x{h}@{backing} retina={retina} caps={caps:?}");
        }
        let expected_scale = if retina && backing >= 1.5 { 200 } else { 100 };
        prop_assert_eq!(l.desktop_scale_factor, expected_scale);
    }

    #[test]
    fn in_range_sizes_are_kept_exactly(w in 200u32..=8192, h in 200u32..=8192) {
        let l = desired_layout(view(f64::from(w), f64::from(h), 1.0), RETINA_OFF, &DisplayControlCaps::default());
        prop_assert_eq!((l.width, l.height), (w & !1, h));
    }
}
