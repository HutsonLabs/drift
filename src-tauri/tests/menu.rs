//! M6-2 / M2-4 / M8-3 Red: the menu bar model.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test fixtures

use drift_app::menu::{MenuAction, MenuEntry, SubmenuSpec, accelerator, menu_spec};
use drift_input::MenuShortcut;

fn actions(spec: &[SubmenuSpec]) -> Vec<(String, MenuAction, String, Option<MenuShortcut>)> {
    spec.iter()
        .flat_map(|m| {
            m.entries.iter().filter_map(|e| match e {
                MenuEntry::Action { action, title, shortcut } => {
                    Some((m.title.clone(), *action, title.clone(), *shortcut))
                }
                _ => None,
            })
        })
        .collect()
}

fn find(spec: &[SubmenuSpec], action: MenuAction) -> (String, String, Option<MenuShortcut>) {
    actions(spec)
        .into_iter()
        .find(|(_, a, _, _)| *a == action)
        .map(|(menu, _, title, shortcut)| (menu, title, shortcut))
        .unwrap_or_else(|| panic!("{action:?} is not in the menu"))
}

#[test]
fn action_ids_round_trip() {
    let mut all = vec![
        MenuAction::NewTab,
        MenuAction::CloseTab,
        MenuAction::PreviousTab,
        MenuAction::NextTab,
        MenuAction::SendCtrlAltDel,
        MenuAction::Reconnect,
        MenuAction::Disconnect,
        MenuAction::ToggleStats,
        MenuAction::Quit,
        MenuAction::ToggleRecording,
    ];
    all.extend((1..=9).map(MenuAction::SelectTab));
    let mut ids = std::collections::HashSet::new();
    for a in all {
        assert!(ids.insert(a.id()), "duplicate id {}", a.id());
        assert_eq!(MenuAction::from_id(&a.id()), Some(a), "{a:?}");
    }
    for bad in ["", "nope", "drift.select-tab.0", "drift.select-tab.10", "drift.select-tab.x", "new-tab"] {
        assert_eq!(MenuAction::from_id(bad), None, "{bad}");
    }
}

#[test]
fn tab_shortcuts() {
    let spec = menu_spec();
    let (menu, title, shortcut) = find(&spec, MenuAction::NewTab);
    assert_eq!((menu.as_str(), title.as_str(), shortcut), ("File", "New Tab", Some(MenuShortcut::NewTab)));
    let (menu, title, shortcut) = find(&spec, MenuAction::CloseTab);
    assert_eq!(
        (menu.as_str(), title.as_str(), shortcut),
        ("File", "Close Tab", Some(MenuShortcut::CloseTab))
    );
    for n in 1..=9u8 {
        let (menu, title, shortcut) = find(&spec, MenuAction::SelectTab(n));
        assert_eq!(menu, "Window");
        assert_eq!(title, format!("Show Tab {n}"));
        assert_eq!(shortcut, Some(MenuShortcut::SelectTab(n)));
    }
    assert_eq!(find(&spec, MenuAction::PreviousTab).2, Some(MenuShortcut::PreviousTab));
    assert_eq!(find(&spec, MenuAction::NextTab).2, Some(MenuShortcut::NextTab));
    let (menu, _, shortcut) = find(&spec, MenuAction::Quit);
    assert_eq!((menu.as_str(), shortcut), ("Drift", Some(MenuShortcut::Quit)));
    assert!(spec.iter().filter(|m| m.is_window_menu).map(|m| m.title.as_str()).eq(["Window"]));
}

#[test]
fn session_menu_has_no_shortcuts() {
    let spec = menu_spec();
    let session: Vec<_> =
        actions(&spec).into_iter().filter(|(m, ..)| m == "Session").map(|(_, a, t, s)| (a, t, s)).collect();
    assert_eq!(
        session,
        [
            (MenuAction::SendCtrlAltDel, "Send Ctrl+Alt+Del".to_owned(), None),
            (MenuAction::Reconnect, "Reconnect".to_owned(), None),
            (MenuAction::Disconnect, "Disconnect".to_owned(), None),
            // M1 "Done (manual M1)": the fps HUD over the live picture, off by default.
            (MenuAction::ToggleStats, "Show Statistics".to_owned(), None),
        ]
    );
}

#[test]
fn menu_order() {
    let titles: Vec<_> = menu_spec().into_iter().map(|m| m.title).collect();
    if cfg!(feature = "recording") {
        assert_eq!(titles, ["Drift", "File", "Edit", "Session", "Window", "Debug"]);
    } else {
        assert_eq!(titles, ["Drift", "File", "Edit", "Session", "Window"]);
    }
}

#[test]
fn record_session_is_hidden_without_the_recording_feature() {
    let spec = menu_spec();
    let recording = actions(&spec).into_iter().find(|(_, a, ..)| *a == MenuAction::ToggleRecording);
    if cfg!(feature = "recording") {
        let (menu, _, title, shortcut) = recording.expect("Debug ▸ Record Session");
        assert_eq!(
            (menu.as_str(), title.as_str(), shortcut),
            ("Debug", "Record Session (experimental)", None)
        );
    } else {
        assert!(recording.is_none());
    }
}

#[test]
fn accelerators() {
    #[rustfmt::skip]
    let table = [
        (MenuShortcut::NewTab, "CmdOrCtrl+T"),
        (MenuShortcut::CloseTab, "CmdOrCtrl+W"),
        (MenuShortcut::Quit, "CmdOrCtrl+Q"),
        (MenuShortcut::SelectTab(1), "CmdOrCtrl+1"),
        (MenuShortcut::SelectTab(9), "CmdOrCtrl+9"),
        (MenuShortcut::PreviousTab, "CmdOrCtrl+Shift+BracketLeft"),
        (MenuShortcut::NextTab, "CmdOrCtrl+Shift+BracketRight"),
        (MenuShortcut::CycleWindows, "CmdOrCtrl+Backquote"),
    ];
    for (s, want) in table {
        assert_eq!(accelerator(s), want, "{s:?}");
        // Tauri hands the string to muda, which parses it at runtime.
        assert!(want.parse::<muda::accelerator::Accelerator>().is_ok(), "{want} parses");
    }
}
