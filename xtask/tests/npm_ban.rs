//! M0-1 Red: the npm-ban check detects lockfiles and npm/npx/yarn/pnpm invocations, and
//! the repository itself is clean.

use std::path::{Path, PathBuf};

use xtask::npm_ban::{is_forbidden_lockfile, is_scanned_file, scan_files, scan_text};

fn lines(path: &str, content: &str) -> Vec<usize> {
    scan_text(Path::new(path), content).into_iter().map(|v| v.line).collect()
}

#[test]
fn lockfiles_are_forbidden() {
    for f in ["package-lock.json", "ui/yarn.lock", "a/b/pnpm-lock.yaml", "npm-shrinkwrap.json"] {
        assert!(is_forbidden_lockfile(Path::new(f)), "{f}");
    }
    for f in ["ui/bun.lock", "ui/bun.lockb", "Cargo.lock", "ui/package.json"] {
        assert!(!is_forbidden_lockfile(Path::new(f)), "{f}");
    }
}

#[test]
fn scanned_file_kinds() {
    for f in [
        "scripts/x.sh",
        ".github/workflows/ci.yml",
        "docs/a.md",
        "ui/package.json",
        "src-tauri/tauri.conf.json",
        "Makefile",
        "justfile",
        "host/setup.bash",
        "a.zsh",
        "ci.yaml",
        "bunfig.toml",
    ] {
        assert!(is_scanned_file(Path::new(f)), "{f}");
    }
    for f in ["src/main.rs", "fixtures/x.bin", "ui/src/main.ts"] {
        assert!(!is_scanned_file(Path::new(f)), "{f}");
    }
}

#[test]
fn detects_invocations_in_command_position() {
    let script = "#!/bin/sh\nset -e\nnpm install\ncd ui && npx tauri dev\nfoo; yarn build\n(pnpm i)\necho $(npm bin)\n";
    assert_eq!(lines("x.sh", script), vec![3, 4, 5, 6, 7]);

    let workflow = "steps:\n  - run: npm ci\n  - run: |\n      bun install\n      npm test\n  - name: x\n";
    assert_eq!(lines("ci.yml", workflow), vec![2, 5]);

    let pkg = r#"{ "scripts": { "build": "npm run bundle", "dev": "vite", "x": "bun x" } }"#;
    assert_eq!(lines("package.json", pkg), vec![1]);

    let conf = r#"{ "build": { "beforeBuildCommand": "npm run build" } }"#;
    assert_eq!(lines("tauri.conf.json", conf), vec![1]);

    let doc = "Install:\n\n```sh\n$ npm install\n```\n\n- npx create-tauri-app\n";
    assert_eq!(lines("README.md", doc), vec![4, 7]);
}

#[test]
fn prose_and_bun_are_not_invocations() {
    let doc = "Bun, never npm.\nNo `npm`, `npx`, `yarn` or `pnpm` in scripts.\n\
               src-tauri via cargo tauri init with the npm references removed.\n\
               Run `bun install` then `bun run build`.\nThe npm-ban check.\n\
               ignore this: npm install  <!-- npm-ban: allow -->\n";
    assert!(lines("docs/x.md", doc).is_empty(), "{:?}", scan_text(Path::new("docs/x.md"), doc));
}

#[test]
fn scan_files_reports_lockfiles_and_skips_third_party() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("ui")).unwrap();
    std::fs::create_dir_all(root.join("third_party/x")).unwrap();
    std::fs::write(root.join("ui/package-lock.json"), "{}").unwrap();
    std::fs::write(root.join("run.sh"), "npm install\n").unwrap();
    std::fs::write(root.join("third_party/x/build.sh"), "npm install\n").unwrap();
    std::fs::write(root.join("third_party/x/yarn.lock"), "").unwrap();
    let files: Vec<PathBuf> = ["ui/package-lock.json", "run.sh", "third_party/x/build.sh", "third_party/x/yarn.lock"]
        .iter()
        .map(PathBuf::from)
        .collect();
    let v = scan_files(root, &files);
    let paths: Vec<_> = v.iter().map(|v| v.path.to_string_lossy().into_owned()).collect();
    assert_eq!(paths, vec!["ui/package-lock.json", "run.sh"], "{v:?}");
}

#[test]
fn repository_is_npm_free() {
    let root = xtask::repo::root();
    let files = xtask::repo::list_files(&root).unwrap();
    assert!(!files.is_empty());
    let v = scan_files(&root, &files);
    assert!(v.is_empty(), "npm-ban violations: {v:#?}");
}
