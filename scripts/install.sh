#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_DEPS=true
INIT_CONFIG=true
ASSUME_YES=false
PLAN_ONLY=false
PACKAGE_CHANGE_STARTED=false
FLUID_SERVICE_MASKED=false
PERMISSION_CHANGE_STARTED=false

install_recovery_report() {
  local result=$1
  ((result != 0)) || return 0
  if $PACKAGE_CHANGE_STARTED || $PERMISSION_CHANGE_STARTED; then
    printf '\nInstallation stopped after a system-changing phase began.\n' >&2
    if $PACKAGE_CHANGE_STARTED; then
      printf '%s\n' \
        '  If apt/dpkg reports an interruption: sudo dpkg --configure -a' \
        '  Then rerun this installer; package installation is idempotent.' >&2
    fi
    if $FLUID_SERVICE_MASKED; then
      printf '%s\n' \
        '  The verified FluidSynth mask remains intentional.' \
        '  Undo only if wanted: systemctl --user unmask fluidsynth.service' >&2
    fi
    if $PERMISSION_CHANGE_STARTED; then
      printf '%s\n' \
        '  Inspect permission state: shr-audio-tune doctor none' \
        '  Recover an interrupted helper transaction: sudo shr-audio-tune recover' >&2
    fi
  fi
}

trap 'install_recovery_report "$?"' EXIT

for arg in "$@"; do
  case "$arg" in
    --no-deps) INSTALL_DEPS=false ;;
    --no-config) INIT_CONFIG=false ;;
    --yes) ASSUME_YES=true ;;
    --plan) PLAN_ONLY=true ;;
    -h|--help)
      printf 'Usage: %s [--no-deps] [--no-config] [--yes] [--plan]\n' "$0"
      exit 0
      ;;
    *) printf 'Unknown option: %s\n' "$arg" >&2; exit 2 ;;
  esac
done

if ((EUID == 0)); then
  printf '%s\n' \
    'Do not run install.sh as root.' \
    'Run it as the musician account; the installer uses sudo only for owned system changes.' >&2
  exit 1
fi

ask_consent() {
  local prompt=$1 answer
  if $ASSUME_YES; then
    printf '%s [accepted by --yes]\n' "$prompt"
    return 0
  fi
  if [[ ! -t 0 ]]; then
    printf '%s\n' \
      "Cannot ask for consent without a terminal: $prompt" \
      'Rerun interactively, use --yes, or skip the changing phase.' >&2
    return 1
  fi
  read -r -p "$prompt [y/N] " answer
  [[ "${answer,,}" == y || "${answer,,}" == yes ]]
}

printf '%s\n' \
  'SHR-DAW installation plan (consequences before changes):'
if $INSTALL_DEPS; then
  printf '%s\n' \
    '  1. apt-get updates package metadata and installs build/audio/MIDI packages.' \
    '  2. the per-user fluidsynth.service is stopped and persistently masked;' \
    '     the FluidSynth executable remains available for SHR-owned use.'
else
  printf '%s\n' '  1–2. dependency installation and FluidSynth service masking are skipped.'
fi
printf '%s\n' \
  '  3. Rust 1.85 may be installed for the current user when missing.' \
  '  4. locked tests and a locked release build run in this checkout.' \
  '  5. sudo installs public application files below /usr/local.'
if $INIT_CONFIG; then
  printf '%s\n' \
    '  6. shr-setup seeds private user data and offers explicit configuration/service/tuning changes.'
else
  printf '%s\n' '  6. hardware/configuration setup is skipped.'
fi
printf '%s\n' 'JACK and synth engines are not started by this installer.'

if $INSTALL_DEPS; then
  command -v apt-get >/dev/null || {
    printf 'Automatic dependencies require Debian/Raspberry Pi OS (apt-get).\n' >&2
    exit 1
  }
  command -v sudo >/dev/null || {
    printf 'Automatic dependency and policy changes require sudo.\n' >&2
    exit 1
  }
  if $PLAN_ONLY; then
    printf '%s\n' \
      'Preflight ready: apt-get and sudo are available.' \
      'Plan only: no package, service, policy, build, install, or setup action ran.'
    exit 0
  fi
  if ! ask_consent \
    'Install the listed packages and stop/mask only the per-user fluidsynth.service?'; then
    printf 'Installation cancelled before package or service changes.\n'
    exit 0
  fi
  PACKAGE_CHANGE_STARTED=true
  sudo apt-get update
  sudo apt-get install -y --no-install-recommends \
    alsa-utils build-essential ca-certificates curl jackd2 libasound2-dev \
    fluidsynth pkg-config python3 ripgrep sox synthv1 timgm6mb-soundfont unzip \
    yoshimi yoshimi-data
  if command -v systemctl >/dev/null 2>&1; then
    printf '%s\n' \
      'Stopping and masking only the per-user fluidsynth.service to prevent an unowned layered synth.'
    systemctl --user daemon-reload
    systemctl --user mask --now fluidsynth.service
    [[ "$(systemctl --user is-enabled fluidsynth.service 2>/dev/null || true)" == masked ]] || {
      printf 'Could not verify the per-user FluidSynth service mask.\n' >&2
      exit 1
    }
    FLUID_SERVICE_MASKED=true
    printf '%s\n' \
      'Masked the standalone FluidSynth service; SHR can still start FluidSynth on demand.'
  fi

  rtprio="$(ulimit -r)"
  memlock="$(ulimit -l)"
  if [[ ! "$rtprio" =~ ^[0-9]+$ ]] || ((rtprio < 95)) || [[ "$memlock" != unlimited ]]; then
    printf '%s\n' \
      'This login does not yet have JACK-ready rtprio 95 and unlimited memlock.' \
      'The owned policy uses only the audio group and requires logout/login to become live.'
    if ask_consent 'Configure real-time audio permissions for the current user?'; then
      PERMISSION_CHANGE_STARTED=true
      sudo "$ROOT/scripts/audio-performance.sh" permissions-install "${USER:?USER is not set}"
    else
      printf '%s\n' \
        'Real-time permissions deferred.' \
        "Repair later: sudo shr-audio-tune permissions-install ${USER:?USER is not set}"
    fi
  else
    printf 'Real-time audio limits are already active; retaining the distribution policy.\n'
  fi
fi

if $PLAN_ONLY; then
  printf '%s\n' \
    'Preflight ready: dependency changes were disabled.' \
    'Plan only: no build, install, or setup action ran.'
  exit 0
fi

version_ok() {
  local version
  version="$($1 --version 2>/dev/null | awk '{print $2}')"
  [[ "$(printf '%s\n' 1.85.0 "$version" | sort -V | head -n1)" == 1.85.0 ]]
}

CARGO=(cargo)
if ! command -v cargo >/dev/null || ! version_ok cargo; then
  if ! command -v rustup >/dev/null; then
    printf 'Installing the official minimal Rust toolchain for the current user.\n'
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
      sh -s -- -y --profile minimal --default-toolchain 1.85.0
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
  rustup toolchain install 1.85.0 --profile minimal
  CARGO=(cargo +1.85.0)
fi

cd "$ROOT"
"${CARGO[@]}" test --locked
"${CARGO[@]}" build --release --locked
sudo make install-files

if $INIT_CONFIG; then
  shr-setup
fi

printf '\nInstalled: shr (Rust app), shs and synth-player (compatibility aliases)\n'
printf 'Run shr doctor, then run shr. Reconfigure hardware with shr-setup.\n'
