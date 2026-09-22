# M4-2 / M5-2 / M6-3 / M7-3 — the session actor's channels, timers and resume

- Status: accepted
- Date: 2026-09-22
- Code: `crates/drift-rdp/src/{actor,resize,clipboard,greeter,pointer,stats,graphics}.rs`,
  `crates/drift-testkit/src/{fake_channels,fake_server}.rs`, `tests/e2e/`

## Context

M1-1/M3-1 left the actor with connect, the redirect loop and fast-path input. The remaining
stream-A tasks hang everything else off the same Tokio task: Display Control (M4-2), CLIPRDR
(M5-2), Suppress Output (M6-3), auto-reconnect (M7-3), the greeter hygiene of M3-4 and the
session-reuse evidence of M3-3. Several details were open, and the real host answered a few of
them differently than expected.

## Decisions

1. **One task, one 25 ms tick.** The leg loop selects over: the transport, the GFX ack wake-up,
   a `watch` on the frame sink's output size, the command channel, and a 25 ms interval that
   drives every clock-based timer (resize debounce, greeter typing, clipboard timeouts,
   statistics). A single low-frequency tick keeps `ManualClock` tests deterministic and costs
   less than a frame of work per second; hidden tabs stay far below the 2 % CPU budget because
   the tick body is a handful of comparisons.
2. **Resize (`resize::ResizeDriver`).** 250 ms trailing debounce on the injected clock; a layout
   is sent only when `desired_layout` differs from what the server has (connect size, or the
   last layout sent); changes that arrive before the `DISPLAYCONTROL_CAPS_PDU` are held and
   flushed as soon as the channel is ready. Non-adaptive profiles and Desktop Sharing never
   send: `SessionCapabilities { display_control, scale_mode }` is emitted at activation from the
   **mode** (plan §1.2), so Desktop Sharing goes to `ScaleMode::Fit` without waiting for a
   channel that will never open. `ScaleMode::Fit` is what the app always gets today (with an
   adaptive desktop it is an exact 1:1 mapping); the field exists so a future "native pixels"
   profile option can switch it without another interface change.
   The first leg already connects with the view's geometry: the actor spends 20 ms at startup
   collecting the app's initial `Resize`/`Focus`/`SetVisible`, so a Retina tab opens at
   2560×1600 scale 200 instead of resizing right after activation.
3. **Desktop size comes from the graphics pipeline.** `ResetGraphics` reaches the actor through
   a `watch` channel fed by the shared frame sink, and updates `Connected { desktop }`. The
   server is the source of truth (it rounds the width down to an even value, plan §1.4).
4. **Clipboard (`clipboard.rs`).** The `CliprdrBackend` only queues callbacks; the actor drains
   them after each `ActiveStage::process` and feeds `drift_clipboard::ClipboardSync`, whose
   actions it executes. The initial format-list request is always answered (plan §1.6), the
   temporary-directory PDU is skipped, and a fresh `ClipboardSync` is built per leg (CLIPRDR
   restarts with every connection) seeded with the last local contents. Clipboard payloads are
   never logged, only the kind of input.
5. **Suppress Output (M6-3).** `SetVisible(false)` sends `SuppressOutput { desktop_rect: None }`
   and then switches the GFX client to hidden (suspend acks + `FrameSink::set_visible(false)`);
   `SetVisible(true)` resumes acks first and then allows output with the full desktop rectangle,
   so the full frame g-r-d sends is acknowledged normally. A tab that is hidden before the first
   activation suppresses as soon as the leg is up.
6. **Reconnect (M7-3).** `ReconnectPolicy` lives in the actor. A connection that **never
   activated** does not retry: the user sees the error and the overlay's "Try again" sends
   `ReconnectNow` (auto-retrying an unreachable host or a stopped daemon only hides the
   problem). After an activation, retryable drops go to `Reconnecting { attempt, next_in }`,
   waiting on the injected clock; `Cancel` stops the timers (`Disconnected{reason}`),
   `ReconnectNow` skips the backoff, `NetworkReachable(false)` pauses it and `true` retries at
   once. A non-retryable failure during a reconnect ends in `Failed` with no further attempt.
   Every activation after the first sends `ReleaseAll` + the last `SyncToggles` (the encoder is
   kept across legs, so keys held when the transport died are released on the new one).
7. **Greeter typing (`greeter::GreeterTypist`, M3-2/M7-3).** The opt-in Linux password is typed
   only into a **focused password field**: the typist arms when the greeter appears (a
   `linux-login` secret exists), waits for the user's first left-click (which is how the GDM
   user tile is selected and the password field takes focus), waits `TYPE_DELAY` = 1.5 s and
   then types the password as Unicode events plus Enter, once per greeter. Without the click
   nothing is typed, so a password can never end up in the "Not listed?" user-name field
   without the user having selected a tile.
8. **FakeServer grew server-side channels** (`fake_channels.rs`): DRDYNVC with a Display Control
   server (records monitor layouts) and a graphics pipeline (confirms caps, answers every layout
   with `ResetGraphics` + a **new surface id**, records frame acks and suspends), plus CLIPRDR
   with a scriptable clipboard. Leg records now also carry the decoded fast-path input events,
   Suppress Output rectangles and the GCC desktop size, which is what the M2-4, M4-2, M5-2,
   M6-3 and M7-3 loopback tests assert.

## Real-host findings (they shaped the e2e suite)

- **`cargo xtask e2e` runs with `--test-threads 1`.** The tests share one host and one daemon
  per mode; parallel connections make g-r-d drop legs mid-handshake.
- **A locked GNOME session silently swallows everything.** With the screensaver active, input
  events do nothing and clipboard changes are not propagated — it looks exactly like a broken
  client. `host::ensure_unlocked_session` turns the idle lock off (`gsettings`) and restarts the
  headless session when it is locked, because GNOME cannot be unlocked over D-Bus without the
  user's password.
- **A Wayland client needs focus *and* a fresh input serial to take the clipboard.** Setting the
  clipboard from a launched helper's startup or focus callback never reaches mutter;
  `host/cliptool.py` therefore sets it from a key-press handler, and the test clicks the window
  and presses a key. That is also why the remote-copy tests drive real apps (GNOME Text Editor
  with Ctrl+A/Ctrl+C).
- **Print Screen does not open the screenshot UI in the headless session**, so the remote image
  copy uses `host/cliptool.py` instead of the GNOME screenshot tool from plan §1.6.
- **g-r-d turns RDP Unicode events into XKB keysyms** (`xkb_utf32_to_keysym`), and mutter only
  injects keysyms the session's layout can produce. On the US-layout test session `Grüße ✓`
  never arrives, while ASCII — including shifted characters typed *without* sending Shift —
  does. `e2e_unicode_typing` asserts the latter; the non-ASCII case is a manual acceptance step
  (`docs/acceptance.md`, M2-2). This refines plan §1.5's "Unicode events are layout-independent".
- The reconnect tests cut the transport by killing **their own** SSH forward
  (`host::SshForward`), so they never disturb the suite's shared forwards.

## Consequences

- `SessionCapabilities` gained `scale_mode` and `SessionOptions` gained `reconnect` /
  `reconnect_seed`; both are additive and the M0-5 ADR's list of commands and events is
  unchanged.
- `host/cliptool.py` and `host/scrolltool.py` are Drift's own e2e helpers; the tests upload them
  to the test user's home, so a re-imaged host needs no manual preparation.
- The 20 ms startup grace delays leg 1 by 20 ms. It is the price for connecting at the right
  size, and it is also what lets a tab that opens hidden suppress output immediately.
