# Build prompt — UI-windows: Connections gallery and one window per connection

You are a team of **at most three agents** that builds, tests, lands and releases the design in
`docs/design/mockup-windows.html`. Do not start more agents than the three roles below, and do
not have any agent hand its work off to further sub-agents. When this prompt says "the lead",
"platform" or "UI", it means one of those three agents.

Task ID: **`UI-windows`**. Branch: **`task/UI-windows-gallery`**. Release: **0.2.0**. Tag it
locally and push nothing.

## 0. Read first (every agent)

1. `docs/team-conventions.md`: TDD order, commit format, ADRs, commands, and the rule that
   **nothing is pushed**. It overrides anything in this prompt that conflicts with it.
2. `plan.md` §0–§5.
3. `docs/design/mockup-windows.html`: the design. Open it in a browser (serve `docs/design/`
   with `python3 -m http.server` because `file://` may be blocked). It opens in dark mode; add
   `#light` to the URL for light mode. The bullets above each board are the behaviour spec. The
   boards show layout and materials.
4. `docs/adr/UI-tabs-connection-manager.md` and `docs/adr/M6-1-session-manager-and-windows.md`:
   the tab model this work replaces. Know what each of their decisions was protecting (Window
   menu, Cmd+1…9, full screen, focus, the `on_main` deadlock fix) before you remove anything.
5. `docs/design/mockup-glass.html`: the previous design. Its materials, sheets, HUD, reconnect
   ring, error sheet and form copy stay unless the new mockup changes them.

Where the mockup's example text differs from the app's text (error messages, `grdctl`
commands, hints), **the code's text wins**: `drift_core::messages`, `view::grdctl_status_command`
and the `MODES` / `CREDENTIALS` tables in `ui/src/views/profileForm.ts`. The mockup's error
board is only an example.

## 1. What changes

### Window model (board 0)
- **Connections** is a single window. It is created at launch, and Cmd+0 or
  File ▸ Show Connections brings it forward. Closing it only hides it; sessions keep running.
  Quitting is the only way to close it for good.
- Every connected profile gets **its own window**. Connecting opens a new session window at once
  and runs the connect stages in it. Connecting to a profile that is already live brings its
  window forward and does not start a second session. This keeps the behaviour of ADR UI-tabs
  decision 8.
- **Stop using native window tabbing.** Set `tabbingMode = disallowed` and remove the
  `drift.sessions` tab group, the hidden-accessory hack, the `<window>-strip` webview,
  `strip.html`, `strip.ts`, `stripApp.ts`, `tabStrip.ts`, `src-tauri/src/strip.rs` and the
  `tab_strip` / `select_tab` / `close_tab` / `new_tab` commands. Keep the `focus_content` idea
  only if something still needs it.
- Closing a session window (red button or Cmd+W) disconnects that session. If the session is
  live, ask for confirmation first. A window that is connecting or has failed closes without
  asking.
- Each session window remembers its frame per profile and restores it the next time.
- Quitting with sessions open asks once for all of them. The existing quit cap from M6-1 stays.
- **Dock menu**: a "Sessions" section lists each session window with its mode glyph and status
  dot, followed by Connections (Cmd+0) and New Connection… (Cmd+N). The glyph and dot may be
  text fallbacks, as in ADR UI-tabs decision 11. Tauri has no Dock-menu API, so add
  `applicationDockMenu:` in `drift-macos`.

### Connections window (board 1)
- The toolbar sits in the transparent title bar: the title "Connections" with a count, an
  **All / Open** segmented filter, search (filters by name and host), and a **+** button.
- The gallery is a grid of large glass cards with a 16:9 preview, mode glyph, name,
  `host · mode` and a ⋯ button. It has two sections: **Open** (profiles with a window) and
  **Saved** (the rest), followed by a dashed **New Connection** card.
- Preview states:
  - Idle: the mode's colour gradient with its glyph.
  - Live: a downscaled snapshot of that session's latest composited frame, refreshed a few
    times a minute and only while the Connections window is visible. It is never written to
    disk and never logged.
  - Pills: Live (green, with uptime), Connecting (spinner), Reconnecting in N s (amber, dimmed
    preview) and Failed (red).
- Hovering a card, or focusing it with the keyboard, puts one primary button on the preview:
  **Connect**, or **Show Window** if the profile is live. Double-click and Return do the same.
- The ⋯ menu has Connect, Edit… (Cmd+E), Duplicate (Cmd+D) and Delete… (Cmd+Delete, with a
  confirmation). A live profile also gets Disconnect. The same actions are on the card's
  context menu.
- The cards are a keyboard-navigable grid: arrow keys move, Return activates and Space opens
  the ⋯ menu. VoiceOver reads name, mode, host and state.

### Configuration sheet (boards 2–3)
- This is a document-modal sheet on the Connections window. It replaces the sidebar form. The
  header and footer stay fixed and the grouped rows scroll between them. Esc cancels.
- **The default mode is Headless.** Mode order everywhere is **Headless → Desktop Sharing →
  Remote Login**:
  - `ConnectMode::ALL`
  - `DEFAULT_MODE` in `ui/src/app.ts`
  - the mode tiles, the list labels, and any menu that lists modes.

  This changes only the default for *new* profiles. Existing `profiles.toml` files load as
  before.
- Switching mode swaps only the credentials section and keeps name, host and port.
- New connection footer: **Cancel**, **Add** and **Add & Connect**. Add & Connect is the default
  button and is disabled until the profile is valid. Errors show inline on the field, using the
  validation that already exists in Rust.
- Edit footer: **Delete…** on the left, then Cancel and Save. Save is disabled until something
  changes. The header shows host:port, trust status and Connect, or Show Window if the profile
  is live. Editing a live profile is allowed, and the footer says "Applies on next connect".
- "Keyboard, display and clipboard" is a disclosure, closed by default for new profiles, with a
  one-line summary of the current values.

### Session window (boards 4–5, 7)
- The title bar is transparent with no strip. It has the traffic lights in their standard
  position, a centred **identity capsule** (mode glyph, name, host, status dot, or a spinner
  while connecting), and two buttons on the right: **Show Connections** (grid icon) and
  **Statistics** (gauge, toggles the HUD). The window title is the profile name, so Mission
  Control, Cmd+` and the Window menu show it.
- The remote picture starts directly under the title bar. Rework `present::chrome` and
  `band_y` for the new title bar height and remove the strip band.
- The connecting stages, certificate sheet, greeter banner, HUD, reconnect ring and error sheet
  work as before, but belong to that window. The error sheet gains **Edit Connection…**, which
  brings Connections forward with the edit sheet open. Closing a failed window dismisses it and
  its card goes back to idle.
- Keep UI-tabs decision 7: clicking the title bar never keeps keyboard focus away from the
  remote desktop. Keep decision 12's `on_main` double hop for all AppKit work.

### Full screen and menus (board 6)
- Each session window goes full screen in its own Space. Nothing draws over the picture, and
  the menu bar reveal must not bring back any title bar chrome.
- **File**:
  - New Connection… (Cmd+N)
  - Show Connections (Cmd+0)
  - Edit <name>… (Cmd+E)
  - Disconnect <name> (Shift+Cmd+D)
  - Close Window (Cmd+W)

  Remove New Tab (Cmd+T) and New Window.
- **Window**: Connections (Cmd+0), then a "Sessions" section with every session window and
  Cmd+1…9. The current one is checked. Remove Show Previous/Next Tab.
- Drift's shortcuts keep working while the remote desktop has the keyboard. Extend the existing
  key-equivalent allow-list for Cmd+0, Cmd+N, Cmd+E and Shift+Cmd+D.

## 2. The team

| Agent | Owns | Never touches |
|---|---|---|
| **Lead / integrator** | The ADR, the plan below, the Red-test review, code review, `cargo xtask ci` on the merged tree, `docs/acceptance.md`, the `--no-ff` merge, the release | Feature code, except small review fixes |
| **Platform** (Rust / Tauri / AppKit) | `crates/drift-macos`, `crates/drift-core`, `src-tauri/src/*`, `tauri.conf.json`, capabilities, `cargo xtask bindings`, real-window tests | `ui/src/views/*` styling |
| **UI** (TypeScript / HTML / CSS) | `ui/src/**`, `ui/index.html`, `ui/build.ts`, `ui/test/**`, glass styles, gallery, sheet, identity capsule | Rust crates |

Contract first: before either engineer writes feature code, the lead writes the IPC surface
into the ADR. That covers command names, argument and return types, and events (for example
`ConnectionsChanged`, `ThumbnailUpdated`, and a per-window `WindowIdentity`). Platform lands it
first so that `ui/src/bindings.ts` is regenerated before UI depends on it. Any later change to
the contract goes through the lead and the ADR.

Everyone works on the task branch in their own git worktree (`git worktree add`) and commits
there. The lead merges those worktree branches into `task/UI-windows-gallery`, then merges that
branch into `main`. Builds are large and other agents share the machine, so run long cargo
commands in the background and poll them.

## 3. Phases

1. **ADR (lead).** Write `docs/adr/UI-windows-gallery.md` in the same shape as UI-tabs:
   context, numbered decisions, and the list of UI-tabs and M6-1 decisions it supersedes.
   Record these choices:
   - how the Connections window is hidden rather than closed;
   - where thumbnails come from, at what size and rate, and the privacy rule (memory only);
   - how window frames are stored;
   - how the Dock menu is built;
   - the IPC contract;
   - the new focus rules.

   Update the ADR table in `docs/team-conventions.md`. Commit `docs(UI-windows): …`.
2. **Red (platform and UI in parallel).** Write failing tests first and run them to confirm they
   fail for the right reason. Commit `test(UI-windows): …`. The lead reviews the Red set against
   §1 before anyone goes green. Minimum coverage:
   - Rust:
     - `ConnectMode::ALL` order;
     - new-profile default = Headless;
     - one Connections window whose close hides it;
     - connect opens a window, and connecting to a live profile focuses the existing window;
     - closing a live window asks first, then disconnects;
     - the menu model (File/Window items, shortcuts, no New Tab);
     - the Dock menu model;
     - window-frame persistence;
     - `present::chrome` without a strip;
     - the thumbnail throttle, and no thumbnail when hidden;
     - identity and status for each `SessionView` screen, reusing the UI-tabs decision 5 table.
   - `bun test`:
     - gallery sections, card states, pills, filter and search, keyboard grid, VoiceOver labels;
     - hover and focus actions;
     - the ⋯ menu;
     - the sheet: default Headless, mode order, credentials swap keeping name/host/port, inline
       errors, disabled default button, Add vs Add & Connect, edit footer and live note;
     - the identity capsule;
     - Edit Connection… from the error sheet.
   - Real-window tests: replace `tabs_ui` with `windows_ui`. It checks that two sessions give
     three independent windows, that none is tabbed, that the traffic lights are in the
     standard position, and that full screen covers the whole screen.
3. **Green.** Platform and UI implement against the contract and commit `feat(UI-windows): …`,
   then `refactor(UI-windows): …`. Delete the tab-strip code and its tests in the same branch,
   with no dead code left behind. `cargo xtask check` must be clean before every commit.
4. **Review (lead).** Read the whole diff against §1 and against the mockup in both light and
   dark. Check for leftovers of the tab-strip code and for focus theft. Check that no
   credential or frame data reaches logs or disk (`cargo xtask secret-scan`). Run
   `bun run typecheck`. Send findings back to the owning agent, which lands them as
   `fix(UI-windows): …`.
5. **Verify (lead, with platform).**
   - `cargo xtask ci` must pass on the merged tree, with coverage.
   - Run the app with `cargo tauri dev` and walk through boards 0–7.
   - Run the unattended smoke from `docs/adr/E2E-1-live-app-smoke.md` against the homelab in
     all three modes, Headless first.
   - If the host is reachable, run `cargo xtask e2e`. If it is not, say so in the report; do
     not skip it silently.
   - Replace the "UI-tabs" section of `docs/acceptance.md` with a "UI-windows" section: manual
     checks per board, full screen and Spaces, Dock menu, VoiceOver on the gallery, and light
     and dark. Mark older items it supersedes the way the file already does.
6. **Land (lead).** `git merge --no-ff task/UI-windows-gallery` into `main`. The merge message
   follows the format of `a6d2524`: a summary, the bullets, and the Red tests with their failing
   output. Don't tick `plan.md` checkboxes beyond what the conventions allow.
7. **Release (lead).**
   - Commit `chore(release): 0.2.0`. It bumps the version in `Cargo.toml`, `Cargo.lock`,
     `src-tauri/tauri.conf.json` and `ui/package.json`, like `cfddf85`. Add annotated tag
     `v0.2.0`.
   - `cargo tauri build`, which signs with the pinned identity. If codesign fails with
     `errSecInternalComponent`, run `signing-unlock -v`. Check the bundle with
     `codesign -dv --verbose=2 target/release/bundle/macos/Drift.app`.
   - Notarize and staple the `.dmg` (`xcrun notarytool submit … --wait`, then
     `xcrun stapler staple`).
   - Launch the stapled app once and connect a Headless profile.
   - **Do not push** the branch, the tag or anything else.

## 4. Done means

- `main` has the `--no-ff` merge and `chore(release): 0.2.0`, and the `v0.2.0` tag exists
  locally.
- `cargo xtask ci` passes on `main`, and the signed, notarized, stapled `.dmg` exists at its
  path.
- The ADR is written, the team-conventions table is updated, and the acceptance section is
  written.
- No tab-strip code remains (`rg -i "tab.?strip|drift\.sessions|new_tab|NewTab"` finds only
  history and superseded ADR text).
- The lead's final report lists:
  - what landed (commits);
  - the test counts before and after;
  - the e2e and smoke results, or why they could not run;
  - the manual acceptance items that still need a human;
  - any place the build departs from the mockup, and why.

If a decision is genuinely ambiguous and is not settled by this prompt, the mockup's board
bullets or the conventions, the lead records the choice in the ADR and moves on. Stop and ask
only for things that are irreversible or leave this machine.
