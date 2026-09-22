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

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::sanitize::{self, KnownSecret};

/// Provenance file name inside the staging directory.
pub const PROVENANCE_FILE: &str = "provenance.toml";
/// Checksum manifest written into `fixtures/`.
pub const MANIFEST_FILE: &str = "MANIFEST.sha256";
/// Provenance README written into `fixtures/`.
pub const README_FILE: &str = "README.md";

/// Files in `fixtures/` that are not fixtures (never listed, never pruned).
const META_FILES: &[&str] = &[MANIFEST_FILE, README_FILE];

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
    /// A golden image (or decoded frame) for render/decode tests.
    Golden,
    /// Reference data that was not captured from g-r-d (specification examples, synthetic
    /// streams).
    Reference,
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
    let p: Provenance = toml::from_str(text).context("parsing provenance.toml")?;
    let mut seen = BTreeSet::new();
    for f in &p.files {
        check_rel_path(&f.path)?;
        ensure!(seen.insert(f.path.as_str()), "{} is listed twice", f.path);
        ensure!(!META_FILES.contains(&f.path.as_str()), "{} is reserved", f.path);
    }
    Ok(p)
}

/// A fixture path must be relative, normal and `/`-separated.
fn check_rel_path(rel: &str) -> Result<()> {
    let p = Path::new(rel);
    ensure!(
        !rel.is_empty() && !rel.contains('\\') && p.components().all(|c| matches!(c, Component::Normal(_))),
        "fixture path {rel:?} must be relative without `..`"
    );
    Ok(())
}

/// Lower-case hex SHA-256 of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Renders the manifest (`<sha256>  <path>` lines, sorted by path, `sha256sum` format).
pub fn render_manifest(entries: &[ManifestEntry]) -> String {
    let mut sorted: Vec<&ManifestEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    sorted.iter().map(|e| format!("{}  {}\n", e.sha256, e.path)).collect()
}

/// Parses a manifest rendered by [`render_manifest`].
pub fn parse_manifest(text: &str) -> Result<Vec<ManifestEntry>> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let (sha, path) =
            line.split_once("  ").with_context(|| format!("manifest line {}: {line:?}", i + 1))?;
        ensure!(
            sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "manifest line {}: bad sha256",
            i + 1
        );
        check_rel_path(path)?;
        out.push(ManifestEntry { sha256: sha.to_owned(), path: path.to_owned() });
    }
    Ok(out)
}

/// Every regular file under `dir`, as sorted `/`-separated relative paths.
fn list_tree(dir: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = match std::fs::read_dir(&d) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && d == dir => return Ok(out),
            Err(e) => return Err(e).with_context(|| format!("reading {}", d.display())),
        };
        for e in entries {
            let p = e?.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p.strip_prefix(dir).context("path outside tree")?;
                let parts: Vec<String> =
                    rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
                out.push(parts.join("/"));
            }
        }
    }
    out.retain(|p| !p.ends_with(".DS_Store"));
    out.sort();
    Ok(out)
}

/// Checks `dir/MANIFEST.sha256` against the files in `dir`. Returns human-readable
/// problems (checksum mismatch, missing file, file not in manifest); empty means OK.
pub fn verify_manifest(dir: &Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(dir.join(MANIFEST_FILE))
        .with_context(|| format!("reading {}", dir.join(MANIFEST_FILE).display()))?;
    let entries = parse_manifest(&text)?;
    let mut problems = Vec::new();
    let listed: BTreeSet<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    for e in &entries {
        match std::fs::read(dir.join(&e.path)) {
            Ok(data) if sha256_hex(&data) == e.sha256 => {}
            Ok(_) => problems.push(format!("{}: checksum mismatch", e.path)),
            Err(err) => problems.push(format!("{}: missing ({err})", e.path)),
        }
    }
    for f in list_tree(dir)? {
        if !listed.contains(f.as_str()) && !META_FILES.contains(&f.as_str()) {
            problems.push(format!("{f}: not in {MANIFEST_FILE}"));
        }
    }
    Ok(problems)
}

/// Imports `staging` into `dest` (see module docs), sanitizing with `known` secrets plus
/// the one-time secrets found in the staged redirection/RDSTLS PDUs.
pub fn import(staging: &Path, dest: &Path, known: &[KnownSecret]) -> Result<ImportReport> {
    let prov_text = std::fs::read_to_string(staging.join(PROVENANCE_FILE))
        .with_context(|| format!("reading {}", staging.join(PROVENANCE_FILE).display()))?;
    let prov = parse_provenance(&prov_text)?;

    // The staging tree must match the provenance exactly.
    let staged: BTreeSet<String> = list_tree(staging)?.into_iter().filter(|p| p != PROVENANCE_FILE).collect();
    let listed: BTreeSet<String> = prov.files.iter().map(|f| f.path.clone()).collect();
    if let Some(extra) = staged.difference(&listed).next() {
        bail!("{extra} is in the staging directory but not in {PROVENANCE_FILE}");
    }
    if let Some(missing) = listed.difference(&staged).next() {
        bail!("{missing} is listed in {PROVENANCE_FILE} but missing from the staging directory");
    }

    // Load everything and collect the one-time credentials first: the same credentials
    // appear in the redirection PDU of one leg and the RDSTLS AuthRequest of the next.
    let mut staged_data = Vec::with_capacity(prov.files.len());
    let mut secrets: Vec<KnownSecret> = known.to_vec();
    let mut one_time = 0;
    for f in &prov.files {
        let data = std::fs::read(staging.join(&f.path)).with_context(|| format!("reading {}", f.path))?;
        let extracted = match f.kind {
            FileKind::ServerRedirection => sanitize::one_time_from_server_redirection(&f.path, &data)
                .with_context(|| format!("{}: not a Server Redirection PDU", f.path))?,
            FileKind::RdstlsAuthRequest => sanitize::one_time_from_rdstls_auth_request(&f.path, &data)
                .with_context(|| format!("{}: not an RDSTLS AuthRequest", f.path))?,
            _ => Vec::new(),
        };
        one_time += extracted.len();
        secrets.extend(extracted);
        staged_data.push(data);
    }

    let mut report = ImportReport { one_time_secrets: one_time, ..ImportReport::default() };
    let mut manifest = Vec::with_capacity(prov.files.len());
    let mut rows = Vec::with_capacity(prov.files.len());
    for (f, data) in prov.files.iter().zip(&staged_data) {
        let (clean, n) = sanitize::sanitize(data, &secrets);
        let left = sanitize::find(&clean, &secrets);
        ensure!(left.is_empty(), "{}: secrets survived sanitizing: {left:?}", f.path);
        let out = dest.join(&f.path);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        if std::fs::read(&out).ok().as_deref() != Some(clean.as_slice()) {
            std::fs::write(&out, &clean).with_context(|| format!("writing {}", out.display()))?;
        }
        let entry = ManifestEntry { sha256: sha256_hex(&clean), path: f.path.clone() };
        rows.push((f.clone(), entry.clone(), clean.len(), n));
        manifest.push(entry);
        report.replacements += n;
        report.files += 1;
    }

    // Prune stale fixtures from earlier imports.
    for existing in list_tree(dest)? {
        if !listed.contains(&existing) && !META_FILES.contains(&existing.as_str()) {
            std::fs::remove_file(dest.join(&existing)).with_context(|| format!("removing {existing}"))?;
            report.removed.push(existing);
        }
    }
    remove_empty_dirs(dest)?;

    std::fs::write(dest.join(MANIFEST_FILE), render_manifest(&manifest))?;
    std::fs::write(dest.join(README_FILE), render_readme(&prov, &rows))?;
    Ok(report)
}

fn remove_empty_dirs(dir: &Path) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(()) };
    let subdirs: Vec<PathBuf> =
        entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir()).collect();
    for d in subdirs {
        remove_empty_dirs(&d)?;
        if std::fs::read_dir(&d)?.next().is_none() {
            std::fs::remove_dir(&d)?;
        }
    }
    Ok(())
}

fn kind_title(kind: FileKind) -> &'static str {
    match kind {
        FileKind::H264 => "H.264 elementary streams (Annex-B)",
        FileKind::GfxServerRaw | FileKind::GfxServer | FileKind::GfxClient => {
            "Graphics pipeline (GFX DVC) streams"
        }
        FileKind::ServerRedirection
        | FileKind::TargetCert
        | FileKind::TlsCert
        | FileKind::RdstlsCaps
        | FileKind::RdstlsAuthRequest
        | FileKind::RdstlsAuthResponse
        | FileKind::FastpathPointer => "PDUs, certificates and fast-path updates",
        FileKind::Clipboard => "Clipboard (CLIPRDR format data)",
        FileKind::Screenshot | FileKind::Golden => "Images (screenshots and goldens)",
        FileKind::Reference => "Reference data (not captured from g-r-d)",
    }
}

/// Renders `fixtures/README.md`.
fn render_readme(prov: &Provenance, rows: &[(FileEntry, ManifestEntry, usize, usize)]) -> String {
    let mut s = String::new();
    let c = &prov.capture;
    let _ = writeln!(s, "# Drift wire fixtures\n");
    let _ = writeln!(
        s,
        "Generated by `cargo xtask import-fixtures` (task M0-3). **Do not edit by hand**: change the \
         staging directory's `{PROVENANCE_FILE}` and re-import. Files are stored with git-lfs \
         (`git lfs install --local && git lfs pull`); load them in tests through \
         `drift_testkit::fixtures`. `{MANIFEST_FILE}` holds the SHA-256 of every file and is \
         verified by `drift-testkit`'s `fixture_manifest_checksums_match` test.\n"
    );
    let _ = writeln!(s, "## Provenance\n");
    let _ = writeln!(s, "- **Captured:** {}", c.date);
    let _ = writeln!(s, "- **Host:** {}", c.host);
    let _ = writeln!(s, "- **Tool:** {}", c.tool);
    if let Some(n) = &c.notes {
        let _ = writeln!(s, "\n{}", n.trim());
    }
    let _ = writeln!(s, "\n## Sanitization\n");
    let _ = writeln!(
        s,
        "Every file was sanitized on import: each known secret (every value in the dev machine's \
         secrets directory, plus the one-time user name and password blob found in each Server \
         Redirection PDU and RDSTLS AuthRequest) is replaced, as raw bytes, UTF-8 and UTF-16LE, by \
         a same-length placeholder cycling `REDACTED`, so PDU length fields stay valid. The import \
         fails if any secret survives. The `Replaced` column counts replacements per file.\n"
    );
    let _ = writeln!(s, "## Record format (`*.rec`)\n");
    let _ = writeln!(
        s,
        "A sequence of records, each a little-endian `u32` byte length followed by the bytes: one \
         GFX DVC payload per record (`*.server.raw.rec`: as received, ZGFX `RDP_SEGMENTED_DATA`; \
         `*.server.rec`: the same payload after ZGFX decompression, i.e. concatenated GFX PDUs; \
         `*.client.rec`: GFX PDUs the client sent), or one fast-path output PDU per record.\n"
    );
    let mut sections: Vec<&'static str> = Vec::new();
    for (f, _, _, _) in rows {
        let t = kind_title(f.kind);
        if !sections.contains(&t) {
            sections.push(t);
        }
    }
    for title in sections {
        let _ = writeln!(s, "## {title}\n");
        let _ = writeln!(s, "| File | Bytes | SHA-256 | Replaced | Source | Contents |");
        let _ = writeln!(s, "|---|---:|---|---:|---|---|");
        for (f, m, size, n) in rows.iter().filter(|(f, _, _, _)| kind_title(f.kind) == title) {
            let _ = writeln!(
                s,
                "| `{}` | {} | `{}…` | {} | {} | {} |",
                f.path,
                size,
                m.sha256.get(..12).unwrap_or_default(),
                n,
                f.source.replace('|', "\\|"),
                f.description.replace('|', "\\|")
            );
        }
        let _ = writeln!(s);
    }
    s
}
