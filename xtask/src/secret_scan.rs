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
    if value.len() < MIN_SECRET_LEN {
        return false;
    }
    let has = |f: fn(&char) -> bool| value.chars().any(|c| f(&c));
    let classes = [
        has(char::is_ascii_lowercase),
        has(char::is_ascii_uppercase),
        has(char::is_ascii_digit),
        has(|c| !c.is_ascii_alphanumeric() && !matches!(c, '-' | '_' | '.') && !c.is_whitespace()),
    ];
    classes.iter().filter(|b| **b).count() >= 2
}

/// Extracts secret values from one secrets file's text.
pub fn extract_secrets(source: &str, text: &str) -> Vec<Secret> {
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let line = line.trim();
            let value = match line.split_once(": ") {
                Some((key, v))
                    if !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ') =>
                {
                    v.trim()
                }
                _ => line,
            };
            is_secret_like(value).then(|| Secret {
                source: source.to_owned(),
                line: i + 1,
                value: value.to_owned(),
            })
        })
        .collect()
}

/// Loads secrets from every `*.txt` in `dir`. A missing directory yields no secrets.
pub fn load_secrets(dir: &Path) -> Result<Vec<Secret>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "txt") && p.is_file())
        .collect();
    paths.sort();
    let mut out = Vec::new();
    for p in paths {
        let text = String::from_utf8_lossy(&std::fs::read(&p)?).into_owned();
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        out.extend(extract_secrets(&name, &text));
    }
    Ok(out)
}

/// Encodings of `value` to search for.
pub fn needles(value: &str) -> Vec<(Encoding, Vec<u8>)> {
    vec![
        (Encoding::Utf8, value.as_bytes().to_vec()),
        (Encoding::Utf16Le, value.encode_utf16().flat_map(u16::to_le_bytes).collect()),
    ]
}

/// Searches one file's bytes for every secret.
pub fn scan_bytes(path: &Path, data: &[u8], secrets: &[Secret]) -> Vec<Hit> {
    let mut hits = Vec::new();
    for s in secrets {
        for (encoding, needle) in needles(&s.value) {
            if memchr::memmem::find(data, &needle).is_some() {
                hits.push(Hit {
                    path: path.to_path_buf(),
                    secret: format!("{}:{}", s.source, s.line),
                    encoding,
                });
            }
        }
    }
    hits
}

/// Scans repository-relative `files` under `root`.
pub fn scan_files(root: &Path, files: &[PathBuf], secrets: &[Secret]) -> Result<Vec<Hit>> {
    let mut hits = Vec::new();
    for rel in files {
        let data = std::fs::read(root.join(rel))?;
        hits.extend(scan_bytes(rel, &data, secrets));
    }
    Ok(hits)
}

/// Default secrets directory: `$DRIFT_SECRETS_DIR` or `~/code/drift-spikes/secrets`.
pub fn default_secrets_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("DRIFT_SECRETS_DIR") {
        return Some(PathBuf::from(d));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("code/drift-spikes/secrets"))
}
