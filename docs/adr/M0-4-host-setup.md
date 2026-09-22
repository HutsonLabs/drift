# M0-4: Host setup script and host-setup-check

## Status
Accepted (task M0-4).

## Context
Plan §5.2 lists the verified host configuration steps and asks for an idempotent script plus
`cargo xtask host-setup-check`. The homelab host already ran with the spike's configuration,
which differed from §5.2 in a few details: headless ports were negotiated (not pinned), the
drop-in file was named `drift-test.conf`, and `gnome-headless-session@drifttest2.service` was
running but not enabled at boot.

## Decision
1. `host/drift-host-setup.sh` is **check-first**: `--check` (default; alias `--dry-run`)
   changes nothing and exits 2 when changes are pending; `--apply` converges. It is fed to
   `sudo bash -s` over SSH, so it embeds the drop-in text (an xtask test keeps it identical
   to `host/gnome-headless-session-dropin.conf`).
2. The drop-in is checked by **effective unit properties** (`systemctl show -p DynamicUser -p
   User`), not by file name, so any existing equivalent drop-in satisfies it.
3. **Credentials are never generated or changed implicitly.** They are written only from
   explicit environment variables and only when they differ from the current values;
   credential commands are never echoed.
4. Headless ports are **pinned** (`set-port` + `disable-port-negotiation`) as §5.2 says:
   negotiated ports depend on daemon start order. g-r-d reads the port at RDP-server start,
   so pinning a port that is already in use by that daemon changes nothing until its next
   start.
5. The script also disables automatic suspend for headless users (observed: "Suspending soon
   because of inactivity" in the headless session). `disable-view-only` is omitted:
   headless settings force `rdp-view-only=false`.
6. `host-setup-check` uses a separate, **read-only** report script (`host/drift-host-report.sh`,
   compiled into xtask with `include_str!`) whose output is a `### section` text format.
   Parsing and evaluation are pure functions tested on reports captured from the host; the
   live command retries up to three times because the system daemon briefly stops
   listening on :3389 during a handover.

## Consequences
- On 2026-09-22 the script was run on the homelab: `--check` listed 7 changes, one `--apply`
  made them, and subsequent `--check`/`--apply` runs report 0 changes. No credentials changed.
- Adding a test user means one more `--headless USER:PORT[:persistent]` in both commands.
- `shellcheck` is required to run the xtask tests (installed on the dev Mac and on the
  GitHub macOS runner image).
