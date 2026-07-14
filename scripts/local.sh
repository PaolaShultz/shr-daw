#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
USER_DIR="${SHSYNTH_USER_DIR:-$ROOT/user}"

export XDG_STATE_HOME="$USER_DIR/state"
export XDG_DATA_HOME="$USER_DIR/data"
export SHSYNTH_PRESET_DIR="$USER_DIR/presets/synthv1"

mkdir -p \
  "$XDG_STATE_HOME/shsynth" \
  "$XDG_DATA_HOME/shsynth" \
  "$SHSYNTH_PRESET_DIR"

for preset in "$ROOT"/presets/synthv1/*.synthv1; do
  destination="$SHSYNTH_PRESET_DIR/${preset##*/}"
  [[ -e "$destination" ]] || cp "$preset" "$destination"
done

if [[ -x "$ROOT/target/release/shr" ]]; then
  SHSYNTH_BIN="$ROOT/target/release/shr"
elif command -v shr >/dev/null 2>&1; then
  SHSYNTH_BIN="$(command -v shr)"
else
  printf 'Build or install SHR-DAW first.\n' >&2
  exit 1
fi

if [[ ! -f "$XDG_STATE_HOME/shsynth/shsynth.conf" ]]; then
  printf 'Run scripts/setup-local.sh before starting SHR-DAW.\n' >&2
  exit 1
fi

exec "$SHSYNTH_BIN" "$@"
