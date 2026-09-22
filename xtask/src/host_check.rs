//! `cargo xtask host-setup-check` (task M0-4).
//!
//! Runs `host/drift-host-report.sh` on the GNOME host over SSH (read-only), parses its
//! `### <section>` report and checks ports, daemons, the headless session and certificates
//! against [`Expectations`]. Parsing and evaluation are pure and unit-tested on reports
//! captured from the homelab host (`xtask/tests/data/host/`).

use std::str::FromStr;

use anyhow::Result;

/// The report script, run on the host with `bash -s -- <specs>`.
pub const REPORT_SCRIPT: &str = include_str!("../../host/drift-host-report.sh");

/// A headless daemon Drift expects: `USER:PORT[:persistent]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessSpec {
    /// Linux user owning the headless daemon.
    pub user: String,
    /// Fixed RDP port.
    pub port: u16,
    /// Whether `gnome-headless-session@USER.service` must be enabled and running.
    pub persistent: bool,
}

impl FromStr for HeadlessSpec {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let _ = s;
        unimplemented!("M0-4")
    }
}

impl std::fmt::Display for HeadlessSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.user, self.port)?;
        if self.persistent {
            write!(f, ":persistent")?;
        }
        Ok(())
    }
}

/// What the host must look like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expectations {
    /// Remote Login (system daemon) port.
    pub system_port: u16,
    /// Headless daemons.
    pub headless: Vec<HeadlessSpec>,
}

impl Default for Expectations {
    /// The homelab test host (plan §5.2).
    fn default() -> Self {
        Self {
            system_port: 3389,
            headless: vec![
                HeadlessSpec { user: "drifttest".into(), port: 3391, persistent: false },
                HeadlessSpec { user: "drifttest2".into(), port: 3392, persistent: true },
            ],
        }
    }
}

/// One `### name args…` block of the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// Section name (`ss-ltnp`, `grdctl-headless`, …).
    pub name: String,
    /// Words after the name.
    pub args: Vec<String>,
    /// Body lines.
    pub body: Vec<String>,
}

/// Parsed `grdctl status` output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrdStatus {
    /// `Overall: Unit status: active`.
    pub unit_active: bool,
    /// `RDP: Status: enabled|disabled`.
    pub enabled: Option<bool>,
    /// Configured port.
    pub port: Option<u16>,
    /// `Negotiate port: yes|no` (absent for the system daemon).
    pub negotiate_port: Option<bool>,
    /// TLS certificate path, if set.
    pub tls_cert: Option<String>,
    /// TLS key path, if set.
    pub tls_key: Option<String>,
    /// TLS SHA-256 fingerprint (`aa:bb:…`), if set.
    pub tls_fingerprint: Option<String>,
    /// Username is set (`(hidden)`).
    pub username_set: bool,
    /// Password is set (`(hidden)`).
    pub password_set: bool,
}

/// A listening TCP socket from `ss -ltnp`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    /// Local port.
    pub port: u16,
    /// Process name (as truncated by `ss`).
    pub process: String,
    /// Process id.
    pub pid: u32,
}

/// A process line from `ps -o pid=,user:32=,args=`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proc {
    /// Process id.
    pub pid: u32,
    /// User name.
    pub user: String,
    /// Command line.
    pub args: String,
}

/// `systemctl is-active` / `is-enabled` answers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitState {
    /// `is-active` output (`active`, `inactive`, …).
    pub active: String,
    /// `is-enabled` output (`enabled`, `disabled`, …).
    pub enabled: String,
}

/// The parsed report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostReport {
    /// Listening sockets.
    pub listeners: Vec<Listener>,
    /// g-r-d daemon processes.
    pub processes: Vec<Proc>,
    /// System units by name.
    pub units: Vec<(String, UnitState)>,
    /// `systemctl show -p DynamicUser -p User` per unit.
    pub unit_props: Vec<(String, Vec<(String, String)>)>,
    /// User units: (user, unit, state).
    pub user_units: Vec<(String, String, UnitState)>,
    /// System daemon status.
    pub system: Option<GrdStatus>,
    /// Headless daemon status per user.
    pub headless: Vec<(String, GrdStatus)>,
    /// Certificate state per owner (`system` or a user): `valid`, `expired`, `unset`, `missing …`.
    pub certs: Vec<(String, String)>,
    /// Whether the report ended with `### end` (not truncated).
    pub complete: bool,
}

/// One check result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// What was checked.
    pub name: String,
    /// Passed?
    pub ok: bool,
    /// Observed value / reason.
    pub detail: String,
}

/// Splits a report into sections (lines before the first `###` are ignored).
pub fn split_sections(report: &str) -> Vec<Section> {
    let _ = report;
    unimplemented!("M0-4")
}

/// Parses `grdctl status` output.
pub fn parse_grdctl_status(text: &str) -> GrdStatus {
    let _ = text;
    unimplemented!("M0-4")
}

/// Parses `ss -ltnp` output.
pub fn parse_ss_ltnp(text: &str) -> Vec<Listener> {
    let _ = text;
    unimplemented!("M0-4")
}

/// Parses `ps -o pid=,user:32=,args=` output.
pub fn parse_ps(text: &str) -> Vec<Proc> {
    let _ = text;
    unimplemented!("M0-4")
}

/// Parses a full report.
pub fn parse_report(report: &str) -> Result<HostReport> {
    let _ = report;
    unimplemented!("M0-4")
}

/// Evaluates a report against expectations.
pub fn evaluate(report: &HostReport, exp: &Expectations) -> Vec<Check> {
    let _ = (report, exp);
    unimplemented!("M0-4")
}
