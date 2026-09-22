//! Secret scan tests (synthetic secrets only; real values are never used in tests).

use std::path::{Path, PathBuf};

use xtask::secret_scan::{
    Encoding, extract_secrets, is_secret_like, load_secrets, needles, scan_bytes, scan_files,
};

// Synthetic, obviously fake credentials.
const PW: &str = "Fake9Passw0rdXyz";
const HEX: &str = "0123abcd4567ef0123abcd";

fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

#[test]
fn secret_like_heuristic() {
    assert!(is_secret_like(PW));
    assert!(is_secret_like(HEX));
    assert!(is_secret_like(")?A_9a'}*A+9<9AA"));
    assert!(!is_secret_like("drifttest"), "plain user names are not secrets");
    assert!(!is_secret_like("drift-headless"));
    assert!(!is_secret_like("Ab1"), "too short");
    assert!(!is_secret_like("123456"));
}

#[test]
fn extracts_values_from_plain_and_key_value_lines() {
    let text = format!("\tUsername: someuser\n\tPassword: {PW}\n\n{HEX}\nplainuser\n");
    let s = extract_secrets("system.txt", &text);
    let got: Vec<(&str, usize)> = s.iter().map(|s| (s.value.as_str(), s.line)).collect();
    assert_eq!(got, vec![(PW, 2), (HEX, 4)]);
    assert_eq!(s[0].source, "system.txt");
    assert!(!format!("{:?}", s[0]).contains(PW), "Debug must redact");
}

#[test]
fn needles_cover_utf8_and_utf16le() {
    let n = needles(PW);
    assert_eq!(n, vec![(Encoding::Utf8, PW.as_bytes().to_vec()), (Encoding::Utf16Le, utf16le(PW))]);
}

#[test]
fn finds_secrets_in_both_encodings() {
    let secrets = extract_secrets("s.txt", &format!("{PW}\n{HEX}\n"));
    let mut data = b"header ".to_vec();
    data.extend_from_slice(&utf16le(PW));
    data.extend_from_slice(b" and ");
    data.extend_from_slice(HEX.as_bytes());
    let hits = scan_bytes(Path::new("f.bin"), &data, &secrets);
    let got: Vec<(String, Encoding)> = hits.iter().map(|h| (h.secret.clone(), h.encoding)).collect();
    assert_eq!(got, vec![("s.txt:1".into(), Encoding::Utf16Le), ("s.txt:2".into(), Encoding::Utf8)]);
    assert!(scan_bytes(Path::new("f"), b"nothing to see", &secrets).is_empty());
}

#[test]
fn loads_directory_and_scans_files() {
    let secrets_dir = tempfile::tempdir().unwrap();
    std::fs::write(secrets_dir.path().join("a.txt"), format!("user\n{PW}\n")).unwrap();
    std::fs::write(secrets_dir.path().join("ignored.md"), format!("{HEX}\n")).unwrap();
    let secrets = load_secrets(secrets_dir.path()).unwrap();
    assert_eq!(secrets.len(), 1);
    assert!(load_secrets(Path::new("/nonexistent/drift/secrets")).unwrap().is_empty());

    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("clean.txt"), "hello").unwrap();
    std::fs::write(repo.path().join("leak.bin"), utf16le(PW)).unwrap();
    let files = vec![PathBuf::from("clean.txt"), PathBuf::from("leak.bin")];
    let hits = scan_files(repo.path(), &files, &secrets).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, PathBuf::from("leak.bin"));
}
