//! Fixture loader for `fixtures/` (git-lfs). Owned by task **M0-3**.

use std::path::PathBuf;

/// Absolute path of the repository's `fixtures/` directory.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}
