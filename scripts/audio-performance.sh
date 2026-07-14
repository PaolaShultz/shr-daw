#!/usr/bin/env bash
set -euo pipefail

# Reversible host tuning for a dedicated SHR-DAW audio CPU. This deliberately
# does not start/restart JACK or any synth process.

PREFIX="${SHR_TUNE_ROOT:-}"
STATE_DIR="$PREFIX/var/lib/shr-audio-tune"
JACK_DROPIN="$PREFIX/etc/systemd/system/jack.service.d/90-shr-audio-cpu.conf"
TUNE_SERVICE="$PREFIX/etc/systemd/system/shr-audio-performance.service"
RUNTIME_HELPER="$PREFIX/usr/local/libexec/shr-audio-tune-runtime"
RUN_DIR="$PREFIX/run/shr-audio-tune"
SELF="$(readlink -f "$0")"

usage() {
  cat <<'EOF'
Usage: shr-audio-tune install [CPU] | remove | status

install reserves one zero-based CPU for JACK and SHR-DAW's managed engine,
enables the performance governor, and keeps ordinary IRQs on the remaining
CPUs. CPU defaults to the highest online CPU. It backs up the boot command line
and never starts or restarts JACK. A reboot is required for CPU isolation.

remove reverses only settings owned by this tool. It does not delete backups.
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

online_cpus() {
  local online
  online="$(cat "$(root_path /sys/devices/system/cpu/online)" 2>/dev/null || true)"
  if [[ "$online" =~ ^0-([0-9]+)$ ]]; then
    seq 0 "${BASH_REMATCH[1]}"
  elif [[ "$online" =~ ^[0-9]+$ ]]; then
    printf '%s\n' "$online"
  else
    # Raspberry Pi systems normally expose a contiguous 0-N range. Refuse
    # unusual/hotplug layouts rather than constructing an unsafe mask.
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

systemctl_safe() {
  [[ -n "$PREFIX" ]] && return 0
  systemctl "$@"
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
  rm -rf "$RUN_DIR"
}

install_tuning() {
  need_root
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

  local cmdline line key
  cmdline="$(boot_cmdline)"
  line="$(tr '\n' ' ' <"$cmdline" | xargs)"
  [[ -n "$line" ]] || { printf 'Boot command line is empty.\n' >&2; exit 1; }
  for key in isolcpus nohz_full rcu_nocbs irqaffinity; do
    if [[ " $line " == *" $key="* && ! -f "$STATE_DIR/cpu" ]]; then
      printf 'Existing %s setting was not created by SHR-DAW; refusing to overwrite it.\n' "$key" >&2
      exit 1
    fi
  done
  local managed
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
  printf '%s\n' "$cpu" >"$STATE_DIR/cpu"
  printf '%s\n' "$housekeeping_csv" >"$STATE_DIR/housekeeping"

  for key in isolcpus nohz_full rcu_nocbs irqaffinity; do
    line="$(printf '%s\n' "$line" | tr ' ' '\n' | awk -F= -v key="$key" '$1 != key' | paste -sd' ' -)"
  done
  line="$line isolcpus=domain,managed_irq,$cpu nohz_full=$cpu rcu_nocbs=$cpu irqaffinity=$housekeeping_csv"
  printf '%s\n' "$line" >"$cmdline"

  install -Dm755 "$SELF" "$RUNTIME_HELPER"
  mkdir -p "${JACK_DROPIN%/*}"
  cat >"$JACK_DROPIN" <<EOF
# Managed by shr-audio-tune. Applied on the next JACK start.
[Service]
CPUAffinity=$cpu
LimitRTPRIO=95
LimitMEMLOCK=infinity
EOF
  cat >"$TUNE_SERVICE" <<EOF
# Managed by shr-audio-tune.
[Unit]
Description=SHR-DAW conservative audio performance tuning
Before=jack.service

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/usr/local/libexec/shr-audio-tune-runtime runtime-start
ExecStop=/usr/local/libexec/shr-audio-tune-runtime runtime-stop

[Install]
WantedBy=multi-user.target
EOF
  touch "$STATE_DIR/installed"
  systemctl_safe daemon-reload
  systemctl_safe enable --now shr-audio-performance.service
  printf 'Installed dedicated audio CPU %s; housekeeping CPUs: %s.\n' "$cpu" "$housekeeping_csv"
  printf 'JACK was not restarted. Reboot when convenient, then set audio.engine_cpu=%s.\n' "$cpu"
}

remove_tuning() {
  need_root
  if [[ ! -f "$STATE_DIR/cpu" ]]; then
    printf 'SHR-DAW audio tuning is not installed.\n'
    return 0
  fi
  local cpu housekeeping cmdline line key token
  cpu="$(cat "$STATE_DIR/cpu")"
  housekeeping="$(cat "$STATE_DIR/housekeeping")"
  cmdline="$(cat "$STATE_DIR/cmdline.path")"
  if [[ -f "$cmdline" ]]; then
    line="$(tr '\n' ' ' <"$cmdline" | xargs)"
    for token in "isolcpus=domain,managed_irq,$cpu" "nohz_full=$cpu" "rcu_nocbs=$cpu" "irqaffinity=$housekeeping"; do
      line="$(printf '%s\n' "$line" | tr ' ' '\n' | awk -v token="$token" '$0 != token' | paste -sd' ' -)"
    done
    printf '%s\n' "$line" >"$cmdline"
  fi
  systemctl_safe disable --now shr-audio-performance.service
  rm -f "$JACK_DROPIN" "$TUNE_SERVICE" "$RUNTIME_HELPER"
  rmdir "${JACK_DROPIN%/*}" 2>/dev/null || true
  systemctl_safe daemon-reload
  rm -f "$STATE_DIR/cpu" "$STATE_DIR/housekeeping" "$STATE_DIR/cmdline.path" "$STATE_DIR/installed"
  printf 'Removed SHR-DAW audio tuning. The original backup remains in %s.\n' "$STATE_DIR"
  printf 'Clear audio.engine_cpu in shsynth.conf and reboot when convenient.\n'
}

status() {
  local cpu='not installed'
  [[ -f "$STATE_DIR/cpu" ]] && cpu="$(cat "$STATE_DIR/cpu")"
  printf 'Dedicated audio CPU: %s\n' "$cpu"
  local kernel
  kernel="$(cat "$(root_path /proc/cmdline)" 2>/dev/null || true)"
  if [[ "$cpu" != 'not installed' && " $kernel " == *" nohz_full=$cpu "* ]]; then
    printf 'Kernel isolation active: yes\n'
  elif [[ "$cpu" != 'not installed' ]]; then
    printf 'Kernel isolation active: no (reboot pending)\n'
  else
    printf 'Kernel isolation active: no\n'
  fi
  local governor
  for governor in "$(root_path /sys/devices/system/cpu/cpufreq)"/policy*/scaling_governor; do
    [[ -r "$governor" ]] && printf 'Governor %s: %s\n' "${governor%/*}" "$(cat "$governor")"
  done
  if [[ -f "$JACK_DROPIN" ]]; then
    printf 'JACK affinity drop-in: %s\n' "$JACK_DROPIN"
  else
    printf 'JACK affinity drop-in: absent\n'
  fi
}

case "${1:-status}" in
  install) install_tuning "${2:-}" ;;
  remove) remove_tuning ;;
  status) status ;;
  runtime-start) runtime_start ;;
  runtime-stop) runtime_stop ;;
  -h|--help|help) usage ;;
  *) usage >&2; exit 2 ;;
esac
