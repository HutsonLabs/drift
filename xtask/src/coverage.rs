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

use anyhow::Result;

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
        let _ = self;
        0.0
    }

    /// `true` when the target has code (so the gate applies).
    pub fn enforced(&self) -> bool {
        false
    }

    /// `true` when the target passes (or is not yet enforced).
    pub fn passed(&self) -> bool {
        false
    }
}

/// Evaluates `targets` against an llvm-cov JSON summary. File names in the JSON are
/// absolute; `root` is stripped to get repository-relative paths.
pub fn evaluate(json: &str, root: &Path, targets: &[Target]) -> Result<Vec<TargetResult>> {
    let _ = (json, root, targets);
    Ok(Vec::new())
}
