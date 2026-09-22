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

/// `(variable, value)` pairs derived from the secrets directory; missing files are skipped.
pub fn vars_from_dir(dir: &Path) -> Vec<(String, String)> {
    let _ = dir;
    Vec::new()
}

/// Parses a user/password pair from a secrets file's text (`Key: value` or bare lines).
pub fn parse_credentials(text: &str) -> Option<(String, String)> {
    let _ = text;
    None
}
