# Team conventions

Read `plan.md` §0–§5 first; it is the source of truth. This page summarises how we work.

## Branches, commits, merges

- One branch per task: `task/<ID>-<slug>` (e.g. `task/M1-2-gfx-state-machine`).
- Strict TDD (plan §0):
  1. write the task's **Red** tests, run them, confirm they fail for the right reason, commit
     `test(<ID>): …`;
  2. implement until green, commit `feat(<ID>): …`;
  3. refactor, commit `refactor(<ID>): …`; `cargo xtask ci` must pass.
- Commit messages end with a blank line and `Co-Authored-By: Claude <noreply@anthropic.com>`.
- **Nothing is pushed.** The GitHub remote is public: no `git push`, no GitHub PRs, no forks, no
  upstream IronRDP PRs. A "PR" is a local branch that the integrator merges with `--no-ff`; the
  merge commit message records the Red tests and their failing output.
- Do not tick `plan.md` checkboxes; the integrator does that.

## ADRs

Any decision not already fixed by `plan.md` gets an ADR in `docs/adr/<TASKID>-<slug>.md`
(task-ID prefix, so parallel tasks never collide on numbers). Existing ADRs:

| ADR | Topic |
|---|---|
| `M0-1-workspace-conventions` | workspace, lints, coverage gate, bindings, UI build |
| `M0-2-ironrdp-vendored-fork` | IronRDP vendored in `third_party/ironrdp` + patch queue |
| `M0-5-session-interface` | `spawn_session` / `SessionHandle` / `SessionEvent` seam |
| `M0-5-nv12-frame-location` | `Nv12Frame` / `H264Decoder` live in `drift_core::video` |
| `M0-6-dependency-policy` | cargo-deny licenses (MPL-2.0 allowance), advisories |

## Commands

Always `export PATH="$HOME/.cargo/bin:$PATH"`. Use `bun` for anything JavaScript; never
another JS package manager.

| Command | What it does |
|---|---|
| `cargo xtask check` | fast loop: `cargo fmt --check`, clippy `-D warnings`, nextest |
| `cargo xtask ci` | the merge gate: npm-ban, secret-scan, `bun install --frozen-lockfile`, `bun test`, `bun run typecheck`, `bun run build`, bindings freshness, fmt, clippy, nextest under llvm-cov + coverage gate, `cargo deny` |
| `cargo xtask ci --no-coverage` | same, without instrumentation (faster locally) |
| `cargo xtask bindings` | regenerate `ui/src/bindings.ts` after changing IPC commands/types |
| `cargo xtask npm-ban` / `secret-scan` | the individual hygiene checks |
| `cargo xtask e2e [nextest args]` | real-host tests through SSH forwards (below) |
| `cargo xtask import-fixtures` | M0-3 (stub until then) |
| `cargo xtask host-setup-check` | M0-4 (stub until then) |
| `cargo xtask bundle` | M9-5 (stub until then) |
| `cargo tauri dev` / `cargo tauri build` | run / bundle the app (UI is built by bun first) |

Builds are large; long cargo runs can exceed a 10-minute tool call — run them in the background
and poll. Other agents build in parallel on the same machine.

## Code rules

- Pure logic in pure crates with unit/property tests; FFI crates (`drift-macos`, `drift-video`,
  `drift-render`) are humble objects with smoke tests.
- No `unwrap`/`expect` on network data (clippy `unwrap_used` is an error outside tests); every
  `unsafe` block has a `// SAFETY:` comment; public items are documented (`missing_docs`).
- macOS only: no `cfg(target_os)` branches for other platforms.
- Shared contracts (plan §3) live in `drift-core` (+ `FrameSink` in `drift-gfx`, the session
  interface in `drift-rdp::session`). Changing them is a cross-stream change: update the ADR.
- IronRDP changes: separate commits touching only `third_party/ironrdp/`, exported to
  `third_party/ironrdp-patches/` (see the M0-2 ADR).

## Fixtures

`fixtures/` is tracked with git-lfs (`.gitattributes`: `fixtures/**`, except `README.md` and
`MANIFEST*`). Run `git lfs install --local` once per clone. Fixtures come from
`~/code/drift-spikes` via `cargo xtask import-fixtures` (M0-3), which sanitises them; the
secret scan must stay green. Load them in tests through `drift_testkit::fixtures`.

## Secrets and e2e

- Real credentials live only in `~/code/drift-spikes/secrets/*.txt` (mode 600) on the dev Mac.
  Never print them, commit them, or put them in fixtures, logs, snapshots or test source.
  Tests use obviously fake values.
- `cargo xtask e2e` opens `ssh -N -L 1339x:localhost:339x homelab@10.1.2.40` (required by macOS
  Local Network Privacy) and runs `cargo nextest run -p drift-e2e --run-ignored only` with:
  `DRIFT_E2E_HOST=127.0.0.1`, `DRIFT_E2E_TLS_NAME=10.1.2.40`, `DRIFT_E2E_PORT_<3389..3392>`
  (local forward ports), and credentials mapped from the secrets directory unless already set:
  `DRIFT_E2E_SYS_USER/PASS` (system.txt), `DRIFT_E2E_LOGIN_USER/PASS` (testuser.txt),
  `DRIFT_E2E_HL_USER/PASS` + `DRIFT_E2E_HL_PORT` (headless2.txt), `DRIFT_E2E_HL1_*`
  (headless.txt), `DRIFT_E2E_SHARE_*` (share.txt).
- E2E tests live in `tests/e2e` and are all `#[ignore]` so `cargo xtask ci` never needs the host.
- Anything that genuinely needs a human (permission prompts, Wi-Fi toggles, clean machines)
  goes into `docs/acceptance.md` as a manual step.
