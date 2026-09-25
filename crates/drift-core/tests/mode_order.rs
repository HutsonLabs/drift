//! UI-windows Red: the mode order (ADR UI-windows-gallery decision 14).
#![allow(missing_docs)]

use drift_core::{ConnectMode, ConnectionProfile};

/// Headless first (the default for new profiles), then Desktop Sharing, then Remote Login,
/// everywhere modes are listed.
#[test]
fn modes_are_listed_headless_first() {
    assert_eq!(
        ConnectMode::ALL,
        [ConnectMode::Headless, ConnectMode::DesktopSharing, ConnectMode::RemoteLogin]
    );
}

/// Only the order changes: every mode still round-trips through its saved name, so existing
/// `profiles.toml` files load unchanged.
#[test]
fn saved_mode_names_are_unchanged() {
    for (mode, name) in ConnectMode::ALL.into_iter().zip(["headless", "desktop-sharing", "remote-login"]) {
        let toml = ConnectionProfile::new("x", "h", mode).to_toml().expect("serialize");
        assert!(toml.contains(&format!("mode = \"{name}\"")), "{mode:?}: {toml}");
        assert_eq!(ConnectionProfile::from_toml(&toml).expect("parse").mode, mode);
    }
}
