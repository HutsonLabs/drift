//! M9-1 Red: the performance report of `cargo xtask e2e --bench`.
//!
//! The e2e suite measures against the real GNOME host and writes a JSON report; this module
//! turns that report into a table, compares it with the stored baseline and decides whether
//! the run fails. Plan M9-1: the budgets, and "a regression of more than 10 % fails the
//! nightly run".
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use xtask::bench::{self, Direction, REGRESSION_TOLERANCE, Report, Verdict};

fn report(pairs: &[(&str, f64)]) -> Report {
    Report { metrics: pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect(), ..Report::default() }
}

/// A report that meets every budget with room to spare.
fn good() -> Report {
    report(&[
        ("fps_1280x800", 60.0),
        ("fps_2560x1600", 58.0),
        ("decode_present_p95_ms", 4.0),
        ("input_to_wire_p99_ms", 0.5),
        ("rss_per_session_mb", 180.0),
    ])
}

fn verdicts(rows: &[bench::Row]) -> BTreeMap<&str, Verdict> {
    rows.iter().map(|r| (r.metric.key, r.verdict)).collect()
}

#[test]
fn the_reported_metrics_are_the_five_m9_1_numbers() {
    let keys: Vec<&str> = bench::METRICS.iter().map(|m| m.key).collect();
    assert_eq!(
        keys,
        [
            "fps_1280x800",
            "fps_2560x1600",
            "decode_present_p95_ms",
            "input_to_wire_p99_ms",
            "rss_per_session_mb"
        ]
    );
    for m in bench::METRICS {
        assert!(!m.label.is_empty() && !m.unit.is_empty(), "{}", m.key);
    }
}

#[test]
fn the_budgets_are_the_ones_the_plan_states() {
    let budget = |key: &str| bench::METRICS.iter().find(|m| m.key == key).expect(key).budget;
    assert_eq!(budget("fps_1280x800"), 60.0, "60 fps at 1280x800");
    assert_eq!(budget("fps_2560x1600"), 55.0, ">= 55 fps at 2560x1600");
    assert_eq!(budget("decode_present_p95_ms"), 8.0, "decode + present < 8 ms p95");
    assert_eq!(budget("input_to_wire_p99_ms"), 2.0, "input-to-wire < 2 ms p99");
    assert_eq!(budget("rss_per_session_mb"), 300.0, "<= 300 MB RSS per session");
    let dir = |key: &str| bench::METRICS.iter().find(|m| m.key == key).expect(key).direction;
    assert_eq!(dir("fps_1280x800"), Direction::HigherIsBetter);
    assert_eq!(dir("decode_present_p95_ms"), Direction::LowerIsBetter);
    assert_eq!(dir("rss_per_session_mb"), Direction::LowerIsBetter);
}

#[test]
fn a_report_meeting_every_budget_passes_against_itself() {
    let rows = bench::compare(&good(), Some(&good()));
    assert_eq!(rows.len(), bench::METRICS.len());
    assert!(rows.iter().all(|r| r.verdict == Verdict::Ok), "{rows:#?}");
    assert!(bench::problems(&rows).is_empty());
}

#[test]
fn a_missing_metric_fails_the_run() {
    let mut r = good();
    r.metrics.remove("fps_2560x1600");
    let rows = bench::compare(&r, Some(&good()));
    assert_eq!(verdicts(&rows)["fps_2560x1600"], Verdict::Missing);
    let problems = bench::problems(&rows);
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("fps_2560x1600"), "{problems:?}");
}

#[test]
fn exactly_ten_percent_worse_is_tolerated_and_more_is_not() {
    assert_eq!(REGRESSION_TOLERANCE, 0.10, "plan M9-1: more than 10 % fails");

    // Higher-is-better, with enough headroom that the budget stays met either way, so only
    // the baseline comparison speaks: 10 % fewer frames is the limit.
    let mut baseline = good();
    baseline.metrics.insert("fps_1280x800".into(), 70.0);
    let mut at_limit = good();
    at_limit.metrics.insert("fps_1280x800".into(), 63.0);
    assert_eq!(verdicts(&bench::compare(&at_limit, Some(&baseline)))["fps_1280x800"], Verdict::Ok);
    let mut over = good();
    over.metrics.insert("fps_1280x800".into(), 62.0);
    let rows = bench::compare(&over, Some(&baseline));
    assert_eq!(verdicts(&rows)["fps_1280x800"], Verdict::Regressed);
    assert_eq!(bench::problems(&rows).len(), 1);

    let baseline = good();
    // Lower-is-better, well inside every budget, so again only the baseline speaks.
    let mut at_limit = good();
    at_limit.metrics.insert("rss_per_session_mb".into(), 180.0 * 1.10);
    assert_eq!(verdicts(&bench::compare(&at_limit, Some(&baseline)))["rss_per_session_mb"], Verdict::Ok);

    let mut over = good();
    over.metrics.insert("rss_per_session_mb".into(), 180.0 * 1.1001);
    let rows = bench::compare(&over, Some(&baseline));
    assert_eq!(verdicts(&rows)["rss_per_session_mb"], Verdict::Regressed);
    let problems = bench::problems(&rows);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("rss_per_session_mb") && problems[0].contains("10"), "{problems:?}");
}

#[test]
fn an_improvement_never_fails_and_is_reported_as_such() {
    let mut better = good();
    better.metrics.insert("decode_present_p95_ms".into(), 1.0);
    better.metrics.insert("fps_2560x1600".into(), 59.9);
    let rows = bench::compare(&better, Some(&good()));
    assert!(bench::problems(&rows).is_empty(), "{rows:#?}");
    let row = rows.iter().find(|r| r.metric.key == "decode_present_p95_ms").unwrap();
    assert_eq!(row.verdict, Verdict::Improved);
    assert!(row.change_pct.unwrap() > 0.0, "{row:?}");
}

#[test]
fn a_budget_miss_fails_even_without_a_regression() {
    let mut slow = good();
    slow.metrics.insert("decode_present_p95_ms".into(), 9.5);
    let rows = bench::compare(&slow, Some(&slow));
    assert_eq!(verdicts(&rows)["decode_present_p95_ms"], Verdict::BudgetMiss);
    let problems = bench::problems(&rows);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("8") && problems[0].contains("9.5"), "{problems:?}");
}

#[test]
fn without_a_baseline_the_budgets_still_decide() {
    let rows = bench::compare(&good(), None);
    assert!(rows.iter().all(|r| r.verdict == Verdict::NoBaseline), "{rows:#?}");
    assert!(bench::problems(&rows).is_empty());

    let mut slow = good();
    slow.metrics.insert("input_to_wire_p99_ms".into(), 3.0);
    let rows = bench::compare(&slow, None);
    assert_eq!(verdicts(&rows)["input_to_wire_p99_ms"], Verdict::BudgetMiss);
    assert_eq!(bench::problems(&rows).len(), 1);
}

#[test]
fn a_zero_baseline_is_not_a_division_by_zero() {
    let mut zero = good();
    zero.metrics.insert("input_to_wire_p99_ms".into(), 0.0);
    let rows = bench::compare(&good(), Some(&zero));
    let row = rows.iter().find(|r| r.metric.key == "input_to_wire_p99_ms").unwrap();
    assert_eq!(row.verdict, Verdict::NoBaseline);
    assert_eq!(row.change_pct, None);
    assert!(bench::problems(&rows).is_empty());
}

#[test]
fn reports_round_trip_through_json() {
    let json = serde_json::to_string_pretty(&good()).unwrap();
    let back: Report = serde_json::from_str(&json).unwrap();
    assert_eq!(back.metrics, good().metrics);
    // Unknown keys in a report are kept (the e2e suite may record more than the budgets).
    let extra: Report = serde_json::from_str(r#"{"metrics":{"fps_1280x800":60.0,"mystery":1.0}}"#).unwrap();
    assert_eq!(extra.metrics.len(), 2);
    assert!(extra.notes.is_empty());
}

#[test]
fn the_table_names_every_metric_its_unit_and_its_budget() {
    let table = bench::table(&bench::compare(&good(), Some(&good())));
    for m in bench::METRICS {
        assert!(table.contains(m.label), "{} missing from\n{table}", m.label);
        assert!(table.contains(m.unit), "{} missing from\n{table}", m.unit);
    }
    assert!(table.contains("60.0"), "{table}");
    assert!(table.contains("baseline"), "{table}");
}

#[test]
fn the_stored_baseline_is_committed_and_complete() {
    // `cargo xtask e2e --bench` compares against this file; a missing metric would silently
    // disable the regression gate for it.
    let path = xtask::repo::root().join(bench::BASELINE_PATH);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let baseline: Report = serde_json::from_str(&text).expect("the baseline is valid JSON");
    for m in bench::METRICS {
        assert!(baseline.metrics.contains_key(m.key), "the baseline records {}", m.key);
    }
    // The baseline is a measurement of a passing run: replaying it must not fail the gate.
    assert!(
        bench::problems(&bench::compare(&baseline, Some(&baseline))).is_empty(),
        "{:#?}",
        baseline.metrics
    );
}

// --- budget waivers (M9-1: the homelab's encoder, not Drift) --------------------------------

/// A budget the reference host cannot deliver is waived **in the baseline, with a reason**, and
/// the 10 % regression rule keeps guarding the number.
#[test]
fn a_waived_budget_is_reported_but_does_not_fail_the_run() {
    let mut slow = good();
    slow.metrics.insert("fps_2560x1600".into(), 31.0);
    let mut baseline = bench::baseline_from(&slow);
    baseline.exceptions.insert("fps_2560x1600".into(), "the host's VAAPI encoder saturates".into());

    let rows = bench::compare(&slow, Some(&baseline));
    assert_eq!(verdicts(&rows)["fps_2560x1600"], Verdict::BudgetWaived);
    assert!(bench::problems(&rows).is_empty(), "{rows:#?}");
    let table = bench::table(&rows);
    assert!(table.contains("waived"), "the table says the budget was waived:\n{table}");

    // The waiver does not disable the regression gate.
    let mut worse = slow.clone();
    worse.metrics.insert("fps_2560x1600".into(), 27.0);
    let rows = bench::compare(&worse, Some(&baseline));
    assert_eq!(verdicts(&rows)["fps_2560x1600"], Verdict::Regressed);
    assert_eq!(bench::problems(&rows).len(), 1);

    // Without the waiver the same number fails.
    let plain = bench::baseline_from(&slow);
    assert_eq!(verdicts(&bench::compare(&slow, Some(&plain)))["fps_2560x1600"], Verdict::BudgetMiss);
}

#[test]
fn every_waiver_in_the_stored_baseline_gives_a_reason() {
    let path = xtask::repo::root().join(bench::BASELINE_PATH);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let baseline: Report = serde_json::from_str(&text).expect("the baseline is valid JSON");
    for (key, reason) in &baseline.exceptions {
        assert!(bench::METRICS.iter().any(|m| m.key == key), "unknown metric {key} is waived");
        assert!(reason.len() > 40, "{key}: a waiver explains itself ({reason:?})");
    }
}

#[test]
fn small_values_keep_their_precision_in_the_table() {
    // An input latency of 48 microseconds must not print as "0.0 ms".
    let mut tiny = good();
    tiny.metrics.insert("input_to_wire_p99_ms".into(), 0.048);
    let table = bench::table(&bench::compare(&tiny, None));
    assert!(table.contains("0.048"), "{table}");
}

// --- measurement noise ----------------------------------------------------------------------

/// A regression must be both more than 10 % *and* bigger than the metric's noise floor.
/// Without the floor the gate cries wolf: the measured latencies are around 1.4 ms, where
/// 10 % is 0.14 ms — less than the run-to-run spread on the reference host.
#[test]
fn a_change_within_the_noise_floor_is_not_a_regression() {
    let floor = |key: &str| bench::METRICS.iter().find(|m| m.key == key).expect(key).noise_floor;
    // Measured run-to-run spread on the reference pair while the dev machine also builds:
    // 1.3-2.5 ms decode+present, 0.04-0.27 ms input, 59.9-61.0 fps, 37.9-39.1 MB.
    assert_eq!(floor("decode_present_p95_ms"), 2.0);
    assert_eq!(floor("input_to_wire_p99_ms"), 0.5);
    assert_eq!(floor("fps_1280x800"), 3.0);
    assert_eq!(floor("rss_per_session_mb"), 10.0);

    let baseline = report(&[
        ("fps_1280x800", 61.0),
        ("fps_2560x1600", 58.0),
        ("decode_present_p95_ms", 1.5),
        ("input_to_wire_p99_ms", 0.04),
        ("rss_per_session_mb", 38.0),
    ]);
    // Nearly twice as slow in relative terms, 1.9 ms in absolute terms: inside the floor.
    let mut jittery = baseline.clone();
    jittery.metrics.insert("decode_present_p95_ms".into(), 3.4);
    assert_eq!(verdicts(&bench::compare(&jittery, Some(&baseline)))["decode_present_p95_ms"], Verdict::Ok);
    assert!(bench::problems(&bench::compare(&jittery, Some(&baseline))).is_empty());

    // Past the floor and past 10 %: a real regression.
    let mut slow = baseline.clone();
    slow.metrics.insert("decode_present_p95_ms".into(), 3.6);
    let rows = bench::compare(&slow, Some(&baseline));
    assert_eq!(verdicts(&rows)["decode_present_p95_ms"], Verdict::Regressed);
    assert_eq!(bench::problems(&rows).len(), 1);
}
