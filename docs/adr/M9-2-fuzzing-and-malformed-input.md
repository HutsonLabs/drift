# M9-2 — Fuzzing the server-controlled parsers, and what the merge gate checks

- Status: accepted
- Task: plan M9-2 ("Nightly fuzzing of the GFX, ZGFX, redirection, RDSTLS, CLIPRDR and pointer
  parsers. Malformed server input never panics; the actor boundary maps it to `ProtocolError`.")
- Code: `crates/drift-gfx/fuzz/`, `crates/drift-rdp/fuzz/`, `crates/drift-macos/fuzz/`,
  `xtask/src/fuzz_audit.rs`, `.github/workflows/fuzz-nightly.yml`
- Tests: `xtask/tests/fuzz_audit.rs`, `crates/drift-rdp/tests/m9_2_malformed.rs`,
  `crates/drift-macos/tests/m9_2_pointer.rs`

## Context

Every byte Drift parses comes from a server it has not authenticated yet (leg 1's redirection
PDU arrives before RDSTLS, the GFX stream arrives before anything is drawn). M1-2 already made
the graphics client return `GfxError` instead of panicking and added one fuzz target; M9-2
extends that to the rest of the parsers and, more importantly, makes the coverage *checkable*.

## Decisions

1. **Three standalone fuzz workspaces, one per owning crate.** `crates/drift-gfx/fuzz`
   (`gfx_pdu_zgfx`, `zgfx`), `crates/drift-rdp/fuzz` (`redirection`, `rdstls`, `cliprdr`) and
   `crates/drift-macos/fuzz` (`pointer`). Each is excluded from the root workspace, as M1-2
   established, because cargo-fuzz needs a nightly toolchain and sanitizer flags. Putting a
   target next to the crate that owns the parser keeps the dependency edges honest: the
   pointer decoder lives in `drift-macos`, so its fuzz crate does too.

2. **Targets drive the *stateful* entry point, not a single decode call.** A one-shot
   `decode::<Pdu>(data)` target finds almost nothing, because the interesting bugs live in the
   state that survives between PDUs:
   - `zgfx` keeps one `Decompressor` and feeds it a sequence of length-prefixed segments, so
     the history buffer and cross-segment matches are reachable (the whole-client target
     spends its budget elsewhere);
   - `pointer` feeds a sequence of length-prefixed fast-path updates into one `PointerDecoder`,
     so fragment reassembly and the pointer cache interact, and it asserts after every update
     that each cached bitmap still matches its advertised size — a corrupted cache entry would
     otherwise only crash later, inside AppKit;
   - `redirection` runs `RedirectLoop::on_redirect` (routing token, certificate container,
     credential extraction, loop cap), not just the PDU decode;
   - `cliprdr` chains `ClipboardPdu` decoding into `ClipboardSync` and the format converters,
     which is where a bogus `biWidth`/`biHeight` would bite.

3. **The corpus is the captured g-r-d traffic.** `fixtures/pdus/*.bin`,
   `fixtures/clipboard/*.bin` and the records of `fixtures/pdus/fastpath_pointer_*.rec` are
   real, sanitized captures (M0-3); libFuzzer mutates far more effectively from them than from
   scratch. The seed corpus is built ad hoc (fixtures are git-lfs, and corpora are ignored by
   `.gitignore`), and the nightly workflow runs without one.

4. **`cargo xtask ci` audits the targets** (`xtask::fuzz_audit`, `Check::FuzzTargets`). A fuzz
   crate is invisible to the workspace build: renaming a target, or forgetting to add it to the
   nightly matrix, would silently stop the fuzzing without breaking anything. The audit checks,
   for each of the six parsers the plan names, that the `[[bin]]` exists, that
   `fuzz_targets/<name>.rs` exists, and that `fuzz-nightly.yml` runs it for at least 120 s.
   Textual, pure and cheap — the same trade-off as `xtask::workflows`.

5. **A fast mirror of the fuzzers runs in the gate.** `m9_2_malformed.rs` and
   `m9_2_pointer.rs` replay *damaged* copies of the captured PDUs (byte flips, truncations,
   splices) through the same entry points, plus one loopback test in which the `FakeServer`
   puts a broken `RDP_SEGMENTED_DATA` frame on the graphics channel and the session must end in
   `DisconnectReason::ProtocolError`. Nightly fuzzing finds new inputs; these tests make sure a
   regression that breaks the *known* ones fails a PR. `ServerAction::GfxRaw` was added to
   `drift-testkit` for that loopback test.

## Results

Local run on nightly, 150 s per target, seeded with the captured PDUs:

| Target | Execs | Crashes |
|---|---|---|
| `redirection` | 444 593 | none |
| `rdstls` | 443 040 | none |
| `cliprdr` | 821 202 | none |
| `pointer` | 399 060 | none |
| `zgfx` | 705 061 | none |
| `gfx_pdu_zgfx` | (M1-2 target, re-run) | none |

No crash, OOM or timeout, and the malformed-input sweeps passed as written: the hardening
landed with M1-2 (`GfxError`), M1-4 (codec bounds) and M3-1 (redirect validation). M9-2's value
is therefore the *coverage*, the audit that keeps it, and the evidence above.

## Consequences

- Six nightly jobs instead of one; each builds its own workspace (~1 min on a macos-15 runner)
  and fuzzes for five minutes.
- A new parser needs an entry in `xtask::fuzz_audit::REQUIRED`, a target and a matrix entry, or
  `cargo xtask ci` fails — which is the point.
- The fuzz workspaces have their own `Cargo.lock` (git-ignored, as in M1-2): they are tools, not
  shipped artefacts.
