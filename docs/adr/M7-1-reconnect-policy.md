# M7-1 / M7-2 — Reconnect policy and trigger merging details

- Status: accepted
- Date: 2026-09-22
- Code: `crates/drift-core/src/reconnect.rs`, `crates/drift-core/src/triggers.rs`

## Context

Plan M7-1: exponential full-jitter backoff (base 500 ms, ×2, cap 30 s), "unlimited while the
network is reachable, with a user-visible cap (default 20)", reset after 60 s stable,
retryability from `DisconnectReason`. Plan M7-2: offline pauses, online and wake retry
immediately, duplicates are debounced. Some details were open.

## Decision

- **RNG:** a built-in SplitMix64 (published reference outputs are asserted in tests), seeded by
  the caller. Delay for attempt *n* is `(r × (ceiling_n + 1)) >> 64` ms, i.e. uniform in
  `[0, ceiling_n]`; `ceiling_n = min(30 s, 500 ms × 2^(n-1))`. No `rand` dependency.
- **Budget semantics:** attempts are only made and counted while the network is reachable.
  While unreachable the policy answers `WaitForNetwork` (the actor waits for the trigger
  merger's `RetryNow`), so an outage of any length never exhausts the budget. The budget itself
  (`max_attempts`, default 20, `None` = unlimited) is shown in the overlay ("Attempt 3 of 20").
- **`retry_now()`** ("Now" button, network back, wake) consumes an attempt with zero delay and
  does not draw from the RNG, so seeded delay sequences stay reproducible.
- **Reset:** `on_connected(now)` records the start; the next `on_disconnect` resets the count
  if at least 60 s passed. Non-retryable reasons return `GiveUp(NotRetryable)` without
  consuming an attempt.
- **Trigger merger:** duplicate `NetworkOffline`/`NetworkOnline` are dropped; `RetryNow` is
  debounced for 2 s (wake and path updates usually arrive together); `Wake` while offline does
  nothing (the retry comes with `NetworkOnline`); a real outage clears the debounce so the
  following `NetworkOnline` always retries.

## Consequences

The actor (stream A) owns timers and applies decisions; everything decision-making is pure and
table-tested in `drift-core`.
