# M0-6 — Dependency and license policy (cargo-deny)

- Status: accepted
- Date: 2026-09-22
- Config: `deny.toml`, run by `cargo xtask ci` (`cargo deny --workspace check`)

## Decision

- Allowed licenses: MIT, MIT-0, Apache-2.0 (+LLVM-exception), BSD-2/3-Clause, 0BSD, ISC, Zlib,
  Unicode-3.0/Unicode-DFS-2016 (plan M0-6), **plus MPL-2.0** (below). Anything else, including all
  GPL/LGPL/AGPL variants, is rejected.
- **MPL-2.0 allowance:** required by tauri's own dependency tree (`cssparser`,
  `cssparser-macros`, `selectors`, `dtoa-short` via wry/kuchikiki; `option-ext` via `dirs`).
  MPL-2.0 is file-level copyleft: using the crates unmodified imposes no obligation on Drift's
  code; modified MPL files would have to be published. Drift does not patch them.
- Advisories: vulnerabilities always fail. `unmaintained = "workspace"` — unmaintained-crate
  advisories fail only for Drift's direct dependencies; transitive ones pulled in by tauri (e.g.
  `paste`, `unic-*`) are outside our control. Every entry added to `[advisories].ignore` must be
  justified in this ADR with the advisory id and why it does not apply.
- Sources: crates.io only; no git dependencies (IronRDP is a path dependency, see M0-2 ADR).
- Wildcard version requirements are denied (path dependencies excepted).
- Drift crates are `publish = false`, MIT; `[licenses.private] ignore = true`.

## Advisory ignores

None yet.
