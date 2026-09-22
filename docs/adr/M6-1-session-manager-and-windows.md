# M6-1 / M6-2 / M1-6 / M3-2 / M8-3 — SessionManager, session windows, tabs and menus

- Status: accepted
- Date: 2026-09-22
- Code: `src-tauri/src/{manager,host,windows,menu,present,options,recording}.rs`,
  `src-tauri/tauri.conf.json`, `ui/src/app.ts`

## Context

Plan M6-1 requires a `SessionManager` mapping sessions to windows with fake-actor tests, M6-2 one
`NSWindow` per session in the native tab group `drift.sessions` plus menus, and M1-6/M3-2 the app
wiring of `drift-macos` (RemoteView, Keychain) and `drift-rdp` (`spawn_session`, ADR
`M0-5-session-interface`). §1.8 fixes the AppKit techniques; this ADR records the app-level
decisions.

## Decisions

1. **Window = tab = at most one live session.** Labels are `session-<n>` (covered by the
   `session-*` capability). `tauri.conf.json` declares **no** static window: every window is
   created hidden by `windows::open_tab`, prepared with `tabbingMode = Preferred` and joined to
   the group with `addTabbedWindow:ordered:` before it is shown, exactly as the spike verified.
   Windows are built off the main thread (`WebviewWindowBuilder::build` dispatches to the event
   loop) and finished on it (`windows::on_main`, which runs inline when already on the main
   thread).
2. **`SessionHost` seam.** The manager never touches Tauri or AppKit: it starts actors and
   applies their output through the `SessionHost` trait (`spawn`, `view_changed`,
   `session_event`, `certificate_pinned`, `session_ended`). `TauriHost` is production; the
   M6-1 tests use a recorder with fake actors (`SessionHandle::from_sender`), so open/close/quit,
   event routing and the "no leaked handles" property are tested without a window server.
3. **Generations, not identity.** Each session gets a generation number. A pump only touches its
   window while its generation is the live one, so a replaced or closed actor can never clobber a
   window's view, and `session_ended` fires **exactly once** per started session (the closer calls
   it when it took the session, the pump when the actor ended by itself).
4. **Graceful close, hard cap.** `close`/`shutdown` forget the window(s) immediately, send
   `SessionCommand::Close`, drop the manager's handle and await the pump until a single deadline
   (`CLOSE_CAP` = 2 s, plan M6-1). Anything still running is abandoned (pump aborted). The window
   close button and Cmd+W go through the same path: `CloseRequested` is prevented once, the
   session closes, then the window is destroyed.
5. **Quit is a Drift menu item, not `terminate:`.** AppKit's predefined Quit calls `terminate:`,
   which tao turns into an immediate exit — no chance to close sessions. Drift ▸ Quit Drift
   (Cmd+Q) is therefore a normal item that shuts every session down (2 s cap) and then
   `app.exit(0)`. `RunEvent::Exit` still runs a blocking shutdown as a last resort (Dock ▸ Quit),
   and `ExitRequested` with no exit code is prevented, so closing the last tab leaves Drift
   running like other Mac apps (`Reopen` opens a new tab).
6. **Greeter hint is the window subtitle.** Plan §2 decision 6 shows the webview whenever there is
   no live picture. The GDM greeter *is* a live picture the user must type into, so hiding it
   behind the webview would break Remote Login. `present::surface_for` keeps the RemoteView in
   front for `Live` **and** `GreeterHint`, and `present::window_subtitle` puts
   "Log in as “drifttest” to start your session" / "Session is still running — log in as … to
   resume" into the title bar. The webview's greeter banner stays for the (rare) case where the
   picture is not up yet.
7. **Titles.** `present::window_title` = `drift_macos::tabs::tab_title` (state glyph + profile
   name); a window showing the connect form is "New Session".
8. **`disconnect` vs `close_session`.** `close_session` ends the session *and* the tab (the error
   screen's "Close"); the new `disconnect` command ends the session and returns the tab to the
   connect form — that is what "Cancel" during connecting and Session ▸ Disconnect do. A new
   `CommandError::NoSession` reports intents sent to a tab whose session has ended.
9. **Reconnect after the actor exited.** `SessionManager::reconnect_now` answers `Reopen(profile)`
   when the window has no live actor; the caller reloads the profile (so a freshly pinned
   certificate is used) and opens it again.
10. **Menus are data.** `menu::menu_spec()` is a pure description (tested); `windows::build_menu`
    turns it into Tauri items and `MenuAction::from_id` routes clicks. Every accelerator is one of
    `drift_input::MenuShortcut`'s allow-listed combos, so `performKeyEquivalent:` lets it reach
    the menu (M2-4); session actions deliberately have **no** shortcut, because every other
    Command combo belongs to the remote desktop. Cmd+1…9 select tabs through
    `NSWindowTabGroup`; Cmd+Shift+[ / ] call `selectPreviousTab:` / `selectNextTab:`.
11. **Debug ▸ Record Session (experimental)** exists only with the `recording` cargo feature
    (M8-3): it starts composite capture on the tab's render thread, encodes with `drift-video`
    and writes `~/Movies/Drift <profile> <timestamp>.mp4` on one dedicated thread.
12. **Per-window AppKit state is main-thread-only** (`thread_local!`): the `RemoteView`, its
    `WindowObserver`, the `SessionLink` (RemoteView → `SessionCommand`) and the tab's
    `RenderThread<Compositor<LayerTarget>>`. The render thread is created when a session starts
    and dropped in `session_ended`, so an idle tab owns no Metal resources.
13. **Launch options.** `DRIFT_AUTOCONNECT=<profile name>` connects that profile in the first tab
    (dev convenience / smoke test; exact name match, else a unique case-insensitive one) and
    `DRIFT_CONFIG_DIR` relocates `profiles.toml`. The environment can never switch the Keychain
    off; only `RunOptions` (tests) can. `tracing_subscriber` is initialised from `RUST_LOG`
    (default `info`).

## Consequences

- `M1-6-profiles-and-view-model` decision 7 ("session intents return `NotImplemented`") is
  superseded: the intents are wired. `CommandError::NotImplemented` remains for future stubs.
- The window-group behaviour is covered by a real-app test,
  `cargo test -p drift-app --features macos-ui-tests --test tabs_ui` (needs a window server):
  it launches Drift with a temporary config, opens two more tabs, asserts
  `tabbedWindows.count == 3`, then sends `newWindowForTab:` and asserts a fourth tab.
- Things that need a person (tab bar "+", tab overview, VoiceOver, quitting with a wedged
  session, a real recording) are in `docs/acceptance.md`.
