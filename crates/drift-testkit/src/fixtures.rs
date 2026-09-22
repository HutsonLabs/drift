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
    pub const ALL: &[&str] = &[];
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
    let _ = data;
    unimplemented!("M0-3")
}

/// Reads a fixture.
pub fn try_read(rel: &str) -> Result<Vec<u8>, FixtureError> {
    let _ = rel;
    unimplemented!("M0-3")
}

/// Reads a fixture, panicking with an actionable message on failure.
pub fn read(rel: &str) -> Vec<u8> {
    try_read(rel).unwrap_or_else(|e| panic!("{e}"))
}

/// Splits `.rec` data into records.
pub fn parse_records(data: &[u8]) -> Result<Vec<&[u8]>, FixtureError> {
    let _ = data;
    unimplemented!("M0-3")
}

/// Reads a `.rec` fixture into owned records, panicking on failure.
pub fn records(rel: &str) -> Vec<Vec<u8>> {
    let data = read(rel);
    parse_records(&data)
        .unwrap_or_else(|e| panic!("{rel}: {e}"))
        .into_iter()
        .map(<[u8]>::to_vec)
        .collect()
}

/// Parses `MANIFEST.sha256` text into `(sha256 hex, path)` pairs.
pub fn parse_manifest(text: &str) -> Result<Vec<(String, String)>, FixtureError> {
    let _ = text;
    unimplemented!("M0-3")
}

/// Verifies every file listed in `fixtures/MANIFEST.sha256` against its checksum and that
/// no unlisted file exists. Returns the number of verified files, or the problems found.
pub fn verify_manifest() -> Result<usize, Vec<String>> {
    unimplemented!("M0-3")
}
