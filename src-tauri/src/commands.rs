//! IPC commands exposed to the webview.

use serde::Serialize;

/// Static information about the running app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Product name.
    pub name: String,
    /// Crate version.
    pub version: String,
}

/// Returns the app name and version.
#[tauri::command]
#[specta::specta]
pub fn app_info() -> AppInfo {
    AppInfo { name: "Drift".into(), version: env!("CARGO_PKG_VERSION").into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_reports_version() {
        let info = app_info();
        assert_eq!(info.name, "Drift");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    }
}
