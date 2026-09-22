//! Repository discovery and file listing.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The workspace root (parent of the `xtask` crate).
pub fn root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().map(Path::to_path_buf).unwrap_or(manifest)
}

/// Tracked plus untracked-but-not-ignored files, repository-relative (`git ls-files`).
pub fn list_files(root: &Path) -> Result<Vec<PathBuf>> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["ls-files", "-z", "--cached", "--others", "--exclude-standard"])
        .output()
        .context("running git ls-files")?;
    if !out.status.success() {
        bail!("git ls-files failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    let mut files: Vec<PathBuf> = out
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| PathBuf::from(String::from_utf8_lossy(s).into_owned()))
        .filter(|p| root.join(p).is_file())
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}
