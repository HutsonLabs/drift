# Manual acceptance checklist

Steps that cannot be automated (they need a human, a GUI permission prompt, physical network
changes or a clean machine). Each milestone's owner appends its "Done (manual …)" items here.

## M0 — Foundation

- [ ] `cargo tauri dev` opens a window titled "Drift" showing "Drift / Version 0.1.0".

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
