# GNOME 50 host setup

How to prepare a GNOME Remote Desktop (g-r-d) 50 host for Drift, and how the homelab test
host (`homelab@10.1.2.40`, plan §5.2) is kept in that state. Everything here was verified on
AnduinOS 2.0.3 (Ubuntu 26.04 base, GNOME Shell 50.1, gdm3 50.1, gnome-remote-desktop 50.2).

| File | Purpose |
|---|---|
| `host/drift-host-setup.sh` | Idempotent setup script (`--check` by default, `--apply` to change). |
| `host/gnome-headless-session-dropin.conf` | The systemd drop-in that makes `gnome-headless-session@.service` work on AnduinOS 2.0.3. |
| `host/drift-host-report.sh` | Read-only state report used by `cargo xtask host-setup-check`. |

## What Drift needs on the host

| Mode | Server side | Set up by |
|---|---|---|
| Remote Login | System daemon (`gnome-remote-desktop.service`, `grdctl --system`) on :3389 with a TLS certificate and RDP credentials | `--system` |
| Headless | Per-user headless daemon (`grdctl --headless`) on a **fixed** port, TLS certificate, RDP credentials; optionally a persistent `gnome-headless-session@USER.service` | `--headless USER:PORT[:persistent]` |
| Desktop Sharing | The user's own daemon, configured in GNOME Settings → System → Remote Desktop → Desktop Sharing | By hand (credentials live in the login keyring and can't be set over SSH, plan §1.9) |

## The setup script

Run it as root on the host. From the dev Mac:

```bash
# 1. See what would change (changes nothing; exit 0 = already converged, 2 = changes pending).
ssh homelab@10.1.2.40 'sudo -n bash -s -- --check --system \
    --headless drifttest:3391 --headless drifttest2:3392:persistent' < host/drift-host-setup.sh

# 2. Apply.
ssh homelab@10.1.2.40 'sudo -n bash -s -- --apply --system \
    --headless drifttest:3391 --headless drifttest2:3392:persistent' < host/drift-host-setup.sh

# 3. Verify from the Mac (read-only, over SSH).
cargo xtask host-setup-check
```

Each step reads the current state first and acts only when it differs, so re-running
`--apply` reports `0 change(s) applied`. The steps:

**`--system` (Remote Login)**
1. TLS certificate and key: kept if `grdctl --system status` shows them; otherwise a
   self-signed RSA-4096 certificate is generated in
   `/var/lib/gnome-remote-desktop/.local/share/gnome-remote-desktop/certificates/` and set with
   `grdctl --system rdp set-tls-key/set-tls-cert`.
2. Credentials: kept if set. Written only when `DRIFT_SYSTEM_RDP_USER` and
   `DRIFT_SYSTEM_RDP_PASS` are exported *and* differ from the current ones. Empty credentials
   without those variables are an error.
3. `grdctl --system rdp enable`, then `systemctl enable --now gnome-remote-desktop.service`.

**`--headless USER:PORT[:persistent]`**
1. With `persistent`: the drop-in (`DynamicUser=no`, `User=gdm`) is installed as
   `/etc/systemd/system/gnome-headless-session@.service.d/drift.conf` unless
   `systemctl show` already reports those values (the homelab host has an identical
   `drift-test.conf` from the spike, which satisfies the check), then
   `systemctl enable --now gnome-headless-session@USER.service`. Without the drop-in the
   shipped unit fails with "Failed: The connection is closed" (exit 70, plan §1.9).
2. The remaining steps run as USER on the user's session bus (`/run/user/UID/bus`); a
   non-persistent user must already have a session.
3. TLS certificate and key, as for the system daemon, under
   `~USER/.local/share/gnome-remote-desktop/certificates/`.
4. Credentials from `DRIFT_HEADLESS_RDP_USER_<USER>` / `DRIFT_HEADLESS_RDP_PASS_<USER>`
   (upper-cased, non-alphanumerics as `_`, e.g. `..._DRIFTTEST2`); same rules as above. They
   are stored in a GKeyFile on hosts without a TPM, which works over SSH.
5. `grdctl --headless rdp set-port PORT` and `disable-port-negotiation`, so the daemon never
   moves to another port (with negotiation on, the port depends on start order). g-r-d reads the
   port when the RDP server starts, so this takes effect at the next daemon start and does not
   drop live connections.
6. `grdctl --headless rdp enable`; `systemctl --user enable --now gnome-remote-desktop-headless.service`.
7. Automatic suspend off for the user
   (`org.gnome.settings-daemon.plugins.power sleep-inactive-ac-type 'nothing'`): an idle
   headless session otherwise shows "Suspending soon because of inactivity".

Headless daemons are always read-write (`grd_settings_headless_new` forces `rdp-view-only`
to false), so the plan's `disable-view-only` step is not needed.

Credentials are never printed: plain `grdctl status` shows only `(hidden)`/`(empty)`, and
credential changes are reported without their command line.

## `cargo xtask host-setup-check`

Pipes `host/drift-host-report.sh` to `ssh homelab@10.1.2.40 bash -s` (override with
`--host` or `DRIFT_E2E_SSH`, and `--headless USER:PORT[:persistent]` for other users), parses
the report and checks:

- the system daemon: unit enabled and active, RDP enabled, port 3389, TLS certificate
  valid (not expired), credentials set, and a `--system` g-r-d process listening on :3389;
- each headless user: daemon active, RDP enabled, port pinned (configured port = expected
  and negotiation off), TLS certificate valid, credentials set, a `--headless` g-r-d
  process owned by that user listening on the port, and the user unit enabled and active;
- for persistent users: `gnome-headless-session@USER.service` enabled and active, with
  `DynamicUser=no` and `User=gdm`.

The system daemon stops listening for a moment while it hands a connection over, so a
failing live check is retried up to three times. `--report FILE` evaluates a saved report
and `--save-report FILE` keeps the raw report. The parsers are unit-tested on reports
captured from the homelab (`xtask/tests/data/host/`).

## Homelab state (2026-09-22)

`--check` before the first `--apply` reported 7 pending changes: pin :3391 and :3392 and
disable negotiation for both users, enable `gnome-headless-session@drifttest2.service` at
boot (it was running but not enabled), and disable automatic suspend for both users. After
one `--apply`, `--check` and a second `--apply` both report 0 changes, and
`cargo xtask host-setup-check` passes all 24 checks.

Things the script deliberately does not do:
- set or change credentials unless they are given in the environment;
- touch Desktop Sharing (:3390, the `homelab` user's daemon);
- clean up greeter sessions that clients left behind by disconnecting at the GDM greeter
  (plan §1.9; Drift itself closes greeter tabs gracefully, M3-4).

## E2E helpers and session state (stream A, M4-2/M5-2/M6-3/M7-3)

`cargo xtask e2e` needs a little more than the daemons:

- **The test session must be unlocked.** A locked GNOME session swallows input events and stops
  propagating clipboard changes, which looks exactly like a broken client. The tests call
  `drift_e2e::host::ensure_unlocked_session`, which turns the idle lock off
  (`org.gnome.desktop.screensaver lock-enabled false`, `idle-activation-enabled false`,
  `org.gnome.desktop.session idle-delay 0`) and restarts
  `gnome-headless-session@<user>.service` when the session is already locked — GNOME cannot be
  unlocked over D-Bus without the user's password.
- **Helpers live in the repository.** `host/cliptool.py` (clipboard read/write) and
  `host/scrolltool.py` (scroll, key and motion counters) are uploaded into the test user's home
  by the tests themselves; `anim.py` and `clip_in.png` are expected in that home directory
  (`/home/<user>/`), as set up during the spike.
- **The tests run one at a time** (`--test-threads 1`): they share one host and one daemon per
  mode.
