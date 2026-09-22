//! Connection profiles and per-profile preferences (plan §3).
//!
//! Profiles are persisted as TOML by the app. **Secrets are never part of a profile**:
//! passwords live in the Keychain keyed by `profile_id` + role (plan §2 decision 7).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// Default RDP port used by every mode unless the profile overrides it.
pub const DEFAULT_RDP_PORT: u16 = 3389;

/// How Drift reaches the GNOME session (plan §1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum ConnectMode {
    /// System daemon: NLA leg, then RDSTLS legs through the GDM greeter.
    RemoteLogin,
    /// Persistent per-user headless session with fixed RDP credentials.
    Headless,
    /// The user's Desktop Sharing daemon mirroring an existing session (no DISP channel).
    DesktopSharing,
}

impl ConnectMode {
    /// Every mode, in UI order.
    pub const ALL: [Self; 3] = [Self::RemoteLogin, Self::Headless, Self::DesktopSharing];

    /// Default TCP port for the mode.
    pub const fn default_port(self) -> u16 {
        DEFAULT_RDP_PORT
    }

    /// Whether the server offers the Display Control channel (remote resize) in this mode.
    pub const fn supports_display_control(self) -> bool {
        !matches!(self, Self::DesktopSharing)
    }
}

/// What the Mac Command key is sent as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum CmdAs {
    /// Left/right Windows ("Super") key, scancode `0x5B`/`0x5C` extended. Default.
    #[default]
    Super,
    /// Control, scancode `0x1D`.
    Ctrl,
}

/// Keyboard preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct KeyboardPrefs {
    /// Mapping of the Command key.
    pub cmd_as: CmdAs,
    /// "Type using Mac layout": printable keys without Ctrl/Cmd go out as Unicode events.
    pub type_with_mac_layout: bool,
}

/// Display preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct DisplayPrefs {
    /// Resize the remote desktop to follow the window (Display Control).
    pub adaptive: bool,
    /// Use physical pixels with `DesktopScaleFactor=200` on Retina displays.
    pub retina: bool,
}

impl Default for DisplayPrefs {
    fn default() -> Self {
        Self { adaptive: true, retina: true }
    }
}

/// Clipboard synchronisation level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum ClipboardPrefs {
    /// No clipboard channel traffic.
    Off,
    /// Text only.
    Text,
    /// Text and images (PNG/TIFF/DIB). Default.
    #[default]
    TextAndImages,
}

/// SHA-256 of a server's leaf certificate DER, used for TOFU pinning.
///
/// `Display` and `FromStr` use the `grdctl status` format: 32 lowercase hex byte pairs
/// separated by `:` (for example `f3:e7:a2:…`), so users can compare the two directly.
/// Serialized (serde) as that same string.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "specta", derive(specta::Type), specta(transparent))]
pub struct CertFingerprint(#[cfg_attr(feature = "specta", specta(type = String))] pub [u8; 32]);

impl CertFingerprint {
    /// Wraps a raw SHA-256 digest.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The raw digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for CertFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, b) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(":")?;
            }
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for CertFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CertFingerprint({self})")
    }
}

impl FromStr for CertFingerprint {
    type Err = ProfileError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut out = [0u8; 32];
        let mut parts = s.trim().split(':');
        for byte in &mut out {
            let part = parts.next().ok_or(ProfileError::InvalidFingerprint)?;
            if part.len() != 2 || !part.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err(ProfileError::InvalidFingerprint);
            }
            *byte = u8::from_str_radix(part, 16).map_err(|_| ProfileError::InvalidFingerprint)?;
        }
        if parts.next().is_some() {
            return Err(ProfileError::InvalidFingerprint);
        }
        Ok(Self(out))
    }
}

impl Serialize for CertFingerprint {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for CertFingerprint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// A saved connection (plan §3). Contains no secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ConnectionProfile {
    /// Stable identifier; also the Keychain account key prefix.
    pub id: Uuid,
    /// Display name (tab title).
    pub name: String,
    /// Host name or IP address.
    pub host: String,
    /// TCP port (RemoteLogin default 3389).
    pub port: u16,
    /// Connection mode.
    pub mode: ConnectMode,
    /// RDP user: system credentials (RemoteLogin) or daemon credentials (Headless/Sharing).
    pub rdp_username: String,
    /// RemoteLogin only: the Linux user, shown in the greeter hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux_username: Option<String>,
    /// TOFU-pinned certificate fingerprint of the first leg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cert_pin: Option<CertFingerprint>,
    /// Keyboard preferences.
    #[serde(default)]
    pub keyboard: KeyboardPrefs,
    /// Display preferences.
    #[serde(default)]
    pub display: DisplayPrefs,
    /// Clipboard preferences.
    #[serde(default)]
    pub clipboard: ClipboardPrefs,
}

impl ConnectionProfile {
    /// Creates a profile with a fresh id, the mode's default port and default preferences.
    pub fn new(name: impl Into<String>, host: impl Into<String>, mode: ConnectMode) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            host: host.into(),
            port: mode.default_port(),
            mode,
            rdp_username: String::new(),
            linux_username: None,
            cert_pin: None,
            keyboard: KeyboardPrefs::default(),
            display: DisplayPrefs::default(),
            clipboard: ClipboardPrefs::default(),
        }
    }

    /// Serializes the profile as a TOML document.
    pub fn to_toml(&self) -> Result<String, ProfileError> {
        toml::to_string(self).map_err(|e| ProfileError::Toml(e.to_string()))
    }

    /// Parses a profile from a TOML document.
    pub fn from_toml(s: &str) -> Result<Self, ProfileError> {
        toml::from_str(s).map_err(|e| ProfileError::Toml(e.to_string()))
    }
}

/// Errors from profile (de)serialization and parsing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileError {
    /// The string is not a `grdctl`-style SHA-256 fingerprint.
    #[error("invalid certificate fingerprint: expected 32 colon-separated hex bytes")]
    InvalidFingerprint,
    /// TOML (de)serialization failed.
    #[error("profile TOML error: {0}")]
    Toml(String),
}
