# E2E-1 — How the live-app smoke is run (and what it cannot prove)

Status: accepted (e2e round 1)

## Context

Plan M1-6's "Done (manual M1)" asks for a live, correctly coloured desktop in the app window and
`anim.py` at ≥ 55 fps. Running that unattended on the dev Mac hits three walls:

1. **Screen lock.** A locked screen makes every window `occlusionState ∌ Visible`, so Drift does
   the right thing (M6-3: Suppress Output, pause the render thread) and no frame ever arrives.
2. **Screen Recording.** `screencapture` is refused both from an SSH shell and from a launchd
   agent in the Aqua session, so the window cannot be photographed by a script.
3. **Keychain ACL partitions.** A generic-password item written by `/usr/bin/security` carries the
   partition list `apple-tool:`. Any other binary reading it gets an authorization panel, and an
   unattended run blocks on it forever — this is how the launch hang in `SecretStore::has` was
   found.

## Decision

Split the evidence in two.

**Automated, every nightly run:** `e2e_live_desktop_pixels` (`tests/e2e/tests/render.rs`) builds
the production render wiring — one `Gpu`, a `Compositor` on a `RenderThread` — over an
`OffscreenTarget` instead of a `LayerTarget`, hands the session actor that sink through the new
`E2eSession::start_with_sink`, and reads the composite back to `target/e2e/*.png`. Everything
except the `CAMetalLayer` is the shipping code path: GFX state machine, VideoToolbox decode,
the BT.709 full-range shader, the compositor and the ack policy. It asserts the picture is not
blank, that it changes while `anim.py` runs, and that the session's own frame-rate statistic
reaches the plan's ≥ 55 fps (first run: peak 60.5 fps at 1280×800).

**Manual, per milestone:** the three window-level checks above, in `docs/acceptance.md`.

**Unattended harness:** `cargo run -p drift-app --example smoke` runs the real app with the
passwords supplied through `DRIFT_SMOKE_SECRET=<uuid>:<role>:<password>` and kept in a
`MemorySecretStore` (`RunOptions::secrets`). The shipped `drift-app` binary always uses the
Keychain; `RunOptions::from_vars` still refuses to switch it off from the environment. The
harness proved, on the real app against the homelab:

- Headless (:3392 forwarded): NLA, capabilities, `display_control=true`, `clipboard=true`,
  a live socket, and — with the screen locked — `fps=0.0` plus **0 bytes received in 4 s** while
  `anim.py` ran full-screen. That is Suppress Output working, not a stall.
- Remote Login (:3389 forwarded): leg 1 `HYBRID`, `following Server Redirection leg=2`, leg 2
  `RDSTLS`, clipboard channel ready, `GFX capabilities confirmed V8_1{AVC420_ENABLED}` — the
  greeter, reached by the real app. Stopping it released its greeter session.

## Consequences

- The GPU path has real-host regression cover that does not need a person or an unlocked screen.
- `SecretStore::has` may never be implemented in terms of `get` (there is no default any more):
  on macOS that is the difference between a metadata lookup and a modal panel on the main thread.
- The smoke harness is an example, not a binary target, so it is built by CI (`--all-targets`)
  but never shipped.
