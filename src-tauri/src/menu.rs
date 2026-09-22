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
    /// Session ▸ Show Statistics: the fps HUD over the live picture (M1, M9-1).
    ToggleStats,
    /// Drift ▸ Quit Drift (Cmd+Q): graceful shutdown of every session (2 s cap), then exit.
    Quit,
    /// Debug ▸ Record Session (experimental) — only built with the `recording` feature.
    ToggleRecording,
}

const SELECT_TAB_PREFIX: &str = "drift.select-tab.";

impl MenuAction {
    /// The menu item id.
    pub fn id(self) -> String {
        match self {
            Self::NewTab => "drift.new-tab".to_owned(),
            Self::CloseTab => "drift.close-tab".to_owned(),
            Self::SelectTab(n) => format!("{SELECT_TAB_PREFIX}{n}"),
            Self::PreviousTab => "drift.previous-tab".to_owned(),
            Self::NextTab => "drift.next-tab".to_owned(),
            Self::SendCtrlAltDel => "drift.send-ctrl-alt-del".to_owned(),
            Self::Reconnect => "drift.reconnect".to_owned(),
            Self::Disconnect => "drift.disconnect".to_owned(),
            Self::ToggleStats => "drift.toggle-stats".to_owned(),
            Self::Quit => "drift.quit".to_owned(),
            Self::ToggleRecording => "drift.toggle-recording".to_owned(),
        }
    }

    /// Parses a menu item id.
    pub fn from_id(id: &str) -> Option<Self> {
        if let Some(n) = id.strip_prefix(SELECT_TAB_PREFIX) {
            return match n.parse::<u8>() {
                Ok(n @ 1..=9) => Some(Self::SelectTab(n)),
                _ => None,
            };
        }
        match id {
            "drift.new-tab" => Some(Self::NewTab),
            "drift.close-tab" => Some(Self::CloseTab),
            "drift.previous-tab" => Some(Self::PreviousTab),
            "drift.next-tab" => Some(Self::NextTab),
            "drift.send-ctrl-alt-del" => Some(Self::SendCtrlAltDel),
            "drift.reconnect" => Some(Self::Reconnect),
            "drift.disconnect" => Some(Self::Disconnect),
            "drift.toggle-stats" => Some(Self::ToggleStats),
            "drift.quit" => Some(Self::Quit),
            "drift.toggle-recording" => Some(Self::ToggleRecording),
            _ => None,
        }
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

impl MenuEntry {
    fn action(action: MenuAction, title: &str, shortcut: Option<MenuShortcut>) -> Self {
        Self::Action { action, title: title.to_owned(), shortcut }
    }
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

fn submenu(title: &str, entries: Vec<MenuEntry>) -> SubmenuSpec {
    SubmenuSpec { title: title.to_owned(), entries, is_window_menu: false }
}

/// The menu bar: Drift, File, Edit, Session, Window (+ Debug with feature `recording`).
pub fn menu_spec() -> Vec<SubmenuSpec> {
    use MenuAction as A;
    use MenuEntry as E;
    use Standard as S;

    let drift = submenu(
        "Drift",
        vec![
            E::Standard(S::About),
            E::Separator,
            E::Standard(S::Services),
            E::Separator,
            E::Standard(S::Hide),
            E::Standard(S::HideOthers),
            E::Standard(S::ShowAll),
            E::Separator,
            E::action(A::Quit, "Quit Drift", Some(MenuShortcut::Quit)),
        ],
    );
    let file = submenu(
        "File",
        vec![
            E::action(A::NewTab, "New Tab", Some(MenuShortcut::NewTab)),
            E::action(A::CloseTab, "Close Tab", Some(MenuShortcut::CloseTab)),
        ],
    );
    let edit = submenu(
        "Edit",
        vec![
            E::Standard(S::Undo),
            E::Standard(S::Redo),
            E::Separator,
            E::Standard(S::Cut),
            E::Standard(S::Copy),
            E::Standard(S::Paste),
            E::Standard(S::SelectAll),
        ],
    );
    let session = submenu(
        "Session",
        vec![
            E::action(A::SendCtrlAltDel, "Send Ctrl+Alt+Del", None),
            E::action(A::Reconnect, "Reconnect", None),
            E::action(A::Disconnect, "Disconnect", None),
            E::action(A::ToggleStats, "Show Statistics", None),
        ],
    );
    let mut window_entries = vec![
        MenuEntry::Standard(S::Minimize),
        MenuEntry::Standard(S::Zoom),
        MenuEntry::Standard(S::Fullscreen),
        MenuEntry::Separator,
        MenuEntry::action(A::PreviousTab, "Show Previous Tab", Some(MenuShortcut::PreviousTab)),
        MenuEntry::action(A::NextTab, "Show Next Tab", Some(MenuShortcut::NextTab)),
        MenuEntry::Separator,
    ];
    window_entries.extend((1..=9u8).map(|n| {
        MenuEntry::action(A::SelectTab(n), &format!("Show Tab {n}"), Some(MenuShortcut::SelectTab(n)))
    }));
    let window = SubmenuSpec { title: "Window".to_owned(), entries: window_entries, is_window_menu: true };

    let mut menus = vec![drift, file, edit, session, window];
    if cfg!(feature = "recording") {
        menus.push(submenu(
            "Debug",
            vec![E::action(A::ToggleRecording, "Record Session (experimental)", None)],
        ));
    }
    menus
}

/// Tauri accelerator string for an allow-listed shortcut.
pub fn accelerator(shortcut: MenuShortcut) -> String {
    match shortcut {
        MenuShortcut::NewTab => "CmdOrCtrl+T".to_owned(),
        MenuShortcut::CloseTab => "CmdOrCtrl+W".to_owned(),
        MenuShortcut::Quit => "CmdOrCtrl+Q".to_owned(),
        MenuShortcut::SelectTab(n) => format!("CmdOrCtrl+{n}"),
        MenuShortcut::PreviousTab => "CmdOrCtrl+Shift+BracketLeft".to_owned(),
        MenuShortcut::NextTab => "CmdOrCtrl+Shift+BracketRight".to_owned(),
        MenuShortcut::CycleWindows => "CmdOrCtrl+Backquote".to_owned(),
    }
}
