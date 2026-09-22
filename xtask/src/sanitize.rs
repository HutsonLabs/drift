//! Fixture sanitizer (task M0-3).
//!
//! Replaces every known secret in a fixture by a placeholder of the **same length**, so
//! offsets and length fields inside captured PDUs stay valid. Secrets come from two places:
//!
//! 1. the dev-machine secrets directory (`~/code/drift-spikes/secrets/*.txt`): every value
//!    line of at least [`MIN_TEXT_LEN`] characters (user names too, not only passwords),
//!    plus the decoded bytes of hex lines of at least [`MIN_HEX_BYTES`] bytes (the
//!    `lastredir.txt` one-time password and GUID blobs);
//! 2. the fixtures themselves: the one-time user name and password blob of every captured
//!    Server Redirection PDU and RDSTLS AuthRequest (plan §1.3). They are single-use and
//!    already invalid, but are removed anyway.
//!
//! Text secrets are matched as UTF-8 bytes and as UTF-16LE; byte secrets as raw bytes.
//! Placeholders cycle `REDACTED` (as UTF-16LE for UTF-16LE matches).

use std::path::Path;

use anyhow::Result;

/// Minimum length (in characters) of a text value treated as a secret.
pub const MIN_TEXT_LEN: usize = 6;
/// Minimum decoded length of a hex line treated as a byte secret.
pub const MIN_HEX_BYTES: usize = 8;
/// Placeholder pattern, cycled to the secret's length.
pub const PLACEHOLDER: &[u8] = b"REDACTED";

/// What a secret looks like.
#[derive(Clone, PartialEq, Eq)]
pub enum SecretValue {
    /// A string, matched as UTF-8 and as UTF-16LE.
    Text(String),
    /// An opaque blob, matched as raw bytes.
    Bytes(Vec<u8>),
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(t) => write!(f, "Text(<{} chars redacted>)", t.chars().count()),
            Self::Bytes(b) => write!(f, "Bytes(<{} bytes redacted>)", b.len()),
        }
    }
}

/// A secret to remove from fixtures. `Debug` never shows the value.
#[derive(Clone, PartialEq, Eq)]
pub struct KnownSecret {
    /// Where the secret came from (`system.txt:2`, `pdus/x.bin: one-time password`).
    pub label: String,
    /// The value.
    pub value: SecretValue,
}

impl std::fmt::Debug for KnownSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "KnownSecret({}, <redacted>)", self.label)
    }
}

/// Form in which a secret was matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// Raw bytes (byte secrets).
    Bytes,
    /// UTF-8 text.
    Utf8,
    /// UTF-16 little endian text.
    Utf16Le,
}

impl KnownSecret {
    /// A text secret.
    pub fn text(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self { label: label.into(), value: SecretValue::Text(value.into()) }
    }

    /// A byte secret.
    pub fn bytes(label: impl Into<String>, value: impl Into<Vec<u8>>) -> Self {
        Self { label: label.into(), value: SecretValue::Bytes(value.into()) }
    }

    /// `(form, needle, same-length placeholder)` for every form this secret is matched in.
    pub fn needles(&self) -> Vec<(Form, Vec<u8>, Vec<u8>)> {
        unimplemented!("M0-3")
    }
}

/// `n` bytes of the cycled [`PLACEHOLDER`].
pub fn placeholder(n: usize) -> Vec<u8> {
    let _ = n;
    unimplemented!("M0-3")
}

/// Secrets in one secrets file's text (see module docs).
pub fn secrets_from_text(source: &str, text: &str) -> Vec<KnownSecret> {
    let _ = (source, text);
    unimplemented!("M0-3")
}

/// Secrets from every `*.txt` in `dir`. A missing directory yields none.
pub fn load_known_secrets(dir: &Path) -> Result<Vec<KnownSecret>> {
    let _ = dir;
    unimplemented!("M0-3")
}

/// Replaces every occurrence of every secret. Returns the sanitized bytes and the number
/// of replacements. The output always has the input's length.
pub fn sanitize(data: &[u8], secrets: &[KnownSecret]) -> (Vec<u8>, usize) {
    let _ = (data, secrets);
    unimplemented!("M0-3")
}

/// Secrets (label, form) still present in `data`.
pub fn find(data: &[u8], secrets: &[KnownSecret]) -> Vec<(String, Form)> {
    let _ = (data, secrets);
    unimplemented!("M0-3")
}

/// The one-time user name and password blob of a captured Server Redirection PDU frame
/// (TPKT + X.224 + MCS Send Data Indication + Share Control PDU of type 0xA).
pub fn one_time_from_server_redirection(label: &str, frame: &[u8]) -> Result<Vec<KnownSecret>> {
    let _ = (label, frame);
    unimplemented!("M0-3")
}

/// The one-time user name and password blob of a captured RDSTLS AuthRequest.
pub fn one_time_from_rdstls_auth_request(label: &str, pdu: &[u8]) -> Result<Vec<KnownSecret>> {
    let _ = (label, pdu);
    unimplemented!("M0-3")
}
