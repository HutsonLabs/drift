//! M0-3 Red: the fixture sanitizer removes every known secret, as bytes, UTF-8 and
//! UTF-16LE, without changing any length. Synthetic secrets only; the real-secrets check
//! over the committed fixtures lives in `committed_fixtures_contain_no_known_secret`.

use std::path::PathBuf;

use xtask::sanitize::{
    Form, KnownSecret, PLACEHOLDER, SecretValue, find, load_known_secrets,
    one_time_from_rdstls_auth_request, one_time_from_server_redirection, placeholder, sanitize,
    secrets_from_text,
};

// Obviously fake values.
const PW: &str = "Fake9Passw0rd!xyz";
const USER: &str = "fakeuser42";
const UNI: &str = "pässwörd-日本";
const BLOB: [u8; 10] = [0xde, 0xad, 0xbe, 0xef, 0x00, 0x01, 0x02, 0xfe, 0xed, 0x99];

mod common;
use common::{fake_rdstls_auth_request, fake_redirection_frame, utf16le};

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn fake_secrets() -> Vec<KnownSecret> {
    vec![
        KnownSecret::text("fake:pw", PW),
        KnownSecret::text("fake:user", USER),
        KnownSecret::text("fake:uni", UNI),
        KnownSecret::bytes("fake:blob", BLOB.to_vec()),
    ]
}

#[test]
fn placeholder_cycles_redacted_to_the_requested_length() {
    assert_eq!(PLACEHOLDER, b"REDACTED");
    assert_eq!(placeholder(0), b"");
    assert_eq!(placeholder(3), b"RED");
    assert_eq!(placeholder(11), b"REDACTEDRED");
}

#[test]
fn needles_cover_utf8_utf16le_and_raw_bytes_with_same_length_placeholders() {
    let t = KnownSecret::text("t", UNI);
    let n = t.needles();
    let forms: Vec<Form> = n.iter().map(|(f, _, _)| *f).collect();
    assert_eq!(forms, vec![Form::Utf8, Form::Utf16Le]);
    assert_eq!(n[0].1, UNI.as_bytes());
    assert_eq!(n[1].1, utf16le(UNI));
    for (_, needle, repl) in &n {
        assert_eq!(needle.len(), repl.len(), "placeholder must keep the length");
    }
    // UTF-16LE placeholder is itself UTF-16LE text ("REDACTED…").
    assert_eq!(&n[1].2[..4], &utf16le("RE")[..]);

    let b = KnownSecret::bytes("b", BLOB.to_vec());
    let n = b.needles();
    assert_eq!(n.len(), 1);
    assert_eq!(n[0].0, Form::Bytes);
    assert_eq!(n[0].1, BLOB);
    assert_eq!(n[0].2, placeholder(BLOB.len()));
}

#[test]
fn sanitize_removes_every_secret_in_every_encoding_and_keeps_length() {
    let mut data = b"head ".to_vec();
    data.extend_from_slice(PW.as_bytes());
    data.extend_from_slice(b" mid ");
    data.extend(utf16le(PW));
    data.extend(utf16le(USER));
    data.extend_from_slice(USER.as_bytes());
    data.extend_from_slice(UNI.as_bytes());
    data.extend(utf16le(UNI));
    data.extend_from_slice(&BLOB);
    data.extend_from_slice(PW.as_bytes()); // second occurrence
    data.extend_from_slice(b" tail");

    let secrets = fake_secrets();
    assert!(!find(&data, &secrets).is_empty());
    let (out, n) = sanitize(&data, &secrets);

    assert_eq!(out.len(), data.len());
    assert_eq!(n, 8, "every occurrence is counted");
    assert!(find(&out, &secrets).is_empty(), "no known secret survives");
    for s in &secrets {
        for (_, needle, _) in s.needles() {
            assert!(!contains(&out, &needle), "{s:?} still present");
        }
    }
    assert!(out.starts_with(b"head REDACTED"));
    assert!(out.ends_with(b" tail"));
    assert!(contains(&out, &utf16le("REDACTED")));
}

#[test]
fn sanitize_leaves_clean_data_untouched() {
    let data = b"nothing secret here \x00\x01\x02".to_vec();
    let (out, n) = sanitize(&data, &fake_secrets());
    assert_eq!(out, data);
    assert_eq!(n, 0);
}

#[test]
fn find_reports_label_and_form_without_the_value() {
    let mut data = utf16le(PW);
    data.extend_from_slice(&BLOB);
    let hits = find(&data, &fake_secrets());
    assert_eq!(hits, vec![("fake:pw".to_string(), Form::Utf16Le), ("fake:blob".to_string(), Form::Bytes)]);
    assert!(!format!("{:?}", fake_secrets()).contains(PW), "Debug must redact");
}

#[test]
fn secrets_from_text_takes_every_value_line_and_decodes_hex_blobs() {
    // Shapes of the real files: "\tUsername: x" lines, plain two-line files, and the
    // probe2 `lastredir.txt` layout (hex LB info, user, domain, hex password, hex GUID, flags).
    let system = format!("\tUsername: {USER}\n\tPassword: {PW}\n");
    let s = secrets_from_text("system.txt", &system);
    assert_eq!(
        s.iter().map(|x| x.value.clone()).collect::<Vec<_>>(),
        vec![SecretValue::Text(USER.into()), SecretValue::Text(PW.into())]
    );
    assert_eq!(s[1].label, "system.txt:2");

    let short = "abc\n12345\n";
    assert!(secrets_from_text("short.txt", short).is_empty(), "values shorter than 6 chars are ignored");

    let hex_pw = "00112233445566778899aabbccddeeff0011";
    let redir = format!("436f6f6b69653a206d7374733d310d0a\n{USER}\n\n{hex_pw}\n114710\n");
    let s = secrets_from_text("lastredir.txt", &redir);
    let values: Vec<SecretValue> = s.iter().map(|x| x.value.clone()).collect();
    assert!(values.contains(&SecretValue::Text(USER.into())));
    assert!(values.contains(&SecretValue::Text(hex_pw.into())), "hex text itself is kept");
    let decoded: Vec<u8> = (0..hex_pw.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex_pw[i..i + 2], 16).unwrap_or_default())
        .collect();
    assert!(values.contains(&SecretValue::Bytes(decoded)), "hex blobs are also matched decoded");
    assert!(values.contains(&SecretValue::Text("114710".into())));
}

#[test]
fn load_known_secrets_reads_txt_files_and_tolerates_a_missing_dir() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    std::fs::write(dir.path().join("a.txt"), format!("{USER}\n{PW}\n")).unwrap_or_default();
    std::fs::write(dir.path().join("ignored.md"), format!("{UNI}\n")).unwrap_or_default();
    let s = load_known_secrets(dir.path()).unwrap_or_else(|e| panic!("load: {e}"));
    assert_eq!(s.len(), 2);
    let missing = load_known_secrets(&dir.path().join("nope")).unwrap_or_else(|e| panic!("{e}"));
    assert!(missing.is_empty());
}

#[test]
fn one_time_credentials_are_extracted_from_a_server_redirection_pdu() {
    let pw = [0x5Au8; 34];
    let frame = fake_redirection_frame("OneTimeUser16chr", &pw);
    let s = one_time_from_server_redirection("pdus/r.bin", &frame).unwrap_or_else(|e| panic!("{e:#}"));
    let values: Vec<SecretValue> = s.iter().map(|x| x.value.clone()).collect();
    assert!(values.contains(&SecretValue::Text("OneTimeUser16chr".into())));
    assert!(values.contains(&SecretValue::Bytes(pw.to_vec())));
    assert!(s.iter().all(|x| x.label.starts_with("pdus/r.bin")));

    // Sanitizing the frame with its own one-time secrets removes them, keeps the cookie.
    let (out, n) = sanitize(&frame, &s);
    assert!(n >= 2);
    assert_eq!(out.len(), frame.len());
    assert!(contains(&out, b"Cookie: msts=1234567890\r\n"));
    assert!(contains(&out, &utf16le("REDACTEDREDACTED")));
    assert!(find(&out, &s).is_empty());
}

#[test]
fn one_time_credentials_are_extracted_from_an_rdstls_auth_request() {
    let pw = [0x77u8; 34];
    let pdu = fake_rdstls_auth_request("AnotherUser16chr", &pw);
    let s = one_time_from_rdstls_auth_request("pdus/a.bin", &pdu).unwrap_or_else(|e| panic!("{e:#}"));
    let values: Vec<SecretValue> = s.iter().map(|x| x.value.clone()).collect();
    assert!(values.contains(&SecretValue::Text("AnotherUser16chr".into())));
    assert!(values.contains(&SecretValue::Bytes(pw.to_vec())));
}

#[test]
fn malformed_pdus_are_errors_not_panics() {
    for bad in [&b""[..], &b"\x03\x00\x00\x05\x02"[..], &[0xFFu8; 40][..]] {
        assert!(one_time_from_server_redirection("x", bad).is_err());
        assert!(one_time_from_rdstls_auth_request("x", bad).is_err());
    }
    let mut truncated = fake_redirection_frame("OneTimeUser16chr", &[1; 34]);
    truncated.truncate(truncated.len() - 20);
    assert!(one_time_from_server_redirection("x", &truncated).is_err());
}

/// The committed fixtures contain none of the dev machine's real secrets. Skips (with a
/// message) where the secrets directory does not exist, e.g. on CI runners, where the
/// `secret-scan` step covers the secret-like subset.
#[test]
fn committed_fixtures_contain_no_known_secret() {
    let root = xtask::repo::root();
    let fixtures = root.join("fixtures");
    assert!(fixtures.join("MANIFEST.sha256").is_file(), "fixtures/ has not been imported yet");
    let Some(dir) = xtask::secret_scan::default_secrets_dir().filter(|d| d.is_dir()) else {
        eprintln!("no secrets directory; skipping the real-secret scan");
        return;
    };
    let secrets = load_known_secrets(&dir).unwrap_or_else(|e| panic!("{e:#}"));
    assert!(!secrets.is_empty());
    let mut stack: Vec<PathBuf> = vec![fixtures];
    let mut scanned = 0;
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            for e in std::fs::read_dir(&p).unwrap_or_else(|e| panic!("{e}")).flatten() {
                stack.push(e.path());
            }
        } else {
            let data = std::fs::read(&p).unwrap_or_else(|e| panic!("{e}"));
            let hits = find(&data, &secrets);
            assert!(hits.is_empty(), "{} contains {:?}", p.display(), hits);
            scanned += 1;
        }
    }
    assert!(scanned > 10, "expected the imported fixtures, scanned {scanned}");
}
