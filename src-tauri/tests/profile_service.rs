//! M1-6 / M3-2 Red: profile store on disk and the credential rules per mode.
#![allow(clippy::unwrap_used)] // fixture helpers outside #[test] fns

use std::sync::Arc;

use drift_app::profiles::{CommandError, LinuxPasswordUpdate, ProfileFile, ProfileService, SecretsUpdate};
use drift_app::secrets::{MemorySecretStore, SecretStore};
use drift_core::{CertFingerprint, ConnectMode, ConnectionProfile, ProfileField, SecretRole};

struct Fixture {
    _dir: tempfile::TempDir,
    file: ProfileFile,
    secrets: Arc<MemorySecretStore>,
    service: ProfileService,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let file = ProfileFile::in_dir(&dir.path().join("config"));
    let secrets = Arc::new(MemorySecretStore::new());
    let service = ProfileService::open(file.clone(), secrets.clone()).unwrap();
    Fixture { _dir: dir, file, secrets, service }
}

fn profile(mode: ConnectMode) -> ConnectionProfile {
    let mut p = ConnectionProfile::new("Homelab", "gnome.local", mode);
    p.rdp_username = "rdp-user".into();
    p
}

fn update(rdp: Option<&str>, linux: LinuxPasswordUpdate) -> SecretsUpdate {
    SecretsUpdate { rdp_password: rdp.map(Into::into), linux_password: linux }
}

fn secret(f: &Fixture, p: &ConnectionProfile, role: SecretRole) -> Option<String> {
    f.secrets.get(p.id, role).unwrap().map(|s| s.to_string())
}

#[test]
fn missing_file_is_empty_and_save_persists_to_disk() {
    let f = fixture();
    assert!(f.service.list().unwrap().is_empty());
    let p = profile(ConnectMode::Headless);
    let entry = f.service.save(p.clone(), update(Some("pw-Fake1"), LinuxPasswordUpdate::Keep)).unwrap();
    assert_eq!(entry.profile, p);
    assert!(entry.has_rdp_password);
    assert!(!entry.has_linux_password);

    let text = std::fs::read_to_string(f.file.path()).unwrap();
    assert!(text.contains("name = \"Homelab\""), "{text}");
    assert!(!text.contains("pw-Fake1"), "passwords never go to profiles.toml");

    let reopened = ProfileService::open(f.file.clone(), f.secrets.clone()).unwrap();
    assert_eq!(reopened.list().unwrap()[0].profile, p);
}

#[test]
fn invalid_profile_is_rejected_with_issues_and_not_written() {
    let f = fixture();
    let mut p = profile(ConnectMode::Headless);
    p.host = "gnome.local:3389".into();
    match f.service.save(p, update(Some("x"), LinuxPasswordUpdate::Keep)) {
        Err(CommandError::Invalid { issues }) => assert_eq!(issues[0].field, ProfileField::Host),
        other => panic!("{other:?}"),
    }
    assert!(!f.file.path().exists());
    assert!(f.service.list().unwrap().is_empty());
}

#[test]
fn remote_login_stores_system_rdp_credentials_and_opt_in_linux_password() {
    let f = fixture();
    let mut p = profile(ConnectMode::RemoteLogin);
    p.linux_username = Some("drifttest".into());

    // Without opt-in: only the system RDP password.
    let e = f.service.save(p.clone(), update(Some("sys-Fake1"), LinuxPasswordUpdate::Keep)).unwrap();
    assert!(e.has_rdp_password && !e.has_linux_password);
    assert_eq!(secret(&f, &p, SecretRole::RdpSystem).as_deref(), Some("sys-Fake1"));
    assert_eq!(secret(&f, &p, SecretRole::RdpUser), None);
    assert_eq!(secret(&f, &p, SecretRole::LinuxLogin), None);

    // Opt in.
    let e = f.service.save(p.clone(), update(None, LinuxPasswordUpdate::Store("lx-Fake1".into()))).unwrap();
    assert!(e.has_rdp_password && e.has_linux_password);
    assert_eq!(
        secret(&f, &p, SecretRole::RdpSystem).as_deref(),
        Some("sys-Fake1"),
        "None keeps the RDP password"
    );
    assert_eq!(secret(&f, &p, SecretRole::LinuxLogin).as_deref(), Some("lx-Fake1"));

    // Empty strings mean "unchanged".
    let e = f.service.save(p.clone(), update(Some(""), LinuxPasswordUpdate::Store(String::new()))).unwrap();
    assert!(e.has_rdp_password && e.has_linux_password);

    // Opt out.
    let e = f.service.save(p.clone(), update(None, LinuxPasswordUpdate::Forget)).unwrap();
    assert!(!e.has_linux_password);
    assert_eq!(secret(&f, &p, SecretRole::LinuxLogin), None);
}

#[test]
fn headless_and_sharing_store_one_rdp_credential_and_never_a_linux_password() {
    for mode in [ConnectMode::Headless, ConnectMode::DesktopSharing] {
        let f = fixture();
        let p = profile(mode);
        let e = f
            .service
            .save(p.clone(), update(Some("d-Fake1"), LinuxPasswordUpdate::Store("lx".into())))
            .unwrap();
        assert!(e.has_rdp_password && !e.has_linux_password, "{mode:?}");
        assert_eq!(secret(&f, &p, SecretRole::RdpUser).as_deref(), Some("d-Fake1"));
        assert_eq!(secret(&f, &p, SecretRole::RdpSystem), None);
        assert_eq!(secret(&f, &p, SecretRole::LinuxLogin), None, "{mode:?}");
    }
}

#[test]
fn switching_mode_moves_the_rdp_password_and_drops_the_linux_password() {
    let f = fixture();
    let mut p = profile(ConnectMode::RemoteLogin);
    f.service
        .save(p.clone(), update(Some("sys-Fake1"), LinuxPasswordUpdate::Store("lx-Fake1".into())))
        .unwrap();

    p.mode = ConnectMode::Headless;
    let e = f.service.save(p.clone(), update(None, LinuxPasswordUpdate::Keep)).unwrap();
    assert!(e.has_rdp_password && !e.has_linux_password);
    assert_eq!(secret(&f, &p, SecretRole::RdpUser).as_deref(), Some("sys-Fake1"));
    assert_eq!(secret(&f, &p, SecretRole::RdpSystem), None);
    assert_eq!(secret(&f, &p, SecretRole::LinuxLogin), None);

    // Switching with a new password stores the new one under the new role.
    p.mode = ConnectMode::RemoteLogin;
    f.service.save(p.clone(), update(Some("new-Fake2"), LinuxPasswordUpdate::Keep)).unwrap();
    assert_eq!(secret(&f, &p, SecretRole::RdpSystem).as_deref(), Some("new-Fake2"));
    assert_eq!(secret(&f, &p, SecretRole::RdpUser), None);
}

#[test]
fn delete_removes_profile_and_all_passwords() {
    let f = fixture();
    let p = profile(ConnectMode::RemoteLogin);
    f.service.save(p.clone(), update(Some("a"), LinuxPasswordUpdate::Store("b".into()))).unwrap();
    f.service.delete(p.id).unwrap();
    assert!(f.service.list().unwrap().is_empty());
    for role in [SecretRole::RdpSystem, SecretRole::RdpUser, SecretRole::LinuxLogin] {
        assert_eq!(secret(&f, &p, role), None);
    }
    assert_eq!(f.service.delete(p.id), Err(CommandError::NotFound));
    let reopened = ProfileService::open(f.file.clone(), f.secrets.clone()).unwrap();
    assert!(reopened.list().unwrap().is_empty());
}

#[test]
fn pins_are_persisted_and_cleared() {
    let f = fixture();
    let p = profile(ConnectMode::Headless);
    f.service.save(p.clone(), update(Some("a"), LinuxPasswordUpdate::Keep)).unwrap();
    let fp = CertFingerprint::from_bytes([0xab; 32]);
    f.service.set_pin(p.id, Some(fp)).unwrap();
    let reopened = ProfileService::open(f.file.clone(), f.secrets.clone()).unwrap();
    assert_eq!(reopened.get(p.id).unwrap().profile.cert_pin, Some(fp));
    f.service.set_pin(p.id, None).unwrap();
    assert_eq!(f.service.get(p.id).unwrap().profile.cert_pin, None);
    assert_eq!(f.service.set_pin(uuid::Uuid::nil(), None), Err(CommandError::NotFound));
}

#[test]
fn corrupt_file_is_a_storage_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("profiles.toml"), "version = [").unwrap();
    let r = ProfileService::open(ProfileFile::in_dir(dir.path()), Arc::new(MemorySecretStore::new()));
    assert!(matches!(r, Err(CommandError::Storage { .. })), "{r:?}");
}

#[test]
fn secrets_never_appear_in_debug_output() {
    let u = update(Some("hunter2-Fake9"), LinuxPasswordUpdate::Store("other-Fake9".into()));
    let d = format!("{u:?}");
    assert!(!d.contains("hunter2") && !d.contains("other-Fake9"), "{d}");
    let s = MemorySecretStore::new();
    s.set(uuid::Uuid::nil(), SecretRole::RdpUser, "hunter2-Fake9").unwrap();
    assert!(!format!("{s:?}").contains("hunter2"));
}
