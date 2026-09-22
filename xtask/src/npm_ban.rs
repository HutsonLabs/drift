//! "Bun, never npm" enforcement (plan §0; task M0-1 Red test).
//!
//! Two kinds of violation:
//! 1. A JS package-manager lockfile other than Bun's (`package-lock.json`,
//!    `npm-shrinkwrap.json`, `yarn.lock`, `pnpm-lock.yaml`, `.pnpmfile.cjs`) anywhere
//!    outside `third_party/`.
//! 2. An `npm`/`npx`/`yarn`/`pnpm` **invocation** in a script, workflow, doc or config
//!    file. An invocation is the tool name in command position (start of a line or
//!    after `&&`, `||`, `;`, `|`, `(`, `` ` ``, `$(`, `run:`, a quote, `- `, or `$ `) followed by
//!    an argument. Prose such as "never use `npm`" is not an invocation.
//!
//! Lines containing `npm-ban: allow` are skipped (for documenting the rule itself).

use std::path::{Path, PathBuf};

/// A single npm-ban violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// Repository-relative path.
    pub path: PathBuf,
    /// 1-based line number (0 for lockfiles).
    pub line: usize,
    /// Human-readable description.
    pub message: String,
}

/// Lockfile names that must never exist.
pub const FORBIDDEN_LOCKFILES: &[&str] =
    &["package-lock.json", "npm-shrinkwrap.json", "yarn.lock", "pnpm-lock.yaml", ".pnpmfile.cjs"];

/// Package-manager commands that are banned.
pub const BANNED_TOOLS: &[&str] = &["npm", "npx", "yarn", "pnpm"];

/// Marker that exempts a line from the invocation scan.
pub const ALLOW_MARKER: &str = "npm-ban: allow";

/// File names always scanned regardless of extension.
const SCANNED_NAMES: &[&str] =
    &["package.json", "tauri.conf.json", "Makefile", "justfile", "Dockerfile", "bunfig.toml"];

/// Extensions of scanned files (scripts, workflows, docs, configs).
const SCANNED_EXTS: &[&str] = &["sh", "bash", "zsh", "fish", "yml", "yaml", "md", "markdown", "json", "toml"];

/// Text that, when it ends the part of a line before a tool name, puts the tool in
/// command position.
const COMMAND_PREFIXES: &[&str] = &[
    "&&", "||", ";", "|", "(", "`", "$(", "run:", "\"", "'", "-", "$", "sudo", "exec", "then", "do", "else",
];

fn file_name(path: &Path) -> &str {
    path.file_name().and_then(|n| n.to_str()).unwrap_or("")
}

/// `true` if `path`'s file name is a forbidden lockfile.
pub fn is_forbidden_lockfile(path: &Path) -> bool {
    FORBIDDEN_LOCKFILES.contains(&file_name(path))
}

/// `true` if the file type is scanned for invocations (scripts, workflows, docs, configs).
pub fn is_scanned_file(path: &Path) -> bool {
    let name = file_name(path);
    if SCANNED_NAMES.contains(&name) {
        return true;
    }
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| SCANNED_EXTS.contains(&e))
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'/')
}

fn is_command_position(before: &str) -> bool {
    let before = before.trim_end();
    before.is_empty() || COMMAND_PREFIXES.iter().any(|p| before.ends_with(p))
}

/// `true` if `line` invokes a banned tool.
fn line_invokes(line: &str) -> Option<&'static str> {
    let bytes = line.as_bytes();
    for tool in BANNED_TOOLS {
        let mut from = 0;
        while let Some(off) = line[from..].find(tool) {
            let start = from + off;
            let end = start + tool.len();
            from = end;
            let boundary_before = start == 0 || !is_word_byte(bytes[start - 1]);
            let followed_by_arg = bytes.get(end).is_some_and(|b| *b == b' ' || *b == b'\t')
                && line[end..].trim_start().bytes().next().is_some_and(|b| is_word_byte(b) || b == b'@');
            if boundary_before && followed_by_arg && is_command_position(&line[..start]) {
                return Some(tool);
            }
        }
    }
    None
}

/// Finds npm/npx/yarn/pnpm invocations in one file's text.
pub fn scan_text(path: &Path, content: &str) -> Vec<Violation> {
    content
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.contains(ALLOW_MARKER))
        .filter_map(|(i, line)| {
            line_invokes(line).map(|tool| Violation {
                path: path.to_path_buf(),
                line: i + 1,
                message: format!("`{tool}` invocation; use bun instead: {}", line.trim()),
            })
        })
        .collect()
}

/// Checks a list of repository-relative paths (reading contents from `root`).
/// Paths under `third_party/` are ignored (vendored code).
pub fn scan_files(root: &Path, files: &[PathBuf]) -> Vec<Violation> {
    let mut out = Vec::new();
    for rel in files {
        if rel.starts_with("third_party") {
            continue;
        }
        if is_forbidden_lockfile(rel) {
            out.push(Violation {
                path: rel.clone(),
                line: 0,
                message: "forbidden JS package-manager lockfile (only ui/bun.lock is allowed)".into(),
            });
            continue;
        }
        if is_scanned_file(rel)
            && let Ok(bytes) = std::fs::read(root.join(rel))
        {
            out.extend(scan_text(rel, &String::from_utf8_lossy(&bytes)));
        }
    }
    out
}
