# M2-1/M2-2/M2-3/M2-4 — Pure input translation (`drift-input`)

- Status: accepted
- Date: 2026-09-22

Plan §2.4 fixes *what* is sent (scancodes by default, Unicode with "Type using Mac layout").
This ADR records the decisions `plan.md` leaves open. All of them live in the pure
`drift-input` crate; `drift-macos` (RemoteView) only extracts plain values from `NSEvent`s and
forwards the returned `InputEvent`s.

## Keyboard (M2-1)

1. **Single stateful translator.** `drift_input::Keyboard` receives `keyDown:`/`keyUp:`/
   `flagsChanged:`/`insertText:`/focus callbacks and returns exact `InputEvent`s. It tracks the
   *wire* state as a reference-counted set of pressed scancodes, so the remote never sees a double
   press or a release of an unpressed key, even when two Mac keys share a scancode (Control and
   Command-as-Ctrl). A proptest drives arbitrary callback sequences and checks this.
2. **Table.** All 120 `kVK_*` constants of the macOS 27 SDK (`HIToolbox/Events.h`) are listed in
   `keymap::ALL_KVK` and covered by a table test. Unmapped: `kVK_Function` (handled by macOS) and
   `kVK_ANSI_KeypadClear` (sits on Num Lock, which Drift keeps on). `Help` → Insert (`E0 52`),
   `ContextualMenu` → Apps (`E0 5D`), F13–F20 → `0x64..0x6B`, volume keys → `E0 30/2E/20`.
3. **ISO swap.** On ISO keyboards macOS reports the key left of `1` as `kVK_ISO_Section` and the
   key right of left Shift as `kVK_ANSI_Grave`. With `KeyboardType::Iso` (from
   `KBGetLayoutType(LMGetKbdType())`) they map to `0x29` and `0x56` respectively, preserving the
   physical position. ANSI/JIS use the plain table. *Verify on a real ISO keyboard* (acceptance).
4. **JIS.** `¥` → `0x7D` (International3), `_`/`ろ` → `0x73` (International1), keypad `,` →
   `0x7E`, `英数` → `0x71` (LANG2/Hanja), `かな` → `0x72` (LANG1/Hangul) — the same keycodes Linux's
   `hid-apple` produces for Apple JIS keyboards, so a GNOME Japanese IME sees what it would locally.
5. **Deferred Command.** The Command press is sent only when another key or a pointer button is
   used with it, or as a tap (press+release) on release when used alone. When
   `performKeyEquivalent:` gives an allow-listed combo to the menu, `menu_shortcut_taken()` cancels
   the deferred press. So Cmd+T/Cmd+Q/Cmd+Tab never leak a Super tap (which would toggle the GNOME
   overview) and the M2-4 guarantee "the server never sees an allow-listed combo" holds. Applies to
   both `CmdAs` settings. Focus loss drops a deferred Command silently.
6. **Auto-repeat** is not re-sent as scancodes: g-r-d drops repeated presses and Wayland clients
   repeat held keys themselves.
7. **Lock keys.** Caps Lock is never sent as a key. Its state goes out as `SyncToggles` when it
   changes and on every focus gain. **Num Lock is always reported on** (Mac keypads type digits).
8. **Focus loss** returns explicit key releases for exactly the scancodes the remote considers
   pressed (not `InputEvent::ReleaseAll`, which the session actor may still use for buttons /
   reconnect). Focus gain re-presses the modifiers currently held.
9. Missed modifier changes are resynchronised from the flags carried by each scancode `keyDown:`.

## Unicode typing (M2-2)

10. With `type_with_mac_layout`, `route_key_down` returns `Text` for text keys without Control/
    Command (and for every key while an IME composition is active); the view then calls
    `interpretKeyEvents:` and forwards `insertText:` to `Keyboard::insert_text`. Non-text keys and
    chords go as scancodes; `doCommandBySelector:` falls back to `key_down_scancode`.
11. Each UTF-16 unit is sent as press immediately followed by release (g-r-d de-duplicates pressed
    keysyms, so `ll` must not overlap). Characters outside the BMP go as two units, as the plan
    requires. **Note:** g-r-d 50.2 converts every unit on its own (`g_utf16_to_ucs4(&unit, 1)`), so
    lone surrogates are dropped server-side; emoji will not appear until g-r-d handles pairs. The
    e2e test (`e2e_unicode_typing`, `Grüße ✓`) uses BMP characters only.
12. Held Shift/Option/AltGr scancodes are released around Unicode events and pressed again
    afterwards, so mutter's keysym injection is not shifted/Alt-modified (e.g. Option+s → `ß`).

## Pointer and scroll (M2-3)

13. `Viewport` (origin + points-per-pixel) is the single description of where the desktop is drawn;
    `drift-render` should use the same value for its transform. View points are top-left origin.
    `Fit` scales uniformly (up or down) and letterboxes; `OneToOne` is one desktop pixel per backing
    pixel, centred when smaller, top-left anchored when larger. Points over a bar clamp to the
    nearest edge pixel.
14. Scroll: precise deltas × `units_per_point` (default 2), lines × 120 (events of at most one
    notch), remainder carried per axis (exact for binary-fraction deltas, proptested), events split
    at ±255, at most 64 events per axis per `scrollWheel:`. AppKit deltas already include the
    natural-scrolling preference, so vertical passes through (`+` = up) and horizontal is negated
    (`+dx` = towards the left, RDP HWHEEL `+` = right). A `reverse` option flips both. The final
    HWHEEL sign against g-r-d is locked by `e2e_hscroll`.

## Shortcuts (M2-4)

15. `menu_shortcut` matches the allow-list by `charactersIgnoringModifiers` (Dvorak-aware), falling
    back to the ANSI key position for non-ASCII layouts; Cmd+1…9 also match by position (AZERTY).
    Exactly Command (+Shift for `[`/`]`) is required; Control or Option makes it a remote combo.
