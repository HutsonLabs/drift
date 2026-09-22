# M9-4 — VoiceOver labels, the RemoteView's accessibility role, and the app icon

Status: accepted (2026-09-22)

## Context

Plan M9-4 asks for "VoiceOver labels; mode-specific errors with next steps" and Drift still
shipped the scaffold's placeholder app icon. The mode-specific error texts already existed
(`drift_core::messages`, landed with M1-6 and table-tested); what was missing was the
*announcement* side: a window whose whole content is a picture of another computer, and screens
whose explanatory text was never read out.

## Decision

### The live picture is one accessibility element with an image role

`RemoteView` is a layer-hosting `NSView`. Left alone, VoiceOver reads it as an unnamed "group"
that contains nothing. It now answers:

| Method | Value |
|---|---|
| `isAccessibilityElement` | `YES` |
| `accessibilityRole` | `NSAccessibilityImageRole` |
| `accessibilityRoleDescription` | `"remote desktop"` |
| `accessibilityLabel` | `"<profile> — remote desktop, <w> by <h> pixels"` |
| `accessibilityHelp` | "Keyboard, pointer and scroll input go to the remote computer …" |
| `accessibilityChildren` | none |

An **image** is the honest role: the contents are pixels from another machine, and their own
accessibility tree lives in that machine's GNOME session (Orca reads it there, not here). The
label is what tells two session tabs apart, so it is recomputed from the `SessionView` in
`present::accessibility_label` — pure and unit-tested — and pushed to the view by the window
glue, next to the title and subtitle.

Drift deliberately does **not** claim to expose the remote UI to VoiceOver. Bridging AT-SPI to
NSAccessibility is not in scope for v1 (plan §9), and pretending otherwise would be worse than
an honest image.

### A hint nobody points at is not read

The webview screens are audited by `ui/test/accessibility.test.ts`, which walks each rendered
screen and fails on: a control with no accessible name, an `aria-*` reference that resolves to
nothing, a duplicate id, a decorative glyph that is not `aria-hidden`, and — the rule that
found real gaps — **a `.hint` paragraph that no `aria-describedby` points at**. Visible helper
text that is not referenced is invisible to VoiceOver, which is how these shipped:

* the connection-type explanation ("Log in at the GNOME login screen, as if you were at the
  computer…") was floating text next to the radio group;
* the certificate dialog asked the user to verify a fingerprint but never read out *how* ("On
  the host, run `sudo grdctl --system status`…");
* the reconnect overlay announced "Reconnecting in 4 s…" without saying which connection or
  which attempt.

Each is now referenced from the element that owns it. The statistics HUD stays a
`role="status"` with no focusable child, so the picture keeps the keyboard.

### The app icon

`src-tauri/icons/icon.svg` is the source of truth: three drifting waves on deep water, drawn on
the macOS icon grid (1024 canvas, 824 rounded square, corner radius 185). The bundled PNGs and
`icon.icns` are generated from it with `cargo tauri icon`, which also writes Windows, Android
and iOS artefacts; those are deleted, and `xtask::icons` fails the build if any of them comes
back, if a bundled size is wrong, or if `tauri.conf.json` bundles a file that does not exist.

## Consequences

* `drift-macos` exposes `RemoteView::set_accessibility_label` plus the `ROLE_DESCRIPTION`,
  `ACCESSIBILITY_HELP` and `DEFAULT_ACCESSIBILITY_LABEL` constants.
* The `.hint` rule applies to every future screen: give the hint an id and point at it.
* VoiceOver's *spoken* output cannot be asserted in an automated test — the checks here are
  structural. `docs/acceptance.md` gains a manual pass with VoiceOver on.
