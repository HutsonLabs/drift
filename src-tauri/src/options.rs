//! Launch options (environment) for the app.

use std::path::PathBuf;

/// How the app is launched.
#[derive(Default)]
pub struct RunOptions {
    /// Directory for `profiles.toml` (default: the Tauri app config directory).
    /// Environment: `DRIFT_CONFIG_DIR`.
    pub config_dir: Option<PathBuf>,
    /// Keep passwords in memory instead of the Keychain (tests only; not settable from the
    /// environment).
    pub memory_secrets: bool,
    /// Connect the saved profile with this name in the first tab at launch (dev convenience
    /// used by the smoke test). Environment: `DRIFT_AUTOCONNECT`.
    pub autoconnect: Option<String>,
    /// Called once on the main thread after the first tab has been requested (tests).
    pub on_ready: Option<Box<dyn FnOnce(tauri::AppHandle) + Send>>,
}

impl std::fmt::Debug for RunOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunOptions")
            .field("config_dir", &self.config_dir)
            .field("memory_secrets", &self.memory_secrets)
            .field("autoconnect", &self.autoconnect)
            .field("on_ready", &self.on_ready.is_some())
            .finish()
    }
}

impl RunOptions {
    /// Options from `(name, value)` pairs (blank values are ignored).
    ///
    /// Only `DRIFT_CONFIG_DIR` and `DRIFT_AUTOCONNECT` are read; the Keychain can never be
    /// switched off from the environment.
    pub fn from_vars(vars: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut options = Self::default();
        for (name, value) in vars {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            match name.as_str() {
                "DRIFT_CONFIG_DIR" => options.config_dir = Some(PathBuf::from(value)),
                "DRIFT_AUTOCONNECT" => options.autoconnect = Some(value.to_owned()),
                _ => {}
            }
        }
        options
    }

    /// Options from the process environment.
    pub fn from_env() -> Self {
        Self::from_vars(std::env::vars())
    }
}
