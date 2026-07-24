#!/usr/bin/env bash
set -Eeuo pipefail

# Reversible host policy and tuning for SHR-DAW. The helper never starts or
# restarts JACK, a synth, MIDI, playback, or recording.

PREFIX="${SHR_TUNE_ROOT:-}"
STATE_DIR="$PREFIX/var/lib/shr-audio-tune"
MANIFEST="$STATE_DIR/manifest"
TRANSACTION_DIR="$STATE_DIR/transaction"
PERMISSIONS_TRANSACTION_DIR="$STATE_DIR/permissions-transaction"
JACK_DROPIN="$PREFIX/etc/systemd/system/jack.service.d/90-shr-audio-cpu.conf"
TUNE_SERVICE="$PREFIX/etc/systemd/system/shr-audio-performance.service"
RUNTIME_HELPER="$PREFIX/usr/local/libexec/shr-audio-tune-runtime"
RUN_DIR="$PREFIX/run/shr-audio-tune"
LIMITS_FILE="$PREFIX/etc/security/limits.d/95-shr-audio.conf"
SELF="$(readlink -f "$0")"
TRANSACTION_ACTIVE=false

usage() {
  cat <<'EOF'
Usage:
  shr-audio-tune plan [CPU]
  sudo shr-audio-tune permissions-install USER
  sudo shr-audio-tune permissions-remove
  sudo shr-audio-tune install [CPU]
  sudo shr-audio-tune recover
  sudo shr-audio-tune remove
  shr-audio-tune status
  shr-audio-tune doctor [CONFIGURED_CPU|none]

plan and status are read-only. install reserves one zero-based CPU for JACK and
SHR-DAW's managed engine, enables a managed performance-governor service for
the next boot, and keeps ordinary IRQs on the housekeeping CPUs. The CPU
defaults to the highest online CPU.

Full-tickless and RCU callback offload are included only when the installed
kernel supports them. install backs up the selected Raspberry Pi boot command
line, records ownership, and never starts or restarts JACK. A reboot is
required for boot isolation.

remove reverses only unchanged settings owned by this helper. It retains the
original boot-command-line backup and leaves later administrator edits alone.
EOF
}

root_path() {
  printf '%s%s\n' "$PREFIX" "$1"
}

need_root() {
  if [[ -z "$PREFIX" && $EUID -ne 0 ]]; then
    printf 'Run this operation with sudo.\n' >&2
    exit 1
  fi
}

clear_directory() {
  local directory=$1
  [[ -d "$directory" ]] || return 0
  find "$directory" -depth -mindepth 1 -delete
  rmdir "$directory"
}

sha_file() {
  [[ -f "$1" ]] || return 1
  sha256sum "$1" | awk '{print $1}'
}

manifest_value() {
  local key=$1
  [[ -f "$MANIFEST" ]] || return 0
  awk -F= -v wanted="$key" '$1 == wanted { value=substr($0, length(wanted) + 2) } END { print value }' \
    "$MANIFEST"
}

online_cpus() {
  local online
  online="$(cat "$(root_path /sys/devices/system/cpu/online)" 2>/dev/null || true)"
  if [[ "$online" =~ ^0-([0-9]+)$ ]]; then
    seq 0 "${BASH_REMATCH[1]}"
  elif [[ "$online" =~ ^[0-9]+$ ]]; then
    printf '%s\n' "$online"
  else
    printf 'Unsupported online CPU layout: %s\n' "${online:-unknown}" >&2
    return 1
  fi
}

boot_cmdline() {
  local candidate
  for candidate in /boot/firmware/cmdline.txt /boot/cmdline.txt; do
    if [[ -f "$(root_path "$candidate")" ]]; then
      printf '%s\n' "$(root_path "$candidate")"
      return 0
    fi
  done
  printf 'No Raspberry Pi boot command line found.\n' >&2
  return 1
}

kernel_release() {
  local release_file
  release_file="$(root_path /proc/sys/kernel/osrelease)"
  if [[ -r "$release_file" ]]; then
    tr -d '\n' <"$release_file"
  elif [[ -z "$PREFIX" ]]; then
    uname -r
  else
    printf 'unknown'
  fi
}

architecture() {
  local arch_file
  arch_file="$(root_path /proc/sys/kernel/arch)"
  if [[ -r "$arch_file" ]]; then
    tr -d '\n' <"$arch_file"
  elif [[ -z "$PREFIX" ]]; then
    uname -m
  else
    printf 'unknown'
  fi
}

kernel_config_file() {
  local release candidate
  release="$(kernel_release)"
  for candidate in \
    "$(root_path "/boot/config-$release")" \
    "$(root_path /proc/config)"; do
    if [[ -r "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

kernel_feature() {
  local feature=$1 config
  config="$(kernel_config_file 2>/dev/null || true)"
  [[ -n "$config" ]] || return 2
  if rg -q "^${feature}=y$" "$config"; then
    return 0
  fi
  return 1
}

platform_kind() {
  if [[ -e "$(root_path /usr/share/doc/patchbox)" ]] ||
     [[ -e "$(root_path /etc/patchbox-release)" ]] ||
     [[ -e "$(root_path /etc/update-motd.d/20-patchbox-motd-header)" ]]; then
    printf 'patchbox'
    return
  fi
  local os_release
  os_release="$(root_path /etc/os-release)"
  if [[ -r "$os_release" ]] && rg -qi 'Raspberry Pi|raspbian' "$os_release"; then
    printf 'raspberry-pi-os'
  elif [[ -r "$(root_path /proc/device-tree/model)" ]] &&
       rg -aq 'Raspberry Pi' "$(root_path /proc/device-tree/model)"; then
    printf 'raspberry-pi-os'
  else
    printf 'debian'
  fi
}

require_pi_audio_platform() {
  local platform arch
  platform="$(platform_kind)"
  arch="$(architecture)"
  if [[ "$platform" != patchbox && "$platform" != raspberry-pi-os ]]; then
    printf 'Dedicated CPU tuning is supported only on detected Raspberry Pi systems.\n' >&2
    return 1
  fi
  if [[ "$arch" != aarch64 ]]; then
    printf 'Dedicated CPU tuning requires 64-bit aarch64; detected %s.\n' "$arch" >&2
    return 1
  fi
}

systemctl_safe() {
  if [[ -n "${SHR_TUNE_SYSTEMCTL:-}" ]]; then
    "$SHR_TUNE_SYSTEMCTL" "$@"
  elif [[ -n "$PREFIX" ]]; then
    return 0
  else
    systemctl "$@"
  fi
}

fixture_systemctl_value() {
  local unit=$1 property=$2 file
  file="$(root_path "/run/shr-audio-tune-fixture/systemctl/$unit.$property")"
  if [[ -r "$file" ]]; then
    sed -n '1p' "$file"
  fi
  return 0
}

systemctl_value() {
  local unit=$1 property=$2
  if [[ -n "$PREFIX" && -z "${SHR_TUNE_SYSTEMCTL:-}" ]]; then
    fixture_systemctl_value "$unit" "$property"
  else
    systemctl_safe show "$unit" "--property=$property" --value 2>/dev/null || true
  fi
}

systemctl_enabled() {
  local unit=$1
  if [[ -n "$PREFIX" && -z "${SHR_TUNE_SYSTEMCTL:-}" ]]; then
    fixture_systemctl_value "$unit" enabled
  else
    systemctl_safe is-enabled "$unit" 2>/dev/null || true
  fi
}

systemctl_active() {
  local unit=$1
  if [[ -n "$PREFIX" && -z "${SHR_TUNE_SYSTEMCTL:-}" ]]; then
    fixture_systemctl_value "$unit" active
  else
    systemctl_safe is-active "$unit" 2>/dev/null || true
  fi
}

audio_policy_ready() {
  local file
  local rt_soft=false rt_hard=false memlock_soft=false memlock_hard=false
  for file in "$(root_path /etc/security/limits.conf)" \
    "$(root_path /etc/security/limits.d)"/*.conf; do
    [[ -r "$file" ]] || continue
    if awk '
      $1 == "@audio" && ($2 == "-" || $2 == "soft") &&
      $3 == "rtprio" && $4 + 0 >= 95 { found=1 }
      END { exit !found }
    ' "$file"; then
      rt_soft=true
    fi
    if awk '
      $1 == "@audio" && ($2 == "-" || $2 == "hard") &&
      $3 == "rtprio" && $4 + 0 >= 95 { found=1 }
      END { exit !found }
    ' "$file"; then
      rt_hard=true
    fi
    if awk '
      $1 == "@audio" && ($2 == "-" || $2 == "soft") &&
      $3 == "memlock" && $4 == "unlimited" { found=1 }
      END { exit !found }
    ' "$file"; then
      memlock_soft=true
    fi
    if awk '
      $1 == "@audio" && ($2 == "-" || $2 == "hard") &&
      $3 == "memlock" && $4 == "unlimited" { found=1 }
      END { exit !found }
    ' "$file"; then
      memlock_hard=true
    fi
  done
  $rt_soft && $rt_hard && $memlock_soft && $memlock_hard
}

user_exists() {
  local user=$1
  if [[ -n "$PREFIX" ]]; then
    awk -F: -v wanted="$user" '$1 == wanted { found=1 } END { exit !found }' \
      "$(root_path /etc/passwd)"
  else
    getent passwd "$user" >/dev/null
  fi
}

user_in_audio_group() {
  local user=$1
  if [[ -n "$PREFIX" ]]; then
    awk -F: -v wanted="$user" '
      $1 == "audio" {
        count=split($4, members, ",")
        for (i=1; i<=count; i++) if (members[i] == wanted) found=1
      }
      END { exit !found }
    ' "$(root_path /etc/group)"
  else
    id -nG "$user" 2>/dev/null | tr ' ' '\n' | rg -qx audio
  fi
}

audio_group_exists() {
  if [[ -n "$PREFIX" ]]; then
    awk -F: '$1 == "audio" { found=1 } END { exit !found }' \
      "$(root_path /etc/group)"
  else
    getent group audio >/dev/null
  fi
}

restore_permissions_transaction() {
  local result=${1:-0}
  [[ -d "$PERMISSIONS_TRANSACTION_DIR" ]] || return "$result"
  set +e
  trap - ERR INT TERM HUP
  local conflict=false group_file expected current before_hash
  group_file="$(cat "$PERMISSIONS_TRANSACTION_DIR/group.path" 2>/dev/null || true)"
  expected="$(cat "$PERMISSIONS_TRANSACTION_DIR/group.after-sha" 2>/dev/null || true)"
  current="$(sha_file "$group_file")"
  before_hash="$(sha_file "$PERMISSIONS_TRANSACTION_DIR/group.before")"
  if [[ -n "$group_file" && -f "$PERMISSIONS_TRANSACTION_DIR/group.before" ]]; then
    if [[ "$current" == "$expected" ]]; then
      if [[ -z "$PREFIX" &&
            -f "$PERMISSIONS_TRANSACTION_DIR/membership-added" ]]; then
        local transaction_user
        transaction_user="$(cat "$PERMISSIONS_TRANSACTION_DIR/user")"
        gpasswd -d "$transaction_user" audio >/dev/null 2>&1 || conflict=true
        if [[ "$(sha_file "$group_file")" != "$before_hash" ]]; then
          conflict=true
        fi
      else
        cp -p "$PERMISSIONS_TRANSACTION_DIR/group.before" "$group_file"
      fi
    elif [[ "$current" != "$before_hash" ]]; then
      printf 'Administrator group edit detected; recovery left %s untouched.\n' "$group_file"
      conflict=true
    fi
  fi

  expected="$(cat "$PERMISSIONS_TRANSACTION_DIR/policy.after-sha" 2>/dev/null || true)"
  current="$(sha_file "$LIMITS_FILE")"
  if [[ -f "$PERMISSIONS_TRANSACTION_DIR/policy.before" ]]; then
    if [[ "$current" == "$expected" ]]; then
      cp -p "$PERMISSIONS_TRANSACTION_DIR/policy.before" "$LIMITS_FILE"
    elif [[ "$current" != "$(sha_file "$PERMISSIONS_TRANSACTION_DIR/policy.before")" ]]; then
      printf 'Administrator limits edit detected; recovery left %s untouched.\n' "$LIMITS_FILE"
      conflict=true
    fi
  elif [[ -f "$PERMISSIONS_TRANSACTION_DIR/policy.created" ]]; then
    if [[ "$current" == "$expected" ]]; then
      find "$LIMITS_FILE" -maxdepth 0 -type f -delete
    elif [[ -e "$LIMITS_FILE" ]]; then
      printf 'Administrator limits edit detected; recovery left %s untouched.\n' "$LIMITS_FILE"
      conflict=true
    fi
  fi

  if $conflict; then
    printf '%s\n' \
      'Permission recovery retained the transaction ledger.' \
      'Resolve the named administrator edit, then rerun sudo shr-audio-tune recover.' >&2
    set -e
    return 1
  fi

  find "$STATE_DIR" -maxdepth 1 -type f -name 'permissions.*' -delete
  if [[ -d "$PERMISSIONS_TRANSACTION_DIR/state.before" ]]; then
    cp -p "$PERMISSIONS_TRANSACTION_DIR/state.before/"* "$STATE_DIR/" 2>/dev/null || true
  fi
  clear_directory "$PERMISSIONS_TRANSACTION_DIR"
  printf 'Interrupted real-time permission change recovered.\n'
  set -e
  return "$result"
}

permissions_install() {
  need_root
  local user=${1:-}
  [[ -n "$user" ]] || { printf 'permissions-install requires a user name.\n' >&2; exit 2; }
  user_exists "$user" || { printf 'User does not exist: %s\n' "$user" >&2; exit 1; }
  audio_group_exists || {
    printf 'The audio group is absent; install jackd2 before configuring its policy.\n' >&2
    exit 1
  }
  mkdir -p "$STATE_DIR"
  if [[ -d "$TRANSACTION_DIR" ]]; then
    printf 'An interrupted CPU-tuning change exists. Run sudo shr-audio-tune recover first.\n' >&2
    exit 1
  fi
  if [[ -d "$PERMISSIONS_TRANSACTION_DIR" ]]; then
    printf 'An interrupted permission change exists. Run sudo shr-audio-tune recover first.\n' >&2
    exit 1
  fi
  if [[ -f "$STATE_DIR/permissions.user" ]] &&
     [[ "$(cat "$STATE_DIR/permissions.user")" != "$user" ]]; then
    printf 'Audio permissions are already managed for %s; remove them first.\n' \
      "$(cat "$STATE_DIR/permissions.user")" >&2
    exit 1
  fi

  local group_file group_before group_after membership_added=false policy_added=false
  local policy_needed=false limits_tmp='' limits_hash='' group_tmp=''
  group_file="$(root_path /etc/group)"
  group_before="$(sha_file "$group_file")"

  if ! audio_policy_ready; then
    policy_needed=true
    if [[ -e "$LIMITS_FILE" && ! -f "$STATE_DIR/permissions.policy-owned" ]]; then
      printf 'Refusing to replace administrator-owned limits file: %s\n' "$LIMITS_FILE" >&2
      exit 1
    fi
    mkdir -p "${LIMITS_FILE%/*}"
    limits_tmp="$(mktemp "${LIMITS_FILE}.tmp.XXXXXX")"
    printf '%s\n' \
      '# Managed by shr-audio-tune. JACK clients require both limits.' \
      '@audio - rtprio 95' \
      '@audio - memlock unlimited' >"$limits_tmp"
    chmod 0644 "$limits_tmp"
    limits_hash="$(sha_file "$limits_tmp")"
  fi

  group_tmp="$(mktemp "${group_file}.tmp.XXXXXX")"
  awk -F: -v OFS=: -v wanted="$user" '
    $1 == "audio" {
      count=split($4, members, ",")
      present=0
      for (i=1; i<=count; i++) if (members[i] == wanted) present=1
      if (!present) {
        if ($4 == "") $4=wanted
        else $4=$4 "," wanted
      }
    }
    { print }
  ' "$group_file" >"$group_tmp"
  chmod --reference="$group_file" "$group_tmp" 2>/dev/null || true

  mkdir "$PERMISSIONS_TRANSACTION_DIR"
  printf '%s\n' "$group_file" >"$PERMISSIONS_TRANSACTION_DIR/group.path"
  cp -p "$group_file" "$PERMISSIONS_TRANSACTION_DIR/group.before"
  sha_file "$group_tmp" >"$PERMISSIONS_TRANSACTION_DIR/group.after-sha"
  printf '%s\n' "$user" >"$PERMISSIONS_TRANSACTION_DIR/user"
  if ! user_in_audio_group "$user"; then
    touch "$PERMISSIONS_TRANSACTION_DIR/membership-added"
  fi
  mkdir "$PERMISSIONS_TRANSACTION_DIR/state.before"
  cp -p "$STATE_DIR"/permissions.* \
    "$PERMISSIONS_TRANSACTION_DIR/state.before/" 2>/dev/null || true
  if [[ -f "$LIMITS_FILE" ]]; then
    cp -p "$LIMITS_FILE" "$PERMISSIONS_TRANSACTION_DIR/policy.before"
  else
    touch "$PERMISSIONS_TRANSACTION_DIR/policy.created"
  fi
  if $policy_needed; then
    printf '%s\n' "$limits_hash" >"$PERMISSIONS_TRANSACTION_DIR/policy.after-sha"
  else
    sha_file "$LIMITS_FILE" >"$PERMISSIONS_TRANSACTION_DIR/policy.after-sha"
  fi
  trap 'restore_permissions_transaction $?' ERR INT TERM HUP

  if $policy_needed; then
    mv "$limits_tmp" "$LIMITS_FILE"
    limits_tmp=
    policy_added=true
    touch "$STATE_DIR/permissions.policy-owned"
  fi

  if ! user_in_audio_group "$user"; then
    if [[ -n "$PREFIX" ]]; then
      mv "$group_tmp" "$group_file"
      group_tmp=
    else
      find "$group_tmp" -maxdepth 0 -delete
      group_tmp=
      usermod -a -G audio "$user"
    fi
    membership_added=true
  else
    find "$group_tmp" -maxdepth 0 -delete
    group_tmp=
  fi
  group_after="$(sha_file "$group_file")"

  printf '%s\n' "$user" >"$STATE_DIR/permissions.user"
  printf '%s\n' "$group_before" >"$STATE_DIR/permissions.group-before"
  printf '%s\n' "$group_after" >"$STATE_DIR/permissions.group-after"
  printf '%s\n' "$membership_added" >"$STATE_DIR/permissions.membership-added"
  printf '%s\n' "$policy_added" >"$STATE_DIR/permissions.policy-added"
  if [[ -f "$LIMITS_FILE" ]]; then
    sha_file "$LIMITS_FILE" >"$STATE_DIR/permissions.policy-hash"
  fi
  clear_directory "$PERMISSIONS_TRANSACTION_DIR"
  trap - ERR INT TERM HUP
  printf 'Real-time audio policy is configured for %s.\n' "$user"
  printf 'Log out and back in before judging the live rtprio/memlock limits.\n'
}

permissions_remove() {
  need_root
  if [[ -d "$PERMISSIONS_TRANSACTION_DIR" ]]; then
    printf 'Recover the interrupted permission change before removal.\n' >&2
    exit 1
  fi
  if [[ ! -f "$STATE_DIR/permissions.user" ]]; then
    printf 'SHR-DAW does not own an audio-permissions change.\n'
    return 0
  fi
  local user group_file expected current
  user="$(cat "$STATE_DIR/permissions.user")"
  group_file="$(root_path /etc/group)"
  expected="$(cat "$STATE_DIR/permissions.group-after" 2>/dev/null || true)"
  current="$(sha_file "$group_file")"
  if [[ "$(cat "$STATE_DIR/permissions.membership-added" 2>/dev/null || true)" == true ]]; then
    if [[ -n "$expected" && "$current" == "$expected" ]]; then
      if [[ -n "$PREFIX" ]]; then
        local tmp
        tmp="$(mktemp "${group_file}.tmp.XXXXXX")"
        awk -F: -v OFS=: -v wanted="$user" '
          $1 == "audio" {
            count=split($4, members, ",")
            output=""
            for (i=1; i<=count; i++) {
              if (members[i] != wanted && members[i] != "")
                output=(output == "" ? members[i] : output "," members[i])
            }
            $4=output
          }
          { print }
        ' "$group_file" >"$tmp"
        chmod --reference="$group_file" "$tmp" 2>/dev/null || true
        mv "$tmp" "$group_file"
      else
        gpasswd -d "$user" audio >/dev/null
      fi
      printf 'Removed helper-added audio-group membership for %s.\n' "$user"
    else
      printf 'Manual administrator group edits detected; audio membership for %s was untouched.\n' "$user"
    fi
  fi
  if [[ -f "$STATE_DIR/permissions.policy-owned" && -f "$LIMITS_FILE" ]]; then
    expected="$(cat "$STATE_DIR/permissions.policy-hash" 2>/dev/null || true)"
    current="$(sha_file "$LIMITS_FILE")"
    if [[ -n "$expected" && "$current" == "$expected" ]]; then
      find "$LIMITS_FILE" -maxdepth 0 -delete
      printf 'Removed the unchanged helper-owned real-time limits file.\n'
    else
      printf 'Manual administrator limits edits detected; %s was untouched.\n' "$LIMITS_FILE"
    fi
  fi
  find "$STATE_DIR" -maxdepth 1 -type f -name 'permissions.*' -delete
}

runtime_start() {
  need_root
  mkdir -p "$RUN_DIR"
  : >"$RUN_DIR/governors"
  local governor current available
  for governor in "$(root_path /sys/devices/system/cpu/cpufreq)"/policy*/scaling_governor; do
    [[ -w "$governor" ]] || continue
    current="$(cat "$governor")"
    available="$(cat "${governor%/*}/scaling_available_governors" 2>/dev/null || true)"
    printf '%s|%s\n' "$governor" "$current" >>"$RUN_DIR/governors"
    if [[ " $available " == *' performance '* ]]; then
      printf 'performance\n' >"$governor"
    fi
  done
}

runtime_stop() {
  need_root
  local governor previous
  if [[ -f "$RUN_DIR/governors" ]]; then
    while IFS='|' read -r governor previous; do
      [[ -w "$governor" && -n "$previous" ]] && printf '%s\n' "$previous" >"$governor"
    done <"$RUN_DIR/governors"
  fi
  clear_directory "$RUN_DIR"
}

plan_tuning() {
  require_pi_audio_platform
  local -a cpus=()
  mapfile -t cpus < <(online_cpus)
  ((${#cpus[@]} >= 4)) || {
    printf 'Dedicated-core mode requires at least four online CPUs.\n' >&2
    return 1
  }
  local cpu="${1:-${cpus[-1]}}" item found=false
  [[ "$cpu" =~ ^[0-9]+$ ]] || { printf 'CPU must be a zero-based number.\n' >&2; return 2; }
  local -a housekeeping=()
  for item in "${cpus[@]}"; do
    if [[ "$item" == "$cpu" ]]; then found=true; else housekeeping+=("$item"); fi
  done
  $found || { printf 'CPU %s is not online.\n' "$cpu" >&2; return 1; }
  local housekeeping_csv
  housekeeping_csv="$(IFS=,; printf '%s' "${housekeeping[*]}")"
  printf 'Platform baseline: %s (%s)\n' "$(platform_kind)" "$(architecture)"
  printf 'Audio CPU: %s; housekeeping/build/disk CPUs: %s\n' "$cpu" "$housekeeping_csv"
  printf 'Boot isolation: isolcpus=domain,managed_irq,%s; irqaffinity=%s\n' \
    "$cpu" "$housekeeping_csv"
  if kernel_feature CONFIG_NO_HZ_FULL; then
    printf 'Full tickless: supported; nohz_full=%s will be configured.\n' "$cpu"
  else
    case $? in
      1) printf 'Full tickless: unsupported by this kernel; nohz_full will be omitted.\n' ;;
      *) printf 'Full tickless: kernel configuration unavailable; nohz_full will be omitted safely.\n' ;;
    esac
  fi
  if kernel_feature CONFIG_RCU_NOCB_CPU; then
    printf 'RCU callback offload: supported; rcu_nocbs=%s will be configured.\n' "$cpu"
  else
    case $? in
      1) printf 'RCU callback offload: unsupported by this kernel; rcu_nocbs will be omitted.\n' ;;
      *) printf 'RCU callback offload: kernel configuration unavailable; rcu_nocbs will be omitted safely.\n' ;;
    esac
  fi
  printf '%s\n' \
    'JACK will be pinned on its next start; the managed synth inherits the same CPU.' \
    'The performance governor begins on the next boot. JACK is not started or restarted.' \
    'The isolated CPU remains unavailable to ordinary work until removal and reboot.'
}

rollback_transaction() {
  local result=${1:-$?}
  $TRANSACTION_ACTIVE || return "$result"
  set +e
  trap - ERR INT TERM HUP
  local cmdline path
  cmdline="$(cat "$TRANSACTION_DIR/cmdline.path" 2>/dev/null || true)"
  if [[ -n "$cmdline" && -f "$TRANSACTION_DIR/cmdline.before" ]]; then
    cp -p "$TRANSACTION_DIR/cmdline.before" "$cmdline"
  fi
  for path in \
    "$JACK_DROPIN" "$TUNE_SERVICE" "$RUNTIME_HELPER" \
    "$STATE_DIR/cpu" "$STATE_DIR/housekeeping" "$MANIFEST" "$STATE_DIR/installed"; do
    local name="${path##*/}"
    if [[ -f "$TRANSACTION_DIR/$name.before" ]]; then
      mkdir -p "${path%/*}"
      cp -p "$TRANSACTION_DIR/$name.before" "$path"
    elif [[ -f "$TRANSACTION_DIR/$name.created" ]]; then
      find "$path" -maxdepth 0 -type f -delete 2>/dev/null || true
    fi
  done
  if [[ -f "$TRANSACTION_DIR/service-newly-enabled" ]]; then
    systemctl_safe disable shr-audio-performance.service >/dev/null 2>&1 || true
  fi
  clear_directory "$TRANSACTION_DIR" 2>/dev/null || true
  TRANSACTION_ACTIVE=false
  printf 'Install failed; helper-owned files and the boot command line were rolled back.\n' >&2
  return "$result"
}

begin_transaction() {
  local cmdline=$1 path name
  mkdir -p "$STATE_DIR"
  if [[ -d "$TRANSACTION_DIR" ]]; then
    printf 'An interrupted transaction exists. Run sudo shr-audio-tune recover first.\n' >&2
    exit 1
  fi
  mkdir "$TRANSACTION_DIR"
  printf '%s\n' "$cmdline" >"$TRANSACTION_DIR/cmdline.path"
  cp -p "$cmdline" "$TRANSACTION_DIR/cmdline.before"
  for path in \
    "$JACK_DROPIN" "$TUNE_SERVICE" "$RUNTIME_HELPER" \
    "$STATE_DIR/cpu" "$STATE_DIR/housekeeping" "$MANIFEST" "$STATE_DIR/installed"; do
    name="${path##*/}"
    if [[ -f "$path" ]]; then
      cp -p "$path" "$TRANSACTION_DIR/$name.before"
    else
      touch "$TRANSACTION_DIR/$name.created"
    fi
  done
  TRANSACTION_ACTIVE=true
  trap rollback_transaction ERR INT TERM HUP
}

install_tuning() {
  need_root
  require_pi_audio_platform
  if [[ -d "$PERMISSIONS_TRANSACTION_DIR" ]]; then
    printf 'Recover the interrupted permission change before CPU tuning.\n' >&2
    exit 1
  fi
  local -a cpus=()
  mapfile -t cpus < <(online_cpus)
  ((${#cpus[@]} >= 4)) || {
    printf 'Dedicated-core mode requires at least four online CPUs.\n' >&2
    exit 1
  }
  local cpu="${1:-${cpus[-1]}}" found=false item
  [[ "$cpu" =~ ^[0-9]+$ ]] || { printf 'CPU must be a zero-based number.\n' >&2; exit 2; }
  local -a housekeeping=()
  for item in "${cpus[@]}"; do
    if [[ "$item" == "$cpu" ]]; then found=true; else housekeeping+=("$item"); fi
  done
  $found || { printf 'CPU %s is not online.\n' "$cpu" >&2; exit 1; }
  local housekeeping_csv
  housekeeping_csv="$(IFS=,; printf '%s' "${housekeeping[*]}")"

  mkdir -p "$STATE_DIR"
  if [[ -f "$STATE_DIR/cpu" ]]; then
    local installed_cpu
    installed_cpu="$(cat "$STATE_DIR/cpu")"
    [[ "$installed_cpu" == "$cpu" ]] || {
      printf 'Tuning is already installed for CPU %s; remove it first.\n' "$installed_cpu" >&2
      exit 1
    }
  fi

  local cmdline line key managed
  cmdline="$(boot_cmdline)"
  line="$(tr '\n' ' ' <"$cmdline" | xargs)"
  [[ -n "$line" ]] || { printf 'Boot command line is empty.\n' >&2; exit 1; }
  for key in isolcpus nohz_full rcu_nocbs irqaffinity; do
    if [[ " $line " == *" $key="* && ! -f "$STATE_DIR/cpu" ]]; then
      printf 'Existing %s setting was not created by SHR-DAW; refusing to overwrite it.\n' "$key" >&2
      exit 1
    fi
  done
  for managed in "$JACK_DROPIN" "$TUNE_SERVICE" "$RUNTIME_HELPER"; do
    if [[ -e "$managed" && ! -f "$STATE_DIR/installed" ]]; then
      printf 'Refusing to replace pre-existing path: %s\n' "$managed" >&2
      exit 1
    fi
  done
  if [[ ! -f "$STATE_DIR/cmdline.original" ]]; then
    cp -p "$cmdline" "$STATE_DIR/cmdline.original"
    printf '%s\n' "$cmdline" >"$STATE_DIR/cmdline.path"
  fi

  begin_transaction "$cmdline"
  printf '%s\n' "$cpu" | sha256sum | awk '{print $1}' \
    >"$TRANSACTION_DIR/cpu.after-sha"
  printf '%s\n' "$housekeeping_csv" | sha256sum | awk '{print $1}' \
    >"$TRANSACTION_DIR/housekeeping.after-sha"
  printf '%s\n' "$cpu" >"$STATE_DIR/cpu"
  printf '%s\n' "$housekeeping_csv" >"$STATE_DIR/housekeeping"

  for key in isolcpus nohz_full rcu_nocbs irqaffinity; do
    line="$(printf '%s\n' "$line" | tr ' ' '\n' | awk -F= -v key="$key" '$1 != key' | paste -sd' ' -)"
  done
  local -a tokens=("isolcpus=domain,managed_irq,$cpu" "irqaffinity=$housekeeping_csv")
  kernel_feature CONFIG_NO_HZ_FULL && tokens+=("nohz_full=$cpu")
  kernel_feature CONFIG_RCU_NOCB_CPU && tokens+=("rcu_nocbs=$cpu")
  line="$line ${tokens[*]}"
  local cmdline_tmp
  cmdline_tmp="$(mktemp "${cmdline}.tmp.XXXXXX")"
  printf '%s\n' "$line" >"$cmdline_tmp"
  chmod --reference="$cmdline" "$cmdline_tmp" 2>/dev/null || true
  sha_file "$cmdline_tmp" >"$TRANSACTION_DIR/cmdline.after-sha"
  mv "$cmdline_tmp" "$cmdline"

  sha_file "$SELF" >"$TRANSACTION_DIR/shr-audio-tune-runtime.after-sha"
  install -Dm755 "$SELF" "$RUNTIME_HELPER"
  mkdir -p "${JACK_DROPIN%/*}"
  local dropin_tmp service_tmp
  dropin_tmp="$(mktemp "${JACK_DROPIN}.tmp.XXXXXX")"
  printf '%s\n' \
    '# Managed by shr-audio-tune. Applied on the next JACK start.' \
    '[Service]' \
    "CPUAffinity=$cpu" \
    'LimitRTPRIO=95' \
    'LimitMEMLOCK=infinity' >"$dropin_tmp"
  chmod 0644 "$dropin_tmp"
  sha_file "$dropin_tmp" >"$TRANSACTION_DIR/90-shr-audio-cpu.conf.after-sha"
  mv "$dropin_tmp" "$JACK_DROPIN"

  mkdir -p "${TUNE_SERVICE%/*}"
  service_tmp="$(mktemp "${TUNE_SERVICE}.tmp.XXXXXX")"
  printf '%s\n' \
    '# Managed by shr-audio-tune.' \
    '[Unit]' \
    'Description=SHR-DAW conservative audio performance tuning' \
    'Before=jack.service' \
    '' \
    '[Service]' \
    'Type=oneshot' \
    'RemainAfterExit=yes' \
    'ExecStart=/usr/local/libexec/shr-audio-tune-runtime runtime-start' \
    'ExecStop=/usr/local/libexec/shr-audio-tune-runtime runtime-stop' \
    '' \
    '[Install]' \
    'WantedBy=multi-user.target' >"$service_tmp"
  chmod 0644 "$service_tmp"
  sha_file "$service_tmp" >"$TRANSACTION_DIR/shr-audio-performance.service.after-sha"
  mv "$service_tmp" "$TUNE_SERVICE"

  local enabled_before
  enabled_before="$(systemctl_enabled shr-audio-performance.service)"
  if ! systemctl_safe daemon-reload; then
    rollback_transaction 1
    return 1
  fi
  if [[ "$enabled_before" != enabled ]]; then
    touch "$TRANSACTION_DIR/service-newly-enabled"
  fi
  if ! systemctl_safe enable shr-audio-performance.service; then
    rollback_transaction 1
    return 1
  fi

  local manifest_tmp token
  manifest_tmp="$(mktemp "${MANIFEST}.tmp.XXXXXX")"
  {
    printf 'schema=2\n'
    printf 'cpu=%s\n' "$cpu"
    printf 'housekeeping=%s\n' "$housekeeping_csv"
    printf 'cmdline=%s\n' "$cmdline"
    for token in "${tokens[@]}"; do printf 'token=%s\n' "$token"; done
    printf 'jack_dropin_sha=%s\n' "$(sha_file "$JACK_DROPIN")"
    printf 'service_sha=%s\n' "$(sha_file "$TUNE_SERVICE")"
    printf 'runtime_sha=%s\n' "$(sha_file "$RUNTIME_HELPER")"
  } >"$manifest_tmp"
  chmod 0600 "$manifest_tmp"
  sha_file "$manifest_tmp" >"$TRANSACTION_DIR/manifest.after-sha"
  : | sha256sum | awk '{print $1}' >"$TRANSACTION_DIR/installed.after-sha"
  mv "$manifest_tmp" "$MANIFEST"
  touch "$STATE_DIR/installed"
  clear_directory "$TRANSACTION_DIR"
  TRANSACTION_ACTIVE=false
  trap - ERR INT TERM HUP
  printf 'Installed dedicated audio CPU %s; housekeeping CPUs: %s.\n' "$cpu" "$housekeeping_csv"
  printf 'JACK was not restarted and the governor was not changed live.\n'
  printf 'Reboot when convenient, then set audio.engine_cpu=%s.\n' "$cpu"
}

recover_tuning() {
  need_root
  local recovered=false
  if [[ -d "$PERMISSIONS_TRANSACTION_DIR" ]]; then
    if restore_permissions_transaction 0; then
      recovered=true
    else
      return 1
    fi
  fi
  if [[ ! -d "$TRANSACTION_DIR" ]]; then
    if ! $recovered; then
      printf 'No interrupted SHR-DAW audio-tuning transaction exists.\n'
    fi
    return 0
  fi
  local cmdline path name expected current before conflict=false
  cmdline="$(cat "$TRANSACTION_DIR/cmdline.path" 2>/dev/null || true)"
  if [[ -n "$cmdline" && -f "$TRANSACTION_DIR/cmdline.before" ]]; then
    expected="$(cat "$TRANSACTION_DIR/cmdline.after-sha" 2>/dev/null || true)"
    current="$(sha_file "$cmdline")"
    before="$(sha_file "$TRANSACTION_DIR/cmdline.before")"
    if [[ -n "$expected" && "$current" == "$expected" ]]; then
      cp -p "$TRANSACTION_DIR/cmdline.before" "$cmdline"
      printf 'Restored the pre-transaction boot command line.\n'
    elif [[ "$current" != "$before" ]]; then
      printf 'Administrator edit detected; recovery left %s untouched.\n' "$cmdline"
      conflict=true
    fi
  fi
  for path in \
    "$JACK_DROPIN" "$TUNE_SERVICE" "$RUNTIME_HELPER" \
    "$STATE_DIR/cpu" "$STATE_DIR/housekeeping" "$MANIFEST" "$STATE_DIR/installed"; do
    name="${path##*/}"
    if [[ -f "$TRANSACTION_DIR/$name.before" ]]; then
      expected="$(cat "$TRANSACTION_DIR/$name.after-sha" 2>/dev/null || true)"
      current="$(sha_file "$path")"
      before="$(sha_file "$TRANSACTION_DIR/$name.before")"
      if [[ -n "$expected" && "$current" == "$expected" ]]; then
        mkdir -p "${path%/*}"
        cp -p "$TRANSACTION_DIR/$name.before" "$path"
      elif [[ "$current" != "$before" ]]; then
        printf 'Administrator edit detected; recovery left %s untouched.\n' "$path"
        conflict=true
      fi
    elif [[ -f "$TRANSACTION_DIR/$name.created" && -f "$path" ]]; then
      expected="$(cat "$TRANSACTION_DIR/$name.after-sha" 2>/dev/null || true)"
      current="$(sha_file "$path")"
      if [[ -n "$expected" && "$current" == "$expected" ]]; then
        find "$path" -maxdepth 0 -type f -delete
      else
        printf 'Administrator edit detected; recovery left %s untouched.\n' "$path"
        conflict=true
      fi
    fi
  done
  if $conflict; then
    systemctl_safe daemon-reload
    printf '%s\n' \
      'Recovery retained the transaction ledger because administrator edits were found.' \
      'Resolve the named paths, then rerun sudo shr-audio-tune recover.' >&2
    return 1
  fi
  if [[ -f "$TRANSACTION_DIR/service-newly-enabled" ]]; then
    systemctl_safe disable shr-audio-performance.service
  fi
  systemctl_safe daemon-reload
  clear_directory "$TRANSACTION_DIR"
  printf 'Interrupted tuning state recovered. Rerun install when ready.\n'
}

remove_owned_file() {
  local path=$1 expected_key=$2 label=$3 expected current
  [[ -f "$path" ]] || return 0
  expected="$(manifest_value "$expected_key")"
  current="$(sha_file "$path")"
  if [[ -n "$expected" && "$current" == "$expected" ]]; then
    find "$path" -maxdepth 0 -type f -delete
    return 0
  fi
  if [[ ! -f "$MANIFEST" ]] && rg -q '^# Managed by shr-audio-tune' "$path"; then
    find "$path" -maxdepth 0 -type f -delete
    return 0
  fi
  printf 'Manual administrator edit detected; %s was left untouched: %s\n' "$label" "$path"
}

remove_tuning() {
  need_root
  if [[ -d "$TRANSACTION_DIR" || -d "$PERMISSIONS_TRANSACTION_DIR" ]]; then
    printf 'Recover the interrupted transaction before removal.\n' >&2
    exit 1
  fi
  if [[ ! -f "$STATE_DIR/cpu" ]]; then
    printf 'SHR-DAW audio CPU tuning is not installed.\n'
    return 0
  fi
  local cpu housekeeping cmdline line token
  cpu="$(cat "$STATE_DIR/cpu")"
  housekeeping="$(cat "$STATE_DIR/housekeeping")"
  cmdline="$(cat "$STATE_DIR/cmdline.path")"
  if [[ -f "$cmdline" ]]; then
    line="$(tr '\n' ' ' <"$cmdline" | xargs)"
    local -a tokens=()
    if [[ -f "$MANIFEST" ]]; then
      mapfile -t tokens < <(sed -n 's/^token=//p' "$MANIFEST")
    else
      tokens=(
        "isolcpus=domain,managed_irq,$cpu"
        "nohz_full=$cpu"
        "rcu_nocbs=$cpu"
        "irqaffinity=$housekeeping"
      )
    fi
    for token in "${tokens[@]}"; do
      line="$(printf '%s\n' "$line" | tr ' ' '\n' | awk -v token="$token" '$0 != token' | paste -sd' ' -)"
    done
    local cmdline_tmp
    cmdline_tmp="$(mktemp "${cmdline}.tmp.XXXXXX")"
    printf '%s\n' "$line" >"$cmdline_tmp"
    chmod --reference="$cmdline" "$cmdline_tmp" 2>/dev/null || true
    mv "$cmdline_tmp" "$cmdline"
  fi
  systemctl_safe disable --now shr-audio-performance.service
  remove_owned_file "$JACK_DROPIN" jack_dropin_sha 'JACK affinity drop-in'
  remove_owned_file "$TUNE_SERVICE" service_sha 'performance service'
  remove_owned_file "$RUNTIME_HELPER" runtime_sha 'runtime helper'
  rmdir "${JACK_DROPIN%/*}" 2>/dev/null || true
  systemctl_safe daemon-reload
  find "$STATE_DIR" -maxdepth 1 -type f \
    \( -name cpu -o -name housekeeping -o -name cmdline.path -o \
       -name installed -o -name manifest \) -delete
  printf 'Removed unchanged SHR-DAW audio CPU tuning.\n'
  printf 'The original backup remains in %s.\n' "$STATE_DIR"
  printf 'Clear audio.engine_cpu in shsynth.conf and reboot when convenient.\n'
}

cpu_list_contains() {
  local list=$1 wanted=$2 part start end
  IFS=, read -ra parts <<<"$list"
  for part in "${parts[@]}"; do
    if [[ "$part" =~ ^([0-9]+)-([0-9]+)$ ]]; then
      start="${BASH_REMATCH[1]}"
      end="${BASH_REMATCH[2]}"
      ((wanted >= start && wanted <= end)) && return 0
    elif [[ "$part" == "$wanted" ]]; then
      return 0
    fi
  done
  return 1
}

cpu_csv_hex_mask() {
  local csv=$1 cpu mask=0
  IFS=, read -ra mask_cpus <<<"$csv"
  for cpu in "${mask_cpus[@]}"; do
    [[ "$cpu" =~ ^[0-9]+$ ]] || return 1
    ((cpu < 63)) || return 1
    ((mask |= 1 << cpu))
  done
  printf '%x\n' "$mask"
}

normalize_hex_mask() {
  local value=${1//,/}
  value="${value#0x}"
  value="$(printf '%s' "$value" | sed 's/^0*//')"
  printf '%s\n' "${value:-0}"
}

jack_process_count() {
  local comm count=0
  for comm in "$(root_path /proc)"/[0-9]*/comm; do
    [[ -r "$comm" ]] || continue
    [[ "$(cat "$comm")" == jackd ]] && ((count += 1))
  done
  printf '%s\n' "$count"
}

jack_process_pid() {
  local comm
  for comm in "$(root_path /proc)"/[0-9]*/comm; do
    [[ -r "$comm" ]] || continue
    if [[ "$(cat "$comm")" == jackd ]]; then
      basename "${comm%/comm}"
      return 0
    fi
  done
  return 1
}

jack_config_command() {
  local file=$1
  awk '
    /^[[:space:]]*($|#)/ { next }
    {
      sub(/^[[:space:]]*exec[[:space:]]+/, "")
      print
      exit
    }
  ' "$file" | xargs
}

live_process_command() {
  local pid=$1 file
  file="$(root_path "/proc/$pid/cmdline")"
  [[ -r "$file" ]] || return 1
  tr '\0' ' ' <"$file" | xargs
}

throttled_flags() {
  local fixture_value
  fixture_value="$(root_path /run/shr-audio-tune-fixture/throttled)"
  if [[ -r "$fixture_value" ]]; then
    sed -n '1p' "$fixture_value"
  elif [[ -z "$PREFIX" ]] && command -v vcgencmd >/dev/null 2>&1; then
    vcgencmd get_throttled 2>/dev/null | sed -n 's/^throttled=//p'
  fi
}

status() {
  local cpu='not installed'
  [[ -f "$STATE_DIR/cpu" ]] && cpu="$(cat "$STATE_DIR/cpu")"
  printf 'Platform: %s; architecture: %s; kernel: %s\n' \
    "$(platform_kind)" "$(architecture)" "$(kernel_release)"
  printf 'Dedicated audio CPU intent: %s\n' "$cpu"
  if kernel_feature CONFIG_PREEMPT_RT; then
    printf 'Kernel preemption: PREEMPT_RT (not required by SHR-DAW)\n'
  elif kernel_feature CONFIG_PREEMPT; then
    printf 'Kernel preemption: PREEMPT\n'
  else
    printf 'Kernel preemption: unsupported or configuration unavailable\n'
  fi
  if kernel_feature CONFIG_NO_HZ_FULL; then
    printf 'Kernel full-tickless support: yes\n'
  else
    printf 'Kernel full-tickless support: no/unknown\n'
  fi
  if kernel_feature CONFIG_RCU_NOCB_CPU; then
    printf 'Kernel RCU callback-offload support: yes\n'
  else
    printf 'Kernel RCU callback-offload support: no/unknown\n'
  fi
  local boot live isolated
  boot="$(boot_cmdline 2>/dev/null || true)"
  live="$(cat "$(root_path /proc/cmdline)" 2>/dev/null || true)"
  isolated="$(cat "$(root_path /sys/devices/system/cpu/isolated)" 2>/dev/null || true)"
  printf 'Boot command line: %s\n' "${boot:-unsupported}"
  printf 'Live isolated CPUs: %s\n' "${isolated:-none}"
  if [[ "$cpu" != 'not installed' ]]; then
    if cpu_list_contains "$isolated" "$cpu"; then
      printf 'Scheduler-domain isolation active: yes\n'
    elif [[ " $live " == *" isolcpus=domain,managed_irq,$cpu "* ]]; then
      printf 'Scheduler-domain isolation active: partial/unsupported kernel report\n'
    else
      printf 'Scheduler-domain isolation active: no (reboot or repair required)\n'
    fi
  fi
  local governor
  for governor in "$(root_path /sys/devices/system/cpu/cpufreq)"/policy*/scaling_governor; do
    [[ -r "$governor" ]] && printf 'Governor %s: %s\n' "${governor%/*}" "$(cat "$governor")"
  done
  printf 'JACK service: enabled=%s active=%s owner=%s\n' \
    "$(systemctl_enabled jack.service)" "$(systemctl_active jack.service)" \
    "$(systemctl_value jack.service FragmentPath)"
  if [[ -f "$JACK_DROPIN" ]]; then
    printf 'JACK affinity drop-in: %s\n' "$JACK_DROPIN"
  else
    printf 'JACK affinity drop-in: absent\n'
  fi
  printf 'Real-time audio policy configured: %s\n' \
    "$(audio_policy_ready && printf yes || printf no)"
  [[ -f "$STATE_DIR/cmdline.original" ]] &&
    printf 'Rollback backup: %s\n' "$STATE_DIR/cmdline.original"
  [[ -d "$TRANSACTION_DIR" ]] &&
    printf 'Interrupted transaction: yes (run sudo shr-audio-tune recover)\n'
  [[ -d "$PERMISSIONS_TRANSACTION_DIR" ]] &&
    printf 'Interrupted permission change: yes (run sudo shr-audio-tune recover)\n'
}

doctor_line() {
  local state=$1 message=$2
  printf '[%s] %s\n' "$state" "$message"
}

doctor() {
  local configured_cpu=${1:-none} issues=0 cpu='not installed'
  [[ -f "$STATE_DIR/cpu" ]] && cpu="$(cat "$STATE_DIR/cpu")"
  local platform arch
  platform="$(platform_kind)"
  arch="$(architecture)"
  printf 'Audio system configured intent\n'
  if [[ "$platform" =~ ^(patchbox|raspberry-pi-os)$ && "$arch" == aarch64 ]]; then
    doctor_line ready "platform $platform, architecture $arch, kernel $(kernel_release)"
  elif [[ "$configured_cpu" != none || "$cpu" != 'not installed' ]]; then
    doctor_line unsupported \
      "dedicated CPU intent requires a detected 64-bit Raspberry Pi; found $platform/$arch"
    ((issues += 1))
  else
    doctor_line optional \
      "dedicated Raspberry Pi CPU tuning is unavailable on $platform/$arch"
  fi
  if audio_policy_ready; then
    doctor_line ready 'rtprio 95 and unlimited memlock policy is configured for @audio'
  else
    doctor_line partial 'real-time limits are incomplete; run sudo shr-audio-tune permissions-install USER'
    ((issues += 1))
  fi
  local doctor_user live_rtprio live_memlock
  doctor_user="${SHR_TUNE_USER:-${USER:-}}"
  if [[ -n "$doctor_user" ]] && user_exists "$doctor_user" &&
     user_in_audio_group "$doctor_user"; then
    doctor_line ready "$doctor_user is configured in the audio group"
  else
    doctor_line partial "${doctor_user:-current user} is not configured in the audio group; run sudo shr-audio-tune permissions-install USER"
    ((issues += 1))
  fi
  if [[ -n "$PREFIX" && -r "$(root_path /proc/self/limits)" ]]; then
    live_rtprio="$(
      awk '$1 == "Max" && $2 == "realtime" && $3 == "priority" { print $4 }' \
        "$(root_path /proc/self/limits)"
    )"
    live_memlock="$(
      awk '$1 == "Max" && $2 == "locked" && $3 == "memory" { print $4 }' \
        "$(root_path /proc/self/limits)"
    )"
  else
    live_rtprio="$(ulimit -r)"
    live_memlock="$(ulimit -l)"
  fi
  if [[ "$live_rtprio" =~ ^[0-9]+$ ]] && ((live_rtprio >= 95)) &&
     [[ "$live_memlock" == unlimited ]]; then
    doctor_line ready 'live login has rtprio 95 and unlimited memlock'
  else
    doctor_line live-differs \
      "configured policy is not live in this login (rtprio=${live_rtprio:-unknown}, memlock=${live_memlock:-unknown}); log out and back in"
    ((issues += 1))
  fi

  if [[ "$configured_cpu" == none && "$cpu" == 'not installed' ]]; then
    doctor_line optional 'dedicated audio CPU is absent; use shr-audio-tune plan to review the workload tradeoff'
  elif [[ "$configured_cpu" != none && "$cpu" == 'not installed' ]]; then
    doctor_line partial "audio.engine_cpu=$configured_cpu has no helper-owned host tuning; run sudo shr-audio-tune install $configured_cpu"
    ((issues += 1))
  elif [[ "$configured_cpu" == none && "$cpu" != 'not installed' ]]; then
    doctor_line partial "host tuning owns CPU $cpu but audio.engine_cpu is unset; set it or run sudo shr-audio-tune remove"
    ((issues += 1))
  elif [[ "$configured_cpu" != "$cpu" ]]; then
    doctor_line stale "audio.engine_cpu=$configured_cpu differs from helper-owned CPU $cpu"
    ((issues += 1))
  else
    doctor_line ready "helper-owned dedicated audio CPU is $cpu"
  fi

  local cmdline boot_line live isolated housekeeping governor all_governors=true
  cmdline="$(boot_cmdline 2>/dev/null || true)"
  boot_line=''
  if [[ -n "$cmdline" ]]; then
    boot_line="$(tr '\n' ' ' <"$cmdline" | xargs)"
  fi
  live="$(cat "$(root_path /proc/cmdline)" 2>/dev/null || true)"
  isolated="$(cat "$(root_path /sys/devices/system/cpu/isolated)" 2>/dev/null || true)"
  if [[ "$cpu" == 'not installed' ]]; then
    local -a foreign_boot_tokens=()
    mapfile -t foreign_boot_tokens < <(
      printf '%s\n' "$boot_line" | tr ' ' '\n' |
        awk -F= '$1 == "isolcpus" || $1 == "nohz_full" ||
                   $1 == "rcu_nocbs" || $1 == "irqaffinity"'
    )
    if ((${#foreign_boot_tokens[@]})); then
      if [[ "$configured_cpu" == none ]]; then
        doctor_line manual-owner \
          "administrator boot tuning detected without an SHR ledger and intentionally untouched: ${foreign_boot_tokens[*]}"
      else
        doctor_line conflicting-owner \
          "audio.engine_cpu=$configured_cpu conflicts with administrator boot tuning: ${foreign_boot_tokens[*]}"
        ((issues += 1))
      fi
    fi
    local unmanaged_path
    for unmanaged_path in "$JACK_DROPIN" "$TUNE_SERVICE" "$RUNTIME_HELPER"; do
      [[ -e "$unmanaged_path" ]] || continue
      if [[ -f "$unmanaged_path" ]] &&
         rg -q '^# Managed by shr-audio-tune' "$unmanaged_path"; then
        doctor_line stale \
          "SHR-marked file exists without an ownership ledger: $unmanaged_path; inspect and back it up before removal"
        ((issues += 1))
      elif [[ "$configured_cpu" == none ]]; then
        doctor_line manual-owner \
          "administrator tuning path detected and intentionally untouched: $unmanaged_path"
      else
        doctor_line conflicting-owner \
          "administrator tuning path blocks audio.engine_cpu=$configured_cpu: $unmanaged_path"
        ((issues += 1))
      fi
    done
  fi
  if [[ "$cpu" != 'not installed' ]]; then
    housekeeping="$(cat "$STATE_DIR/housekeeping" 2>/dev/null || true)"
    if [[ " $boot_line " != *" isolcpus=domain,managed_irq,$cpu "* ]] ||
       [[ " $boot_line " != *" irqaffinity=$housekeeping "* ]]; then
      doctor_line stale 'persistent boot isolation differs from helper ownership; run sudo shr-audio-tune install again'
      ((issues += 1))
    elif [[ " $live " != *" isolcpus=domain,managed_irq,$cpu "* ]] ||
         ! cpu_list_contains "$isolated" "$cpu"; then
      doctor_line reboot-required 'persistent CPU isolation is ready but not active; reboot at a safe point'
      ((issues += 1))
    else
      doctor_line ready 'scheduler-domain isolation is active'
      local expected_irq_mask live_irq_mask normalized_expected normalized_live
      expected_irq_mask="$(cpu_csv_hex_mask "$housekeeping" 2>/dev/null || true)"
      live_irq_mask="$(
        cat "$(root_path /proc/irq/default_smp_affinity)" 2>/dev/null || true
      )"
      normalized_expected="$(normalize_hex_mask "$expected_irq_mask")"
      normalized_live="$(normalize_hex_mask "$live_irq_mask")"
      if [[ -n "$expected_irq_mask" && -n "$live_irq_mask" ]] &&
         [[ "$normalized_live" == "$normalized_expected" ]]; then
        doctor_line ready \
          "live default IRQ affinity keeps ordinary interrupts on CPUs $housekeeping"
      else
        doctor_line live-differs \
          "live default IRQ affinity is '${live_irq_mask:-unknown}', expected housekeeping CPUs $housekeeping"
        ((issues += 1))
      fi
    fi

    if [[ " $live " == *" nohz_full=$cpu "* ]] && ! kernel_feature CONFIG_NO_HZ_FULL; then
      doctor_line stale 'nohz_full is present but unsupported by this kernel; rerun install to remove the SHR-owned stale token'
      ((issues += 1))
    elif kernel_feature CONFIG_NO_HZ_FULL; then
      if [[ " $live " == *" nohz_full=$cpu "* ]] &&
         cpu_list_contains "$(cat "$(root_path /sys/devices/system/cpu/nohz_full)" 2>/dev/null || true)" "$cpu"; then
        doctor_line ready 'full-tickless isolation is active'
      else
        doctor_line partial 'kernel supports full tickless but live nohz_full state is absent'
        ((issues += 1))
      fi
    else
      doctor_line retained 'kernel lacks full-tickless support; scheduler isolation remains active without it'
    fi

    if [[ " $live " == *" rcu_nocbs=$cpu "* ]] && ! kernel_feature CONFIG_RCU_NOCB_CPU; then
      doctor_line stale 'rcu_nocbs is present but unsupported by this kernel; rerun install to remove the SHR-owned stale token'
      ((issues += 1))
    elif kernel_feature CONFIG_RCU_NOCB_CPU; then
      doctor_line ready 'kernel supports the configured RCU callback offload'
    else
      doctor_line retained 'kernel lacks RCU callback offload; scheduler isolation remains active without it'
    fi

    for governor in "$(root_path /sys/devices/system/cpu/cpufreq)"/policy*/scaling_governor; do
      [[ -r "$governor" ]] || continue
      [[ "$(cat "$governor")" == performance ]] || all_governors=false
    done
    if $all_governors && compgen -G "$(root_path /sys/devices/system/cpu/cpufreq)/policy*/scaling_governor" >/dev/null; then
      doctor_line ready 'performance governor is active'
    elif [[ "$(systemctl_enabled shr-audio-performance.service)" == enabled ]] &&
         [[ "$(systemctl_active shr-audio-performance.service)" != active ]]; then
      doctor_line reboot-required 'performance-governor service is configured but inactive; reboot or start it explicitly when safe'
      ((issues += 1))
    else
      doctor_line partial 'performance governor is not active; run sudo systemctl start shr-audio-performance.service when safe'
      ((issues += 1))
    fi
  fi

  printf '\nAudio system actual/live state\n'
  local jack_enabled jack_active jack_owner jack_count
  jack_enabled="$(systemctl_enabled jack.service)"
  jack_active="$(systemctl_active jack.service)"
  jack_owner="$(systemctl_value jack.service FragmentPath)"
  jack_count="$(jack_process_count)"
  if [[ "$jack_count" -gt 1 ]]; then
    doctor_line duplicate-service "$jack_count jackd processes are live; stop the unintended owner before audio work"
    ((issues += 1))
  elif [[ "$jack_count" -eq 1 ]]; then
    doctor_line ready 'one JACK server process is live'
  elif [[ "$jack_active" == active ]]; then
    doctor_line partial 'jack.service reports active but no jackd process is visible'
    ((issues += 1))
  elif [[ "$jack_enabled" == enabled ]]; then
    doctor_line live-differs 'JACK is enabled persistently but inactive now; inspect systemctl status jack.service'
    ((issues += 1))
  elif [[ -n "$jack_owner" ]]; then
    doctor_line optional 'JACK service exists but is disabled; start it only at an explicit safe point'
  else
    doctor_line optional 'no system JACK service owns lifecycle; use the configured .jackdrc explicitly'
  fi
  if [[ "$(platform_kind)" == patchbox && -n "$jack_owner" ]]; then
    doctor_line retained "Patchbox owns the shared JACK service at $jack_owner; SHR retains that owner"
  elif [[ -n "$jack_owner" && "$jack_owner" != /etc/systemd/system/jack.service ]]; then
    doctor_line manual-owner "external JACK service owner detected at $jack_owner and intentionally untouched"
  fi
  local jack_config_file='' configured_jack_command='' live_jack_command='' jack_pid=''
  if [[ -n "$jack_owner" && -r "$(root_path /etc/jackdrc)" ]]; then
    jack_config_file="$(root_path /etc/jackdrc)"
  elif [[ -z "$PREFIX" && -n "${HOME:-}" && -r "$HOME/.jackdrc" ]]; then
    jack_config_file="$HOME/.jackdrc"
  fi
  if [[ -n "$jack_config_file" ]]; then
    configured_jack_command="$(jack_config_command "$jack_config_file")"
    if [[ -n "$configured_jack_command" ]]; then
      doctor_line ready "configured JACK launch command is owned by $jack_config_file"
    else
      doctor_line partial "JACK configuration at $jack_config_file has no launch command"
      ((issues += 1))
    fi
  fi
  if [[ "$jack_count" -eq 1 ]]; then
    jack_pid="$(jack_process_pid 2>/dev/null || true)"
    if [[ -n "$jack_pid" ]]; then
      live_jack_command="$(live_process_command "$jack_pid" 2>/dev/null || true)"
    fi
    if [[ -n "$configured_jack_command" && -n "$live_jack_command" &&
          "$configured_jack_command" == "$live_jack_command" ]]; then
      doctor_line ready 'live JACK device/rate/buffer command matches configured intent'
    elif [[ -n "$configured_jack_command" && -n "$live_jack_command" ]]; then
      doctor_line live-differs \
        "live JACK command differs from $jack_config_file; inspect the service/process before restarting anything"
      ((issues += 1))
    elif [[ -z "$live_jack_command" ]]; then
      doctor_line partial 'one JACK process is live, but its command line is unreadable'
      ((issues += 1))
    else
      doctor_line manual-owner \
        'one JACK process is live without a detected persistent command owner'
    fi
  elif [[ -n "$configured_jack_command" ]]; then
    doctor_line optional \
      "configured JACK command is not live; start it only at an explicit safe point"
  fi
  local thermal_raw throttle_state
  thermal_raw="$(cat "$(root_path /sys/class/thermal/thermal_zone0/temp)" 2>/dev/null || true)"
  if [[ "$thermal_raw" =~ ^[0-9]+$ ]]; then
    doctor_line ready \
      "SoC temperature is $((thermal_raw / 1000))°C now; sustained audio/build temperature still needs workload observation"
  else
    doctor_line optional 'SoC temperature telemetry is unavailable'
  fi
  throttle_state="$(throttled_flags)"
  if [[ "$throttle_state" == 0x0 || "$throttle_state" == 0 ]]; then
    doctor_line ready 'firmware reports no current or historical power/thermal throttle flag'
  elif [[ -n "$throttle_state" ]]; then
    doctor_line partial \
      "firmware throttle flags are $throttle_state; resolve cooling/power evidence before low-latency work"
    ((issues += 1))
  else
    doctor_line optional 'firmware power/throttle telemetry is unavailable'
  fi
  if [[ -f "$JACK_DROPIN" && "$cpu" != 'not installed' && -n "$jack_owner" ]]; then
    local configured_affinity live_affinity main_pid
    configured_affinity="$(systemctl_value jack.service CPUAffinity)"
    if [[ "$configured_affinity" == "$cpu" ]]; then
      doctor_line ready "persistent JACK CPU affinity is $cpu"
    else
      doctor_line partial "effective JACK service affinity is '${configured_affinity:-unknown}', expected $cpu"
      ((issues += 1))
    fi
    main_pid="$(systemctl_value jack.service MainPID)"
    live_affinity=''
    if [[ "$main_pid" =~ ^[1-9][0-9]*$ ]]; then
      live_affinity="$(sed -n 's/^Cpus_allowed_list:[[:space:]]*//p' \
        "$(root_path "/proc/$main_pid/status")" 2>/dev/null || true)"
    fi
    if [[ "$jack_active" == active && "$live_affinity" != "$cpu" ]]; then
      doctor_line live-differs "live JACK affinity is '${live_affinity:-unknown}', expected $cpu; restart JACK only at a safe point"
      ((issues += 1))
    elif [[ "$jack_active" == active ]]; then
      doctor_line ready "live JACK process is confined to CPU $cpu"
    fi
  elif [[ -f "$JACK_DROPIN" && "$cpu" != 'not installed' ]]; then
    doctor_line retained \
      'JACK affinity drop-in is prepared, but no system JACK service currently owns lifecycle'
  fi
  if [[ -d "$TRANSACTION_DIR" ]]; then
    doctor_line interrupted 'an interrupted install needs sudo shr-audio-tune recover'
    ((issues += 1))
  fi
  if [[ -d "$PERMISSIONS_TRANSACTION_DIR" ]]; then
    doctor_line interrupted 'an interrupted real-time permission change needs sudo shr-audio-tune recover'
    ((issues += 1))
  fi
  if [[ -f "$STATE_DIR/cmdline.original" ]]; then
    doctor_line rollback-available "original boot command line retained at $STATE_DIR/cmdline.original"
  fi
  if ((issues > 0)); then
    printf '\nAudio policy state: %s issue(s) need attention.\n' "$issues"
    return 1
  fi
  printf '\nAudio policy state: ready.\n'
}

case "${1:-status}" in
  plan) plan_tuning "${2:-}" ;;
  permissions-install) permissions_install "${2:-}" ;;
  permissions-remove) permissions_remove ;;
  install) install_tuning "${2:-}" ;;
  recover) recover_tuning ;;
  remove) remove_tuning ;;
  status) status ;;
  doctor) doctor "${2:-none}" ;;
  runtime-start) runtime_start ;;
  runtime-stop) runtime_stop ;;
  -h|--help|help) usage ;;
  *) usage >&2; exit 2 ;;
esac
