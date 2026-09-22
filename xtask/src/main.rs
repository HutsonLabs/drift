//! `cargo xtask` — Drift's build automation entry point. See `docs/team-conventions.md`.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use xtask::{coverage, e2e_env, npm_ban, repo, secret_scan};

#[derive(Parser)]
#[command(name = "cargo xtask", about = "Drift build automation")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Full CI gate: npm-ban, secret-scan, bun test/build, bindings freshness, fmt,
    /// clippy -D warnings, nextest under llvm-cov + coverage gate, cargo deny.
    Ci {
        /// Run nextest without llvm-cov instrumentation and skip the coverage gate.
        #[arg(long)]
        no_coverage: bool,
        /// Skip `cargo deny` (e.g. offline).
        #[arg(long)]
        no_deny: bool,
    },
    /// Fast local subset: fmt check, clippy -D warnings, nextest.
    Check,
    /// Fail if npm/yarn/pnpm lockfiles or invocations exist (plan §0).
    NpmBan,
    /// Fail if any dev-machine secret appears in a repository file.
    SecretScan {
        /// Secrets directory (default `$DRIFT_SECRETS_DIR` or `~/code/drift-spikes/secrets`).
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Evaluate the coverage gate against an llvm-cov JSON summary.
    CoverageGate {
        /// Path to the `cargo llvm-cov --json --summary-only` output.
        json: PathBuf,
    },
    /// Regenerate `ui/src/bindings.ts` from the tauri-specta builder.
    Bindings {
        /// Only check that the committed bindings are up to date.
        #[arg(long)]
        check: bool,
    },
    /// Run the real-host e2e suite through SSH local forwards (plan §5.3).
    E2e {
        /// Extra arguments passed to `cargo nextest run`.
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Build the universal .app/.dmg (task M9-5).
    Bundle,
    /// Verify the GNOME host configuration over SSH (task M0-4).
    HostSetupCheck,
    /// Import §1.7 fixtures from ~/code/drift-spikes (task M0-3).
    ImportFixtures,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.cmd) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cmd: Cmd) -> Result<()> {
    let root = repo::root();
    match cmd {
        Cmd::Ci { no_coverage, no_deny } => ci(&root, !no_coverage, !no_deny),
        Cmd::Check => check(&root),
        Cmd::NpmBan => npm_ban_check(&root),
        Cmd::SecretScan { dir } => secret_scan_check(&root, dir),
        Cmd::CoverageGate { json } => coverage_gate(&root, &json),
        Cmd::Bindings { check } => bindings(&root, check),
        Cmd::E2e { args } => e2e(&root, &args),
        Cmd::Bundle => not_yet("bundle", "M9-5"),
        Cmd::HostSetupCheck => not_yet("host-setup-check", "M0-4"),
        Cmd::ImportFixtures => not_yet("import-fixtures", "M0-3"),
    }
}

fn not_yet(name: &str, task: &str) -> Result<()> {
    bail!("`cargo xtask {name}` is not implemented yet (owned by task {task})")
}

fn step(name: &str) {
    eprintln!("\n==> xtask: {name}");
}

/// Runs a command, failing with its description if it exits non-zero.
fn sh(root: &Path, program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .current_dir(root)
        .args(args)
        .status()
        .with_context(|| format!("spawning {program}"))?;
    if !status.success() {
        bail!("`{program} {}` failed ({status})", args.join(" "));
    }
    Ok(())
}

/// Optional features that CI builds, lints and tests too (feature-gated code must not rot).
/// `drift-video/recording`: the M8 encoder + MP4 writer.
const TEST_FEATURES: &str = "drift-video/recording";

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())
}

fn bun() -> String {
    std::env::var("BUN").unwrap_or_else(|_| "bun".into())
}

fn ci(root: &Path, with_coverage: bool, with_deny: bool) -> Result<()> {
    step("npm-ban");
    npm_ban_check(root)?;
    step("secret-scan");
    secret_scan_check(root, None)?;
    ui(root)?;
    step("bindings freshness");
    bindings(root, true)?;
    fmt_clippy(root)?;
    if with_coverage {
        step("nextest under llvm-cov");
        let json = root.join("target/llvm-cov-summary.json");
        let c = cargo();
        sh(
            root,
            &c,
            &[
                "llvm-cov",
                "nextest",
                "--workspace",
                "--features",
                TEST_FEATURES,
                "--json",
                "--summary-only",
                "--output-path",
                json.to_str().context("non-UTF-8 path")?,
            ],
        )?;
        step("coverage gate");
        coverage_gate(root, &json)?;
    } else {
        step("nextest");
        sh(root, &cargo(), &["nextest", "run", "--workspace", "--features", TEST_FEATURES])?;
    }
    if with_deny {
        step("cargo deny");
        sh(root, &cargo(), &["deny", "--workspace", "check"])?;
    }
    eprintln!("\nxtask: ci passed");
    Ok(())
}

fn check(root: &Path) -> Result<()> {
    ensure_ui_dist(root)?;
    fmt_clippy(root)?;
    step("nextest");
    sh(root, &cargo(), &["nextest", "run", "--workspace", "--features", TEST_FEATURES])?;
    eprintln!("\nxtask: check passed");
    Ok(())
}

fn fmt_clippy(root: &Path) -> Result<()> {
    let c = cargo();
    step("cargo fmt --check");
    sh(root, &c, &["fmt", "--all", "--", "--check"])?;
    step("cargo clippy -D warnings");
    sh(
        root,
        &c,
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--features",
            TEST_FEATURES,
            "--",
            "-D",
            "warnings",
        ],
    )
}

fn ui(root: &Path) -> Result<()> {
    let ui = root.join("ui");
    let b = bun();
    step("bun install --frozen-lockfile");
    sh(&ui, &b, &["install", "--frozen-lockfile"])?;
    step("bun test");
    sh(&ui, &b, &["test"])?;
    step("bun run typecheck");
    sh(&ui, &b, &["run", "typecheck"])?;
    step("bun run build");
    sh(&ui, &b, &["run", "build"])
}

/// `tauri::generate_context!` needs `ui/dist`; build it if missing.
fn ensure_ui_dist(root: &Path) -> Result<()> {
    if root.join("ui/dist/index.html").is_file() {
        return Ok(());
    }
    let ui = root.join("ui");
    let b = bun();
    step("ui/dist missing: bun install + bun run build");
    sh(&ui, &b, &["install", "--frozen-lockfile"])?;
    sh(&ui, &b, &["run", "build"])
}

fn npm_ban_check(root: &Path) -> Result<()> {
    let files = repo::list_files(root)?;
    let v = npm_ban::scan_files(root, &files);
    if v.is_empty() {
        eprintln!("npm-ban: {} files clean", files.len());
        return Ok(());
    }
    for x in &v {
        eprintln!("npm-ban: {}:{}: {}", x.path.display(), x.line, x.message);
    }
    bail!("{} npm-ban violation(s); use bun (plan §0)", v.len())
}

fn secret_scan_check(root: &Path, dir: Option<PathBuf>) -> Result<()> {
    let Some(dir) = dir.or_else(secret_scan::default_secrets_dir) else {
        eprintln!("secret-scan: no secrets directory configured; skipped");
        return Ok(());
    };
    let secrets = secret_scan::load_secrets(&dir)?;
    if secrets.is_empty() {
        eprintln!("secret-scan: no secrets found in {}; skipped", dir.display());
        return Ok(());
    }
    let files = repo::list_files(root)?;
    let hits = secret_scan::scan_files(root, &files, &secrets)?;
    if hits.is_empty() {
        eprintln!("secret-scan: {} secrets x {} files clean", secrets.len(), files.len());
        return Ok(());
    }
    for h in &hits {
        eprintln!("secret-scan: {} contains secret {} ({:?})", h.path.display(), h.secret, h.encoding);
    }
    bail!("{} secret(s) found in repository files", hits.len())
}

fn coverage_gate(root: &Path, json: &Path) -> Result<()> {
    let text = std::fs::read_to_string(json).with_context(|| format!("reading {}", json.display()))?;
    let results = coverage::evaluate(&text, root, coverage::TARGETS)?;
    let mut failed = false;
    for r in &results {
        let status = if !r.enforced() {
            "not yet enforced (no code)"
        } else if r.passed() {
            "ok"
        } else {
            failed = true;
            "FAIL"
        };
        eprintln!(
            "coverage: {:<28} {:>6.2}% ({}/{} lines, min {:.0}%) {status}",
            r.name,
            r.percent(),
            r.covered,
            r.lines,
            r.min_percent
        );
    }
    if failed {
        bail!("coverage gate failed");
    }
    Ok(())
}

fn bindings(root: &Path, check_only: bool) -> Result<()> {
    ensure_ui_dist(root)?;
    let committed = root.join("ui/src/bindings.ts");
    let out = if check_only { root.join("target/bindings.check.ts") } else { committed.clone() };
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    sh(
        root,
        &cargo(),
        &[
            "run",
            "--quiet",
            "--locked",
            "-p",
            "drift-app",
            "--example",
            "export_bindings",
            "--",
            out.to_str().context("non-UTF-8 path")?,
        ],
    )?;
    if check_only {
        let fresh = std::fs::read(&out)?;
        let current = std::fs::read(&committed).unwrap_or_default();
        if fresh != current {
            bail!("ui/src/bindings.ts is stale; run `cargo xtask bindings` and commit the result");
        }
        eprintln!("bindings: ui/src/bindings.ts is fresh");
    } else {
        eprintln!("bindings: wrote {}", out.display());
    }
    Ok(())
}

/// Daemon ports on the GNOME host and their local forward ports (plan §5.3).
const E2E_PORTS: &[(u16, u16)] = &[(13389, 3389), (13390, 3390), (13391, 3391), (13392, 3392)];
const E2E_HOST: &str = "homelab@10.1.2.40";

fn e2e(root: &Path, extra: &[String]) -> Result<()> {
    let host = std::env::var("DRIFT_E2E_SSH").unwrap_or_else(|_| E2E_HOST.into());
    let mut ssh = Command::new("ssh");
    ssh.args(["-N", "-o", "BatchMode=yes", "-o", "ExitOnForwardFailure=yes"]);
    for (local, remote) in E2E_PORTS {
        ssh.arg("-L").arg(format!("{local}:localhost:{remote}"));
    }
    ssh.arg(&host).stdin(Stdio::null());
    step(&format!("opening SSH forwards to {host}"));
    let mut child = ForwardGuard(ssh.spawn().context("spawning ssh")?);
    wait_for_port(E2E_PORTS[0].0, &mut child.0)?;

    let mut cmd = Command::new(cargo());
    cmd.current_dir(root).args(["nextest", "run", "-p", "drift-e2e", "--run-ignored", "only"]).args(extra);
    cmd.env("DRIFT_E2E_HOST", "127.0.0.1").env("DRIFT_E2E_TLS_NAME", "10.1.2.40");
    for (local, remote) in E2E_PORTS {
        cmd.env(format!("DRIFT_E2E_PORT_{remote}"), local.to_string());
    }
    if let Some(dir) = secret_scan::default_secrets_dir() {
        for (k, v) in e2e_env::vars_from_dir(&dir) {
            if std::env::var_os(&k).is_none() {
                cmd.env(k, v);
            }
        }
    }
    step("cargo nextest run -p drift-e2e --run-ignored only");
    let status = cmd.status().context("running nextest")?;
    if !status.success() {
        bail!("e2e failed ({status})");
    }
    Ok(())
}

struct ForwardGuard(Child);

impl Drop for ForwardGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for_port(port: u16, child: &mut Child) -> Result<()> {
    for _ in 0..100 {
        if let Some(status) = child.try_wait()? {
            bail!("ssh exited early ({status})");
        }
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    bail!("SSH forward on 127.0.0.1:{port} did not come up")
}
