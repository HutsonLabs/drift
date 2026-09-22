# M0-3: Fixture capture, layout and sanitization

## Status
Accepted (task M0-3).

## Context
Plan §1.7 lists the wire artefacts to import from the spikes. Several spike artefacts were
unusable (`/private/tmp/leg1.h264` and `leg2.h264` were empty) and the spike never kept the
raw GFX DVC stream, the Server Redirection frame, RDSTLS request/response bytes or fast-path
pointer PDUs, which later tasks (M0-2, M1-2, M1-3, M2-5, M3-x, M5-x) need as replayable
inputs. Fixtures are committed to a public repository, so every credential must be removed,
including one-time Remote Login credentials that are already invalid.

## Decision
1. **Fresh capture** on 2026-09-22 from the homelab host with a copy of `probe2`,
   instrumented to dump every artefact (see `fixtures/README.md` for the six runs). The
   capture tool lives outside the repository (`~/code/drift-spikes/capture/`); its procedure
   is recorded in the README provenance.
2. **Staging + import.** Captures are assembled in a staging directory
   (`~/code/drift-spikes/fixtures-staging/`) with a `provenance.toml` listing every file,
   its kind, source and contents. `cargo xtask import-fixtures` requires the staging tree and
   the provenance to match exactly, sanitizes, writes `fixtures/`, prunes stale files and
   generates `fixtures/README.md` and `fixtures/MANIFEST.sha256` (`sha256sum` format).
3. **Layout:** `h264/` (Annex-B), `gfx/` (`*.server.raw.rec` = DVC payloads as received with
   ZGFX, `*.server.rec` = after ZGFX, `*.client.rec` = client GFX PDUs), `pdus/`,
   `clipboard/`, `screenshots/`, `goldens/`. `*.rec` = repeated `u32 LE length + bytes`.
4. **Sanitizer:** every value line (at least 6 characters) of every
   `~/code/drift-spikes/secrets/*.txt`, user names included, plus hex lines decoded to bytes,
   plus the one-time user name and password blob parsed from every Server Redirection PDU
   and RDSTLS AuthRequest being imported. Matches as raw bytes, UTF-8 and UTF-16LE are
   replaced by a **same-length** `REDACTED…` placeholder so length fields and offsets stay
   valid. The import fails if anything survives. The redirection GUID and routing cookie
   are kept (not credentials; tests need them).
5. **Goldens** are decoded by ffmpeg with the g-r-d encoder's matrix (BT.709 full range), not
   ffmpeg's BT.601 limited-range default, so they match what Drift's shader must produce.
6. **Checks:** `drift-testkit`'s `fixture_manifest_checksums_match` verifies the manifest on
   every test run; `xtask`'s `committed_fixtures_contain_no_known_secret` scans `fixtures/`
   for every known secret where the secrets directory exists (dev machine), and the CI
   `secret-scan` step covers the secret-like subset everywhere.

## Consequences
- `pdus/tls_cert_headless.der`: the headless daemon's self-signed certificate had the
  headless RDP user name as subject/issuer CN; it is replaced, so the DER parses but its
  self-signature no longer verifies. The Remote Login certificates were untouched.
- The routing cookie in the redirection PDUs is a real (expired) value.
- Screenshots contain the GDM user list as pixels (display names), which the byte-level
  sanitizer cannot and does not need to change.
- Re-capturing means rebuilding the staging directory and re-running the import; the
  README and manifest are regenerated, never edited by hand.
