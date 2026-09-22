//! Local Network Privacy (plan §1.8, tasks M1-1 / M9-3).
//!
//! A binary without the Local Network permission gets `EHOSTUNREACH` (errno 65) when it
//! connects to a LAN host, while loopback, `ssh` and `nc` keep working. Drift shows a dedicated
//! screen for it (`DisconnectReason::LocalNetworkDenied`) whose button opens System Settings ›
//! Privacy & Security › Local Network.

use std::io;

use drift_core::messages::LOCAL_NETWORK_SETTINGS_URL;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSString, NSURL};

/// `EHOSTUNREACH` on macOS.
pub const EHOSTUNREACH: i32 = 65;

/// Whether a connect error is the Local Network Privacy denial (errno 65).
///
/// Wrapped errors (an `io::Error` whose inner error is an `io::Error`, as tokio and TLS layers
/// produce) are unwrapped.
pub fn is_local_network_denied(error: &io::Error) -> bool {
    let mut current = error;
    loop {
        if current.raw_os_error() == Some(EHOSTUNREACH) {
            return true;
        }
        match current.get_ref().and_then(|inner| inner.downcast_ref::<io::Error>()) {
            Some(inner) => current = inner,
            None => return false,
        }
    }
}

/// The `x-apple.systempreferences:` URL of the Local Network privacy pane.
pub const fn local_network_settings_url() -> &'static str {
    LOCAL_NETWORK_SETTINGS_URL
}

/// Opening System Settings failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("macOS could not open {0}")]
pub struct OpenSettingsError(pub String);

/// Opens System Settings › Privacy & Security › Local Network.
pub fn open_local_network_settings() -> Result<(), OpenSettingsError> {
    let url = local_network_settings_url();
    let err = || OpenSettingsError(url.into());
    let nsurl = NSURL::URLWithString(&NSString::from_str(url)).ok_or_else(err)?;
    if NSWorkspace::sharedWorkspace().openURL(&nsurl) { Ok(()) } else { Err(err()) }
}
