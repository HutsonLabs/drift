//! Secret scan: fail if any credential from the dev-machine secrets directory appears in a
//! repository file, as raw bytes/UTF-8 or as UTF-16LE (the encoding RDP uses on the wire,
//! which is how a captured PDU fixture could leak one).
//!
//! Secret extraction from `*.txt` files: every non-empty line is trimmed; a `Key: value`
//! line contributes `value`. A value is treated as a secret when it is at least
//! [`MIN_SECRET_LEN`] bytes long and mixes at least two character classes among
//! lowercase, uppercase, digits and symbols (`-`, `_`, `.` and whitespace do not count as
//! symbols). This keeps passwords, one-time tokens and hex blobs while ignoring plain
//! user names such as `drifttest` that legitimately appear in docs.
//!
//! Reports never contain the secret value, only the secrets file name, line and encoding.

use std::path::{Path, PathBuf};

use anyhow::Result;

/// Minimum length of a scanned secret value.
pub const MIN_SECRET_LEN: usize = 8;

/// One secret value loaded from a secrets file.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret {
    /// Secrets file name (e.g. `system.txt`), for reports.
    pub source: String,
    /// 1-based line in the secrets file.
    pub line: usize,
    /// The value (never printed).
    pub value: String,
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Secret({}:{}, <redacted>)", self.source, self.line)
    }
}

/// Encoding in which a secret was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// Raw bytes / UTF-8.
    Utf8,
    /// UTF-16 little endian.
    Utf16Le,
}

/// A secret found in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Repository-relative file.
    pub path: PathBuf,
    /// Which secret (`source:line`).
    pub secret: String,
    /// Encoding matched.
    pub encoding: Encoding,
}

/// `true` if `value` looks like a credential (see module docs).
pub fn is_secret_like(value: &str) -> bool {
    let _ = value;
    false
}

/// Extracts secret values from one secrets file's text.
pub fn extract_secrets(source: &str, text: &str) -> Vec<Secret> {
    let _ = (source, text);
    Vec::new()
}

/// Loads secrets from every `*.txt` in `dir`. A missing directory yields no secrets.
pub fn load_secrets(dir: &Path) -> Result<Vec<Secret>> {
    let _ = dir;
    Ok(Vec::new())
}

/// Encodings of `value` to search for.
pub fn needles(value: &str) -> Vec<(Encoding, Vec<u8>)> {
    let _ = value;
    Vec::new()
}

/// Searches one file's bytes for every secret.
pub fn scan_bytes(path: &Path, data: &[u8], secrets: &[Secret]) -> Vec<Hit> {
    let _ = (path, data, secrets);
    Vec::new()
}

/// Scans repository-relative `files` under `root`.
pub fn scan_files(root: &Path, files: &[PathBuf], secrets: &[Secret]) -> Result<Vec<Hit>> {
    let _ = (root, files, secrets);
    Ok(Vec::new())
}

/// Default secrets directory: `$DRIFT_SECRETS_DIR` or `~/code/drift-spikes/secrets`.
pub fn default_secrets_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("DRIFT_SECRETS_DIR") {
        return Some(PathBuf::from(d));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("code/drift-spikes/secrets"))
}
