# UI-tabs — Connection Manager and Session tabs in an HTML tab strip

- Status: accepted
- Date: 2026-09-23
- Design: `docs/design/mockup-glass.html` boards 0–5
- Code: `crates/drift-macos/src/{tabs,tauri_glue,webview,window}.rs`,
  `src-tauri/src/{strip,present,manager,windows,commands}.rs`, `src-tauri/tauri.conf.json`,
  `ui/{strip.html,build.ts}`, `ui/src/{strip,stripApp,app}.ts`, `ui/src/views/{tabStrip,profileList}.ts`
- Supersedes in part: `M6-1-session-manager-and-windows` decisions 6, 7 and 8 (see below)

## Context

The mockup turns every window into a strip of two kinds of tab. A **Connection Manager** tab
("Connections", neutral grey icon) holds the saved connections and the form; connecting turns
that same tab into a **Session** tab (mode glyph, profile name, status dot: green live, amber
reconnecting, red failed; a spinner and "Connecting to <name>…" while connecting). Disconnect
turns a session tab back into a manager; closing the last tab closes the window. The title bar
is transparent with no fill, divider or title: the tabs sit in a 46-point row beside the traffic
lights, the active tab is a raised glass pill with a ×, and a Firefox-style + follows the last
tab. In full screen the strip is hidden entirely and the Window menu switches tabs.

ADR M6-1 built each tab as its own `NSWindow` in the native tab group `drift.sessions`, with
AppKit drawing the tab bar. That keeps the Window menu listing, Cmd+1…9 and full-screen tab
handling for free, so the recommended approach was to keep the group, hide AppKit's bar and
draw the mockup's strip in HTML.

## Decisions

1. **Keep the native tab group; hide AppKit's tab bar through its titlebar accessory.**
   `toggleTabBar:` cannot hide the bar once a group has more than one tab. Measured on macOS 27
   with three tabs, `isTabBarVisible` stays `true` and the bars keep covering 68 pt, and the
   private `NSWindowTabGroup.tabBarEnabled` does nothing either. The bar is an ordinary
   `NSTitlebarAccessoryViewController` in the public `titlebarAccessoryViewControllers`, though,
   and setting its public `hidden` property collapses the covered height to the plain title bar
   row (32 pt). This holds across tab switches, closing tabs and full screen. A window gets a
   fresh, visible accessory whenever it joins a group (also in full screen), so
   `tabs::hide_native_tab_bar` runs on every layout pass (`windows::layout`: window events,
   view changes, new tabs). Drift adds no accessories of its own, so "every accessory" is the
   tab bar. Covered by `drift-macos` `tabs_ui::the_native_tab_bar_is_hidden_in_a_tab_group` and
   the real-app `tabs_ui`.
2. **The strip is a second webview in every window, not part of the page.** The page webview is
   hidden while the picture is live and shrunk to a corner panel for a HUD (ADR M7-3). Its
   single rectangle cannot also cover a full-width strip without covering the picture. A
   full-window transparent page with hit-test pass-through was rejected: WebKit keeps setting
   the cursor from its tracking areas over the whole view (so it would fight the remote pointer),
   and a transparent WebKit layer would sit over the Metal picture on every frame. Each window
   therefore gets a transparent child webview labelled `<window>-strip` (`strip::strip_label`)
   that loads `strip.html`. It is pinned to the top 46 pt (`strip::STRIP_HEIGHT`), and the page,
   the `RemoteView` and HUDs live below it (`present::chrome`, `present::band_y`,
   `windows::layout`). This needs Tauri's `unstable` feature (`Window::add_child`). A window with
   two webviews is no longer a Tauri "webview window", so `get_webview_window(label)` returns
   `None`: the app uses `get_webview(label)` (the page) and `.window()`, and `tauri_glue` takes a
   `Window` plus the page `Webview`. The `session-*` capability covers child webviews of those
   windows (Tauri enables a window's capabilities on all its webviews). Cost: one more `WKWebView`
   per tab.
3. **Full screen hides the strip; the picture fills the screen.** `present::chrome(true)` has no
   strip. The layout reruns on `NSWindowDidEnter/ExitFullScreenNotification`, and nothing
   observes the menu bar sliding down, so the strip stays hidden then too. With the accessory
   hidden, AppKit's tab bar does not appear when the menu bar reveals the title bar either.
4. **Rust pushes the strip; the strip only renders.** `strip::TabStrip` is the ordered list of
   the window's group (AppKit's `tabGroup.windows`, leading to trailing), `active` is the
   receiving window's own label, and `live_profiles` lists profiles with a live actor.
   `windows::broadcast_tabs` emits `TabStripChanged` to the strip webview (`EventTarget::webview`)
   and to the page (`EventTarget::webview_window`) after every new tab, view change, closed tab
   and key-window change. A per-window cache skips identical pushes (the statistics sample
   re-applies the view about once a second). On load both webviews ask for the current strip
   with the `tab_strip` command, so a push that raced the page load is not lost.
5. **Tab states** (`strip::tab_item`, from the window's `SessionView`):

   | Screen | Kind | Title | Status |
   |---|---|---|---|
   | none / `Profiles` | manager | "Connections" | idle (grey glyph) |
   | `Connecting` | session | "Connecting to <name>…" | connecting (spinner) |
   | `Certificate` | session | <name> | connecting (spinner) |
   | `Live`, `GreeterHint` | session | <name> | live (green) |
   | `Reconnecting` | session | <name> | reconnecting (amber) |
   | `Error` | session | <name> | failed (red) |

   Names are cleaned like window titles (`drift_macos::tabs::display_name`).
6. **Strip intents reuse the existing paths.** A click on another tab calls `select_tab`
   (`tabGroup.selectedWindow` + `makeKeyAndOrderFront:`). The × calls `close_tab`, the same
   graceful close as Cmd+W and the red button. The + calls `new_tab`, the same `open_tab` as
   File ▸ New Tab / Cmd+T. A click on the window's own tab calls `focus_content`.
7. **The strip never keeps the keyboard.** A click makes the strip's `WKWebView` first
   responder, which would steal the keys from the remote desktop. The strip therefore calls
   `focus_content` whenever its page gains focus, and Rust hands first responder back to
   whatever `present::focus_for(surface)` says: the `RemoteView` for live/HUD surfaces, the page
   otherwise. Trade-off: the strip cannot be reached with the Tab key. Tabs are switched from
   the keyboard with Cmd+1…9 and the Window menu.
8. **Connecting a profile that is already live switches to its tab.**
   `SessionManager::live_window_for(profile, except)` finds a window with a live actor for the
   profile, and `windows::connect_profile` selects it instead of opening a second session. The
   connection list marks such profiles with a green dot, "Connected in another tab"
   (`TabStrip::live_profiles`).
9. **A failed tab returns to the Connection Manager.** The error sheet's **Close** now calls
   `disconnect` instead of `close_session`, and `SessionManager::disconnect` on a window whose
   actor has already ended resets it to the profiles screen at once (board 5: "Closing it, or
   Disconnect, turns it back into a Connection Manager"). The tab's × still closes the tab.
   `close_session` stays in the API but the UI no longer uses it.
10. **The greeter hint is the floating banner and the tab's tooltip.** The title bar shows no
    title, so M6-1's window subtitle is invisible and has been removed. `present::greeter_hint`
    (the former `window_subtitle` text) feeds the session tab's tooltip (`TabItem::hint`). The
    `Hud::Banner` capsule stays the primary, always-visible hint over the login screen, below the
    strip, as the mockup shows. It is in the page, so it also shows in full screen.
11. **Titles only feed the Window menu.** `present::window_title` is "Connections" for a manager
    and M6-1's glyph + name for a session. AppKit lists every window of the group by that title,
    with a checkmark on the current one, and the existing Show Tab 1…9 items keep Cmd+1…9. The
    mockup's per-item mode icons and coloured dots are not reproducible in AppKit's automatic
    list; the title's text glyph (● ◐ ◌ ↻ ○ ⚠) stands in for them.
12. **Main-thread work runs outside tao's event handler.** `addTabbedWindow:ordered:` syncs tab
    sizes and redraws synchronously. Inside a `run_on_main_thread` closure that re-entered tao's
    `drawRect:` handler, which takes the lock tao already holds while running user events, and
    the app deadlocked on its second tab (the child webview made the redraw necessary).
    `windows::on_main` now hops twice: first through tao's queue, so it runs after pending
    messages such as the creation of a window just built off the main thread, then onto the main
    dispatch queue (`drift_macos::dispatch_main`), so the work runs from the run loop with no tao
    lock held. The real-window tests use the same hop.
13. **Traffic lights.** `trafficLightPosition` is `{ x: 18, y: 28 }`, which puts the close button
    18 pt from the left and centred in the 46-pt strip (±3 pt, asserted by the real-app `tabs_ui`
    test). The strip leaves the first 84 pt for the lights.

## Not done (recorded deviations)

- The mockup's File menu also shows **New Window**, **New Connection…**, **Close Window** and
  **Disconnect <name> ⇧⌘D**. They are not part of this task's spec and their shortcuts are not
  in `drift_input::MenuShortcut`'s allow-list (M2-4: every other Command combo belongs to the
  remote desktop), so the menus are unchanged: File ▸ New Tab / Close Tab and Session ▸
  Disconnect.
- The mockup's statistics button at the right end of the strip (board 3) is not added; Session ▸
  Show Statistics toggles the HUD as before.
- Tabs cannot be reordered or dragged out by mouse (AppKit's bar is gone); Window ▸ Move Tab to
  New Window and Merge All Windows still work, and each window's strip follows its own group.

## Consequences

- M6-1 decision 6 (greeter hint in the subtitle) is superseded by decision 10; decision 7's
  "New Session" title is now "Connections"; decision 8's "error screen Close = `close_session`"
  is superseded by decision 9. M6-1's "tab bar +" manual checks moved to the strip
  (`docs/acceptance.md`, "UI-tabs").
- `present::Chrome` lost its title-bar measurements (`titlebar`, `lights`, `offset`, the CSS
  variables): the page no longer sits under the title bar.
- `cargo xtask ci` cannot run the real-window tests. Run them from a logged-in session:
  `cargo test -p drift-macos --features macos-ui-tests --test tabs_ui` and
  `cargo test -p drift-app --features macos-ui-tests --test tabs_ui --test overlays_ui`.
- The look of the strip, full screen and the Window menu need a person; they are listed in
  `docs/acceptance.md` under "UI-tabs".
