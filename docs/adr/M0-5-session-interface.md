# M0-5 — Session actor public interface

- Status: accepted
- Date: 2026-09-22
- Code: `crates/drift-rdp/src/session.rs`

## Context

The session actor (stream A, M1-1 onward) and the app's `SessionManager` / UI (stream D, M1-6,
M6-1) must be built in parallel. They need a fixed seam before either exists.

## Decision

`drift-rdp` exposes exactly:

```rust
pub fn spawn_session(
    profile: ConnectionProfile,
    secrets: SessionSecrets,          // Zeroizing passwords from the Keychain; Debug redacts
    frame_sink: Box<dyn FrameSink>,   // the tab's Metal compositor (drift-render)
    clock: Arc<dyn Clock>,            // ManualClock in tests
    options: SessionOptions,          // tls_server_name override, client_name, connect_timeout
) -> (SessionHandle, SessionEvents);
```

- `SessionHandle` — `Clone`, wraps `tokio::sync::mpsc::UnboundedSender<SessionCommand>`;
  `send()` never blocks (safe on the AppKit main thread) and returns `Err(SessionClosed)` after the
  actor exits. `SessionHandle::from_sender` lets app tests build fake actors.
- `SessionEvents` = `UnboundedReceiver<SessionEvent>`; yields `None` after the actor exits.
- `SessionCommand`: `Input(InputEvent)`, `Resize(ViewGeometry)`, `SetVisible(bool)`,
  `Focus(bool)`, `ClipboardLocalChanged(ClipboardContents)`,
  `AcceptCertificate { fingerprint, pin }`, `RejectCertificate`, `ReconnectNow`,
  `NetworkReachable(bool)`, `Cancel`, `Close`.
- `SessionEvent`: `State(SessionState)`, `CertificatePrompt { host, port, fingerprint, role }`,
  `CertificatePinned(CertFingerprint)`, `Capabilities(SessionCapabilities)`,
  `Cursor(CursorUpdate)`, `ClipboardRemote(ClipboardContents)`, `Stats(SessionStats)`.

Rules:
1. Every state change is validated by `SessionState::transition` before `State` is emitted.
2. Pixels never cross the channel: the actor drives the `FrameSink` on the session render thread;
   GFX `FrameAcknowledge` is sent from the `presented` callback (plan §1.4).
3. The app persists pins when it sees `CertificatePinned`; the actor never writes profiles.
4. One-time redirect credentials never leave the actor (zeroized after use).
5. `Close` performs the graceful shutdown (M3-4) and ends with `State(Disconnected{UserClosed})`.
6. Unbounded channels are deliberate: commands are tiny and rate-limited by human input; events
   are rate-limited by the actor (stats ≈ 1 Hz, cursor/clipboard on change).

## Consequences

- `spawn_session` is a documented `todo!()` until M1-1; the app can be developed against a fake
  actor built from `SessionHandle::from_sender` + an event sender.
- Adding a command/event variant is a cross-stream change: update this ADR in the same PR.
