//! Credential-safe logging policy for the RDP stack (task **M9-3**).
//!
//! Drift itself never formats a password: the secrets it holds live in `Zeroizing` values whose
//! `Debug` is redacted (`SessionSecrets`, [`crate::rdstls::OneTimeCredentials`]). The crates
//! underneath are not so careful. `sspi`, which performs CredSSP/NTLM for leg 1, is
//! `#[instrument]`ed down to the buffers it writes, so at `DEBUG` and `TRACE` it prints the
//! TS credentials — the user name as UTF-16LE hex and the **password as a decimal byte list**.
//! A `RUST_LOG=trace` session would otherwise write the profile password into the terminal or
//! into whatever collects Drift's output.
//!
//! Redacting after the fact cannot fix that (the e2e harness only knows the credentials it
//! injected), so Drift refuses to *enable* those targets below [`MAX_CREDENTIAL_TARGET_LEVEL`],
//! whatever `RUST_LOG` says. [`allows`] is the rule; every place that installs a subscriber —
//! the app (`drift-app`) and the e2e harness (`drift-e2e`) — adds it as a second global filter
//! next to the `EnvFilter`:
//!
//! ```ignore
//! tracing_subscriber::registry()
//!     .with(env_filter)
//!     .with(tracing_subscriber::filter::filter_fn(|m| {
//!         drift_rdp::logging::allows(m.target(), *m.level())
//!     }))
//!     .with(tracing_subscriber::fmt::layer())
//!     .try_init();
//! ```
//!
//! A filter, not an `EnvFilter` directive: directives are matched most-specific-first, so
//! `sspi=info` appended after `RUST_LOG=sspi::credssp=trace` loses to the longer target and the
//! credential spans come back (found by `crates/drift-rdp/tests/m9_3_log_filter.rs`).
//!
//! The policy stays data plus a pure predicate, so `drift-rdp` needs no `tracing-subscriber`
//! dependency, and `crates/drift-rdp/tests/m9_3_logging.rs` proves the result end to end by
//! running a whole Remote Login connection under a `TRACE` subscriber and searching the output
//! for every credential.

use tracing::Level;

/// Targets (crate or module paths) that log credentials below [`MAX_CREDENTIAL_TARGET_LEVEL`].
///
/// A target matches itself and everything under it, so `sspi` also covers `sspi::ntlm` and
/// `sspi::credssp::ts_request`.
pub const CREDENTIAL_LOGGING_TARGETS: &[&str] = &["sspi"];

/// The most verbose level [`CREDENTIAL_LOGGING_TARGETS`] may ever log at.
///
/// `INFO` keeps their warnings and errors, which are what matters when NLA fails, and drops the
/// `DEBUG`/`TRACE` spans and events that carry the credential buffers.
pub const MAX_CREDENTIAL_TARGET_LEVEL: Level = Level::INFO;

/// Whether `target` is one of [`CREDENTIAL_LOGGING_TARGETS`] or a module inside one.
pub fn is_credential_logging_target(target: &str) -> bool {
    CREDENTIAL_LOGGING_TARGETS.iter().any(|capped| {
        target == *capped
            || (target.len() > capped.len()
                && target.starts_with(capped)
                && target[capped.len()..].starts_with("::"))
    })
}

/// Whether a span or event with this `target` and `level` may be recorded.
///
/// Everything that is not a credential-logging target passes; those are capped at
/// [`MAX_CREDENTIAL_TARGET_LEVEL`].
///
/// ```
/// use drift_rdp::logging::allows;
/// use tracing::Level;
///
/// assert!(allows("drift_rdp::actor", Level::TRACE));
/// assert!(allows("sspi", Level::WARN));
/// assert!(!allows("sspi::credssp::ts_request", Level::DEBUG));
/// ```
pub fn allows(target: &str, level: Level) -> bool {
    !is_credential_logging_target(target) || level <= MAX_CREDENTIAL_TARGET_LEVEL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capped_targets_match_themselves_and_their_modules_only() {
        assert!(is_credential_logging_target("sspi"));
        assert!(is_credential_logging_target("sspi::ntlm"));
        assert!(is_credential_logging_target("sspi::credssp::ts_request"));
        // A different crate whose name merely starts the same way is not capped.
        assert!(!is_credential_logging_target("sspinner"));
        assert!(!is_credential_logging_target("drift_rdp::actor"));
        assert!(!is_credential_logging_target(""));
    }

    #[test]
    fn the_cap_keeps_warnings_and_drops_the_credential_levels() {
        for level in [Level::ERROR, Level::WARN, Level::INFO] {
            assert!(allows("sspi::ntlm", level), "{level} is kept for diagnostics");
        }
        for level in [Level::DEBUG, Level::TRACE] {
            assert!(!allows("sspi::ntlm", level), "{level} carries the credential buffers");
            assert!(allows("drift_rdp::actor", level), "Drift's own logging is untouched");
        }
    }
}
