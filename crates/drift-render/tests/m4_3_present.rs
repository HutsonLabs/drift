//! M4-3 Red: crisp present. Nearest + bit-exact when the drawable equals the desktop;
//! otherwise linear with an aspect-preserving letterbox.
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use drift_core::{Point, Rect, Size};
use drift_gfx::FrameSink;
use drift_render::CpuCompositor;
use drift_testkit::golden::{assert_golden, compare};

fn draw(s: &mut dyn FrameSink, desktop: Size<u32>, seed: u8) {
    s.reset(desktop);
    s.create_surface(1, desktop);
    s.map_surface_to_output(1, Point::new(0, 0));
    let (stride, data) = gradient(desktop.width, desktop.height, 0, seed);
    s.blit_bgra(1, Rect::new(0, 0, desktop.width, desktop.height), stride, &data);
}

/// Renders `desktop` into a `drawable`-sized offscreen target; returns (target, CPU model).
fn present(
    desktop: Size<u32>,
    drawable: Size<u32>,
    seed: u8,
) -> (drift_render::BgraImage, drift_render::BgraImage) {
    let mut cpu = CpuCompositor::new();
    draw(&mut cpu, desktop, seed);
    let mut gpu = offscreen(drawable);
    draw(&mut gpu, desktop, seed);
    gpu.end_frame(1, Box::new(|| {}));
    gpu.wait_idle();
    (gpu.read_target(), cpu.present(drawable))
}

#[test]
fn one_to_one_is_bit_exact() {
    let desktop = Size::new(64, 40);
    let (target, reference) = present(desktop, desktop, 5);
    let mut gpu = offscreen(desktop);
    draw(&mut gpu, desktop, 5);
    gpu.end_frame(1, Box::new(|| {}));
    gpu.wait_idle();
    let composite = gpu.read_output().unwrap();
    // Bit-exact: tolerance 0, against both the composite and the golden.
    compare(&g(&composite), &g(&target), 0).unwrap();
    assert_golden(&golden_path("present_1to1"), &g(&reference), &g(&target), 0);
}

#[test]
fn scaled_present_is_linear() {
    // 2× upscale (e.g. a Retina drawable for a 100 % desktop).
    let (target, reference) = present(Size::new(16, 10), Size::new(32, 20), 9);
    assert_golden(&golden_path("present_scaled_2x"), &g(&reference), &g(&target), 2);
    // Linear filtering produces in-between values that nearest never would.
    let (target, _) = present(Size::new(2, 1), Size::new(8, 4), 0);
    let left = target.pixel(0, 1)[0];
    let right = target.pixel(7, 1)[0];
    let mid = target.pixel(3, 1)[0];
    assert!(mid > left.min(right) && mid < left.max(right), "not linear: {left} {mid} {right}");
}

#[test]
fn letterbox_bars_are_black_and_content_is_centred() {
    // 2:1 desktop in a square drawable → bars top and bottom.
    let (target, reference) = present(Size::new(32, 16), Size::new(32, 32), 21);
    assert_golden(&golden_path("present_letterbox"), &g(&reference), &g(&target), 2);
    for y in (0..8).chain(24..32) {
        for x in 0..32 {
            assert_eq!(target.pixel(x, y), [0, 0, 0, 255], "bar pixel ({x},{y})");
        }
    }
    // Pillarbox: tall desktop in a wide drawable.
    let (target, reference) = present(Size::new(10, 20), Size::new(40, 20), 33);
    assert_golden(&golden_path("present_pillarbox"), &g(&reference), &g(&target), 2);
    assert_eq!(target.pixel(0, 10), [0, 0, 0, 255]);
    assert_eq!(target.pixel(39, 10), [0, 0, 0, 255]);
}
