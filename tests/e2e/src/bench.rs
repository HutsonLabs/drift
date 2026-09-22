//! The performance report of `cargo xtask e2e --bench` (task M9-1).
//!
//! The bench test measures against the real GNOME host and records its numbers here; the
//! harness (`xtask::bench`) compares the JSON file with the stored baseline and fails the run
//! on a budget miss or a regression of more than 10 %.
//!
//! Every test process writes the same file, so [`BenchReport::write`] merges with what is
//! already there (`cargo xtask e2e` runs the suite with `--test-threads 1`).

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Set by `cargo xtask e2e --bench` to the report path; unset outside bench runs.
pub const REPORT_VAR: &str = "DRIFT_E2E_BENCH_REPORT";
/// Set by `cargo xtask e2e --bench`; also enables the frame-rate budget in `render.rs`.
pub const BENCH_VAR: &str = "DRIFT_E2E_BENCH";

/// A merge-on-write metrics file.
#[derive(Debug, Clone)]
pub struct BenchReport {
    path: PathBuf,
    metrics: BTreeMap<String, f64>,
    notes: BTreeMap<String, String>,
}

impl BenchReport {
    /// The report requested by `cargo xtask e2e --bench`, or `None` on a normal run.
    pub fn open() -> Option<Self> {
        let path = PathBuf::from(crate::var(REPORT_VAR)?);
        Some(Self { path, metrics: BTreeMap::new(), notes: BTreeMap::new() })
    }

    /// Records one metric (see `xtask::bench::METRICS` for the keys the gate expects).
    pub fn record(&mut self, key: &str, value: f64) {
        eprintln!("[bench] {key} = {value:.3}");
        self.metrics.insert(key.to_owned(), value);
    }

    /// Records context that is reported but never gated (host load, GPU name, …).
    pub fn note(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        eprintln!("[bench] {key}: {value}");
        self.notes.insert(key.to_owned(), value);
    }

    /// Merges the collected values into the report file.
    ///
    /// # Errors
    /// When the file cannot be read or written.
    pub fn write(&self) -> std::io::Result<()> {
        let mut metrics = BTreeMap::new();
        let mut notes = BTreeMap::new();
        if let Ok(text) = std::fs::read_to_string(&self.path)
            && let Ok(existing) = serde_json::from_str::<serde_json::Value>(&text)
        {
            merge(&mut metrics, existing.get("metrics"));
            merge_notes(&mut notes, existing.get("notes"));
        }
        metrics.extend(self.metrics.clone());
        notes.extend(self.notes.clone());
        let json = serde_json::json!({ "metrics": metrics, "notes": notes });
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, format!("{}\n", serde_json::to_string_pretty(&json)?))
    }
}

fn merge(into: &mut BTreeMap<String, f64>, value: Option<&serde_json::Value>) {
    for (k, v) in value.and_then(serde_json::Value::as_object).into_iter().flatten() {
        if let Some(n) = v.as_f64() {
            into.insert(k.clone(), n);
        }
    }
}

fn merge_notes(into: &mut BTreeMap<String, String>, value: Option<&serde_json::Value>) {
    for (k, v) in value.and_then(serde_json::Value::as_object).into_iter().flatten() {
        if let Some(s) = v.as_str() {
            into.insert(k.clone(), s.to_owned());
        }
    }
}

/// Resident set size of this process in MB, via `ps` (the humble object; the parsing is in
/// [`parse_ps_rss_kib`]).
pub fn rss_mb() -> Option<f64> {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    #[expect(clippy::cast_precision_loss, reason = "a memory figure in MB is a display value")]
    let mb = parse_ps_rss_kib(&String::from_utf8_lossy(&out.stdout))? as f64 / 1024.0;
    Some(mb)
}

/// The kibibyte figure `ps -o rss=` prints, ignoring surrounding whitespace.
pub fn parse_ps_rss_kib(output: &str) -> Option<u64> {
    output.trim().lines().next()?.trim().parse().ok()
}

/// The largest value in `samples`, or `0.0` when there is none.
pub fn peak(samples: &[f32]) -> f64 {
    f64::from(samples.iter().copied().fold(0.0_f32, f32::max))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps_output_is_a_kibibyte_count() {
        assert_eq!(parse_ps_rss_kib("  123456\n"), Some(123_456));
        assert_eq!(parse_ps_rss_kib("123456\n789\n"), Some(123_456));
        assert_eq!(parse_ps_rss_kib(""), None);
        assert_eq!(parse_ps_rss_kib("RSS\n"), None);
    }

    #[test]
    fn peak_of_no_samples_is_zero() {
        assert_eq!(peak(&[]), 0.0);
        assert!((peak(&[58.9, 60.1, 12.0]) - 60.1).abs() < 1e-5);
    }

    #[test]
    fn this_process_reports_a_plausible_rss() {
        let mb = rss_mb().expect("ps reports the resident set size");
        assert!(mb > 1.0 && mb < 100_000.0, "{mb} MB");
    }
}
