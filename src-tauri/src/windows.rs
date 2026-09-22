//! Session windows and native tabs (tasks **M6-2**, **M1-6**; humble object over Tauri/AppKit).

use tauri::{AppHandle, Runtime};

/// Tauri label of the `n`th session window.
pub fn window_label(n: u64) -> String {
    let _ = n;
    todo!("M6-2")
}

/// Opens a new session window as a tab of the current group (returns immediately; the window
/// is created on another thread and finished on the main thread).
pub fn open_tab<R: Runtime>(app: &AppHandle<R>) {
    let _ = app;
    todo!("M6-2")
}

/// Number of tabs in the group of window `label` (main thread only).
pub fn tab_count<R: Runtime>(app: &AppHandle<R>, label: &str) -> Option<usize> {
    let _ = (app, label);
    todo!("M6-2")
}
