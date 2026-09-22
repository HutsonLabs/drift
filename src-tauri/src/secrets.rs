//! Password storage seam (plan §2 decision 7).
//!
//! Profiles never contain secrets; passwords are stored per profile id + [`SecretRole`]. The
//! production implementation is the Keychain adapter from `drift-macos` (M3-2, stream C); until
//! it is wired in, the app uses [`MemorySecretStore`] (process lifetime only).

use std::collections::HashMap;
use std::sync::Mutex;

use drift_core::SecretRole;
use uuid::Uuid;
use zeroize::Zeroizing;

/// Failure of the secret backend (message is safe to show: it never contains the secret).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("password storage failed: {0}")]
pub struct SecretError(pub String);

/// Stores passwords by profile id and role.
pub trait SecretStore: Send + Sync {
    /// Stores (replaces) a secret.
    fn set(&self, profile: Uuid, role: SecretRole, secret: &str) -> Result<(), SecretError>;
    /// Reads a secret.
    fn get(&self, profile: Uuid, role: SecretRole) -> Result<Option<Zeroizing<String>>, SecretError>;
    /// Deletes a secret; deleting a missing item is not an error.
    fn delete(&self, profile: Uuid, role: SecretRole) -> Result<(), SecretError>;

    /// Whether a secret exists.
    fn has(&self, profile: Uuid, role: SecretRole) -> bool {
        matches!(self.get(profile, role), Ok(Some(_)))
    }
}

type SecretMap = HashMap<(Uuid, SecretRole), Zeroizing<String>>;

/// In-memory [`SecretStore`] (tests, and the app until the Keychain adapter lands).
#[derive(Default)]
pub struct MemorySecretStore {
    items: Mutex<SecretMap>,
}

impl MemorySecretStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, SecretMap>, SecretError> {
        self.items.lock().map_err(|_| SecretError("secret store lock poisoned".into()))
    }
}

impl SecretStore for MemorySecretStore {
    fn set(&self, profile: Uuid, role: SecretRole, secret: &str) -> Result<(), SecretError> {
        self.lock()?.insert((profile, role), Zeroizing::new(secret.to_owned()));
        Ok(())
    }

    fn get(&self, profile: Uuid, role: SecretRole) -> Result<Option<Zeroizing<String>>, SecretError> {
        Ok(self.lock()?.get(&(profile, role)).cloned())
    }

    fn delete(&self, profile: Uuid, role: SecretRole) -> Result<(), SecretError> {
        self.lock()?.remove(&(profile, role));
        Ok(())
    }
}

impl std::fmt::Debug for MemorySecretStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = self.items.lock().map(|m| m.len()).unwrap_or(0);
        f.debug_struct("MemorySecretStore").field("items", &n).finish_non_exhaustive()
    }
}
