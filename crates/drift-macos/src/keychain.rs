//! Keychain password adapter (task **M3-2**, plan §2 decision 7).
//!
//! Passwords are generic-password items in the user's login keychain. The **service** is
//! Drift's bundle identifier ([`Keychain::SERVICE`]); the **account** is
//! `SecretRole::account(profile_id)` = `<uuid>/<role>` (`rdp-system`, `rdp-user`,
//! `linux-login`), so every (profile, role) pair has exactly one item. Tests use a unique
//! throw-away service name ([`Keychain::with_service`]) and never touch Drift's real items.
//!
//! One-time Server Redirection credentials never come here: they only live in memory.

use drift_core::SecretRole;
use uuid::Uuid;
use zeroize::Zeroizing;

/// A Keychain operation failed (the message never contains the secret).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Keychain error {code}: {message}")]
pub struct KeychainError {
    /// The `OSStatus`.
    pub code: i32,
    /// Human-readable description from Security.framework.
    pub message: String,
}

/// Generic-password store keyed by profile id and role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keychain {
    service: String,
}

impl Default for Keychain {
    fn default() -> Self {
        Self::new()
    }
}

impl Keychain {
    /// The service name of Drift's items (the app's bundle identifier).
    pub const SERVICE: &'static str = "com.hutsonlabs.drift";

    /// The production store.
    pub fn new() -> Self {
        Self::with_service(Self::SERVICE)
    }

    /// A store under another service name (tests).
    pub fn with_service(service: impl Into<String>) -> Self {
        Self { service: service.into() }
    }

    /// The service name.
    pub fn service(&self) -> &str {
        &self.service
    }

    /// Stores (creates or replaces) the secret for `profile`/`role`.
    pub fn set(&self, profile: Uuid, role: SecretRole, secret: &str) -> Result<(), KeychainError> {
        let _ = (profile, role, secret);
        Err(KeychainError { code: -4, message: "not implemented".into() })
    }

    /// Reads the secret for `profile`/`role`; `None` if there is no item.
    pub fn get(&self, profile: Uuid, role: SecretRole) -> Result<Option<Zeroizing<String>>, KeychainError> {
        let _ = (profile, role);
        Err(KeychainError { code: -4, message: "not implemented".into() })
    }

    /// Deletes the secret for `profile`/`role`; deleting a missing item succeeds.
    pub fn delete(&self, profile: Uuid, role: SecretRole) -> Result<(), KeychainError> {
        let _ = (profile, role);
        Err(KeychainError { code: -4, message: "not implemented".into() })
    }

    /// Deletes every role's secret for `profile` (profile deleted).
    pub fn delete_profile(&self, profile: Uuid) -> Result<(), KeychainError> {
        let _ = profile;
        Err(KeychainError { code: -4, message: "not implemented".into() })
    }
}
