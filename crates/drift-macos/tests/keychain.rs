//! M3-2: Keychain adapter against the real login keychain, under a unique test-only service
//! name (never Drift's real service). Every test cleans up after itself.
#![allow(missing_docs, clippy::unwrap_used)]

use drift_core::SecretRole;
use drift_macos::keychain::Keychain;
use uuid::Uuid;

/// A unique service per test so parallel test processes never collide.
struct TestKeychain(Keychain, Vec<Uuid>);

impl TestKeychain {
    fn new() -> Self {
        Self(Keychain::with_service(format!("com.hutsonlabs.drift.tests.{}", Uuid::new_v4())), Vec::new())
    }

    fn profile(&mut self) -> Uuid {
        let id = Uuid::new_v4();
        self.1.push(id);
        id
    }
}

impl Drop for TestKeychain {
    fn drop(&mut self) {
        for id in &self.1 {
            let _ = self.0.delete_profile(*id);
        }
    }
}

// Obviously fake test values (never real credentials).
const FAKE: &str = "fake-password-🔑-ü";
const FAKE2: &str = "another fake value";

#[test]
fn test_service_is_not_drifts_real_service() {
    let kc = TestKeychain::new();
    assert_ne!(kc.0.service(), Keychain::SERVICE);
    assert_eq!(Keychain::new().service(), "com.hutsonlabs.drift");
}

#[test]
fn set_get_round_trip() {
    let mut kc = TestKeychain::new();
    let p = kc.profile();
    kc.0.set(p, SecretRole::RdpUser, FAKE).unwrap();
    assert_eq!(kc.0.get(p, SecretRole::RdpUser).unwrap().as_deref().map(String::as_str), Some(FAKE));
}

#[test]
fn missing_item_is_none() {
    let mut kc = TestKeychain::new();
    let p = kc.profile();
    assert_eq!(kc.0.get(p, SecretRole::RdpSystem).unwrap(), None);
}

#[test]
fn set_replaces_an_existing_item() {
    let mut kc = TestKeychain::new();
    let p = kc.profile();
    kc.0.set(p, SecretRole::RdpSystem, FAKE).unwrap();
    kc.0.set(p, SecretRole::RdpSystem, FAKE2).unwrap();
    assert_eq!(kc.0.get(p, SecretRole::RdpSystem).unwrap().as_deref().map(String::as_str), Some(FAKE2));
}

#[test]
fn roles_and_profiles_are_separate_items() {
    let mut kc = TestKeychain::new();
    let (a, b) = (kc.profile(), kc.profile());
    kc.0.set(a, SecretRole::RdpSystem, "a-system").unwrap();
    kc.0.set(a, SecretRole::LinuxLogin, "a-linux").unwrap();
    kc.0.set(b, SecretRole::RdpSystem, "b-system").unwrap();
    assert_eq!(kc.0.get(a, SecretRole::RdpSystem).unwrap().as_deref().map(String::as_str), Some("a-system"));
    assert_eq!(kc.0.get(a, SecretRole::LinuxLogin).unwrap().as_deref().map(String::as_str), Some("a-linux"));
    assert_eq!(kc.0.get(b, SecretRole::RdpSystem).unwrap().as_deref().map(String::as_str), Some("b-system"));
    assert_eq!(kc.0.get(b, SecretRole::LinuxLogin).unwrap(), None);
}

#[test]
fn delete_removes_only_that_role_and_is_idempotent() {
    let mut kc = TestKeychain::new();
    let p = kc.profile();
    kc.0.set(p, SecretRole::RdpSystem, FAKE).unwrap();
    kc.0.set(p, SecretRole::LinuxLogin, FAKE2).unwrap();
    kc.0.delete(p, SecretRole::LinuxLogin).unwrap();
    kc.0.delete(p, SecretRole::LinuxLogin).unwrap();
    assert_eq!(kc.0.get(p, SecretRole::LinuxLogin).unwrap(), None);
    assert!(kc.0.get(p, SecretRole::RdpSystem).unwrap().is_some());
}

#[test]
fn delete_profile_removes_every_role() {
    let mut kc = TestKeychain::new();
    let p = kc.profile();
    for role in [SecretRole::RdpSystem, SecretRole::RdpUser, SecretRole::LinuxLogin] {
        kc.0.set(p, role, FAKE).unwrap();
    }
    kc.0.delete_profile(p).unwrap();
    for role in [SecretRole::RdpSystem, SecretRole::RdpUser, SecretRole::LinuxLogin] {
        assert_eq!(kc.0.get(p, role).unwrap(), None, "{role:?}");
    }
}

#[test]
fn services_are_isolated() {
    let mut a = TestKeychain::new();
    let b = TestKeychain::new();
    let p = a.profile();
    a.0.set(p, SecretRole::RdpUser, FAKE).unwrap();
    assert_eq!(b.0.get(p, SecretRole::RdpUser).unwrap(), None);
}
