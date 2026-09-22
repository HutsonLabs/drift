use super::*;
use proptest::prelude::*;

fn view(w: f64, h: f64, scale: f64) -> ViewGeometry {
    ViewGeometry { points: Size::new(w, h), backing_scale: scale }
}

fn p(x: f64, y: f64) -> Point<f64> {
    Point::new(x, y)
}

type Case = (Point<f64>, Option<(u16, u16)>);

fn check(v: ViewGeometry, desktop: DesktopSize, mode: ScaleMode, cases: &[Case]) {
    for (pt, expected) in cases {
        assert_eq!(
            view_to_desktop(*pt, v, desktop, mode),
            expected.map(|(x, y)| Point::new(x, y)),
            "{pt:?} in {v:?} → {desktop:?} ({mode:?})"
        );
    }
}

#[test]
fn one_to_one_non_retina() {
    let v = view(1280.0, 800.0, 1.0);
    let d = Size::new(1280, 800);
    let cases: &[Case] = &[
        (p(0.0, 0.0), Some((0, 0))),
        (p(640.5, 400.2), Some((640, 400))),
        (p(1279.9, 799.9), Some((1279, 799))),
        (p(1280.0, 800.0), Some((1279, 799))),
        (p(-5.0, -0.1), Some((0, 0))),
    ];
    check(v, d, ScaleMode::Fit, cases);
    check(v, d, ScaleMode::OneToOne, cases);
    let vp = Viewport::new(v, d, ScaleMode::Fit);
    assert_eq!(vp.origin, p(0.0, 0.0));
    assert_eq!(vp.points_per_pixel, 1.0);
    assert_eq!(vp.image_size(), Size::new(1280.0, 800.0));
}

#[test]
fn retina_adaptive_desktop_is_pixel_exact() {
    // Retina: 1280×800 pt @2x, desktop 2560×1600 at DesktopScaleFactor 200
    let v = view(1280.0, 800.0, 2.0);
    let d = Size::new(2560, 1600);
    let cases: &[Case] = &[
        (p(0.0, 0.0), Some((0, 0))),
        (p(10.5, 3.25), Some((21, 6))),
        (p(0.49, 0.5), Some((0, 1))),
        (p(1279.75, 799.5), Some((2559, 1599))),
    ];
    check(v, d, ScaleMode::Fit, cases);
    check(v, d, ScaleMode::OneToOne, cases);
    assert_eq!(Viewport::new(v, d, ScaleMode::Fit).points_per_pixel, 0.5);
}

#[test]
fn retina_view_with_non_retina_desktop_fits() {
    // Retina off on a Retina screen: desktop = points
    let v = view(1280.0, 800.0, 2.0);
    let d = Size::new(1280, 800);
    check(v, d, ScaleMode::Fit, &[(p(10.5, 3.25), Some((10, 3)))]);
}

#[test]
fn letterbox_wide_desktop_in_square_view() {
    let v = view(1000.0, 1000.0, 1.0);
    let d = Size::new(2000, 1000);
    let vp = Viewport::new(v, d, ScaleMode::Fit);
    assert_eq!(vp.points_per_pixel, 0.5);
    assert_eq!(vp.origin, p(0.0, 250.0));
    assert_eq!(vp.image_size(), Size::new(1000.0, 500.0));
    check(
        v,
        d,
        ScaleMode::Fit,
        &[
            (p(0.0, 250.0), Some((0, 0))),
            (p(500.0, 500.0), Some((1000, 500))),
            (p(999.99, 749.99), Some((1999, 999))),
            (p(500.0, 100.0), Some((1000, 0))),   // top bar clamps
            (p(500.0, 900.0), Some((1000, 999))), // bottom bar clamps
        ],
    );
    assert!(vp.contains(p(500.0, 500.0)));
    assert!(!vp.contains(p(500.0, 100.0)));
    assert!(!vp.contains(p(500.0, 750.0)));
}

#[test]
fn pillarbox_tall_desktop() {
    let v = view(1000.0, 500.0, 1.0);
    let d = Size::new(500, 500);
    let vp = Viewport::new(v, d, ScaleMode::Fit);
    assert_eq!((vp.origin, vp.points_per_pixel), (p(250.0, 0.0), 1.0));
    check(v, d, ScaleMode::Fit, &[(p(250.0, 0.0), Some((0, 0))), (p(100.0, 10.0), Some((0, 10)))]);
    assert!(!vp.contains(p(100.0, 10.0)));
}

#[test]
fn fit_upscales_small_desktop() {
    // Desktop Sharing 800×600 in a 1600×1200 pt window
    check(
        view(1600.0, 1200.0, 1.0),
        Size::new(800, 600),
        ScaleMode::Fit,
        &[(p(801.0, 601.0), Some((400, 300)))],
    );
}

#[test]
fn one_to_one_smaller_desktop_is_centred_on_retina() {
    let v = view(1280.0, 800.0, 2.0);
    let d = Size::new(1920, 1080);
    let vp = Viewport::new(v, d, ScaleMode::OneToOne);
    assert_eq!(vp.points_per_pixel, 0.5);
    assert_eq!(vp.origin, p(160.0, 130.0));
    check(
        v,
        d,
        ScaleMode::OneToOne,
        &[(p(160.0, 130.0), Some((0, 0))), (p(1119.75, 669.5), Some((1919, 1079)))],
    );
}

#[test]
fn one_to_one_larger_desktop_is_anchored_top_left() {
    let v = view(1280.0, 800.0, 1.0);
    let d = Size::new(4000, 3000);
    let vp = Viewport::new(v, d, ScaleMode::OneToOne);
    assert_eq!(vp.origin, p(0.0, 0.0));
    check(v, d, ScaleMode::OneToOne, &[(p(100.0, 100.0), Some((100, 100)))]);
}

#[test]
fn degenerate_inputs() {
    let v = view(1280.0, 800.0, 1.0);
    assert_eq!(view_to_desktop(p(1.0, 1.0), v, Size::new(0, 800), ScaleMode::Fit), None);
    assert_eq!(view_to_desktop(p(1.0, 1.0), v, Size::new(800, 0), ScaleMode::OneToOne), None);
    // zero-size / NaN view or point never panics and stays in range
    let d = Size::new(100, 100);
    for vv in [view(0.0, 0.0, 1.0), view(f64::NAN, 10.0, 0.0), view(10.0, 10.0, f64::INFINITY)] {
        for mode in [ScaleMode::Fit, ScaleMode::OneToOne] {
            let r = view_to_desktop(p(5.0, 5.0), vv, d, mode);
            assert!(r.is_some_and(|q| q.x < 100 && q.y < 100), "{vv:?} {mode:?} {r:?}");
        }
    }
    assert_eq!(view_to_desktop(p(f64::NAN, f64::INFINITY), v, d, ScaleMode::Fit), Some(Point::new(0, 99)));
    // desktops wider than u16 clamp into u16
    let r = view_to_desktop(p(1e9, 1e9), v, Size::new(100_000, 100_000), ScaleMode::OneToOne);
    assert_eq!(r, Some(Point::new(u16::MAX, u16::MAX)));
}

proptest! {
    #[test]
    fn mapping_is_in_range_and_monotonic(
        vw in 1.0f64..4000.0, vh in 1.0f64..4000.0, scale in proptest::sample::select(vec![1.0, 2.0, 3.0]),
        dw in 1u32..8000, dh in 1u32..8000, one in any::<bool>(),
        x1 in -100.0f64..4100.0, x2 in -100.0f64..4100.0, y in -100.0f64..4100.0,
    ) {
        let mode = if one { ScaleMode::OneToOne } else { ScaleMode::Fit };
        let v = view(vw, vh, scale);
        let d = Size::new(dw, dh);
        let a = view_to_desktop(p(x1.min(x2), y), v, d, mode).unwrap();
        let b = view_to_desktop(p(x1.max(x2), y), v, d, mode).unwrap();
        prop_assert!(u32::from(a.x) < dw && u32::from(a.y) < dh);
        prop_assert!(a.x <= b.x);
        prop_assert_eq!(a.y, b.y);
    }
}
