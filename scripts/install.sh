#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_DEPS=true
INIT_CONFIG=true

for arg in "$@"; do
  case "$arg" in
    --no-deps) INSTALL_DEPS=false ;;
    --no-config) INIT_CONFIG=false ;;
    -h|--help)
      printf 'Usage: %s [--no-deps] [--no-config]\n' "$0"
      exit 0
      ;;
    *) printf 'Unknown option: %s\n' "$arg" >&2; exit 2 ;;
  esac
done

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
  sudo apt-get update
  sudo apt-get install -y --no-install-recommends \
    alsa-utils build-essential ca-certificates curl jackd2 libasound2-dev \
    fluidsynth pkg-config python3 sox synthv1 timgm6mb-soundfont unzip yoshimi yoshimi-data
  if command -v systemctl >/dev/null 2>&1; then
    printf '%s\n' \
      'Stopping and masking only the per-user fluidsynth.service to prevent an unowned layered synth.'
    systemctl --user daemon-reload
    systemctl --user mask --now fluidsynth.service
    [[ "$(systemctl --user is-enabled fluidsynth.service 2>/dev/null || true)" == masked ]] || {
      printf 'Could not verify the per-user FluidSynth service mask.\n' >&2
      exit 1
    }
    printf '%s\n' \
      'Masked the standalone FluidSynth service; SHR can still start FluidSynth on demand.'
  fi
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
