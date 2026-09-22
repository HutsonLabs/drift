//! The performance gate of `cargo xtask e2e --bench` (task M9-1).
//!
//! The e2e bench (`tests/e2e/tests/bench.rs`) measures against the real GNOME 50 host and
//! writes a JSON report; this module compares it with the committed baseline
//! ([`BASELINE_PATH`]) and decides whether the run fails:
//!
//! * a metric the plan budgets is **missing** → failure (the gate must not go quiet);
//! * a metric **misses its budget** → failure;
//! * a metric is **more than [`REGRESSION_TOLERANCE`] worse** than the baseline → failure
//!   (plan M9-1: "a regression of more than 10 % fails the nightly run").
//!
//! Everything here is pure; `main.rs` only reads and writes the files.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The committed baseline, relative to the workspace root.
pub const BASELINE_PATH: &str = "tests/e2e/bench-baseline.json";

/// A run may be this much worse than the baseline before it fails (plan M9-1: 10 %).
pub const REGRESSION_TOLERANCE: f64 = 0.10;

/// Which way a metric improves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// More is better (frames per second).
    HigherIsBetter,
    /// Less is better (latency, memory).
    LowerIsBetter,
}

/// One reported number and the budget plan M9-1 gives it.
#[derive(Debug, Clone, Copy)]
pub struct Metric {
    /// Key in the JSON report.
    pub key: &'static str,
    /// Human name for the table.
    pub label: &'static str,
    /// Unit for the table.
    pub unit: &'static str,
    /// Which way it improves.
    pub direction: Direction,
    /// The plan's budget: a minimum for [`Direction::HigherIsBetter`], a maximum otherwise.
    pub budget: f64,
    /// Run-to-run spread on the reference host. A change smaller than this is noise, never a
    /// regression, however large it looks in percent (a 1.4 ms latency moves by 10 % between
    /// two identical runs).
    pub noise_floor: f64,
}

/// The five numbers plan M9-1 budgets, in report order.
pub const METRICS: &[Metric] = &[
    Metric {
        key: "fps_1280x800",
        label: "frame rate at 1280x800",
        unit: "fps",
        direction: Direction::HigherIsBetter,
        budget: 60.0,
        noise_floor: 3.0,
    },
    Metric {
        key: "fps_2560x1600",
        label: "frame rate at 2560x1600",
        unit: "fps",
        direction: Direction::HigherIsBetter,
        budget: 55.0,
        noise_floor: 3.0,
    },
    Metric {
        key: "decode_present_p95_ms",
        label: "decode + present p95",
        unit: "ms",
        direction: Direction::LowerIsBetter,
        budget: 8.0,
        noise_floor: 2.0,
    },
    Metric {
        key: "input_to_wire_p99_ms",
        label: "input to wire p99",
        unit: "ms",
        direction: Direction::LowerIsBetter,
        budget: 2.0,
        noise_floor: 0.5,
    },
    Metric {
        key: "rss_per_session_mb",
        label: "RSS per session",
        unit: "MB",
        direction: Direction::LowerIsBetter,
        budget: 300.0,
        noise_floor: 10.0,
    },
];

/// A measured run, as the e2e suite writes it (and, through [`baseline_from`], the stored
/// baseline).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Report {
    /// Metric key → value.
    pub metrics: BTreeMap<String, f64>,
    /// Context that is printed but never gated (host load, GPU, raw samples).
    #[serde(default)]
    pub notes: BTreeMap<String, String>,
    /// Budgets the reference host itself cannot deliver, each with the reason it is waived
    /// (only meaningful in the baseline). A waived budget is still reported, and the 10 %
    /// regression rule still applies to the number.
    #[serde(default)]
    pub exceptions: BTreeMap<String, String>,
}

/// What the gate decided about one metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The report does not contain it.
    Missing,
    /// It misses the plan's budget.
    BudgetMiss,
    /// It misses the plan's budget, and the baseline waives that budget with a reason.
    BudgetWaived,
    /// More than [`REGRESSION_TOLERANCE`] worse than the baseline.
    Regressed,
    /// Within budget; there is no baseline to compare with.
    NoBaseline,
    /// Within budget and measurably better than the baseline.
    Improved,
    /// Within budget and within tolerance of the baseline.
    Ok,
}

impl Verdict {
    /// Whether this verdict fails the run.
    #[must_use]
    pub fn is_failure(self) -> bool {
        matches!(self, Verdict::Missing | Verdict::BudgetMiss | Verdict::Regressed)
    }

    /// The word printed in the table.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Verdict::Missing => "MISSING",
            Verdict::BudgetMiss => "OVER BUDGET",
            Verdict::BudgetWaived => "over budget (waived)",
            Verdict::Regressed => "REGRESSED",
            Verdict::NoBaseline => "ok (no baseline)",
            Verdict::Improved => "ok (improved)",
            Verdict::Ok => "ok",
        }
    }
}

/// One row of the comparison table.
#[derive(Debug, Clone)]
pub struct Row {
    /// Which metric.
    pub metric: Metric,
    /// The measured value, if the report had one.
    pub measured: Option<f64>,
    /// The baseline value, if there is a usable one.
    pub baseline: Option<f64>,
    /// Relative change in the *good* direction (`0.05` = 5 % better), if comparable.
    pub change_pct: Option<f64>,
    /// The gate's decision.
    pub verdict: Verdict,
}

/// Whether `value` meets `metric`'s budget.
fn within_budget(metric: &Metric, value: f64) -> bool {
    match metric.direction {
        Direction::HigherIsBetter => value >= metric.budget,
        Direction::LowerIsBetter => value <= metric.budget,
    }
}

/// Relative improvement of `measured` over `baseline`, in the metric's good direction.
fn change(metric: &Metric, measured: f64, baseline: f64) -> Option<f64> {
    if !baseline.is_finite() || baseline <= 0.0 || !measured.is_finite() {
        return None;
    }
    Some(match metric.direction {
        Direction::HigherIsBetter => (measured - baseline) / baseline,
        Direction::LowerIsBetter => (baseline - measured) / baseline,
    })
}

/// Compares `report` with `baseline` (when there is one) over every [`METRICS`] entry.
#[must_use]
pub fn compare(report: &Report, baseline: Option<&Report>) -> Vec<Row> {
    METRICS
        .iter()
        .map(|metric| {
            let measured = report.metrics.get(metric.key).copied().filter(|v| v.is_finite());
            let base = baseline.and_then(|b| b.metrics.get(metric.key)).copied();
            let Some(measured) = measured else {
                return Row {
                    metric: *metric,
                    measured: None,
                    baseline: base,
                    change_pct: None,
                    verdict: Verdict::Missing,
                };
            };
            let change_pct = base.and_then(|b| change(metric, measured, b));
            // Exactly 10 % worse still passes: the plan fails *more than* 10 %. A change that
            // is inside the metric's noise floor is never a regression, however large the
            // percentage looks.
            let past_floor = base.is_some_and(|b| (measured - b).abs() > metric.noise_floor);
            let regressed = past_floor && change_pct.is_some_and(|c| c + REGRESSION_TOLERANCE < -1e-9);
            let over_budget = !within_budget(metric, measured);
            let waived = baseline.is_some_and(|b| b.exceptions.contains_key(metric.key));
            let verdict = match () {
                () if over_budget && !waived => Verdict::BudgetMiss,
                () if regressed => Verdict::Regressed,
                () if over_budget => Verdict::BudgetWaived,
                () => match change_pct {
                    Some(c) if c > 0.0 => Verdict::Improved,
                    Some(_) => Verdict::Ok,
                    None => Verdict::NoBaseline,
                },
            };
            Row { metric: *metric, measured: Some(measured), baseline: base, change_pct, verdict }
        })
        .collect()
}

/// One message per failing row, empty when the run passes.
#[must_use]
pub fn problems(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .filter(|r| r.verdict.is_failure())
        .map(|r| {
            let m = &r.metric;
            match r.verdict {
                Verdict::Missing => {
                    format!("{}: the bench report has no value for `{}`", m.label, m.key)
                }
                Verdict::BudgetMiss => {
                    let (op, budget) = match m.direction {
                        Direction::HigherIsBetter => ("below the", m.budget),
                        Direction::LowerIsBetter => ("over the", m.budget),
                    };
                    format!(
                        "{} (`{}`): {} {} is {op} {} {} budget (plan M9-1)",
                        m.label,
                        m.key,
                        format_value(r.measured),
                        m.unit,
                        budget,
                        m.unit
                    )
                }
                _ => format!(
                    "{} (`{}`): {} {} is {:.1} % worse than the baseline {} {} (more than 10 % fails)",
                    m.label,
                    m.key,
                    format_value(r.measured),
                    m.unit,
                    r.change_pct.unwrap_or(0.0).abs() * 100.0,
                    format_value(r.baseline),
                    m.unit
                ),
            }
        })
        .collect()
}

/// A measured value with enough decimals to stay readable: an input latency of 48 microseconds
/// must not print as `0.0`.
fn format_value(value: Option<f64>) -> String {
    value.map_or_else(
        || "-".to_owned(),
        |v| match v.abs() {
            x if x >= 10.0 => format!("{v:.1}"),
            x if x >= 1.0 => format!("{v:.2}"),
            _ => format!("{v:.3}"),
        },
    )
}

/// The human-readable report table.
#[must_use]
pub fn table(rows: &[Row]) -> String {
    let mut out = format!(
        "{:<26} {:>10} {:>10} {:>9} {:>10}  {}\n",
        "metric", "measured", "baseline", "change", "budget", "verdict"
    );
    for row in rows {
        let m = &row.metric;
        let budget = match m.direction {
            Direction::HigherIsBetter => format!(">= {:.1}", m.budget),
            Direction::LowerIsBetter => format!("<= {:.1}", m.budget),
        };
        out.push_str(&format!(
            "{:<26} {:>10} {:>10} {:>9} {:>10}  {}\n",
            format!("{} ({})", m.label, m.unit),
            format_value(row.measured),
            format_value(row.baseline),
            row.change_pct.map_or_else(|| "-".to_owned(), |c| format!("{:+.1}%", c * 100.0)),
            budget,
            row.verdict.word()
        ));
    }
    out
}

/// A baseline holding only the gated metrics of `report` (what `--update-baseline` writes).
#[must_use]
pub fn baseline_from(report: &Report) -> Report {
    Report {
        metrics: METRICS
            .iter()
            .filter_map(|m| report.metrics.get(m.key).map(|v| (m.key.to_owned(), *v)))
            .collect(),
        notes: report.notes.clone(),
        exceptions: report.exceptions.clone(),
    }
}
