//! M0-3 Red: `cargo xtask import-fixtures` copies a staging directory into `fixtures/`,
//! sanitizes it, and writes a checksum manifest and a provenance README; the manifest
//! verification detects any drift.

use std::path::Path;

use xtask::fixtures::{
    FileKind, MANIFEST_FILE, ManifestEntry, README_FILE, import, parse_manifest, parse_provenance,
    render_manifest, sha256_hex, verify_manifest,
};
use xtask::sanitize::{KnownSecret, find};

mod common;
use common::{fake_rdstls_auth_request, fake_redirection_frame, utf16le};

const PW: &str = "Fake9Passw0rd!xyz";

fn write(path: &Path, data: &[u8]) {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).unwrap_or_else(|e| panic!("{e}"));
    }
    std::fs::write(path, data).unwrap_or_else(|e| panic!("{e}"));
}

const PROVENANCE: &str = r#"
[capture]
date = "2026-09-22"
host = "fake host"
tool = "unit test"
notes = "synthetic"

[[file]]
path = "pdus/server_redirection_leg1.bin"
kind = "server-redirection"
source = "fake leg 1"
description = "redirect"

[[file]]
path = "pdus/rdstls_auth_request_leg2.bin"
kind = "rdstls-auth-request"
source = "fake leg 2"
description = "auth request"

[[file]]
path = "clipboard/remote_clip_d.bin"
kind = "clipboard"
source = "fake clipboard"
description = "CF_UNICODETEXT that happens to contain a known secret"

[[file]]
path = "h264/leg2.h264"
kind = "h264"
source = "fake leg 2"
description = "stream"
"#;

fn staging() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let d = dir.path();
    write(&d.join("provenance.toml"), PROVENANCE.as_bytes());
    write(
        &d.join("pdus/server_redirection_leg1.bin"),
        &fake_redirection_frame("OneTimeUser16chr", &[0x5A; 34]),
    );
    // The same one-time credentials are used in the next leg's AuthRequest.
    write(
        &d.join("pdus/rdstls_auth_request_leg2.bin"),
        &fake_rdstls_auth_request("OneTimeUser16chr", &[0x5A; 34]),
    );
    let mut clip = utf16le("paste: ");
    clip.extend(utf16le(PW));
    write(&d.join("clipboard/remote_clip_d.bin"), &clip);
    write(&d.join("h264/leg2.h264"), b"\x00\x00\x00\x01\x09\x30rest-of-stream");
    dir
}

fn known() -> Vec<KnownSecret> {
    vec![KnownSecret::text("fake.txt:2", PW)]
}

#[test]
fn provenance_parses_with_kinds() {
    let p = parse_provenance(PROVENANCE).unwrap_or_else(|e| panic!("{e:#}"));
    assert_eq!(p.capture.date, "2026-09-22");
    assert_eq!(p.files.len(), 4);
    assert_eq!(p.files[0].kind, FileKind::ServerRedirection);
    assert_eq!(p.files[1].kind, FileKind::RdstlsAuthRequest);
    assert!(parse_provenance("[capture]\ndate='x'\n").is_err(), "missing fields are errors");
    assert!(parse_provenance(&PROVENANCE.replace("kind = \"h264\"", "kind = \"bogus\"")).is_err());
}

#[test]
fn manifest_roundtrips_in_sha256sum_format() {
    assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    let entries = vec![
        ManifestEntry { sha256: sha256_hex(b"b"), path: "z/b.bin".into() },
        ManifestEntry { sha256: sha256_hex(b"a"), path: "a/a.bin".into() },
    ];
    let text = render_manifest(&entries);
    let first = text.lines().next().unwrap_or_default();
    assert_eq!(first, format!("{}  a/a.bin", sha256_hex(b"a")), "sorted by path, two spaces");
    let back = parse_manifest(&text).unwrap_or_else(|e| panic!("{e:#}"));
    assert_eq!(back.len(), 2);
    assert_eq!(back[0].path, "a/a.bin");
    assert!(parse_manifest("not a manifest line\n").is_err());
}

#[test]
fn import_sanitizes_copies_and_writes_manifest_and_readme() {
    let staging = staging();
    let dest = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    // A stale fixture from an earlier import must be removed.
    write(&dest.path().join("old/stale.bin"), b"stale");

    let report = import(staging.path(), dest.path(), &known()).unwrap_or_else(|e| panic!("{e:#}"));
    assert_eq!(report.files, 4);
    assert!(report.one_time_secrets >= 2, "user + password from the redirection PDU");
    assert!(report.replacements >= 5, "redirect user+pw, auth request user+pw, clipboard pw");
    assert_eq!(report.removed, vec!["old/stale.bin".to_string()]);
    assert!(!dest.path().join("old/stale.bin").exists());

    // Nothing secret survives anywhere, lengths are unchanged, the rest is byte-identical.
    let mut all = known();
    all.push(KnownSecret::text("one-time user", "OneTimeUser16chr"));
    all.push(KnownSecret::bytes("one-time pw", vec![0x5A; 34]));
    for rel in [
        "pdus/server_redirection_leg1.bin",
        "pdus/rdstls_auth_request_leg2.bin",
        "clipboard/remote_clip_d.bin",
        "h264/leg2.h264",
    ] {
        let out = std::fs::read(dest.path().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let orig = std::fs::read(staging.path().join(rel)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(out.len(), orig.len(), "{rel} length");
        assert!(find(&out, &all).is_empty(), "{rel} still has a secret");
    }
    let h264 = std::fs::read(dest.path().join("h264/leg2.h264")).unwrap_or_default();
    assert_eq!(h264, b"\x00\x00\x00\x01\x09\x30rest-of-stream");

    // Manifest: one line per fixture, checksums of the sanitized files.
    let manifest = std::fs::read_to_string(dest.path().join(MANIFEST_FILE)).unwrap_or_default();
    let entries = parse_manifest(&manifest).unwrap_or_else(|e| panic!("{e:#}"));
    assert_eq!(entries.len(), 4);
    for e in &entries {
        let data = std::fs::read(dest.path().join(&e.path)).unwrap_or_default();
        assert_eq!(e.sha256, sha256_hex(&data), "{}", e.path);
    }
    assert!(verify_manifest(dest.path()).unwrap_or_else(|e| panic!("{e:#}")).is_empty());

    // README: provenance for every file.
    let readme = std::fs::read_to_string(dest.path().join(README_FILE)).unwrap_or_default();
    for needle in ["2026-09-22", "fake host", "unit test", "h264/leg2.h264", "fake leg 1", "sanitiz"] {
        assert!(readme.contains(needle), "README lacks {needle:?}");
    }
    assert!(!readme.contains(PW));
}

#[test]
fn import_is_idempotent() {
    let staging = staging();
    let dest = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    import(staging.path(), dest.path(), &known()).unwrap_or_else(|e| panic!("{e:#}"));
    let m1 = std::fs::read(dest.path().join(MANIFEST_FILE)).unwrap_or_default();
    let r1 = std::fs::read(dest.path().join(README_FILE)).unwrap_or_default();
    let report = import(staging.path(), dest.path(), &known()).unwrap_or_else(|e| panic!("{e:#}"));
    assert!(report.removed.is_empty());
    assert_eq!(std::fs::read(dest.path().join(MANIFEST_FILE)).unwrap_or_default(), m1);
    assert_eq!(std::fs::read(dest.path().join(README_FILE)).unwrap_or_default(), r1);
}

#[test]
fn import_rejects_unlisted_missing_and_escaping_files() {
    let dest = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));

    let s = staging();
    write(&s.path().join("extra/unlisted.bin"), b"x");
    let e = import(s.path(), dest.path(), &known()).expect_err("unlisted file");
    assert!(format!("{e:#}").contains("extra/unlisted.bin"));

    let s = staging();
    std::fs::remove_file(s.path().join("h264/leg2.h264")).unwrap_or_default();
    let e = import(s.path(), dest.path(), &known()).expect_err("missing file");
    assert!(format!("{e:#}").contains("h264/leg2.h264"));

    let s = staging();
    let bad = PROVENANCE.replace("path = \"h264/leg2.h264\"", "path = \"../escape.h264\"");
    write(&s.path().join("provenance.toml"), bad.as_bytes());
    assert!(import(s.path(), dest.path(), &known()).is_err());
}

#[test]
fn verify_manifest_reports_mismatch_missing_and_unlisted_files() {
    let staging = staging();
    let dest = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    import(staging.path(), dest.path(), &known()).unwrap_or_else(|e| panic!("{e:#}"));

    write(&dest.path().join("h264/leg2.h264"), b"tampered");
    std::fs::remove_file(dest.path().join("clipboard/remote_clip_d.bin")).unwrap_or_default();
    write(&dest.path().join("new/unlisted.bin"), b"x");
    let problems = verify_manifest(dest.path()).unwrap_or_else(|e| panic!("{e:#}"));
    let joined = problems.join("\n");
    assert_eq!(problems.len(), 3, "{joined}");
    assert!(joined.contains("h264/leg2.h264"));
    assert!(joined.contains("clipboard/remote_clip_d.bin"));
    assert!(joined.contains("new/unlisted.bin"));
}
