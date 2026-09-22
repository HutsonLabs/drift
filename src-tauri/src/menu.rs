//! The native menu bar (tasks **M6-2**, **M2-4**, **M8-3**).
//!
//! [`menu_spec`] describes the menus as plain data (tested); `crate::windows` turns it into a
//! Tauri menu and routes clicks by [`MenuAction::id`]. Every keyboard shortcut is one of
//! `drift-input`'s allow-listed [`MenuShortcut`]s, so the RemoteView lets it through to the menu
//! instead of sending it to the remote (M2-4). Session actions have no shortcut: every other
//! Command combo belongs to the remote desktop.

use drift_input::MenuShortcut;

/// A Drift menu command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuAction {
    /// File ▸ New Tab (Cmd+T; also the tab bar's "+").
    NewTab,
    /// File ▸ Close Tab (Cmd+W): graceful session close, then the tab closes.
    CloseTab,
    /// Window ▸ Show Tab n (Cmd+1 … Cmd+9, 1-based).
    SelectTab(u8),
    /// Window ▸ Show Previous Tab (Cmd+Shift+[).
    PreviousTab,
    /// Window ▸ Show Next Tab (Cmd+Shift+]).
    NextTab,
    /// Session ▸ Send Ctrl+Alt+Del.
    SendCtrlAltDel,
    /// Session ▸ Reconnect.
    Reconnect,
    /// Session ▸ Disconnect (back to the connect form).
    Disconnect,
    /// Drift ▸ Quit Drift (Cmd+Q): graceful shutdown of every session (2 s cap), then exit.
    Quit,
    /// Debug ▸ Record Session (experimental) — only built with the `recording` feature.
    ToggleRecording,
}

impl MenuAction {
    /// The menu item id.
    pub fn id(self) -> String {
        todo!("M6-2")
    }

    /// Parses a menu item id.
    pub fn from_id(id: &str) -> Option<Self> {
        let _ = id;
        todo!("M6-2")
    }
}

/// Standard AppKit items (Tauri predefined menu items).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standard {
    /// About Drift.
    About,
    /// Services submenu.
    Services,
    /// Hide Drift.
    Hide,
    /// Hide Others.
    HideOthers,
    /// Show All.
    ShowAll,
    /// Undo (webview form).
    Undo,
    /// Redo.
    Redo,
    /// Cut.
    Cut,
    /// Copy.
    Copy,
    /// Paste.
    Paste,
    /// Select All.
    SelectAll,
    /// Minimize.
    Minimize,
    /// Zoom.
    Zoom,
    /// Enter/Exit Full Screen.
    Fullscreen,
}

/// One entry of a submenu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    /// A Drift command.
    Action {
        /// What it does.
        action: MenuAction,
        /// Title.
        title: String,
        /// Keyboard shortcut (always allow-listed).
        shortcut: Option<MenuShortcut>,
    },
    /// A standard item.
    Standard(Standard),
    /// A separator.
    Separator,
}

/// One top-level menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmenuSpec {
    /// Title in the menu bar.
    pub title: String,
    /// Items.
    pub entries: Vec<MenuEntry>,
    /// This is the Window menu (AppKit adds its window/tab items to it).
    pub is_window_menu: bool,
}

/// The menu bar: Drift, File, Edit, Session, Window (+ Debug with feature `recording`).
pub fn menu_spec() -> Vec<SubmenuSpec> {
    todo!("M6-2")
}

/// Tauri accelerator string for an allow-listed shortcut.
pub fn accelerator(shortcut: MenuShortcut) -> String {
    let _ = shortcut;
    todo!("M6-2")
}
