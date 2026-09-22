//! Red (M6-2, M8-3, M0-2, §10): `cargo xtask ci` must keep building the feature-gated targets
//! and must run the vendored IronRDP workspace's own tests.
//!
//! The gate is described as data by `xtask::ci_plan`, and `main.rs` executes exactly that list,
//! so these assertions are assertions about the real merge gate.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use xtask::ci_plan::{self, Action, Check, Options, Program, Step};

fn root() -> PathBuf {
    xtask::repo::root()
}

fn read(path: &str) -> String {
    let p = root().join(path);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("reading {}: {e}", p.display()))
}

/// Every `Cargo.toml` of a root-workspace member, as `(package name, manifest text)`.
fn workspace_manifests() -> Vec<(String, String)> {
    let root = root();
    let manifest = read("Cargo.toml").parse::<toml::Table>().expect("root Cargo.toml parses");
    let members = manifest["workspace"]["members"].as_array().expect("workspace.members").clone();
    members
        .iter()
        .map(|m| {
            let dir = m.as_str().expect("member path");
            let text = std::fs::read_to_string(root.join(dir).join("Cargo.toml"))
                .unwrap_or_else(|e| panic!("reading {dir}/Cargo.toml: {e}"));
            let name = text
                .parse::<toml::Table>()
                .ok()
                .and_then(|t| t["package"]["name"].as_str().map(str::to_owned))
                .unwrap_or_else(|| dir.to_owned());
            (name, text)
        })
        .collect()
}

/// Every declared `(package, feature)` pair in the root workspace, `default` excluded.
fn declared_features() -> Vec<(String, String)> {
    workspace_manifests()
        .into_iter()
        .flat_map(|(name, text)| {
            ci_plan::declared_features(&text).into_iter().map(move |f| (name.clone(), f))
        })
        .collect()
}

#[test]
fn the_gate_builds_every_declared_feature() {
    let features = declared_features();
    assert!(
        features.iter().any(|(p, f)| p == "drift-app" && f == "recording"),
        "M8-3's recording feature should be declared: {features:?}"
    );
    assert!(
        features.iter().any(|(p, f)| p == "drift-app" && f == "macos-ui-tests"),
        "M6-2's macos-ui-tests feature should be declared: {features:?}"
    );
    let gaps = ci_plan::feature_lint_gaps(&features, &ci_plan::ci(Options::default()));
    assert!(gaps.is_empty(), "cargo xtask ci never builds these features: {gaps:?}");
}

#[test]
fn the_fast_check_subset_builds_every_declared_feature() {
    let gaps = ci_plan::feature_lint_gaps(&declared_features(), &ci_plan::check_only());
    assert!(gaps.is_empty(), "cargo xtask check never builds these features: {gaps:?}");
}

#[test]
fn feature_gaps_are_reported_when_the_lint_list_is_narrow() {
    let narrow = vec![Step {
        name: "clippy".into(),
        action: Action::Run {
            program: Program::Cargo,
            args: ["clippy", "--workspace", "--all-targets", "--features", "drift-video/recording"]
                .map(str::to_owned)
                .to_vec(),
            dir: PathBuf::new(),
        },
    }];
    let features = [
        ("drift-app".to_owned(), "recording".to_owned()),
        ("drift-video".to_owned(), "recording".to_owned()),
    ];
    assert_eq!(ci_plan::feature_lint_gaps(&features, &narrow), vec!["drift-app/recording".to_owned()]);
}

#[test]
fn the_gate_runs_the_vendored_ironrdp_tests() {
    let steps = ci_plan::ci(Options::default());
    let vendored: Vec<&Step> = steps.iter().filter(|s| s.dir() == Path::new(ci_plan::VENDORED_DIR)).collect();
    assert!(!vendored.is_empty(), "no cargo xtask ci step runs in {}", ci_plan::VENDORED_DIR);
    assert!(
        vendored.iter().any(|s| s.is_cargo_with(&["test", "-p ironrdp-testsuite-core"])),
        "M0-2 Done is \"the fork's own tests pass\", but no step runs them: {:?}",
        vendored.iter().map(|s| s.command_line()).collect::<Vec<_>>()
    );
    assert!(
        vendored.iter().any(|s| s.is_cargo_with(&["fmt", "--check"])),
        "the vendored workspace is not format-checked: {:?}",
        vendored.iter().map(|s| s.command_line()).collect::<Vec<_>>()
    );
    assert!(
        vendored.iter().any(|s| s.is_cargo_with(&["test", "--locked"])),
        "the vendored tree has its own committed Cargo.lock; tests must run --locked: {:?}",
        vendored.iter().map(|s| s.command_line()).collect::<Vec<_>>()
    );
}

/// Every vendored crate that Drift both patches and consumes must have its tests run, so a
/// re-applied patch queue or a rev bump cannot silently break it.
///
/// `ironrdp-client` and `ironrdp-web` are patched for upstream compatibility only: Drift does
/// not depend on them and they do not build on a macOS host (libopus / wasm-bindgen), see
/// `docs/adr/M0-6-ci-gate-coverage.md`.
#[test]
fn the_vendored_test_step_covers_every_patched_crate_drift_depends_on() {
    let dir = root().join(ci_plan::VENDORED_PATCHES_DIR);
    let mut patched = BTreeSet::new();
    for entry in std::fs::read_dir(&dir).expect("patch queue directory") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_some_and(|e| e == "patch") {
            patched.extend(ci_plan::patched_crates(&std::fs::read_to_string(&path).expect("patch")));
        }
    }
    assert!(patched.contains("ironrdp-pdu"), "patch queue should touch ironrdp-pdu: {patched:?}");

    let consumed = ci_plan::vendored_dependencies(&read("Cargo.toml"));
    assert!(consumed.contains("ironrdp-session"), "Drift depends on ironrdp-session: {consumed:?}");

    let steps = ci_plan::ci(Options::default());
    let lines: Vec<String> = steps
        .iter()
        .filter(|s| s.dir() == Path::new(ci_plan::VENDORED_DIR))
        .filter_map(Step::command_line)
        .filter(|l| l.contains(" test"))
        .collect();
    let mut want: BTreeSet<&String> = patched.intersection(&consumed).collect();
    let testsuite = "ironrdp-testsuite-core".to_owned();
    want.insert(&testsuite);
    for krate in want {
        assert!(
            lines.iter().any(|l| l.contains(&format!("-p {krate}"))),
            "Drift patches and depends on {krate}, but no vendored test step runs its tests: {lines:?}"
        );
    }
}

#[test]
fn vendored_dependencies_are_the_ironrdp_path_deps() {
    let manifest = "\
[workspace.dependencies]
ironrdp-pdu = { path = \"third_party/ironrdp/crates/ironrdp-pdu\" }
ironrdp-server = { path = \"third_party/ironrdp/crates/ironrdp-server\", default-features = false }
tokio = { version = \"1\" }
drift-core = { path = \"crates/drift-core\" }
";
    assert_eq!(
        ci_plan::vendored_dependencies(manifest),
        ["ironrdp-pdu".to_owned(), "ironrdp-server".to_owned()].into_iter().collect::<BTreeSet<_>>()
    );
}

/// M0-2's four listed Red tests live in these files; a rev bump must not drop them.
#[test]
fn the_m0_2_red_test_files_exist_in_the_vendored_testsuite() {
    for f in [
        "crates/ironrdp-testsuite-core/tests/pdu/server_redirection.rs",
        "crates/ironrdp-testsuite-core/tests/connector/rdstls.rs",
        "crates/ironrdp-testsuite-core/tests/clipboard/temporary_directory.rs",
    ] {
        let p = root().join(ci_plan::VENDORED_DIR).join(f);
        assert!(p.is_file(), "missing M0-2 Red test file {}", p.display());
    }
}

#[test]
fn every_step_runs_in_a_directory_that_exists() {
    for step in ci_plan::ci(Options::default()).iter().chain(ci_plan::check_only().iter()) {
        let dir = root().join(step.dir());
        assert!(dir.is_dir(), "step `{}` runs in missing directory {}", step.name, dir.display());
    }
}

#[test]
fn options_toggle_coverage_and_deny() {
    let full = ci_plan::ci(Options::default());
    assert!(full.iter().any(|s| s.is_cargo_with(&["llvm-cov", "nextest"])));
    assert!(full.iter().any(|s| s.action == Action::InProcess(Check::CoverageGate)));
    assert!(full.iter().any(|s| s.is_cargo_with(&["deny"])));

    let lean = ci_plan::ci(Options { coverage: false, deny: false });
    assert!(!lean.iter().any(|s| s.is_cargo_with(&["llvm-cov"])));
    assert!(!lean.iter().any(|s| s.action == Action::InProcess(Check::CoverageGate)));
    assert!(!lean.iter().any(|s| s.is_cargo_with(&["deny"])));
    assert!(lean.iter().any(|s| s.is_cargo_with(&["nextest", "run"])), "tests still run without coverage");
}

#[test]
fn the_gate_starts_with_the_hygiene_checks_and_never_uses_npm() {
    let steps = ci_plan::ci(Options::default());
    assert_eq!(steps[0].action, Action::InProcess(Check::NpmBan));
    assert_eq!(steps[1].action, Action::InProcess(Check::SecretScan));
    assert!(steps.iter().any(|s| s.action == Action::InProcess(Check::Workflows)), "workflows are audited");
    let ui_steps: Vec<&Step> = steps.iter().filter(|s| s.dir() == Path::new("ui")).collect();
    assert!(!ui_steps.is_empty(), "the UI is built and tested");
    for step in ui_steps {
        assert!(
            matches!(step.action, Action::Run { program: Program::Bun, .. }),
            "`{}` must use bun, never npm (plan §0)",
            step.name
        );
    }
}

#[test]
fn patched_crates_reads_diff_headers() {
    let patch = "\
diff --git a/crates/ironrdp-pdu/src/lib.rs b/crates/ironrdp-pdu/src/lib.rs
--- a/crates/ironrdp-pdu/src/lib.rs
+++ b/crates/ironrdp-pdu/src/lib.rs
@@
diff --git a/crates/ironrdp-testsuite-core/tests/pdu/mod.rs b/crates/ironrdp-testsuite-core/tests/pdu/mod.rs
+++ b/crates/ironrdp-testsuite-core/tests/pdu/mod.rs
+++ b/README.md
";
    let crates = ci_plan::patched_crates(patch);
    assert_eq!(
        crates,
        ["ironrdp-pdu".to_owned(), "ironrdp-testsuite-core".to_owned()].into_iter().collect::<BTreeSet<_>>()
    );
}

#[test]
fn declared_features_skips_default() {
    let manifest = "[package]\nname = \"x\"\n[features]\ndefault = [\"a\"]\na = []\nb = [\"dep:c\"]\n";
    assert_eq!(ci_plan::declared_features(manifest), vec!["a".to_owned(), "b".to_owned()]);
    assert!(ci_plan::declared_features("[package]\nname = \"x\"\n").is_empty());
    assert!(ci_plan::declared_features("not toml {{{").is_empty());
}
