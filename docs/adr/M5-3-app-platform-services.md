# M5-3 / M5-2 / M7-2 — One app-lifetime owner for the pasteboard watcher and the reconnect triggers

- Status: accepted
- Date: 2026-09-22
- Code: `src-tauri/src/services.rs`, `src-tauri/src/lib.rs` (`run_with` setup),
  `src-tauri/src/manager.rs` (`SessionManager::live_session_states`)

## Context

`drift_clipboard::poll::PasteboardWatcher` (M5-3) and `drift_macos::ReconnectTriggers` (M7-2)
were implemented and unit tested in their crates, but nothing in the shipped app ever
constructed them. The only senders of `SessionCommand::ClipboardLocalChanged`,
`NetworkReachable` and a platform-originated `ReconnectNow` were tests, so two advertised
features were dead end to end: copying on the Mac never reached GNOME, and "Wi-Fi on reconnects
within ~1 s" / "after wake it reconnects immediately" (docs/acceptance.md) could not pass —
Drift simply waited out the backoff.

Both observers are per-machine, not per-session: there is one NSPasteboard and one network
path, and a session must not miss a copy made while another tab had focus.

## Decision

- **One owner, `services::PlatformServices`**, created in `run_with`'s `setup` and kept in
  Tauri managed state. Dropping it stops both observers.
- **The pasteboard is polled on the main thread.** `PollTimer` is a plain thread that waits
  `POLL_INTERVAL` on an mpsc channel and then dispatches one tick through
  `windows::on_main`; because `on_main` blocks until the main thread has run it, ticks cannot
  pile up. `NsPasteboard` is not `Send`, so the `ClipboardPump` lives in a main-thread
  `thread_local!` and the timer thread only triggers it. Dropping the `PollTimer` drops the
  channel's sender, which wakes the thread immediately; the thread is never joined, so quitting
  can never block on it.
- **One read per change, at the most permissive level** across live sessions
  (`services::most_permissive`), so images are decoded only when at least one profile is
  `TextAndImages`. `ClipboardPrefs::Off` sessions are skipped entirely; per-session filtering
  and the M5-2 focus scoping stay where they were, in each actor's `ClipboardSync`.
- **A session that opens later is seeded** with `PasteboardWatcher::snapshot`, so its first
  format list already offers what the user copied before connecting. A window whose session
  ended and came back is seeded again (its new actor knows nothing). Without this, a copy made
  while no session was live would be lost, because the watcher consumes the `changeCount`
  change either way.
- **`TriggerAction` → commands** (`services::trigger_commands`):
  `PauseReconnect` → `NetworkReachable(false)` for every live session;
  `RetryNow` → `NetworkReachable(true)` for every live session, plus `ReconnectNow` **only for
  a session in `SessionState::Reconnecting`**. `drift_rdp::actor::idle` reconnects on
  `ReconnectNow`, so broadcasting it would restart a session the user cancelled on the
  reconnect overlay (M7-3) and retry an `AuthFailed` session after every wake.
  `NetworkReachable(true)` alone already resumes a session that is waiting out a backoff
  (`actor::backoff` returns `Resume::Reconnect`), so nothing is lost.
- **Two ports keep it testable.** `PasteboardPort` (already in drift-clipboard) and the new
  `SessionFanout` (`live_sessions` + `send`, implemented for `SessionManager`) mean the whole
  service is covered by `src-tauri/tests/platform_services.rs` with fakes — no AppKit, no Tauri,
  no real actor. The only untested glue left is `PlatformServices::start`: three constructor
  calls.

## Consequences

- Drift polls `changeCount` every 250 ms for as long as it runs, even with no session open.
  That is one Objective-C message per tick and is what plan M5-3 specifies; nothing is read
  unless a session wants it.
- `SessionManager` gained `live_session_states()`. Note that the `SessionFanout::live_sessions`
  trait method is shadowed by the manager's inherent `live_sessions() -> usize`; call it as
  `SessionFanout::live_sessions(&manager)`.
- The manual acceptance items for Wi-Fi and wake (docs/acceptance.md) are now reachable; they
  stay manual because they need a human to toggle Wi-Fi and sleep the Mac.
