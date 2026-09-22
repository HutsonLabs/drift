//! M9-1 Red: the latency statistics the performance bench reports.
//!
//! Plan M9-1 budgets **decode + present < 8 ms p95** and **input-to-wire < 2 ms p99**, and
//! `cargo xtask e2e --bench` reports both. The numbers are produced by the pure
//! [`drift_rdp::stats`] types, so they are unit-tested here; the wiring that feeds them is
//! exercised by `m9_1_latency.rs` (loopback) and `tests/e2e/tests/bench.rs` (real host).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use drift_rdp::stats::{Percentiles, STATS_PERIOD, StatsMeter};

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

#[test]
fn an_empty_reservoir_reports_zero() {
    let p = Percentiles::default();
    assert!(p.is_empty());
    assert_eq!(p.len(), 0);
    assert_eq!(p.percentile_ms(95), 0.0);
    assert_eq!(p.percentile_ms(99), 0.0);
}

#[test]
fn nearest_rank_percentiles_over_one_hundred_samples() {
    // 1..=100 ms, recorded out of order: the percentile must not depend on arrival order.
    let mut p = Percentiles::default();
    for i in (1..=100).rev() {
        p.record(ms(i));
    }
    assert_eq!(p.len(), 100);
    // Nearest rank: p95 of 100 sorted samples is the 95th, p99 the 99th, p50 the 50th.
    assert_eq!(p.percentile_ms(95), 95.0);
    assert_eq!(p.percentile_ms(99), 99.0);
    assert_eq!(p.percentile_ms(50), 50.0);
    assert_eq!(p.percentile_ms(100), 100.0, "p100 is the maximum");
    assert_eq!(p.percentile_ms(0), 1.0, "p0 is the minimum");
}

#[test]
fn sub_millisecond_samples_keep_their_fraction() {
    let mut p = Percentiles::default();
    p.record(Duration::from_micros(250));
    assert!((p.percentile_ms(99) - 0.25).abs() < 1e-4, "{}", p.percentile_ms(99));
}

#[test]
fn the_reservoir_is_bounded_and_keeps_the_newest_samples() {
    let mut p = Percentiles::default();
    for i in 0..(Percentiles::CAPACITY * 2) {
        p.record(ms(u64::try_from(i).unwrap()));
    }
    assert_eq!(p.len(), Percentiles::CAPACITY, "a long session must not grow without bound");
    let oldest_kept = u64::try_from(Percentiles::CAPACITY).unwrap();
    assert_eq!(p.percentile_ms(0), oldest_kept as f32, "the oldest samples were dropped");
}

#[test]
fn clearing_starts_a_new_window() {
    let mut p = Percentiles::default();
    p.record(ms(5));
    p.clear();
    assert!(p.is_empty());
    assert_eq!(p.percentile_ms(95), 0.0);
}

#[test]
fn the_meter_reports_frame_p95_and_input_p99_and_starts_a_new_window() {
    let t0 = Instant::now();
    let mut m = StatsMeter::new(t0);
    m.frames(60);
    m.bytes(150_000);
    for i in 1..=100 {
        m.frame_latency(ms(i));
        m.input_latency(ms(i * 2));
    }
    let s = m.sample(t0 + STATS_PERIOD, 3).expect("a sample after one second");
    assert!((s.fps - 60.0).abs() < 0.1, "{s:?}");
    assert_eq!(s.bitrate_bps, 1_200_000);
    assert_eq!(s.frame_latency_p95_ms, 95.0, "decode + present p95 (plan M9-1: < 8 ms)");
    assert_eq!(s.input_to_wire_p99_ms, 198.0, "input-to-wire p99 (plan M9-1: < 2 ms)");

    // The next window starts empty, so a slow second is not remembered forever.
    m.frames(10);
    let s = m.sample(t0 + STATS_PERIOD * 2, 0).expect("a second sample");
    assert_eq!(s.frame_latency_p95_ms, 0.0);
    assert_eq!(s.input_to_wire_p99_ms, 0.0);
}

#[test]
fn restart_discards_latencies_too() {
    let t0 = Instant::now();
    let mut m = StatsMeter::new(t0);
    m.frame_latency(ms(40));
    m.input_latency(ms(40));
    m.restart(t0 + ms(500));
    let s = m.sample(t0 + ms(1500), 0).expect("a sample");
    assert_eq!((s.frame_latency_p95_ms, s.input_to_wire_p99_ms), (0.0, 0.0));
}
