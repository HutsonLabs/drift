# Manual acceptance checklist

Steps that cannot be automated (they need a human, a GUI permission prompt, physical network
changes or a clean machine). Each milestone's owner appends its "Done (manual …)" items here.

## M0 — Foundation

- [ ] `cargo tauri dev` opens a window titled "Drift" showing the connections screen ("Add a GNOME computer" on first launch).

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

## M1-5 / M4-3 — Rendering (drift-render)

- [ ] With a live Headless session (drifttest2), the desktop colours match the host monitor
      (compare a GNOME Settings window side by side: no tint, blacks are black, whites white).
- [ ] At a Retina drawable equal to the desktop size (Retina on, window not resized) terminal
      text is pixel-sharp (no filtering blur); after resizing to a non-matching size the picture
      scales smoothly with black bars, and snaps back to sharp once DISP re-layout completes.
