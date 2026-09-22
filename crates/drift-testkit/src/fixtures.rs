//! Loader for the wire fixtures in `fixtures/` (git-lfs), imported by
//! `cargo xtask import-fixtures` (task **M0-3**). See `fixtures/README.md` for provenance.
//!
//! ```no_run
//! use drift_testkit::fixtures::{self, names};
//! let stream = fixtures::read(names::H264_LEG2);
//! let pdus = fixtures::records(names::GFX_LEG2_GREETER_AVC420);
//! ```
//!
//! `*.rec` files are sequences of records, each a little-endian `u32` length followed by that
//! many bytes (one DVC payload, GFX PDU batch or fast-path PDU per record).
//!
//! The helpers that return plain values panic with an actionable message (for example when
//! git-lfs has not fetched the file); they are for tests only.

use std::path::PathBuf;

/// Fixture paths relative to `fixtures/`.
pub mod names {
    /// GDM greeter, Remote Login leg 2, AVC420 Annex-B (8 frames, 1280×800).
    pub const H264_LEG2: &str = "h264/leg2.h264";
    /// Remote Login leg 3 user session with full-screen motion, AVC420 Annex-B (431 frames).
    pub const H264_LEG3: &str = "h264/leg3.h264";
    /// Headless session with full-screen motion, AVC420 Annex-B (425 frames, 1280×800).
    pub const H264_HEADLESS_MOTION: &str = "h264/headless_motion.h264";
    /// Headless session at 2560×1600, desktop scale 200 (7 frames).
    pub const H264_HEADLESS_SCALE200: &str = "h264/headless_scale200.h264";

    /// Greeter GFX stream with caps `[V8_1{AVC420}, V8]`: decompressed server PDUs.
    pub const GFX_LEG2_GREETER_AVC420: &str = "gfx/leg2_greeter_avc420.server.rec";
    /// Same stream as received (ZGFX segmented DVC payloads).
    pub const GFX_LEG2_GREETER_AVC420_RAW: &str = "gfx/leg2_greeter_avc420.server.raw.rec";
    /// Same session: client→server GFX PDUs (caps advertise, frame acks).
    pub const GFX_LEG2_GREETER_AVC420_CLIENT: &str = "gfx/leg2_greeter_avc420.client.rec";
    /// Greeter GFX stream with caps `[V8_1{}]` (no AVC): RFX Progressive, decompressed.
    pub const GFX_LEG2_GREETER_PROGRESSIVE: &str = "gfx/leg2_greeter_progressive.server.rec";
    /// Same stream as received (ZGFX segmented).
    pub const GFX_LEG2_GREETER_PROGRESSIVE_RAW: &str = "gfx/leg2_greeter_progressive.server.raw.rec";
    /// Same session: client→server GFX PDUs.
    pub const GFX_LEG2_GREETER_PROGRESSIVE_CLIENT: &str = "gfx/leg2_greeter_progressive.client.rec";
    /// Headless full-screen motion, AVC420: decompressed server PDUs.
    pub const GFX_HEADLESS_MOTION_AVC420: &str = "gfx/headless_motion_avc420.server.rec";
    /// Same stream as received (ZGFX segmented).
    pub const GFX_HEADLESS_MOTION_AVC420_RAW: &str = "gfx/headless_motion_avc420.server.raw.rec";
    /// Same session: client→server GFX PDUs.
    pub const GFX_HEADLESS_MOTION_AVC420_CLIENT: &str = "gfx/headless_motion_avc420.client.rec";
    /// Headless 2560×1600 at scale 200, AVC420: decompressed server PDUs.
    pub const GFX_HEADLESS_SCALE200_AVC420: &str = "gfx/headless_scale200_avc420.server.rec";
    /// Same stream as received (ZGFX segmented).
    pub const GFX_HEADLESS_SCALE200_AVC420_RAW: &str = "gfx/headless_scale200_avc420.server.raw.rec";
    /// Same session: client→server GFX PDUs.
    pub const GFX_HEADLESS_SCALE200_AVC420_CLIENT: &str = "gfx/headless_scale200_avc420.client.rec";

    /// M1-4 capture: progressive greeter, DRFTGFX1 container (`DRFTGFX1` + `.rec`-style
    /// records of raw ZGFX payloads). Read by `drift-codec`'s replay tests.
    pub const GFX_GREETER_V81NOAVC: &str = "gfx/greeter_v81noavc.gfx";
    /// M1-4 capture: progressive headless desktop, DRFTGFX1 container.
    pub const GFX_HEADLESS_V81NOAVC: &str = "gfx/headless_v81noavc.gfx";
    /// M1-4 golden for [`GFX_GREETER_V81NOAVC`].
    pub const GOLDEN_GREETER_PROGRESSIVE: &str = "goldens/greeter_progressive.png";
    /// M1-4 golden for [`GFX_HEADLESS_V81NOAVC`].
    pub const GOLDEN_HEADLESS_PROGRESSIVE: &str = "goldens/headless_progressive.png";

    /// Server Redirection PDU frame received on leg 1 (TPKT…Share Control type 0xA), sanitized.
    pub const SERVER_REDIRECTION_LEG1: &str = "pdus/server_redirection_leg1.bin";
    /// Second Server Redirection PDU (after the greeter login, leg 2), sanitized.
    pub const SERVER_REDIRECTION_LEG2: &str = "pdus/server_redirection_leg2.bin";
    /// Target-certificate container from the leg-1 redirection (UTF-16LE base64).
    pub const TARGET_CERT_LEG1: &str = "pdus/target_cert_leg1.bin";
    /// Target-certificate container from the leg-2 redirection.
    pub const TARGET_CERT_LEG2: &str = "pdus/target_cert_leg2.bin";
    /// TLS certificate (DER) presented on Remote Login leg 1.
    pub const TLS_CERT_LEG1: &str = "pdus/tls_cert_leg1.der";
    /// TLS certificate (DER) presented on leg 2.
    pub const TLS_CERT_LEG2: &str = "pdus/tls_cert_leg2.der";
    /// TLS certificate (DER) presented on leg 3.
    pub const TLS_CERT_LEG3: &str = "pdus/tls_cert_leg3.der";
    /// TLS certificate (DER) of the drifttest2 headless daemon.
    pub const TLS_CERT_HEADLESS: &str = "pdus/tls_cert_headless.der";
    /// RDSTLS capabilities PDU sent by the server (`01 00 01 00 01 00 03 00`).
    pub const RDSTLS_CAPS: &str = "pdus/rdstls_caps.bin";
    /// RDSTLS AuthRequest sent on leg 2 (one-time credentials sanitized).
    pub const RDSTLS_AUTH_REQUEST_LEG2: &str = "pdus/rdstls_auth_request_leg2.bin";
    /// RDSTLS AuthRequest sent on leg 3 (sanitized).
    pub const RDSTLS_AUTH_REQUEST_LEG3: &str = "pdus/rdstls_auth_request_leg3.bin";
    /// RDSTLS AuthRequest replaying already-used one-time credentials (sanitized).
    pub const RDSTLS_AUTH_REQUEST_REUSED: &str = "pdus/rdstls_auth_request_reused.bin";
    /// RDSTLS AuthResponse, result 0 (success).
    pub const RDSTLS_AUTH_RESPONSE_SUCCESS: &str = "pdus/rdstls_auth_response_success.bin";
    /// RDSTLS AuthResponse, result `0x52E` (LOGON_FAILURE, reused credentials).
    pub const RDSTLS_AUTH_RESPONSE_LOGON_FAILURE: &str = "pdus/rdstls_auth_response_logon_failure.bin";
    /// Fast-path pointer update PDUs at desktop scale 100 (43×43 cursors).
    pub const FASTPATH_POINTER_SCALE100: &str = "pdus/fastpath_pointer_scale100.rec";
    /// Fast-path pointer update PDUs at desktop scale 200 (86×86 cursors, fragmented).
    pub const FASTPATH_POINTER_SCALE200: &str = "pdus/fastpath_pointer_scale200.rec";

    /// `image/png` (format id 0xD011) CLIPRDR data from the GNOME screenshot tool.
    pub const CLIP_REMOTE_PNG: &str = "clipboard/remote_clip_d011.bin";
    /// CF_UNICODETEXT CLIPRDR data from GNOME Text Editor (non-ASCII, NUL-terminated).
    pub const CLIP_REMOTE_UNICODETEXT: &str = "clipboard/remote_clip_d.bin";

    /// Golden: greeter frame 0 of [`H264_LEG2`] decoded BT.709 full range.
    pub const GOLDEN_GREETER: &str = "goldens/greeter.png";
    /// Screenshot: headless desktop (frame 0 of [`H264_HEADLESS_MOTION`]).
    pub const SCREENSHOT_DESKTOP_HEADLESS: &str = "screenshots/desktop_headless.png";
    /// Screenshot: 2560×1600 at scale 200 (frame 0 of [`H264_HEADLESS_SCALE200`]).
    pub const SCREENSHOT_RETINA200: &str = "screenshots/retina200.png";
    /// Screenshot: frame 200 of [`H264_LEG3`].
    pub const SCREENSHOT_LEG3_MOTION: &str = "screenshots/leg3_motion_frame200.png";

    /// Every fixture above.
    pub const ALL: &[&str] = &[
        H264_LEG2,
        H264_LEG3,
        H264_HEADLESS_MOTION,
        H264_HEADLESS_SCALE200,
        GFX_LEG2_GREETER_AVC420,
        GFX_LEG2_GREETER_AVC420_RAW,
        GFX_LEG2_GREETER_AVC420_CLIENT,
        GFX_LEG2_GREETER_PROGRESSIVE,
        GFX_LEG2_GREETER_PROGRESSIVE_RAW,
        GFX_LEG2_GREETER_PROGRESSIVE_CLIENT,
        GFX_HEADLESS_MOTION_AVC420,
        GFX_HEADLESS_MOTION_AVC420_RAW,
        GFX_HEADLESS_MOTION_AVC420_CLIENT,
        GFX_HEADLESS_SCALE200_AVC420,
        GFX_HEADLESS_SCALE200_AVC420_RAW,
        GFX_HEADLESS_SCALE200_AVC420_CLIENT,
        GFX_GREETER_V81NOAVC,
        GFX_HEADLESS_V81NOAVC,
        GOLDEN_GREETER_PROGRESSIVE,
        GOLDEN_HEADLESS_PROGRESSIVE,
        SERVER_REDIRECTION_LEG1,
        SERVER_REDIRECTION_LEG2,
        TARGET_CERT_LEG1,
        TARGET_CERT_LEG2,
        TLS_CERT_LEG1,
        TLS_CERT_LEG2,
        TLS_CERT_LEG3,
        TLS_CERT_HEADLESS,
        RDSTLS_CAPS,
        RDSTLS_AUTH_REQUEST_LEG2,
        RDSTLS_AUTH_REQUEST_LEG3,
        RDSTLS_AUTH_REQUEST_REUSED,
        RDSTLS_AUTH_RESPONSE_SUCCESS,
        RDSTLS_AUTH_RESPONSE_LOGON_FAILURE,
        FASTPATH_POINTER_SCALE100,
        FASTPATH_POINTER_SCALE200,
        CLIP_REMOTE_PNG,
        CLIP_REMOTE_UNICODETEXT,
        GOLDEN_GREETER,
        SCREENSHOT_DESKTOP_HEADLESS,
        SCREENSHOT_RETINA200,
        SCREENSHOT_LEG3_MOTION,
    ];
}

/// Why a fixture could not be loaded.
#[derive(Debug)]
pub enum FixtureError {
    /// Reading the file failed.
    Io {
        /// Fixture path.
        path: PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
    /// The file is a git-lfs pointer: run `git lfs install --local && git lfs pull`.
    LfsPointer(PathBuf),
    /// A `.rec` record's length runs past the end of the data.
    Truncated {
        /// Offset of the bad record header.
        offset: usize,
    },
    /// A manifest line is malformed.
    BadManifest {
        /// 1-based line number.
        line: usize,
    },
}

impl std::fmt::Display for FixtureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "reading fixture {}: {source}", path.display()),
            Self::LfsPointer(p) => write!(
                f,
                "fixture {} is a git-lfs pointer; run `git lfs install --local && git lfs pull`",
                p.display()
            ),
            Self::Truncated { offset } => write!(f, "truncated .rec record at offset {offset}"),
            Self::BadManifest { line } => write!(f, "malformed MANIFEST.sha256 line {line}"),
        }
    }
}

impl std::error::Error for FixtureError {}

/// Absolute path of the repository's `fixtures/` directory.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// Absolute path of a fixture.
pub fn path(rel: &str) -> PathBuf {
    fixtures_dir().join(rel)
}

/// `true` if `data` is a git-lfs pointer file instead of the real content.
pub fn is_lfs_pointer(data: &[u8]) -> bool {
    data.starts_with(b"version https://git-lfs.github.com/spec/")
}

/// Reads a fixture.
pub fn try_read(rel: &str) -> Result<Vec<u8>, FixtureError> {
    let p = path(rel);
    let data = std::fs::read(&p).map_err(|source| FixtureError::Io { path: p.clone(), source })?;
    if is_lfs_pointer(&data) {
        return Err(FixtureError::LfsPointer(p));
    }
    Ok(data)
}

/// Reads a fixture, panicking with an actionable message on failure.
pub fn read(rel: &str) -> Vec<u8> {
    try_read(rel).unwrap_or_else(|e| panic!("{e}"))
}

/// Splits `.rec` data into records.
pub fn parse_records(data: &[u8]) -> Result<Vec<&[u8]>, FixtureError> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let header = data.get(pos..pos + 4).ok_or(FixtureError::Truncated { offset: pos })?;
        let len = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let len = usize::try_from(len).map_err(|_| FixtureError::Truncated { offset: pos })?;
        let start = pos + 4;
        let rec = start
            .checked_add(len)
            .and_then(|end| data.get(start..end))
            .ok_or(FixtureError::Truncated { offset: pos })?;
        out.push(rec);
        pos = start + len;
    }
    Ok(out)
}

/// Reads a `.rec` fixture into owned records, panicking on failure.
pub fn records(rel: &str) -> Vec<Vec<u8>> {
    let data = read(rel);
    parse_records(&data).unwrap_or_else(|e| panic!("{rel}: {e}")).into_iter().map(<[u8]>::to_vec).collect()
}

/// Parses `MANIFEST.sha256` text into `(sha256 hex, path)` pairs.
pub fn parse_manifest(text: &str) -> Result<Vec<(String, String)>, FixtureError> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let bad = FixtureError::BadManifest { line: i + 1 };
        let Some((sha, rel)) = line.split_once("  ") else { return Err(bad) };
        let hex = sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !hex || rel.is_empty() {
            return Err(bad);
        }
        out.push((sha.to_owned(), rel.to_owned()));
    }
    Ok(out)
}

/// Lower-case hex SHA-256 of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest as _;
    use std::fmt::Write as _;
    sha2::Sha256::digest(data).iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Every file under `fixtures/` except `README.md` and `MANIFEST.sha256`, relative, sorted.
fn list_fixture_files() -> std::io::Result<Vec<String>> {
    let root = fixtures_dir();
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let p = entry?.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let Ok(rel) = p.strip_prefix(&root) else { continue };
            let rel: Vec<String> =
                rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
            let rel = rel.join("/");
            if rel != "README.md" && rel != "MANIFEST.sha256" && !rel.ends_with(".DS_Store") {
                out.push(rel);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Verifies every file listed in `fixtures/MANIFEST.sha256` against its checksum and that
/// no unlisted file exists. Returns the number of verified files, or the problems found.
pub fn verify_manifest() -> Result<usize, Vec<String>> {
    let manifest_path = path("MANIFEST.sha256");
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| vec![format!("{}: {e}", manifest_path.display())])?;
    let entries = parse_manifest(&text).map_err(|e| vec![e.to_string()])?;
    let mut problems = Vec::new();
    for (sha, rel) in &entries {
        match try_read(rel) {
            Ok(data) if sha256_hex(&data) == *sha => {}
            Ok(_) => problems.push(format!("{rel}: checksum mismatch")),
            Err(e) => problems.push(e.to_string()),
        }
    }
    match list_fixture_files() {
        Ok(files) => {
            for f in files {
                if !entries.iter().any(|(_, rel)| *rel == f) {
                    problems.push(format!("{f}: not listed in MANIFEST.sha256"));
                }
            }
        }
        Err(e) => problems.push(format!("listing fixtures: {e}")),
    }
    if problems.is_empty() { Ok(entries.len()) } else { Err(problems) }
}
