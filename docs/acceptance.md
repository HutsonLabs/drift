# Manual acceptance checklist

Steps that cannot be automated (they need a human, a GUI permission prompt, physical network
changes or a clean machine). Each milestone's owner appends its "Done (manual …)" items here.

## M0 — Foundation

- [ ] `cargo tauri dev` opens a window titled "Drift" showing the connections screen ("Add a GNOME computer" on first launch).

### M0-6 / §5.3 — CI and the nightly e2e workflow

The gate's *contents* are asserted by `cargo test -p xtask --test ci_plan --test workflows`
(ADR `M0-6-ci-gate-coverage`). What is left needs a person, because nothing is pushed to the
public GitHub remote (`docs/team-conventions.md`), so no workflow ever runs there.

- [ ] **Canary (plan M0-6 Red):** on a throwaway branch, make one unit test fail (e.g. change an
      assertion in `crates/drift-core`), run `cargo xtask ci`, and see it stop at the nextest step
      with a non-zero exit; restore the test and see it pass again. The same command is the only
      thing `.github/workflows/ci.yml` runs, so a red test is a red CI run.
- [ ] **Nightly e2e (plan §5.3):** `.github/workflows/e2e-nightly.yml` is written against a
      self-hosted macOS arm64 runner labelled `drift-lan` with an SSH key for the GNOME host and
      the `DRIFT_E2E_*` repository secrets. Until a runner is registered and the repository is
      published, exercise the same steps by hand: `cargo xtask host-setup-check` then
      `cargo xtask e2e` (add `DRIFT_E2E_BENCH=1` on an idle host for the M9-1 budgets), and
      confirm no credential appears in the output.

## M2 — Input translation (drift-input decisions to confirm by hand)

These need a physical keyboard/trackpad and a person watching the remote desktop
(see `docs/adr/M2-1-keyboard-translation.md`).

- [ ] **ISO keyboard:** on an Apple ISO keyboard, the key left of `1` and the key right of left
      Shift type the same characters on the remote (with a matching remote XKB layout) as locally.
- [ ] **JIS keyboard:** `¥`, `ろ`, `英数` and `かな` behave as on a local GNOME session.
- [ ] **Deferred Command:** Cmd+Tab away from Drift and back does *not* open the GNOME overview;
      tapping Cmd alone *does*; Cmd+T opens a Drift tab and nothing happens on the remote.
- [ ] **Super+drag:** holding Cmd and dragging a window moves it on the remote.
- [ ] **Caps Lock:** toggling Caps Lock locally changes the remote caps state; switching tabs keeps
      it in sync.
- [ ] **Type using Mac layout:** Option+e, e → `é`; Option+s → `ß`; Japanese IME (Kotoeri)
      composes locally and commits into gedit; Ctrl+C / Cmd+V still act as chords.
- [ ] **Unicode beyond the remote layout:** type `Grüße ✓` into GNOME Text Editor on a host
      whose session layout provides those keysyms (e.g. add German in Settings › Keyboard and
      switch to it) and copy it back. g-r-d turns each Unicode event into an XKB keysym and
      mutter only injects keysyms the *session's* layout can produce, so on a US-only session
      `ü`, `ß` and `✓` never arrive — `e2e_unicode_typing` therefore only automates the ASCII
      (incl. shifted, typed without Shift) part. See `docs/adr/M4-2-session-actor-channels.md`.
- [ ] **Scroll:** two-finger scrolling in Files and Firefox is smooth and follows the local
      natural-scrolling direction; a notched mouse wheel scrolls one step per notch.

## M1-6 / M3-2 / M7-3 / M9-4 — Connect UI (stream D)

Rendering and intents are covered by `bun test`; these need eyes, VoiceOver or a real prompt.

- [ ] Light and dark appearance (System Settings › Appearance): the connections screen, the
      certificate prompt and the error screens use system colours and stay readable in both.
- [ ] VoiceOver (Cmd+F5): every field of the profile form is announced with its label; the
      connection type is announced as a radio group “Connection type”; an invalid field is
      announced as invalid together with its message; the certificate prompt is announced as an
      alert dialog with its title; the reconnect overlay countdown is announced politely.
- [ ] Switching Connection type between Remote Login, Headless session and Desktop Sharing
      swaps the credential fields without losing the typed name and host.
- [ ] Certificate prompt for the homelab system daemon shows the same fingerprint as
      `sudo grdctl --system status` (“TLS fingerprint: f3:e7:a2:…”) on 10.1.2.40.
- [ ] Local Network Privacy screen (a freshly ad-hoc-signed build connecting to 10.1.2.40
      directly, errno 65): “Open Local Network Settings” opens System Settings › Privacy &
      Security › Local Network.

## M1-1 / M3-1 — Connect and redirect (drift-rdp)

Automated: loopback tests against `FakeServer` and `cargo xtask e2e` (`e2e_headless_connects`,
`e2e_remote_login`). These need macOS Local Network Privacy, which only a GUI prompt can grant:

- [ ] A Drift build **without** the Local Network permission (ad-hoc signed, or permission denied
      in System Settings › Privacy & Security › Local Network) connecting straight to
      `10.1.2.40:3392` ends in `Failed { LocalNetworkDenied }` (errno 65 is mapped by
      `drift_rdp::connect::classify_io_error`; the loopback test injects the errno).
- [ ] After granting the permission once, the same build connects without SSH forwards, and a
      Remote Login profile goes leg 1 → greeter (`AwaitingGreeterLogin`) → desktop after logging
      in at the greeter.

## M3-2 / M7-3 — Greeter password opt-in (stream A)

Automated: the loopback tests in `crates/drift-rdp/tests/m7_3_reconnect.rs` (typed only after a
click, never without the opt-in). What needs a person is the real GDM greeter:

- [ ] With a `linux-login` secret stored for a Remote Login profile, reconnect to the greeter,
      click your user tile and **do nothing else**: Drift types the password and logs in about
      1.5 s later, landing in the same session.
- [ ] With the same profile but no stored Linux password, the greeter stays untouched no matter
      where you click.
- [ ] Clicking "Not listed?" instead of a tile and waiting: Drift types the password into the
      user-name field (known consequence of "type into the focused field"); it is visible, so
      clear it and pick the tile. Nothing is typed before the first click.

## M1-5 / M4-3 — Rendering (drift-render)

- [ ] With a live Headless session (drifttest2), the desktop colours match the host monitor
      (compare a GNOME Settings window side by side: no tint, blacks are black, whites white).
- [ ] At a Retina drawable equal to the desktop size (Retina on, window not resized) terminal
      text is pixel-sharp (no filtering blur); after resizing to a non-matching size the picture
      scales smoothly with black bars, and snaps back to sharp once DISP re-layout completes.

## M1-6 / M2-2 / M2-4 / M2-5 / M3-2 / M6-2 / M7-2 — macOS platform layer (drift-macos)

These need a person, a real keyboard/IME, physical network changes or sleep
(see `docs/adr/M1-6-macos-platform-layer.md`).

- [ ] **RemoteView placement:** after connecting, the webview disappears, the desktop fills the
      window, and typing goes to the remote immediately (no click needed). Resizing the window
      and moving it between a Retina and a 1× display keeps the picture sharp.
- [ ] **Shortcut routing:** Cmd+C / Cmd+V / Ctrl+Tab / Cmd+K act on the remote; Cmd+T, Cmd+W,
      Cmd+Q, Cmd+1…9, Cmd+Shift+[ / ], Cmd+` act on Drift and nothing happens on the remote.
      Holding Cmd+K and releasing K first, then Cmd, leaves no stuck key on the remote.
- [ ] **Connect form still edits text:** with the connect form visible, Cmd+C / Cmd+V / Cmd+A
      work in its text fields (the RemoteView does not claim them when it is not focused).
- [ ] **IME / dead keys (Type using Mac layout on):** Option+E, E types `é`; the Japanese IME
      shows its candidate window near the bottom-left of the session and commits into gedit;
      Return while composing confirms locally instead of sending Enter.
- [ ] **Remote cursor:** the pointer takes GNOME's shapes (I-beam over text, resize arrows on
      window edges, hand over links) at the same size as local cursors, crisp at Retina/200 %;
      it disappears where GNOME hides it and becomes the arrow when leaving the window.
- [ ] **Keychain:** saving a profile with a password creates a Keychain Access item
      "com.hutsonlabs.drift", account `<profile uuid>/rdp-user` (or `rdp-system`); deleting the
      profile removes it; no password prompt appears on relaunch of the same signed build.
- [ ] **Tabs:** three sessions open as tabs of one window even with System Settings › Desktop &
      Dock › "Prefer tabs when opening documents" = "In Full Screen"; the tab bar's "+" opens
      the connect form in a new tab; tab titles show the state glyph (● ◐ ◌ ↻ ○ ⚠).
- [ ] **Network trigger:** with a live Headless session, Wi-Fi off pauses reconnecting (no
      attempts counted); Wi-Fi on reconnects within ~1 s without waiting for the backoff.
- [ ] **Wake trigger:** sleep the Mac for > 1 min with a live session; after wake it reconnects
      immediately (single attempt, not one per trigger).
- [ ] **Cancel survives a wake (M7-2):** in a live session, pull the network, press *Cancel* on
      the reconnect overlay, then sleep and wake the Mac. The session stays `Disconnected` —
      the wake trigger must not restart a session the user cancelled.
- [ ] **Local → remote clipboard in the app (M5-3/M5-2):** with a live session, copy text in a
      Mac app (Cmd+C) and paste it in GNOME Text Editor within ~250 ms of switching back;
      repeat with a PNG copied from Preview into a GTK4 app. With two tabs open, copying on the
      Mac and pasting in the *focused* tab's session works, and the unfocused tab's session
      receives it only once its tab is selected.
- [ ] **Clipboard before connecting (M5-3):** copy text on the Mac *before* opening a session,
      then connect; the first paste in GNOME already gives that text.

## M6-1 / M6-2 / M8-3 — Sessions, tabs and menus (drift-app)

Automated: `cargo nextest run -p drift-app` (fake-actor lifecycle, menu model, presentation) and
`cargo test -p drift-app --features macos-ui-tests --test tabs_ui` (three real windows in one tab
group, `newWindowForTab:`).

- [ ] **Tab-group test (M6-2):** run `cargo test -p drift-app --features macos-ui-tests --test
      tabs_ui` from a logged-in graphical session (not over SSH, not on a locked screen: it opens
      real NSWindows and needs a window server) and see
      `session_windows_join_one_native_tab_group ... ok`. `cargo xtask ci` compiles this binary on
      every run (`clippy --all-features --all-targets`, see `docs/adr/M0-6-ci-gate-coverage.md`)
      but cannot run it unattended.

These need a person:

- [ ] **Three live sessions:** open Remote Login, Headless and Desktop Sharing profiles in three
      tabs of one window; switching tabs shows each desktop instantly, and background tabs stay
      below ~2 % CPU (Activity Monitor).
- [ ] **Tab bar:** the "+" button opens a new tab with the connect form; Cmd+1…9 select tabs;
      Cmd+Shift+[ / ] move between them; the tab overview (Window ▸ Show All Tabs) shows live
      thumbnails; dragging a tab out into its own window keeps that session running, and dragging
      it back rejoins the group.
- [ ] **Title and subtitle:** the tab title is the profile name with its state glyph; at the GDM
      greeter the title bar subtitle reads "Log in as “<user>” to start your session", and after a
      reconnect of a running session "Session is still running — log in as “<user>” to resume".
- [ ] **Close and quit:** Cmd+W on a connected tab closes the session (the GNOME host shows no
      leftover session) and then the tab; Cmd+Q with three live sessions quits within about two
      seconds; the red close button behaves like Cmd+W. Closing the last tab leaves Drift running
      (Dock icon); clicking the Dock icon opens a new tab.
- [ ] **Session menu:** Send Ctrl+Alt+Del shows GNOME's screen; Reconnect restarts a failed
      session in the same tab; Disconnect returns the tab to the connect form without closing it.
- [ ] **Recording (feature `recording` only):** `cargo run -p drift-app --features recording`,
      connect, then Debug ▸ Record Session (experimental); after a minute toggle it off and play
      `~/Movies/Drift <profile> <timestamp>.mp4` in QuickTime — it shows the session at the right
      size and duration.

## E2E round 1 — what the automated live-app smoke cannot reach

`cargo xtask e2e` covers the protocol, input, clipboard, display and reconnect behaviour
against the homelab, and `e2e_live_desktop_pixels` (tests/e2e/tests/render.rs) drives the real
Metal compositor against the live `drifttest2` desktop, writing the composite to
`target/e2e/live-desktop-{still,anim}.png` and asserting ≥ 55 fps under `anim.py`.

The *window* itself still needs a person, because three things on the dev Mac are only
available to a session a human has unlocked:

- [ ] **Unlocked screen.** With the Mac's screen locked (`CGSSessionScreenIsLocked = true`),
      every Drift window reports `occlusionState ∌ Visible`, so Drift correctly sends Suppress
      Output and presents 0 fps. Verified during round 1: with `anim.py` running full-screen on
      the host, the app's socket received **0 bytes in 4 s** and the stats line stayed at
      `fps=0.0`. Repeat the smoke on an unlocked screen and confirm the stats line climbs to
      ~60 fps within a second of the window becoming visible.
- [ ] **Screen Recording permission.** `screencapture` fails with "could not create image from
      display" from an SSH shell *and* from a launchd agent in the Aqua session. Take the
      screenshot of the connected window as the logged-in user and check: GNOME's wallpaper
      gradient is blue (not orange — that would mean swapped R/B), blacks are black and the
      colour-bar test image (`/home/drifttest2/clip_in.png`) shows saturated primaries.
- [ ] **Keychain partition list.** A password written by `/usr/bin/security` (or by a *different*
      build of Drift) carries the ACL partition `apple-tool:`, so the app gets the "Drift wants
      to use your confidential information" panel the first time it reads it, and the session
      actor waits for the click. Save the profile *through Drift's own connect form*, with the
      stable "Apple Development" signing identity from plan §5.1, and confirm that relaunching
      and connecting never shows that panel. The unattended smoke harness
      (`cargo run -p drift-app --example smoke`, `DRIFT_SMOKE_SECRET=<uuid>:<role>:<password>`)
      exists only because this step cannot be scripted.

Repeatable setup for the manual run (round 1 used exactly this):

```bash
ssh -N -L 23392:localhost:3392 -L 23389:localhost:3389 homelab@10.1.2.40 &
DRIFT_CONFIG_DIR=/tmp/drift-smoke DRIFT_AUTOCONNECT="Homelab Headless" \
  cargo run -p drift-app --bin drift-app           # profiles.toml pins the daemon's fingerprint
```
