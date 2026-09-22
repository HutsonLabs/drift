//! M0-4 Red: the host scripts are shellcheck-clean and the setup script embeds exactly the
//! committed systemd drop-in.

use std::path::PathBuf;
use std::process::Command;

fn host(file: &str) -> PathBuf {
    xtask::repo::root().join("host").join(file)
}

fn read(file: &str) -> String {
    std::fs::read_to_string(host(file)).unwrap_or_else(|e| panic!("host/{file}: {e}"))
}

#[test]
fn host_scripts_are_shellcheck_clean() {
    for script in ["drift-host-setup.sh", "drift-host-report.sh"] {
        let path = host(script);
        assert!(path.is_file(), "missing host/{script}");
        let out = Command::new("shellcheck")
            .args(["--shell=bash", "--severity=style"])
            .arg(&path)
            .output()
            .unwrap_or_else(|e| panic!("shellcheck is required (brew install shellcheck): {e}"));
        assert!(
            out.status.success(),
            "shellcheck host/{script}:\n{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn setup_script_is_strict_and_defaults_to_check_mode() {
    let s = read("drift-host-setup.sh");
    assert!(s.starts_with("#!/usr/bin/env bash\n"));
    assert!(s.contains("\nset -euo pipefail\n"));
    assert!(s.contains("\nMODE=check\n"), "default mode must change nothing");
    for flag in ["--check", "--dry-run", "--apply", "--system", "--headless"] {
        assert!(s.contains(flag), "missing {flag}");
    }
    assert!(s.contains("change_secret"), "credential changes must not echo their command line");
}

#[test]
fn setup_script_embeds_the_committed_dropin() {
    let dropin = read("gnome-headless-session-dropin.conf");
    assert_eq!(dropin, "[Service]\nDynamicUser=no\nUser=gdm\n", "plan §1.9 drop-in");
    let s = read("drift-host-setup.sh");
    let start = s.find("readonly DROPIN_CONTENT='").map(|i| i + "readonly DROPIN_CONTENT='".len());
    let embedded = start.and_then(|i| s[i..].find('\'').map(|j| &s[i..i + j]));
    assert_eq!(embedded.map(|e| format!("{e}\n")), Some(dropin));
}

#[test]
fn report_script_is_read_only() {
    let s = read("drift-host-report.sh");
    for forbidden in
        ["set-credentials", "set-port", " enable", "disable-", "install ", "rm ", "gsettings set"]
    {
        assert!(!s.contains(forbidden), "report script must not contain {forbidden:?}");
    }
    assert!(!s.contains("--show-credentials"));
    assert_eq!(xtask::host_check::REPORT_SCRIPT, s);
}
