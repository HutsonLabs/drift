//! The app icon set (task M9-4).
//!
//! Drift's icon is drawn as a vector ([`SOURCE`]) and expanded with
//! `cargo tauri icon src-tauri/icons/icon.svg`. `cargo tauri icon` also writes Windows,
//! Android and iOS artefacts; plan §0 is macOS-only, so [`audit`] fails if any of them is
//! committed, and checks that every file `tauri.conf.json` bundles exists with the right size.

use std::path::Path;

/// The vector source, relative to the workspace root.
pub const SOURCE: &str = "src-tauri/icons/icon.svg";

/// Bundled icons, relative to `src-tauri/`, with the pixel size a PNG must have.
pub const REQUIRED: &[(&str, Option<u32>)] = &[
    ("icons/icon.svg", None),
    ("icons/32x32.png", Some(32)),
    ("icons/128x128.png", Some(128)),
    ("icons/128x128@2x.png", Some(256)),
    ("icons/icon.png", Some(512)),
    ("icons/icon.icns", None),
];

/// Icon artefacts of other platforms; plan §0 ships none of them.
pub const FORBIDDEN_SUFFIXES: &[&str] = &[".ico"];
/// Icon file-name prefixes of other platforms (`cargo tauri icon` writes these for Windows).
pub const FORBIDDEN_PREFIXES: &[&str] = &["Square", "StoreLogo"];

/// The pixel size in a PNG's IHDR chunk, or `None` when `bytes` is not a PNG.
#[must_use]
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || !bytes.starts_with(MAGIC) || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((width, height))
}

/// Whether `bytes` is an Apple icon image (`icns` magic).
#[must_use]
pub fn is_icns(bytes: &[u8]) -> bool {
    bytes.len() > 8 && bytes.starts_with(b"icns")
}

/// Everything wrong with the icon set under `src_tauri` (empty means it is complete).
#[must_use]
pub fn audit(src_tauri: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    for (relative, size) in REQUIRED {
        let path = src_tauri.join(relative);
        let Ok(bytes) = std::fs::read(&path) else {
            problems.push(format!("missing {relative} (run `cargo tauri icon {SOURCE}`)"));
            continue;
        };
        if bytes.is_empty() {
            problems.push(format!("{relative} is empty"));
            continue;
        }
        match (*size, relative.ends_with(".icns")) {
            (Some(expected), _) => match png_size(&bytes) {
                Some((w, h)) if w == expected && h == expected => {}
                Some((w, h)) => problems.push(format!("{relative} is {w}x{h}, expected {expected} square")),
                None => problems.push(format!("{relative} is not a PNG")),
            },
            (None, true) if !is_icns(&bytes) => problems.push(format!("{relative} is not an icns file")),
            (None, _) => {}
        }
    }
    let dir = src_tauri.join("icons");
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if FORBIDDEN_SUFFIXES.iter().any(|s| name.ends_with(s))
                || FORBIDDEN_PREFIXES.iter().any(|p| name.starts_with(p))
            {
                problems.push(format!("icons/{name} is a non-macOS artefact (plan §0: macOS only)"));
            }
        }
    }
    // The bundle must ship exactly the files that exist.
    if let Ok(text) = std::fs::read_to_string(src_tauri.join("tauri.conf.json"))
        && let Ok(config) = serde_json::from_str::<serde_json::Value>(&text)
    {
        let listed: Vec<String> = config
            .get("bundle")
            .and_then(|b| b.get("icon"))
            .and_then(serde_json::Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(ToOwned::to_owned)).collect())
            .unwrap_or_default();
        for entry in &listed {
            if !src_tauri.join(entry).is_file() {
                problems.push(format!("tauri.conf.json bundles {entry}, which does not exist"));
            }
        }
        for (relative, size) in REQUIRED {
            if size.is_some() && !listed.iter().any(|l| l == relative) {
                problems.push(format!("tauri.conf.json does not bundle {relative}"));
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_size_needs_the_magic_and_the_ihdr() {
        let mut png = Vec::from(b"\x89PNG\r\n\x1a\n");
        png.extend_from_slice(&13_u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&64_u32.to_be_bytes());
        png.extend_from_slice(&32_u32.to_be_bytes());
        assert_eq!(png_size(&png), Some((64, 32)));
        png[1] = b'X';
        assert_eq!(png_size(&png), None);
    }

    #[test]
    fn an_empty_directory_reports_every_file() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let problems = audit(dir.path());
        assert_eq!(problems.len(), REQUIRED.len(), "{problems:?}");
        assert!(problems.iter().all(|p| p.starts_with("missing ")), "{problems:?}");
    }
}
