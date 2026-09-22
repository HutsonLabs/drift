//! Line-coverage gate (plan §0): ≥ 85 % for `drift-core`, `drift-input`,
//! `drift-clipboard`, `drift-gfx` and `drift-rdp::redirect`/`rdstls`.
//!
//! Input is the JSON summary written by
//! `cargo llvm-cov nextest --json --summary-only` (llvm-cov export format).
//! Each [`Target`] aggregates the line counts of files whose repository-relative path
//! starts with one of its prefixes. A target with **zero instrumented lines** is reported
//! as "not yet enforced" and does not fail the gate: the gate switches on automatically
//! the moment the owning task lands code in that crate or module.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Minimum line coverage, in percent.
pub const THRESHOLD: f64 = 85.0;

/// A coverage-gated unit of code.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// Display name.
    pub name: &'static str,
    /// Repository-relative path prefixes of the target's source files.
    pub prefixes: &'static [&'static str],
    /// Minimum line coverage in percent.
    pub min_percent: f64,
}

/// The targets from plan §0.
pub const TARGETS: &[Target] = &[
    Target { name: "drift-core", prefixes: &["crates/drift-core/src/"], min_percent: THRESHOLD },
    Target { name: "drift-input", prefixes: &["crates/drift-input/src/"], min_percent: THRESHOLD },
    Target { name: "drift-clipboard", prefixes: &["crates/drift-clipboard/src/"], min_percent: THRESHOLD },
    Target { name: "drift-gfx", prefixes: &["crates/drift-gfx/src/"], min_percent: THRESHOLD },
    Target {
        name: "drift-rdp::redirect+rdstls",
        prefixes: &["crates/drift-rdp/src/redirect", "crates/drift-rdp/src/rdstls"],
        min_percent: THRESHOLD,
    },
];

/// Result for one target.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetResult {
    /// Target name.
    pub name: &'static str,
    /// Instrumented lines.
    pub lines: u64,
    /// Covered lines.
    pub covered: u64,
    /// Required percentage.
    pub min_percent: f64,
}

impl TargetResult {
    /// Coverage percentage (100 when there are no lines).
    pub fn percent(&self) -> f64 {
        if self.lines == 0 {
            return 100.0;
        }
        // Line counts are far below 2^52, so the conversions are exact.
        #[allow(clippy::cast_precision_loss)]
        let pct = self.covered as f64 * 100.0 / self.lines as f64;
        pct
    }

    /// `true` when the target has code (so the gate applies).
    pub fn enforced(&self) -> bool {
        self.lines > 0
    }

    /// `true` when the target passes (or is not yet enforced).
    pub fn passed(&self) -> bool {
        !self.enforced() || self.percent() + 1e-9 >= self.min_percent
    }
}

/// Evaluates `targets` against an llvm-cov JSON summary. File names in the JSON are
/// absolute; `root` is stripped to get repository-relative paths.
pub fn evaluate(json: &str, root: &Path, targets: &[Target]) -> Result<Vec<TargetResult>> {
    let doc: Export = serde_json::from_str(json).context("parsing llvm-cov JSON summary")?;
    let mut results: Vec<TargetResult> = targets
        .iter()
        .map(|t| TargetResult { name: t.name, lines: 0, covered: 0, min_percent: t.min_percent })
        .collect();
    for file in doc.data.iter().flat_map(|d| &d.files) {
        let path = Path::new(&file.filename);
        let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy();
        for (t, r) in targets.iter().zip(&mut results) {
            if t.prefixes.iter().any(|p| rel.starts_with(p)) {
                r.lines += file.summary.lines.count;
                r.covered += file.summary.lines.covered;
            }
        }
    }
    Ok(results)
}

#[derive(Deserialize)]
struct Export {
    data: Vec<ExportData>,
}

#[derive(Deserialize)]
struct ExportData {
    files: Vec<ExportFile>,
}

#[derive(Deserialize)]
struct ExportFile {
    filename: String,
    summary: FileSummary,
}

#[derive(Deserialize)]
struct FileSummary {
    lines: LineSummary,
}

#[derive(Deserialize)]
struct LineSummary {
    count: u64,
    covered: u64,
}
