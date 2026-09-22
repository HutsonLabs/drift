#!/usr/bin/env bash
# Drift GNOME 50 host setup (plan §5.2, task M0-4). See docs/gnome-host-setup.md.
#
# Converges a GNOME Remote Desktop 50 host to the state Drift is tested against:
#   --system                    Remote Login: the system g-r-d daemon (:3389) is enabled,
#                               has a TLS certificate and RDP credentials.
#   --headless USER:PORT[:persistent]
#                               A headless g-r-d daemon for USER on a fixed PORT (port
#                               negotiation off) with a TLS certificate and credentials.
#                               "persistent" also enables gnome-headless-session@USER with
#                               the systemd drop-in that fixes it on AnduinOS 2.0.3
#                               (DynamicUser=no, User=gdm; plan §1.9).
#
# Idempotent: every step reads the current state first and only acts when it differs.
# Modes:
#   --check    (default) change nothing; print what is ok and what would change.
#              Exit 0 when nothing would change, 2 when changes are pending.
#   --dry-run  same as --check.
#   --apply    make the changes. Exit 0 on success.
# Exit 1 on errors (for example missing credentials that the script cannot invent).
#
# Credentials are never printed. They are only written when given through the
# environment and different from the current value:
#   DRIFT_SYSTEM_RDP_USER / DRIFT_SYSTEM_RDP_PASS
#   DRIFT_HEADLESS_RDP_USER_<USER> / DRIFT_HEADLESS_RDP_PASS_<USER>
#     (<USER> upper-cased, non-alphanumerics replaced by "_", e.g. ..._DRIFTTEST2)
# Desktop Sharing credentials live in the user's login keyring and can only be set from
# GNOME Settings in an unlocked session (plan §1.9); this script does not touch them.
#
# Run it as root on the host, e.g. from the dev Mac:
#   ssh homelab@10.1.2.40 'sudo -n bash -s -- --check --system \
#       --headless drifttest:3391 --headless drifttest2:3392:persistent' < host/drift-host-setup.sh
set -euo pipefail

readonly DROPIN_DIR=/etc/systemd/system/gnome-headless-session@.service.d
readonly DROPIN_NAME=drift.conf
# Keep identical to host/gnome-headless-session-dropin.conf (an xtask test enforces it).
readonly DROPIN_CONTENT='[Service]
DynamicUser=no
User=gdm'
readonly SYSTEM_CERT_DIR=/var/lib/gnome-remote-desktop/.local/share/gnome-remote-desktop/certificates

MODE=check
SYSTEM=0
HEADLESS=()
CHANGES=0
ERRORS=0

usage() {
  sed -n '2,/^set -euo/p' "$0" 2>/dev/null | sed -e '$d' -e 's/^# \{0,1\}//' || true
}

ok() { printf 'ok:     %s\n' "$*"; }
err() {
  printf 'error:  %s\n' "$*"
  ERRORS=$((ERRORS + 1))
}

# change DESCRIPTION CMD...: runs CMD in --apply mode, otherwise prints it.
change() {
  local what=$1
  shift
  CHANGES=$((CHANGES + 1))
  if [[ $MODE == apply ]]; then
    printf 'change: %s\n' "$what"
    "$@"
  else
    printf 'would:  %s\n        $ %s\n' "$what" "$*"
  fi
}

# change_secret DESCRIPTION CMD...: like change, but never prints the command line.
change_secret() {
  local what=$1
  shift
  CHANGES=$((CHANGES + 1))
  if [[ $MODE == apply ]]; then
    printf 'change: %s\n' "$what"
    "$@"
  else
    printf 'would:  %s (command not shown: contains credentials)\n' "$what"
  fi
}

# field STATUS_TEXT NAME: value of a "NAME: value" line in `grdctl status` output.
field() {
  printf '%s\n' "$1" | sed -n "s/^[[:space:]]*$2: //p" | head -n1
}

is_unset() { [[ -z "$1" || "$1" == "(null)" ]]; }

# as_user USER CMD...: runs CMD as USER inside the user's session bus.
as_user() {
  local user=$1 uid
  shift
  uid=$(id -u "$user")
  runuser -u "$user" -- env "XDG_RUNTIME_DIR=/run/user/$uid" \
    "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$uid/bus" "$@"
}

# gen_cert OWNER DIR: self-signed RSA certificate rdp-tls.{crt,key} in DIR owned by OWNER.
gen_cert() {
  local owner=$1 dir=$2
  install -d -o "$owner" -g "$(id -gn "$owner")" -m 0755 "$dir"
  runuser -u "$owner" -- openssl req -new -x509 -newkey rsa:4096 -nodes -days 3650 \
    -subj "/CN=$(hostname)" -keyout "$dir/rdp-tls.key" -out "$dir/rdp-tls.crt" 2>/dev/null
  chmod 0600 "$dir/rdp-tls.key"
}

write_dropin() {
  install -d -m 0755 "$DROPIN_DIR"
  printf '%s\n' "$DROPIN_CONTENT" >"$DROPIN_DIR/$DROPIN_NAME"
  chmod 0644 "$DROPIN_DIR/$DROPIN_NAME"
  systemctl daemon-reload
}

ensure_unit() {
  local unit=$1
  if systemctl is-enabled --quiet "$unit" 2>/dev/null && systemctl is-active --quiet "$unit"; then
    ok "$unit enabled and active"
  else
    change "enable and start $unit" systemctl enable --now "$unit"
  fi
}

ensure_user_unit() {
  local user=$1 unit=$2
  if as_user "$user" systemctl --user is-enabled --quiet "$unit" 2>/dev/null &&
    as_user "$user" systemctl --user is-active --quiet "$unit"; then
    ok "$user: $unit enabled and active"
  else
    change "$user: enable and start $unit" as_user "$user" systemctl --user enable --now "$unit"
  fi
}

# Upper-cased, shell-safe form of a user name for environment variable names.
env_suffix() {
  printf '%s' "$1" | tr '[:lower:]' '[:upper:]' | tr -c 'A-Z0-9' '_'
}

setup_system() {
  local st cert key current
  if ! st=$(grdctl --system status 2>/dev/null); then
    err "grdctl --system status failed; is gnome-remote-desktop 50 installed?"
    return
  fi

  cert=$(field "$st" "TLS certificate")
  key=$(field "$st" "TLS key")
  if is_unset "$cert" || is_unset "$key"; then
    change "generate the system TLS certificate in $SYSTEM_CERT_DIR" \
      gen_cert gnome-remote-desktop "$SYSTEM_CERT_DIR"
    change "set the system TLS key" grdctl --system rdp set-tls-key "$SYSTEM_CERT_DIR/rdp-tls.key"
    change "set the system TLS certificate" grdctl --system rdp set-tls-cert "$SYSTEM_CERT_DIR/rdp-tls.crt"
  else
    ok "system TLS certificate configured ($cert)"
  fi

  if [[ -n "${DRIFT_SYSTEM_RDP_USER:-}" && -n "${DRIFT_SYSTEM_RDP_PASS:-}" ]]; then
    current=$(grdctl --system status --show-credentials 2>/dev/null || true)
    if [[ "$(field "$current" Username)" == "$DRIFT_SYSTEM_RDP_USER" &&
      "$(field "$current" Password)" == "$DRIFT_SYSTEM_RDP_PASS" ]]; then
      ok "system RDP credentials match DRIFT_SYSTEM_RDP_USER/PASS"
    else
      change_secret "set the system RDP credentials" \
        grdctl --system rdp set-credentials "$DRIFT_SYSTEM_RDP_USER" "$DRIFT_SYSTEM_RDP_PASS"
    fi
  elif [[ "$(field "$st" Username)" == "(hidden)" && "$(field "$st" Password)" == "(hidden)" ]]; then
    ok "system RDP credentials are set (unchanged)"
  else
    err "system RDP credentials are empty; export DRIFT_SYSTEM_RDP_USER and DRIFT_SYSTEM_RDP_PASS"
  fi

  if [[ "$(field "$st" Status)" == "enabled" ]]; then
    ok "system RDP enabled"
  else
    change "enable system RDP" grdctl --system rdp enable
  fi

  ensure_unit gnome-remote-desktop.service
}

ensure_dropin() {
  local user=$1 props
  props=$(systemctl show "gnome-headless-session@$user.service" -p DynamicUser -p User 2>/dev/null || true)
  if grep -qx 'DynamicUser=no' <<<"$props" && grep -qx 'User=gdm' <<<"$props"; then
    ok "gnome-headless-session@.service runs with DynamicUser=no, User=gdm"
  else
    change "install $DROPIN_DIR/$DROPIN_NAME (DynamicUser=no, User=gdm)" write_dropin
  fi
}

setup_headless() {
  local user=$1 port=$2 persistent=$3 uid st cert key current var_user var_pass
  if ! uid=$(id -u "$user" 2>/dev/null); then
    err "user $user does not exist"
    return
  fi

  if [[ "$persistent" == "persistent" ]]; then
    ensure_dropin "$user"
    ensure_unit "gnome-headless-session@$user.service"
  fi

  if [[ ! -S "/run/user/$uid/bus" ]]; then
    if [[ $MODE == apply && "$persistent" != "persistent" ]]; then
      err "$user has no running session (/run/user/$uid/bus); use --headless $user:$port:persistent"
    else
      printf 'skip:   %s has no session bus yet; re-run after the session has started\n' "$user"
      [[ $MODE == apply ]] || CHANGES=$((CHANGES + 1))
    fi
    return
  fi

  if ! st=$(as_user "$user" grdctl --headless status 2>/dev/null); then
    err "$user: grdctl --headless status failed"
    return
  fi

  cert=$(field "$st" "TLS certificate")
  key=$(field "$st" "TLS key")
  local dir
  dir="$(getent passwd "$user" | cut -d: -f6)/.local/share/gnome-remote-desktop/certificates"
  if is_unset "$cert" || is_unset "$key"; then
    change "$user: generate a TLS certificate in $dir" gen_cert "$user" "$dir"
    change "$user: set the headless TLS key" as_user "$user" grdctl --headless rdp set-tls-key "$dir/rdp-tls.key"
    change "$user: set the headless TLS certificate" as_user "$user" grdctl --headless rdp set-tls-cert "$dir/rdp-tls.crt"
  else
    ok "$user: TLS certificate configured ($cert)"
  fi

  var_user="DRIFT_HEADLESS_RDP_USER_$(env_suffix "$user")"
  var_pass="DRIFT_HEADLESS_RDP_PASS_$(env_suffix "$user")"
  if [[ -n "${!var_user:-}" && -n "${!var_pass:-}" ]]; then
    current=$(as_user "$user" grdctl --headless status --show-credentials 2>/dev/null || true)
    if [[ "$(field "$current" Username)" == "${!var_user}" && "$(field "$current" Password)" == "${!var_pass}" ]]; then
      ok "$user: RDP credentials match $var_user/$var_pass"
    else
      change_secret "$user: set the headless RDP credentials" \
        as_user "$user" grdctl --headless rdp set-credentials "${!var_user}" "${!var_pass}"
    fi
  elif [[ "$(field "$st" Username)" == "(hidden)" && "$(field "$st" Password)" == "(hidden)" ]]; then
    ok "$user: RDP credentials are set (unchanged)"
  else
    err "$user: RDP credentials are empty; export $var_user and $var_pass"
  fi

  if [[ "$(field "$st" Port)" == "$port" ]]; then
    ok "$user: RDP port $port"
  else
    change "$user: set the RDP port to $port (effective at the next daemon start)" \
      as_user "$user" grdctl --headless rdp set-port "$port"
  fi
  if [[ "$(field "$st" "Negotiate port")" == "no" ]]; then
    ok "$user: port negotiation disabled"
  else
    change "$user: disable port negotiation" as_user "$user" grdctl --headless rdp disable-port-negotiation
  fi

  if [[ "$(field "$st" Status)" == "enabled" ]]; then
    ok "$user: headless RDP enabled"
  else
    change "$user: enable headless RDP" as_user "$user" grdctl --headless rdp enable
  fi

  ensure_user_unit "$user" gnome-remote-desktop-headless.service

  # A headless session must not suspend the host because nobody touches it.
  current=$(as_user "$user" gsettings get org.gnome.settings-daemon.plugins.power sleep-inactive-ac-type 2>/dev/null || true)
  if [[ "$current" == "'nothing'" ]]; then
    ok "$user: automatic suspend disabled"
  else
    change "$user: disable automatic suspend" \
      as_user "$user" gsettings set org.gnome.settings-daemon.plugins.power sleep-inactive-ac-type nothing
  fi
}

main() {
  while [[ $# -gt 0 ]]; do
    case $1 in
    --check | --dry-run) MODE=check ;;
    --apply) MODE=apply ;;
    --system) SYSTEM=1 ;;
    --headless)
      [[ $# -ge 2 ]] || {
        echo "--headless needs USER:PORT[:persistent]" >&2
        exit 1
      }
      HEADLESS+=("$2")
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1 (see --help)" >&2
      exit 1
      ;;
    esac
    shift
  done

  if [[ $SYSTEM -eq 0 && ${#HEADLESS[@]} -eq 0 ]]; then
    echo "nothing to do: pass --system and/or --headless USER:PORT[:persistent]" >&2
    exit 1
  fi
  if [[ $EUID -ne 0 ]]; then
    echo "run as root (sudo)" >&2
    exit 1
  fi

  printf 'drift-host-setup: mode=%s\n' "$MODE"
  if [[ $SYSTEM -eq 1 ]]; then
    setup_system
  fi
  local spec user port persistent
  for spec in "${HEADLESS[@]}"; do
    IFS=: read -r user port persistent <<<"$spec"
    if [[ -z "$user" || ! "$port" =~ ^[0-9]+$ ]]; then
      err "bad --headless spec '$spec' (want USER:PORT[:persistent])"
      continue
    fi
    setup_headless "$user" "$port" "${persistent:-}"
  done

  if [[ $MODE == apply ]]; then
    printf 'drift-host-setup: %d change(s) applied, %d error(s)\n' "$CHANGES" "$ERRORS"
  else
    printf 'drift-host-setup: %d change(s) pending, %d error(s)\n' "$CHANGES" "$ERRORS"
  fi
  if [[ $ERRORS -gt 0 ]]; then
    exit 1
  fi
  if [[ $MODE == check && $CHANGES -gt 0 ]]; then
    exit 2
  fi
}

main "$@"
