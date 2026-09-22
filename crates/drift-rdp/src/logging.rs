//! Credential-safe logging policy for the RDP stack (task **M9-3**).
//!
//! Drift itself never formats a password: the secrets it holds live in `Zeroizing` values whose
//! `Debug` is redacted (`SessionSecrets`, [`crate::rdstls::OneTimeCredentials`]). The crates
//! underneath are not so careful. `sspi`, which performs CredSSP/NTLM for leg 1, is
//! `#[instrument]`ed down to the buffers it writes, so at `DEBUG` and `TRACE` it prints the
//! TS credentials — the user name as UTF-16LE hex and the **password as a decimal byte list**.
//! A `RUST_LOG=trace` session would therefore write the profile password into the terminal or
//! into whatever collects Drift's output.
//!
//! Redacting after the fact cannot fix that (the e2e harness only knows the credentials it
//! injected), so Drift instead refuses to enable those targets below
//! [`MAX_CREDENTIAL_TARGET_LEVEL`], whatever `RUST_LOG` says. Every place that installs a
//! subscriber — the app (`drift-app`) and the e2e harness (`drift-e2e`) — appends
//! [`credential_safe_directives`] **after** the user's directives, so the cap wins.
//!
//! The policy is data, not a subscriber: `drift-rdp` stays free of `tracing-subscriber`, and
//! `crates/drift-rdp/tests/m9_3_logging.rs` proves the result by running a whole Remote Login
//! connection under a `TRACE` subscriber and searching the output for every credential.

/// Targets (crate or module paths) that log credentials below [`MAX_CREDENTIAL_TARGET_LEVEL`].
///
/// A target matches by prefix, so `sspi` also covers `sspi::ntlm` and `sspi::credssp::…`.
pub const CREDENTIAL_LOGGING_TARGETS: &[&str] = &["sspi"];

/// The most verbose level [`CREDENTIAL_LOGGING_TARGETS`] may ever log at.
///
/// `INFO` keeps their warnings and errors, which are useful when NLA fails, and drops the
/// `DEBUG`/`TRACE` spans that carry the credential buffers.
pub const MAX_CREDENTIAL_TARGET_LEVEL: &str = "info";

/// `EnvFilter` directives capping every [`CREDENTIAL_LOGGING_TARGETS`] entry.
///
/// Append them to a filter built from `RUST_LOG`; a later directive for the same target
/// replaces an earlier one, so `RUST_LOG=sspi=trace` cannot re-enable the credential spans.
///
/// ```
/// let directives = drift_rdp::logging::credential_safe_directives();
/// assert_eq!(directives, ["sspi=info"]);
/// ```
pub fn credential_safe_directives() -> Vec<String> {
    CREDENTIAL_LOGGING_TARGETS.iter().map(|t| format!("{t}={MAX_CREDENTIAL_TARGET_LEVEL}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_target_is_capped_at_the_policy_level() {
        let directives = credential_safe_directives();
        assert_eq!(directives.len(), CREDENTIAL_LOGGING_TARGETS.len());
        for (directive, target) in directives.iter().zip(CREDENTIAL_LOGGING_TARGETS) {
            let (t, level) = directive.split_once('=').unwrap_or_else(|| unreachable!("{directive}"));
            assert_eq!(t, *target);
            assert_eq!(level, MAX_CREDENTIAL_TARGET_LEVEL);
        }
    }

    #[test]
    fn the_cap_hides_the_levels_that_carry_credentials() {
        // A weaker cap would leave `sspi`'s instrumented spans (and their credential fields)
        // switched on; `trace`/`debug` are exactly what the M9-3 log test caught.
        assert!(matches!(MAX_CREDENTIAL_TARGET_LEVEL, "info" | "warn" | "error" | "off"));
    }
}
