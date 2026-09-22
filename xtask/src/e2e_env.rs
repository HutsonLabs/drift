//! Maps the dev-machine secrets directory onto the `DRIFT_E2E_*` environment variables
//! consumed by `tests/e2e` (plan §5.3). Values are never printed.
//!
//! | File | Format | Variables |
//! |---|---|---|
//! | `system.txt` | `Username: u` / `Password: p` lines | `DRIFT_E2E_SYS_USER`, `DRIFT_E2E_SYS_PASS` |
//! | `testuser.txt` | line 1 user, line 2 password | `DRIFT_E2E_LOGIN_USER`, `DRIFT_E2E_LOGIN_PASS` |
//! | `headless2.txt` | line 1 user, line 2 password | `DRIFT_E2E_HL_USER`, `DRIFT_E2E_HL_PASS` (+ `DRIFT_E2E_HL_PORT`) |
//! | `headless.txt` | line 1 user, line 2 password | `DRIFT_E2E_HL1_USER`, `DRIFT_E2E_HL1_PASS` |
//! | `share.txt` | line 1 user, line 2 password | `DRIFT_E2E_SHARE_USER`, `DRIFT_E2E_SHARE_PASS` |

use std::path::Path;

/// Secrets file → (user variable, password variable).
const FILES: &[(&str, &str, &str)] = &[
    ("system.txt", "DRIFT_E2E_SYS_USER", "DRIFT_E2E_SYS_PASS"),
    ("testuser.txt", "DRIFT_E2E_LOGIN_USER", "DRIFT_E2E_LOGIN_PASS"),
    ("headless2.txt", "DRIFT_E2E_HL_USER", "DRIFT_E2E_HL_PASS"),
    ("headless.txt", "DRIFT_E2E_HL1_USER", "DRIFT_E2E_HL1_PASS"),
    ("share.txt", "DRIFT_E2E_SHARE_USER", "DRIFT_E2E_SHARE_PASS"),
];

/// Local forward port of the `drifttest2` headless daemon (remote :3392).
pub const HL_PORT: &str = "13392";

/// `(variable, value)` pairs derived from the secrets directory; missing files are skipped.
pub fn vars_from_dir(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (file, user_var, pass_var) in FILES {
        let Ok(bytes) = std::fs::read(dir.join(file)) else {
            continue;
        };
        let Some((user, pass)) = parse_credentials(&String::from_utf8_lossy(&bytes)) else {
            continue;
        };
        out.push(((*user_var).to_owned(), user));
        out.push(((*pass_var).to_owned(), pass));
        if *file == "headless2.txt" {
            out.push(("DRIFT_E2E_HL_PORT".to_owned(), HL_PORT.to_owned()));
        }
    }
    out
}

/// Replaces credential values in the e2e run's output (the harness-side redact layer; the
/// tests also redact their own `tracing` output, see `drift-e2e`).
#[derive(Clone, Default)]
pub struct Redactor {
    secrets: Vec<String>,
}

impl std::fmt::Debug for Redactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redactor").field("secrets", &self.secrets.len()).finish()
    }
}

impl Redactor {
    /// Redacts the values of every `*_USER` / `*_PASS` variable in `vars` (longest first).
    pub fn from_vars(vars: &[(String, String)]) -> Self {
        let mut secrets: Vec<String> = vars
            .iter()
            .filter(|(k, v)| (k.ends_with("_USER") || k.ends_with("_PASS")) && !v.is_empty())
            .map(|(_, v)| v.clone())
            .collect();
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
        secrets.dedup();
        Self { secrets }
    }

    /// `text` with every secret replaced by `<redacted>`.
    pub fn redact(&self, text: &str) -> String {
        self.secrets.iter().fold(text.to_owned(), |t, s| t.replace(s.as_str(), "<redacted>"))
    }
}

/// Parses a user/password pair from a secrets file's text (`Key: value` or bare lines).
pub fn parse_credentials(text: &str) -> Option<(String, String)> {
    let mut values =
        text.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| match l.split_once(": ") {
            Some((_, v)) => v.trim().to_owned(),
            None => l.to_owned(),
        });
    Some((values.next()?, values.next()?))
}
