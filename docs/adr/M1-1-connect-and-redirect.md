# M1-1 / M3-1 — Connect, certificate trust, redirect loop and the loopback FakeServer

- Status: accepted
- Date: 2026-09-22
- Code: `crates/drift-rdp/src/{connect,tls,redirect,rdstls,actor,fastpath,gfx_ack}.rs`,
  `crates/drift-testkit/src/{fake_server,e2e}.rs`, `tests/e2e/`, `xtask/src/e2e_env.rs`,
  `third_party/ironrdp-patches/0006-…`

## Decisions

1. **TLS stack.** rustls 0.23 via `tokio-rustls` with the **ring** provider passed explicitly
   (`builder_with_provider`), no process-wide provider install. The verifier accepts any chain
   (g-r-d certificates are self-signed) but still verifies the TLS 1.2/1.3 handshake signatures
   with the leaf key. Trust is decided right after the handshake and **before any credential is
   sent** by `connect::check_certificate`: `Pinned(sha256)` (profile TOFU pin),
   `TrustOnFirstUse` (→ `CertificatePrompt`, the actor waits for `AcceptCertificate` /
   `RejectCertificate` / `Close`), or `Exact(der)` (redirect target). Session resumption is off.
   An accepted-but-not-pinned certificate is remembered for the rest of the session only.
2. **Redirect target certificate.** When the redirection PDU carries a target-certificate
   container, the next leaf must be byte-identical to its DER (verified property of g-r-d, and
   checked against the captured fixture). Without a container the profile pin / session pin /
   TOFU prompt (role `RedirectTarget`) applies. A mismatch is `CertMismatch`, and the one-time
   credentials are never sent to that server.
3. **Loop protection.** `MAX_REDIRECTS` (4, drift-core) redirects per connection attempt; the
   fifth is `RedirectLoop` (legs 1..=5 exist, matching `SessionState` validation).
4. **State mapping.** Remote Login: leg 1 active → stay `Connecting{1}`; leg 2 active →
   `AwaitingGreeterLogin`; leg ≥ 3 → `Connected`. Headless / Desktop Sharing: every activation
   is `Connected`. Stages: `Tcp` (dial + X.224), `Tls` (handshake + certificate decision),
   `Nla` or `Rdstls` (CredSSP / RDSTLS, capability exchange and finalization).
5. **Timeouts.** Each network phase is bounded by `SessionOptions::connect_timeout` measured on
   the injected `Clock`, re-checked every 25 ms of real time (`connect::until`). The prompt
   pauses the timer (the deadline restarts after the decision). The graceful-close wait is
   bounded by the clock and a real-time cap, so a frozen `ManualClock` cannot hang an actor.
6. **Error classification** (pure, table-tested): errno 65 → `LocalNetworkDenied`; `ETIMEDOUT` /
   `TimedOut` → `Timeout`; RDSTLS non-zero result → `RdstlsFailed(code)`; CredSSP errors and a
   connection dropped **during NLA** → `AuthFailed` (FreeRDP-based g-r-d closes the socket on a
   wrong password); EOF after TLS → `TlsEof`; other I/O → `Network`; anything else →
   `ProtocolError`. Terminal state: `Disconnected` for retryable reasons and `UserClosed`,
   `Failed` otherwise. Retry/backoff is M7; RDSTLS `0x52E` is never retried.
7. **One-time credentials.** `OneTimeCredentials` holds them in `Zeroizing` buffers; the PDU's
   copies are zeroized and cleared as soon as they are extracted; IronRDP's `RdstlsCredentials`
   now zeroizes itself on drop (fork patch 0006). They are never logged (`Debug` redacted).
8. **Client name** = first label of `gethostname()`, `[A-Za-z0-9-]`, ≤ 15 chars, else `drift`.
9. **Interim graphics listener.** g-r-d terminates a session whose client does not open
   `Microsoft::Windows::RDS::Graphics` ("Failed to open channel … Terminating session",
   `ERRINFO_BAD_CAPABILITIES`, seen in the first e2e run). Until the actor wires
   `drift_gfx::GfxClient` (M1-2, rendering through the tab's `FrameSink`), `gfx_ack::GfxAckOnly`
   advertises exactly `[V8_1{AVC420_ENABLED}, V8{}]` and acknowledges every `EndFrame`; its caps
   bytes and ack frame ids equal the reference client's on the captured sessions.
10. **Close** sends a Shutdown Request and waits ≤ 1 s for Shutdown Denied / disconnect
    (groundwork for M3-4); dropping every `SessionHandle` counts as `Close`. After a terminal
    state the actor stays addressable until `Close` (M7 adds reconnect commands).
11. **FakeServer** (drift-testkit) is built on `ironrdp-acceptor` (the engine of
    `ironrdp-server`) rather than `RdpServer`, because the tests need per-connection scripts:
    RDSTLS selection (the acceptor cannot select it, so the X.224 exchange is done by hand and a
    primed acceptor takes over after TLS), per-leg certificates, redirect injection, stall legs
    and a log of what the client sent. One listener serves consecutive legs on one port, like
    g-r-d's system daemon. Loopback cannot use a second address (only 127.0.0.1 exists on lo0),
    so the "two FakeServers" of the plan are two scripted server personalities (NLA and RDSTLS)
    behind the same port.
12. **E2E.** `drift_testkit::e2e::Script` ports probe2's step language. The default greeter
    login clicks the "Drift e2e test user" tile at (640, 427) of the 1280×800 greeter, types the
    password as Unicode events and presses Enter; `DRIFT_E2E_GREETER_SCRIPT` overrides it (e.g.
    the "Not listed?" path). Two redact layers: the tests' `tracing` output goes through
    `drift_e2e::RedactingWriter`, and `cargo xtask e2e` pipes the whole nextest output through
    `e2e_env::Redactor`. A run at `RUST_LOG=debug` contained no credential value.

## Consequences

- Wiring `drift_gfx::GfxClient` replaces `GfxAckOnly` in `actor::attach_channels` and needs the
  actor to drain `AckOutbox` (see the M1-2 ADR).
- Display Control, clipboard, Suppress Output and reconnect commands are accepted by the actor
  but ignored until M4-2, M5-2, M6-3 and M7.
