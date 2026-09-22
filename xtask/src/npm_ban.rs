//! "Bun, never npm" enforcement (plan §0; task M0-1 Red test).
//!
//! Two kinds of violation:
//! 1. A JS package-manager lockfile other than Bun's (`package-lock.json`,
//!    `npm-shrinkwrap.json`, `yarn.lock`, `pnpm-lock.yaml`, `.pnpmfile.cjs`) anywhere
//!    outside `third_party/`.
//! 2. An `npm`/`npx`/`yarn`/`pnpm` **invocation** in a script, workflow, doc or config
//!    file. An invocation is the tool name in command position (start of a line or
//!    after `&&`, `||`, `;`, `|`, `(`, `` ` ``, `$(`, `run:`, a quote, `- `, or `$ `) followed by
//!    an argument. Prose such as "never use `npm`" is not an invocation.
//!
//! Lines containing `npm-ban: allow` are skipped (for documenting the rule itself).

use std::path::{Path, PathBuf};

/// A single npm-ban violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// Repository-relative path.
    pub path: PathBuf,
    /// 1-based line number (0 for lockfiles).
    pub line: usize,
    /// Human-readable description.
    pub message: String,
}

/// Lockfile names that must never exist.
pub const FORBIDDEN_LOCKFILES: &[&str] =
    &["package-lock.json", "npm-shrinkwrap.json", "yarn.lock", "pnpm-lock.yaml", ".pnpmfile.cjs"];

/// `true` if `path`'s file name is a forbidden lockfile.
pub fn is_forbidden_lockfile(path: &Path) -> bool {
    let _ = path;
    false
}

/// `true` if the file type is scanned for invocations (scripts, workflows, docs, configs).
pub fn is_scanned_file(path: &Path) -> bool {
    let _ = path;
    false
}

/// Finds npm/npx/yarn/pnpm invocations in one file's text.
pub fn scan_text(path: &Path, content: &str) -> Vec<Violation> {
    let _ = (path, content);
    Vec::new()
}

/// Checks a list of repository-relative paths (reading contents from `root`).
/// Paths under `third_party/` are ignored (vendored code).
pub fn scan_files(root: &Path, files: &[PathBuf]) -> Vec<Violation> {
    let _ = (root, files);
    Vec::new()
}
