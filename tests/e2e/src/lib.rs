//! # drift-e2e
//!
//! Real-host tests (plan §5.3). Every test is `#[ignore]` and is run by
//! `cargo xtask e2e`, which opens `ssh -N -L 1339x:localhost:339x homelab@10.1.2.40`
//! and exports:
//!
//! | Variable | Meaning |
//! |---|---|
//! | `DRIFT_E2E_HOST` | `127.0.0.1` (the SSH forward) |
//! | `DRIFT_E2E_TLS_NAME` | `10.1.2.40` (certificate host) |
//! | `DRIFT_E2E_PORT_<remote>` | local forward port for remote port 3389..3392 |
//! | `DRIFT_E2E_SYS_USER/PASS` | Remote Login system RDP credentials |
//! | `DRIFT_E2E_LOGIN_USER/PASS` | Linux login `drifttest` |
//! | `DRIFT_E2E_HL_USER/PASS`, `DRIFT_E2E_HL_PORT` | headless daemon of `drifttest2` |
//!
//! Never print these values; tests must go through the `redact` layer (M9-3).

/// Reads a `DRIFT_E2E_*` variable, returning `None` when unset or empty.
pub fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Local port that forwards to `remote_port` on the host (e.g. 3392 → 13392).
pub fn forwarded_port(remote_port: u16) -> Option<u16> {
    var(&format!("DRIFT_E2E_PORT_{remote_port}")).and_then(|p| p.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_variables_are_none() {
        assert_eq!(var("DRIFT_E2E_DEFINITELY_UNSET_VARIABLE"), None);
        assert_eq!(forwarded_port(1), None);
    }
}
