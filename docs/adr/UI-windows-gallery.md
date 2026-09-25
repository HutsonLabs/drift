# UI-windows — Connections gallery and one window per connection

- Status: accepted
- Date: 2026-09-25
- Design: `docs/design/mockup-windows.html` boards 0–7 (build prompt `docs/design/mockup-windows.md`)
- Code: `src-tauri/src/{windows,connections,frames,dock,present,menu,manager,commands,lib}.rs`,
  `crates/drift-macos/src/{window,dock,alert,webview,tauri_glue}.rs`,
  `crates/drift-input/src/shortcuts.rs`, `crates/drift-core/src/profile.rs`,
  `src-tauri/tauri.conf.json`, `ui/{index,session,titlebar}.html`, `ui/build.ts`,
  `ui/src/{connectionsApp,sessionApp,titlebarApp}.ts`, `ui/src/views/{gallery,card,sheet,identity}.ts`
- Supersedes: `UI-tabs-connection-manager` decisions 1–6, 8, 9, 11 and 13 (in full), 7 and 10
  (in part); `M6-1-session-manager-and-windows` decisions 1 (tab group), 5 (last tab), 8 and 10
  (tab shortcuts). Details under "Supersessions".

## Context

UI-tabs made every window a native tab of `drift.sessions`, hid AppKit's tab bar and drew an
HTML strip in a second webview per window. The new design drops tabs: **Connections** is one
window with a gallery of large cards and a configuration sheet, and every connected profile gets
an ordinary Mac window of its own (Spaces, tiling, Mission Control, `⌘\``). What UI-tabs and
M6-1 protected and where it goes now:

| Protected by the tab model | Now |
|---|---|
| Window menu listing | Drift builds the Window menu's "Sessions" section itself (decision 9) |
| Cmd+1…9 | "Sessions" items 1…9 focus the n-th session window (decision 9) |
| Full screen | Each session window gets its own Space; no chrome in full screen (decision 5) |
| Focus (UI-tabs 7) | Title-bar webview hands focus back through `focus_content` (decision 11) |
| `on_main` double hop (UI-tabs 12) | Unchanged, used for all AppKit work (decision 12) |

## Decisions

1. **Two kinds of window, no tabbing.** `tauri.conf.json` keeps two templates with
   `create: false`: `connections` (label `connections`, 1100×720, loads `index.html`) and
   `session` (labels `session-<n>`, 1280×800, loads `session.html`). Every window gets
   `NSWindow.tabbingMode = Disallowed` (`drift_macos::window::disallow_tabbing`), so AppKit never
   groups them and the View ▸ Show Tab Bar / Merge All Windows items disappear. The tab group
   identifier, `hide_native_tab_bar`, `install_new_window_for_tab`, `strip.rs`, `strip.html`,
   `strip.ts`, `stripApp.ts`, `tabStrip.ts` and the `tab_strip` / `select_tab` / `close_tab` /
   `new_tab` commands are deleted.
2. **The Connections window is created at launch and only ever hidden.** Its `CloseRequested`
   is always prevented and turned into `orderOut:` (`Window::hide`). Cmd+0, File ▸ Show
   Connections, Window ▸ Connections, the Dock menu, the session title bar's grid button and
   `RunEvent::Reopen` all call `windows::show_connections`, which shows it and makes it key.
   Quitting is the only way it is destroyed. `ExitRequested` without a code stays prevented.
3. **Connecting opens a session window at once.** `connect(profile_id)` (any window, any
   caller) goes through `windows::connect_profile`:
   * a window whose session for that profile is *open* (connecting, certificate, live, greeter,
     reconnecting, or failed-but-not-dismissed) → that window is ordered front and made key
     (`SessionManager::window_for(profile)`); no second session (keeps UI-tabs decision 8);
   * otherwise a new `session-<n>` window is built (hidden, restored frame, decision 7), the
     session is opened in it (`SessionManager::open`) and the window is shown. The window runs
     the connecting stages, certificate sheet, greeter banner, HUD, reconnect ring and error
     sheet exactly as before, in its own page.
   `DRIFT_AUTOCONNECT` connects its profile the same way after the Connections window exists.
4. **Closing a session window ends its session.** Red button and Cmd+W →
   `windows::request_close(label)`:
   * `present::close_needs_confirmation(view)` is `true` only for `Live` and `GreeterHint` (and
     `Reconnecting`, since a live desktop is still held); `Connecting`, `Certificate` and `Error`
     close at once;
   * the confirmation is a native sheet on that window (`drift_macos::alert::confirm_sheet`,
     `NSAlert beginSheetModalForWindow:`): "Disconnect “<name>”?" / "The remote session keeps
     running on the host." / **Disconnect** (default) · Cancel;
   * on confirm the M6-1 graceful close runs (2 s cap), the window's frame is saved, then the
     window is destroyed. The profile's card returns to idle.
   File ▸ Disconnect <name> (Shift+Cmd+D), the card's Disconnect and the error sheet's Close,
   and Cancel while connecting close the window **without** asking (explicit intent).
   `SessionManager::disconnect` (back to the profiles screen) is removed: a session window
   without a session does not exist any more.
5. **Session window chrome.** The title bar is transparent (`titleBarStyle: Overlay`,
   `hiddenTitle: true`) and 52 pt tall (`present::TITLEBAR_HEIGHT`); `trafficLightPosition`
   `{ x: 20, y: 26 }` centres the lights in it at the standard inset. A transparent child
   webview `<label>-titlebar` loads `titlebar.html` and draws the centred **identity capsule**
   (mode glyph, name, host, status dot or spinner, greeter hint as tooltip) and the **Show
   Connections** (grid) and **Statistics** (gauge, pressed while the HUD is on) buttons. The
   page and the `RemoteView` start under it (`present::chrome(false)` =
   `{ titlebar: 52, content_top: 52 }`). In full screen `present::chrome(true)` =
   `{ titlebar: 0, content_top: 0 }`: the title-bar webview is hidden and the picture fills the
   screen, also while the menu bar is revealed (nothing observes the reveal, so nothing comes
   back). The window title (`present::window_title`) is the plain display name, so Mission
   Control, `⌘\`` and the Window menu show it. The Connections window has no child webview: its
   own page draws its toolbar in the transparent title bar (it is a page-only window).
6. **Identity and status per screen** (`present::identity`, reusing UI-tabs decision 5's table):

   | Screen | Status | Capsule |
   |---|---|---|
   | `Connecting`, `Certificate` | `connecting` | spinner instead of the dot |
   | `Live`, `GreeterHint` | `live` | green dot |
   | `Reconnecting` | `reconnecting` | amber dot |
   | `Error` | `failed` | red dot |

   A profile without a window is `idle`. The same `ConnectionStatus` feeds the gallery pills,
   the Dock menu and the Window menu.
7. **Window frames persist per profile** in `window-frames.json` next to `profiles.toml`
   (`frames::FrameStore`, a map `profile id → { x, y, width, height }` in screen points, top-left
   origin as Tauri reports it). Saved on close (and on quit for every open window); read when a
   window for that profile is created. `frames::restore(saved, screens)` keeps a frame only if at
   least 64×64 pt of it is on one of the current screens and it is at least the template's
   minimum size; otherwise the template size, centred (cascaded by 24 pt per open session
   window). The Connections window's frame is stored under the key `connections`. Only
   geometry is written; deleting a profile deletes its frame.
8. **Live thumbnails, memory only.** While the Connections window is visible (its
   `WindowObserver` occlusion state) Drift samples every live session every
   `connections::THUMBNAIL_INTERVAL` = 15 s, plus once right after it becomes visible and once
   when a session first goes live: the render thread's `Compositor::read_output()` is
   downscaled (box filter, pure `connections::downscale`) to fit **320×180**, PNG-encoded and
   emitted as `ThumbnailUpdated` to the `connections` webview only, as a `data:image/png;base64`
   URL. `connections::ThumbnailThrottle` is the pure scheduler (tested: nothing while hidden,
   at most one per session per interval, immediate on show / first live). Thumbnails are never
   written to disk, never logged (no `Debug` of the bytes), and dropped when the session ends
   (`ThumbnailUpdated` with `image: null`). The UI keeps them in memory only.
9. **Menus** (`menu::menu_spec`, still pure data):
   * **File**: New Connection… (Cmd+N), Show Connections (Cmd+0), separator, Edit <name>…
     (Cmd+E), Disconnect <name> (Shift+Cmd+D), separator, Close Window (Cmd+W). "<name>" is the
     key session window's profile; with no session window key the items read "Edit Connection…"
     / "Disconnect" and are disabled (`menu::file_titles(key)`, updated on key-window changes).
     New Tab and New Window are gone.
   * **Session**: Send Ctrl+Alt+Del, Reconnect, Show Statistics (unchanged; Disconnect moved to
     File).
   * **Window**: Minimize, Zoom, Enter Full Screen, separator, **Connections** (Cmd+0),
     separator, "Sessions" header, then one check item per session window in opening order
     (`menu::window_sessions(sessions, key)`: text glyph + name, `● ◐ ◌ ↻ ⚠` as UI-tabs decision
     11, Cmd+1…9 on the first nine, checked = key window). Drift owns this list: the submenu is
     **not** registered as `NSApp.windowsMenu`, so AppKit does not add a second, unordered list.
     Show Previous/Next Tab are gone.
   * `drift_input::MenuShortcut` becomes `NewConnection` (Cmd+N), `ShowConnections` (Cmd+0),
     `EditConnection` (Cmd+E), `Disconnect` (Shift+Cmd+D), `CloseWindow` (Cmd+W), `Quit`
     (Cmd+Q), `SelectSession(1..=9)` (Cmd+1…9) and `CycleWindows` (Cmd+\`). Cmd+T,
     Cmd+Shift+[ and Cmd+Shift+] now belong to the remote desktop.
10. **Dock menu.** Tauri has no Dock-menu API. `drift_macos::dock::install(provider)` adds
    `applicationDockMenu:` to the application delegate's class (tao's, found at runtime, like the
    old `newWindowForTab:` installation) and returns an `NSMenu` built from
    `dock::dock_menu(sessions) -> Vec<DockItem>` (pure, in `src-tauri/src/dock.rs`): a disabled
    "Sessions" header, one item per session window (`<glyph> <name>`, the glyph standing in for
    the mode icon and the text dot for the status, as UI-tabs decision 11 allowed), a separator,
    "Connections" (⌘0 shown) and "New Connection…" (⌘N shown). Clicks call back into Rust with
    the item's id (`dock::DockAction::{Focus(label), ShowConnections, NewConnection}`).
11. **Focus.** Keeps UI-tabs decision 7: the title-bar webview calls `focus_content` whenever it
    gains focus, and Rust hands first responder to `present::focus_for(surface)` (the
    `RemoteView` for live/HUD surfaces, the page otherwise). The title bar cannot be reached with
    Tab; its two buttons have menu equivalents (Cmd+0, Session ▸ Show Statistics). Showing the
    Connections window makes its page first responder; bringing a session window forward gives
    the keyboard to its surface's owner.
12. **Main-thread work** keeps UI-tabs decision 12: every AppKit call goes through
    `windows::on_main` (tao queue, then the main dispatch queue). Window creation stays off the
    main thread (`WebviewWindowBuilder::build`, `Window::add_child`).
13. **Quit** asks once when sessions are open: Drift ▸ Quit (Cmd+Q) shows one app-modal
    `NSAlert` (`drift_macos::alert::confirm`): "Quit Drift?" / "N sessions will be
    disconnected." / **Quit** · Cancel, then runs M6-1's shutdown with the 2 s `CLOSE_CAP`
    (frames saved first). No sessions → quit at once. The Dock's own Quit goes through
    `terminate:`, which tao turns into an exit Drift cannot veto; it keeps M6-1's blocking
    shutdown in `RunEvent::Exit`.
14. **Default mode is Headless; order Headless → Desktop Sharing → Remote Login** everywhere:
    `ConnectMode::ALL`, `DEFAULT_MODE` in `ui/src/app.ts` (now `ui/src/modes.ts`), the sheet's
    tiles, list labels and menus. Only the default for *new* profiles changes; existing
    `profiles.toml` files load unchanged.
15. **Configuration sheet** (Connections page, `ui/src/views/sheet.ts`): document-modal over the
    gallery (dimmed, blurred), fixed header/footer, scrolling body, Esc cancels. New: Cancel /
    Add / **Add & Connect** (default, disabled until `validate_profile` returns no issues;
    issues shown inline on the field). Edit: **Delete…** left, Cancel / Save (disabled until
    dirty); header shows host:port, trust status and Connect or Show Window; a live profile's
    footer says "Applies on next connect". "Keyboard, display and clipboard" is a closed
    disclosure with a one-line summary. Switching mode swaps only the credentials section.
16. **Gallery** (`ui/src/views/gallery.ts`, `card.ts`): toolbar in the transparent title bar
    (title + count, All / Open, search by name and host, +), sections **Open** and **Saved**,
    dashed New Connection card; card = 16:9 preview (mode gradient + glyph, or the live
    thumbnail), glyph, name, `host · mode`, ⋯. Pills: Live + uptime, Connecting, Reconnecting in
    N s (dimmed), Failed. Hover/focus shows one primary button (Connect / Show Window); double-
    click and Return do the same; Space opens ⋯ (Connect, Edit… ⌘E, Duplicate ⌘D, Disconnect when
    open, Delete… ⌘⌫ with confirmation), also as the context menu. Arrow keys move in the grid
    (roving tabindex); each card's `aria-label` is "<name>, <mode>, <host>, <state>".
17. **Error sheet** gains **Edit Connection…** (`ErrorAction::EditProfile`, already in
    `drift_core::messages`, now offered whenever the explanation has it and always for
    credential/host errors): it calls `show_connections(Some(profile))`, which brings Connections
    forward and emits `ConnectionsIntent::Edit` to it. Its **Close** closes the window (the card
    goes back to idle).

## IPC contract

Types are generated into `ui/src/bindings.ts` by `cargo xtask bindings`. `window` below means
the calling window (`tauri::Window`).

### Types (`src-tauri/src/connections.rs`)

```rust
#[serde(rename_all = "kebab-case")]
pub enum ConnectionStatus { Idle, Connecting, Live, Reconnecting, Failed }

/// One profile that has a session window (the gallery's "Open" section).
pub struct OpenConnection {
    pub profile_id: Uuid,
    pub window: String,                 // "session-<n>"
    pub status: ConnectionStatus,
    pub live_secs: Option<u32>,         // uptime at emit time while live (UI ticks locally)
    pub reconnect_in_secs: Option<u32>, // while reconnecting
    pub attempt: Option<u32>,           // while reconnecting
}

/// Everything the Connections window needs besides the profile list.
pub struct Connections { pub open: Vec<OpenConnection> }   // ordered by window opening order

/// A session window's title bar (identity capsule + buttons).
pub struct WindowIdentity {
    pub profile_id: Uuid,
    pub name: String,          // display_name
    pub host: String,
    pub mode: ConnectMode,
    pub status: ConnectionStatus,
    pub hint: Option<String>,  // greeter hint (tooltip)
    pub show_stats: bool,      // gauge pressed
}

pub struct Thumbnail { pub profile_id: Uuid, pub image: Option<String> } // data:image/png;base64,…

#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ConnectionsIntent { New, Edit { profile_id: Uuid } }
```

### Events

| Event | Payload | Target |
|---|---|---|
| `SessionViewChanged` (unchanged) | `SessionView` | the session window's page |
| `ConnectionsChanged` | `Connections` | `connections` page, after any status change / open / close |
| `ThumbnailUpdated` | `Thumbnail` | `connections` page only |
| `ConnectionsIntentRequested` | `ConnectionsIntent` | `connections` page (Cmd+N / Cmd+E / Edit Connection… from anywhere) |
| `WindowIdentityChanged` | `WindowIdentity` | `<label>-titlebar` webview (cached, identical pushes skipped) |

### Commands

| Command | Args → result | Notes |
|---|---|---|
| `app_info`, `list_profiles`, `new_profile`, `validate_profile`, `save_profile`, `delete_profile`, `forget_certificate`, `explain`, `open_local_network_settings`, `accept_certificate`, `reject_certificate`, `reconnect_now`, `cancel_reconnect` | unchanged | `delete_profile` of an open profile closes its window first |
| `connect` | `profile_id: Uuid → ()` | decision 3; any window |
| `duplicate_profile` | `id: Uuid → ProfileEntry` | copies profile + stored passwords, new id, name "<name> copy", no cert pin |
| `connections` | `() → Connections` | pulled by the Connections page on load |
| `show_window` | `profile_id: Uuid → ()` | focus the profile's session window; `NotFound` if none |
| `disconnect_profile` | `profile_id: Uuid → ()` | close that profile's window without asking |
| `close_session` | `() → ()` | close the calling session window without asking (error Close, Cancel while connecting) |
| `show_connections` | `edit: Option<Uuid> → ()` | decision 2 / 17 |
| `new_connection` | `() → ()` | show Connections + `ConnectionsIntent::New` |
| `window_identity` | `() → WindowIdentity` | pulled by the title bar on load |
| `toggle_stats` | `() → ()` | calling session window (gauge) |
| `focus_content` | `() → ()` | unchanged (title bar) |

Removed: `disconnect`, `tab_strip`, `select_tab`, `close_tab`, `new_tab`, `TabStripChanged`.

## Supersessions

- **UI-tabs** 1 (hidden tab bar), 2 (strip webview), 3 (strip hidden in full screen), 4 (strip
  pushes), 5 (tab states — its table lives on as decision 6 here), 6 (strip intents), 8 (switch
  to live tab — now focuses the window, decision 3), 9 (failed tab → manager; now the window
  closes), 11 (titles only for the Window menu — the title is now the plain name and the Window
  menu is Drift's), 13 (traffic lights in the 46-pt strip): superseded in full. 7 (focus) and 10
  (greeter hint: banner + tooltip, now of the identity capsule) carry over to the title-bar
  webview. 12 (`on_main`) is unchanged.
- **M6-1** 1 (window = tab in `drift.sessions`): now window = session, no tabbing. 5: closing
  the last *session* window leaves Connections; the Connections window never closes. 8
  (`disconnect` vs `close_session`): only `close_session` remains, and it closes the window. 10:
  tab shortcuts replaced by decision 9 here.

## Consequences

- Each session window still has two `WKWebView`s (page + title bar), but no AppKit tab bar is
  re-hidden on every layout and the Connections window has only its page.
- The real-window test `tabs_ui` becomes `windows_ui`: two sessions give three independent
  windows, none tabbed, traffic lights at the standard position in the 52-pt bar, full screen
  covers the whole screen. Run by hand:
  `cargo test -p drift-app --features macos-ui-tests --test windows_ui --test overlays_ui`.
- The Dock menu, full screen Spaces, VoiceOver on the gallery and light/dark need a person:
  `docs/acceptance.md` § UI-windows.
