//! Native window tabs (task **M6-2**, platform part; plan §1.8).
//!
//! Each session is its own `NSWindow`. The tabbing identifier alone is not enough (the system
//! preference "Prefer tabs when opening documents" defaults to *in full screen only*), so every
//! session window gets `tabbingMode = Preferred` and is joined explicitly with
//! `addTabbedWindow:ordered:`. The "+" button in the tab bar sends `newWindowForTab:` up the
//! responder chain; tao's window class does not implement it, so
//! [`install_new_window_for_tab`] adds it to the **real** window class (`TaoWindow`), not to the
//! `NSKVONotifying_` subclass that key-value observing installs.

use drift_core::SessionState;
use objc2_app_kit::NSWindow;

/// The tabbing identifier shared by all session windows.
pub const TABBING_IDENTIFIER: &str = "drift.sessions";

/// The window title (= tab title): a state glyph and the profile name.
///
/// | State | Glyph |
/// |---|---|
/// | `Connected` | `●` |
/// | `AwaitingGreeterLogin` | `◐` |
/// | `Connecting` | `◌` |
/// | `Reconnecting` | `↻` |
/// | `Idle`, `Disconnected` | `○` |
/// | `Failed` | `⚠` |
pub fn tab_title(profile_name: &str, state: &SessionState) -> String {
    let _ = (profile_name, state);
    String::new()
}

/// Makes `window` a tab-group member candidate: sets [`TABBING_IDENTIFIER`] and
/// `NSWindowTabbingMode::Preferred`.
pub fn prepare_for_tabs(window: &NSWindow) {
    let _ = window;
}

/// Adds `new` as a tab to `group`'s tab group (after the selected tab) and selects it.
pub fn add_tab(group: &NSWindow, new: &NSWindow) {
    let _ = (group, new);
}

/// Number of tabs in `window`'s group (1 for a window without tabs).
pub fn tab_count(window: &NSWindow) -> usize {
    let _ = window;
    0
}

/// Selects the tab at `index` (0-based) in `window`'s group; `false` if out of range.
pub fn select_tab(window: &NSWindow, index: usize) -> bool {
    let _ = (window, index);
    false
}

/// `newWindowForTab:` could not be installed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TabError {
    /// A different handler was already installed for this process.
    #[error("newWindowForTab: handler already installed")]
    AlreadyInstalled,
    /// The class already implements `newWindowForTab:` (and it is not ours).
    #[error("{0} already implements newWindowForTab:")]
    ClassHasMethod(String),
}

/// Adds `newWindowForTab:` to `window`'s real class, calling `handler` (on the main thread)
/// when the tab bar's "+" is clicked. Idempotent for the same class.
pub fn install_new_window_for_tab(
    window: &NSWindow,
    handler: impl Fn() + Send + Sync + 'static,
) -> Result<(), TabError> {
    let _ = (window, handler);
    Err(TabError::AlreadyInstalled)
}
