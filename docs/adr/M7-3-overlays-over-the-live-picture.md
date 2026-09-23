# M7-3 / M1 — Drawing over the live picture (transparent webview, overlays and HUDs)

Status: accepted

## Context

Two plan items need something painted **over** the remote desktop:

* **M7-3** — on a reconnect Drift keeps "the last frame dimmed under an overlay:
  *Reconnecting in N s… [Now] [Cancel]*".
* **M1 "Done (manual M1)"** and **M9-1** — `anim.py` must show "≥ 55 fps in the **stats
  overlay**". The E2E notes call the same thing "the stats line".

Neither existed:

* `present::surface_for` mapped `Screen::Reconnecting` to `Surface::Webview`, i.e. a webview
  panel *instead of* the picture. `ui/src/styles.css` already made the page background
  transparent for the `reconnecting` / `greeter-hint` / `live` screens ("Overlays drawn over the
  remote picture: the page itself is transparent"), but nothing ever made the `WKWebView`
  non-opaque, so those rules were dead: `tauri.conf.json` had `"macOSPrivateApi": false`,
  `src-tauri/Cargo.toml` had `tauri = { features = [] }`, and the window builder never called
  `transparent(true)`. wry 0.55.1 only clears `drawsBackground` / calls `setOpaque(false)` under
  its `transparent` feature, which Tauri gates behind `macos-private-api`.
* `SessionEvent::Stats(SessionStats { fps, .. })` was consumed by a `tracing::trace!` in
  `host.rs` and thrown away. `SessionView` had no statistics field, no screen rendered one, and
  the Metal compositor draws no text — so the M1 manual acceptance step and M9-1's in-app fps
  check could not be performed at all.

## Decision

### 1. Session windows are transparent

`tauri.conf.json` sets `"macOSPrivateApi": true`, `src-tauri/Cargo.toml` enables the matching
`tauri` feature `macos-private-api`, and `WebviewWindowBuilder` gets `.transparent(true)`. The
`RemoteView` (`CAMetalLayer`) already sits **below** the `WKWebView` in the same superview, so a
transparent page simply shows the last presented frame through it. Screens that are not overlays
(`Surface::Webview`) hide the `RemoteView` instead and show the window's native vibrancy
(`.effects(UnderWindowBackground)`) through glass panels drawn in CSS (glass redesign,
`docs/design/mockup-glass.html`).

**Cost:** `drawsBackground` is a private KVC key on `WKWebView`, so an App Store submission would
be rejected. Drift ships as a Developer ID–signed, notarised `.dmg` (plan M9-5) and the plan
never mentions the App Store, so this is accepted. If Drift is ever submitted to the App Store,
this decision has to be revisited — the reconnect overlay would fall back to an opaque panel and
the statistics HUD would have to move into the Metal composite.

### 2. Three ways to be in front, not two

`present::Surface` grows from `{ Webview, Remote }` to:

| Surface | AppKit | Keyboard |
|---|---|---|
| `Webview` | the web view fills the window; nothing to see behind it | web view |
| `Remote` | the web view is hidden | `RemoteView` |
| `Overlay` | the web view fills the window and paints a wash plus a card over the last frame | web view |
| `Hud(Banner \| Stats)` | the web view is **shrunk** to `present::hud_frame` | `RemoteView` |

`Overlay` is the reconnect screen: there is nothing to type into the remote desktop while it is
disconnected, so the web view may own the keyboard and the whole window.

`Hud` is the hard case: a live picture must keep every key and every click. CSS `pointer-events:
none` does not help — AppKit hit-tests *views*, and a visible `WKWebView` in front swallows the
mouse whatever the page says. So the web view itself is resized to the panel
(`NSView::setFrame` plus an autoresizing mask that pins it to its corner), `RemoteView` is made
first responder again, and AppKit hit-tests everything outside the panel straight down to the
picture. The panel geometry is a pure function, `present::hud_frame(parent, hud, flipped)`:

* `Hud::Banner` — the greeter hint, ≤ 560 × 76 pt, centred against the top edge. This finally
  uses `ui/src/views/greeter.ts` and its CSS, which the plan describes as "a non-modal hint
  banner over the picture" (`Screen::GreeterHint`). The window subtitle keeps the same text.
* `Hud::Stats` — 260 × 44 pt, inset 16 pt from the **bottom-right** corner: GNOME's top bar and
  its dash are elsewhere, so the panel covers nothing the user clicks.

`flipped` is the superview's `isFlipped`, because AppKit's default coordinate space has its
origin bottom-left. The function returns screen-direction "flexible" margins, which
`windows::autoresize_mask` maps to `NSAutoresizingMaskOptions` — that mapping is the only part
that knows about AppKit.

### 3. Statistics reach the view, quantised

`SessionView` gains `stats: Option<StatsView>` and `show_stats: bool`:

* `StatsView::sample` rounds every number to **tenths of its displayed unit** and stores it as
  `u32` (`fps_tenths`, `mbit_tenths`, `latency_p95_tenths_ms`, `unacked_frames`). Rounding at the
  boundary means two samples that would draw the same line compare equal, so the once-a-second
  sample does not re-emit the view; integers also keep the generated TypeScript honest, because
  specta maps `f32` to `number | null` (serde_json writes a non-finite float as `null`).
* `show_stats` is a per-tab user choice (**Session ▸ Show Statistics**, no shortcut — every other
  Command combo belongs to the remote desktop). No session event ever changes it.
* The HUD only appears once there is a sample to draw, so an empty panel never covers the corner.
* The sample is dropped whenever the state leaves `Connected` / `AwaitingGreeterLogin`, so stale
  numbers never linger under the reconnect overlay.

`ui/src/views/stats.ts` renders one line — `58.9 fps · 1.2 Mbit/s · 4.3 ms · 2 unacked` — in a
`role="status"` region with no controls.

## Consequences

* No App Store distribution (see above). Recorded here so M9-5 does not rediscover it.
* `present::surface_for` now takes the whole `&SessionView`, not just the `Screen`, because the
  HUD depends on the statistics toggle as well.
* Everything except two `setFrame`/`setAutoresizingMask` calls stays pure and unit-tested:
  `present::{surface_for, hud_for, hud_frame}` and `view::StatsView::sample`.
* `src-tauri/tests/overlays_ui.rs` (feature `macos-ui-tests`, like `tabs_ui`) checks the AppKit
  half on a real window: the window is not opaque, the reconnect overlay fills the window and
  owns the keyboard, the statistics HUD is a corner panel with `DriftRemoteView` as first
  responder, and the plain live screen hides the web view again. It is not part of
  `cargo xtask ci` because it needs a logged-in window server.
* Known adjacent gap, **not** fixed here: while the state is `AwaitingGreeterLogin` the view has
  no desktop size, so `windows::apply_view` calls `RemoteView::clear_desktop()` and pointer
  events at the GDM greeter are dropped. Typing works (that is what M3-2 needs); clicking does
  not. That is M3-2's area, not this one.
