//! `cargo xtask import-fixtures` (task M0-3).
//!
//! Copies captured artefacts from a staging directory into `fixtures/`, sanitizing each one
//! (see [`crate::sanitize`]), then writes `fixtures/MANIFEST.sha256` and
//! `fixtures/README.md` (provenance). The staging directory holds the files in their final
//! layout plus a [`PROVENANCE_FILE`] describing every file:
//!
//! ```toml
//! [capture]
//! date = "2026-09-22"
//! host = "GNOME 50 test host …"
//! tool = "probe2 + capture instrumentation"
//! notes = "optional free text"
//!
//! [[file]]
//! path = "h264/leg2.h264"
//! kind = "h264"
//! source = "Remote Login :3389, leg 2 (GDM greeter)"
//! description = "AVC420 Annex-B elementary stream …"
//! ```

use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

use crate::sanitize::KnownSecret;

/// Provenance file name inside the staging directory.
pub const PROVENANCE_FILE: &str = "provenance.toml";
/// Checksum manifest written into `fixtures/`.
pub const MANIFEST_FILE: &str = "MANIFEST.sha256";
/// Provenance README written into `fixtures/`.
pub const README_FILE: &str = "README.md";

/// Parsed [`PROVENANCE_FILE`].
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// Capture session information.
    pub capture: CaptureInfo,
    /// One entry per fixture file.
    #[serde(rename = "file")]
    pub files: Vec<FileEntry>,
}

/// Where and how the fixtures were captured.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureInfo {
    /// Capture date (ISO 8601).
    pub date: String,
    /// Host description.
    pub host: String,
    /// Capture tool description.
    pub tool: String,
    /// Free-form notes (procedure, caveats).
    #[serde(default)]
    pub notes: Option<String>,
}

/// Kind of a fixture file; drives one-time secret extraction and README grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileKind {
    /// Annex-B H.264 elementary stream.
    H264,
    /// Server→client GFX DVC payloads as received (ZGFX segmented), `.rec` records.
    GfxServerRaw,
    /// Server→client GFX PDUs after ZGFX decompression, `.rec` records.
    GfxServer,
    /// Client→server GFX PDUs (capabilities advertise, frame acks), `.rec` records.
    GfxClient,
    /// A full Server Redirection PDU frame (contains one-time credentials).
    ServerRedirection,
    /// The redirection target-certificate container.
    TargetCert,
    /// A TLS server certificate (DER).
    TlsCert,
    /// RDSTLS capabilities PDU.
    RdstlsCaps,
    /// RDSTLS AuthRequest PDU (contains one-time credentials).
    RdstlsAuthRequest,
    /// RDSTLS AuthResponse PDU.
    RdstlsAuthResponse,
    /// Fast-path pointer update PDUs, `.rec` records.
    FastpathPointer,
    /// CLIPRDR format data.
    Clipboard,
    /// A decoded screenshot.
    Screenshot,
    /// A golden image for render tests.
    Golden,
}

/// One fixture file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    /// Path relative to `fixtures/` (and to the staging directory).
    pub path: String,
    /// Kind.
    pub kind: FileKind,
    /// Where it was captured.
    pub source: String,
    /// What it contains.
    pub description: String,
}

/// One manifest line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    /// Lower-case hex SHA-256.
    pub sha256: String,
    /// Path relative to `fixtures/`.
    pub path: String,
}

/// Result of an import.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Files written.
    pub files: usize,
    /// Secret occurrences replaced.
    pub replacements: usize,
    /// One-time secrets extracted from redirection/RDSTLS fixtures.
    pub one_time_secrets: usize,
    /// Stale files removed from the destination.
    pub removed: Vec<String>,
}

/// Parses a [`PROVENANCE_FILE`].
pub fn parse_provenance(text: &str) -> Result<Provenance> {
    let _ = text;
    unimplemented!("M0-3")
}

/// Lower-case hex SHA-256 of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    let _ = data;
    unimplemented!("M0-3")
}

/// Renders the manifest (`<sha256>  <path>` lines, sorted by path, `sha256sum` format).
pub fn render_manifest(entries: &[ManifestEntry]) -> String {
    let _ = entries;
    unimplemented!("M0-3")
}

/// Parses a manifest rendered by [`render_manifest`].
pub fn parse_manifest(text: &str) -> Result<Vec<ManifestEntry>> {
    let _ = text;
    unimplemented!("M0-3")
}

/// Checks `dir/MANIFEST.sha256` against the files in `dir`. Returns human-readable
/// problems (checksum mismatch, missing file, file not in manifest); empty means OK.
pub fn verify_manifest(dir: &Path) -> Result<Vec<String>> {
    let _ = dir;
    unimplemented!("M0-3")
}

/// Imports `staging` into `dest` (see module docs), sanitizing with `known` secrets plus
/// the one-time secrets found in the staged redirection/RDSTLS PDUs.
pub fn import(staging: &Path, dest: &Path, known: &[KnownSecret]) -> Result<ImportReport> {
    let _ = (staging, dest, known);
    unimplemented!("M0-3")
}
