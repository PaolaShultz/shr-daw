#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TUNER="$ROOT/scripts/audio-performance.sh"
INSTALLER="$ROOT/scripts/install.sh"
TEST_ROOT="$(mktemp -d /tmp/shr-audio-tests.XXXXXX)"
tests=0

cleanup() {
  find "$TEST_ROOT" -depth -mindepth 1 -delete
  rmdir "$TEST_ROOT"
}
trap cleanup EXIT

pass() {
  tests=$((tests + 1))
  printf 'ok %d - %s\n' "$tests" "$1"
}

fail() {
  printf 'not ok - %s\n' "$1" >&2
  exit 1
}

assert_contains() {
  local text=$1 wanted=$2 label=$3
  [[ "$text" == *"$wanted"* ]] || {
    printf 'Expected %q in output:\n%s\n' "$wanted" "$text" >&2
    fail "$label"
  }
}

assert_file_contains() {
  local file=$1 wanted=$2 label=$3
  rg -q --fixed-strings "$wanted" "$file" || {
    printf 'Expected %q in %s\n' "$wanted" "$file" >&2
    fail "$label"
  }
}

assert_file_not_contains() {
  local file=$1 wanted=$2 label=$3
  if rg -q --fixed-strings "$wanted" "$file"; then
    printf 'Did not expect %q in %s\n' "$wanted" "$file" >&2
    fail "$label"
  fi
}

new_fixture() {
  local name=$1 feature_mode=${2:-full} platform=${3:-stock}
  local fixture="$TEST_ROOT/$name"
  mkdir -p \
    "$fixture/boot/firmware" \
    "$fixture/etc/security/limits.d" \
    "$fixture/proc/irq" \
    "$fixture/proc/self" \
    "$fixture/proc/sys/kernel" \
    "$fixture/proc/device-tree" \
    "$fixture/run/shr-audio-tune-fixture/systemctl" \
    "$fixture/sys/class/thermal/thermal_zone0" \
    "$fixture/sys/devices/system/cpu/cpufreq/policy0" \
    "$fixture/sys/devices/system/cpu"
  printf '%s\n' \
    'PRETTY_NAME="Raspberry Pi OS Lite"' \
    'ID=debian' >"$fixture/etc/os-release"
  printf 'Raspberry Pi 5 Model B Rev 1.0\0' >"$fixture/proc/device-tree/model"
  printf 'aarch64\n' >"$fixture/proc/sys/kernel/arch"
  printf '6.12.0-rpi-test\n' >"$fixture/proc/sys/kernel/osrelease"
  {
    printf 'CONFIG_PREEMPT=y\n'
    printf '# CONFIG_PREEMPT_RT is not set\n'
    if [[ "$feature_mode" == full ]]; then
      printf 'CONFIG_NO_HZ_FULL=y\n'
      printf 'CONFIG_RCU_NOCB_CPU=y\n'
    else
      printf '# CONFIG_NO_HZ_FULL is not set\n'
      printf '# CONFIG_RCU_NOCB_CPU is not set\n'
    fi
  } >"$fixture/boot/config-6.12.0-rpi-test"
  printf 'console=tty1 root=PARTUUID=test rootwait\n' >"$fixture/boot/firmware/cmdline.txt"
  printf 'console=tty1 root=PARTUUID=test rootwait\n' >"$fixture/proc/cmdline"
  printf '7\n' >"$fixture/proc/irq/default_smp_affinity"
  printf '0-3\n' >"$fixture/sys/devices/system/cpu/online"
  : >"$fixture/sys/devices/system/cpu/isolated"
  : >"$fixture/sys/devices/system/cpu/nohz_full"
  printf 'performance\n' >"$fixture/sys/devices/system/cpu/cpufreq/policy0/scaling_governor"
  printf 'performance schedutil\n' \
    >"$fixture/sys/devices/system/cpu/cpufreq/policy0/scaling_available_governors"
  printf '55000\n' >"$fixture/sys/class/thermal/thermal_zone0/temp"
  printf '0x0\n' >"$fixture/run/shr-audio-tune-fixture/throttled"
  printf '%s\n' \
    'root:x:0:0:root:/root:/bin/bash' \
    'patch:x:1000:1000:Patch:/home/patch:/bin/bash' >"$fixture/etc/passwd"
  printf '%s\n' \
    'root:x:0:' \
    'audio:x:29:patch' >"$fixture/etc/group"
  printf '%s\n' \
    '@audio - rtprio 95' \
    '@audio - memlock unlimited' >"$fixture/etc/security/limits.d/audio.conf"
  printf '%-26s %-20s %-20s %s\n' \
    'Limit' 'Soft Limit' 'Hard Limit' 'Units' \
    'Max locked memory' 'unlimited' 'unlimited' 'bytes' \
    'Max realtime priority' '95' '95' '' >"$fixture/proc/self/limits"
  if [[ "$platform" == patchbox ]]; then
    mkdir -p "$fixture/usr/share/doc/patchbox"
  fi
  printf '%s\n' "$fixture"
}

set_jack_state() {
  local fixture=$1 enabled=$2 active=$3 count=$4 owner=$5 affinity=${6:-}
  local state="$fixture/run/shr-audio-tune-fixture/systemctl"
  printf '%s\n' "$enabled" >"$state/jack.service.enabled"
  printf '%s\n' "$active" >"$state/jack.service.active"
  printf '%s\n' "$owner" >"$state/jack.service.FragmentPath"
  printf '%s\n' "$affinity" >"$state/jack.service.CPUAffinity"
  printf '123\n' >"$state/jack.service.MainPID"
  local process
  for process in 123 124 125; do
    find "$fixture/proc/$process" -depth -delete 2>/dev/null || true
  done
  if ((count >= 1)); then
    mkdir -p "$fixture/proc/123"
    printf 'jackd\n' >"$fixture/proc/123/comm"
    printf 'Cpus_allowed_list:\t%s\n' "${affinity:-0-3}" >"$fixture/proc/123/status"
    printf '%s\0' /usr/bin/jackd -R -d alsa -r 48000 -p 256 -n 3 \
      >"$fixture/proc/123/cmdline"
    if [[ -n "$owner" ]]; then
      printf 'exec /usr/bin/jackd -R -d alsa -r 48000 -p 256 -n 3\n' \
        >"$fixture/etc/jackdrc"
    fi
  fi
  if ((count >= 2)); then
    mkdir -p "$fixture/proc/124"
    printf 'jackd\n' >"$fixture/proc/124/comm"
    printf 'Cpus_allowed_list:\t0-3\n' >"$fixture/proc/124/status"
    printf '%s\0' /usr/bin/jackd -R -d alsa -r 44100 -p 1024 -n 2 \
      >"$fixture/proc/124/cmdline"
  fi
}

run_tuner() {
  local fixture=$1
  shift
  SHR_TUNE_ROOT="$fixture" SHR_TUNE_USER=patch "$TUNER" "$@"
}

fixture="$(new_fixture clean)"
output="$(run_tuner "$fixture" plan 3)"
assert_contains "$output" 'Platform baseline: raspberry-pi-os (aarch64)' \
  'clean Raspberry Pi OS Lite is identified'
assert_contains "$output" 'Full tickless: supported' 'supported kernel features are planned'
pass 'clean Raspberry Pi OS Lite plan'

run_tuner "$fixture" install 3 >/dev/null
cmdline="$fixture/boot/firmware/cmdline.txt"
assert_file_contains "$cmdline" 'isolcpus=domain,managed_irq,3' 'domain isolation installed'
assert_file_contains "$cmdline" 'nohz_full=3' 'supported nohz installed'
assert_file_contains "$cmdline" 'rcu_nocbs=3' 'supported RCU offload installed'
assert_file_contains "$cmdline" 'irqaffinity=0,1,2' 'IRQ housekeeping installed'
first_hash="$(sha256sum "$cmdline" | awk '{print $1}')"
run_tuner "$fixture" install 3 >/dev/null
second_hash="$(sha256sum "$cmdline" | awk '{print $1}')"
[[ "$first_hash" == "$second_hash" ]] || fail 'repeated install changed the boot line'
pass 'already-correct repeated install is idempotent'

cp "$cmdline" "$fixture/proc/cmdline"
printf '3\n' >"$fixture/sys/devices/system/cpu/isolated"
printf '3\n' >"$fixture/sys/devices/system/cpu/nohz_full"
printf 'enabled\n' \
  >"$fixture/run/shr-audio-tune-fixture/systemctl/shr-audio-performance.service.enabled"
printf 'active\n' \
  >"$fixture/run/shr-audio-tune-fixture/systemctl/shr-audio-performance.service.active"
set_jack_state "$fixture" enabled active 1 /lib/systemd/system/jack.service 3
output="$(run_tuner "$fixture" doctor 3)"
assert_contains "$output" 'Audio policy state: ready.' 'configured and actual state agree'
assert_contains "$output" 'live JACK process is confined to CPU 3' 'live JACK affinity checked'
pass 'doctor distinguishes and verifies configured plus live state'

printf 'f\n' >"$fixture/proc/irq/default_smp_affinity"
if output="$(run_tuner "$fixture" doctor 3 2>&1)"; then
  fail 'live IRQ affinity drift was accepted from configured boot text'
fi
assert_contains "$output" 'live default IRQ affinity is' \
  'configured and live IRQ affinity are distinguished'
pass 'doctor verifies live IRQ placement separately from boot intent'

partial="$(new_fixture partial)"
run_tuner "$partial" install 3 >/dev/null
if output="$(run_tuner "$partial" doctor 3 2>&1)"; then
  fail 'doctor accepted a reboot-pending fixture'
fi
assert_contains "$output" '[reboot-required]' 'reboot-required state is actionable'
assert_contains "$output" 'not active' 'live state differs from persistent intent'
pass 'partial installation and reboot-required state'

unsupported="$(new_fixture unsupported minimal)"
run_tuner "$unsupported" install 3 >/dev/null
assert_file_not_contains "$unsupported/boot/firmware/cmdline.txt" 'nohz_full=' \
  'unsupported nohz omitted'
assert_file_not_contains "$unsupported/boot/firmware/cmdline.txt" 'rcu_nocbs=' \
  'unsupported RCU omitted'
cp "$unsupported/boot/firmware/cmdline.txt" "$unsupported/proc/cmdline"
printf '3\n' >"$unsupported/sys/devices/system/cpu/isolated"
set_jack_state "$unsupported" disabled inactive 0 '' ''
output="$(run_tuner "$unsupported" doctor 3)"
assert_contains "$output" '[retained] kernel lacks full-tickless support' \
  'unsupported optional feature is not claimed active'
pass 'unsupported kernel omits stale nohz and RCU claims'

stale="$(new_fixture stale minimal)"
printf '%s\n' \
  'console=tty1 isolcpus=domain,managed_irq,3 nohz_full=3 rcu_nocbs=3 irqaffinity=0,1,2' \
  >"$stale/proc/cmdline"
mkdir -p "$stale/var/lib/shr-audio-tune"
printf '3\n' >"$stale/var/lib/shr-audio-tune/cpu"
printf '0,1,2\n' >"$stale/var/lib/shr-audio-tune/housekeeping"
printf '%s\n' "$stale/boot/firmware/cmdline.txt" \
  >"$stale/var/lib/shr-audio-tune/cmdline.path"
if output="$(run_tuner "$stale" doctor 3 2>&1)"; then
  fail 'doctor accepted unsupported live kernel tokens'
fi
assert_contains "$output" 'nohz_full is present but unsupported' \
  'stale unsupported token is identified'
assert_contains "$output" 'rcu_nocbs is present but unsupported' \
  'stale RCU token is identified'
pass 'doctor rejects configured text that the kernel cannot implement'

conflict="$(new_fixture conflict)"
printf 'console=tty1 isolcpus=2\n' >"$conflict/boot/firmware/cmdline.txt"
before="$(sha256sum "$conflict/boot/firmware/cmdline.txt" | awk '{print $1}')"
if output="$(run_tuner "$conflict" install 3 2>&1)"; then
  fail 'foreign kernel key was overwritten'
fi
after="$(sha256sum "$conflict/boot/firmware/cmdline.txt" | awk '{print $1}')"
[[ "$before" == "$after" ]] || fail 'foreign boot command line changed'
assert_contains "$output" 'was not created by SHR-DAW' 'foreign kernel conflict explained'
pass 'conflicting pre-existing kernel keys are untouched'

manual_boot="$(new_fixture manual-boot-owner)"
printf 'console=tty1 isolcpus=2 irqaffinity=0,1,3\n' \
  >"$manual_boot/boot/firmware/cmdline.txt"
output="$(run_tuner "$manual_boot" doctor none)"
assert_contains "$output" '[manual-owner] administrator boot tuning detected' \
  'administrator boot tuning retained without configured SHR intent'
if output="$(run_tuner "$manual_boot" doctor 3 2>&1)"; then
  fail 'configured SHR CPU accepted conflicting administrator boot tuning'
fi
assert_contains "$output" '[conflicting-owner]' \
  'administrator boot tuning conflict is actionable'
pass 'manual administrator boot tuning is retained and conflicts are explicit'

admin_path="$(new_fixture administrator-owned)"
mkdir -p "$admin_path/etc/systemd/system/jack.service.d"
printf '[Service]\nCPUAffinity=2\n' \
  >"$admin_path/etc/systemd/system/jack.service.d/90-shr-audio-cpu.conf"
if output="$(run_tuner "$admin_path" install 3 2>&1)"; then
  fail 'administrator-owned service path was overwritten'
fi
assert_contains "$output" 'Refusing to replace pre-existing path' \
  'administrator-owned file collision explained'
pass 'helper-owned and administrator-owned paths are separated'

legacy="$(new_fixture legacy-boot)"
find "$legacy/boot/firmware/cmdline.txt" -maxdepth 0 -delete
mkdir -p "$legacy/boot"
printf 'console=tty1 rootwait\n' >"$legacy/boot/cmdline.txt"
run_tuner "$legacy" install 3 >/dev/null
assert_file_contains "$legacy/boot/cmdline.txt" 'isolcpus=domain,managed_irq,3' \
  'legacy boot command line used'
pass 'both Raspberry Pi boot command-line locations are supported'

topology="$(new_fixture unsupported-topology)"
printf '0-1,3\n' >"$topology/sys/devices/system/cpu/online"
if output="$(run_tuner "$topology" plan 3 2>&1)"; then
  fail 'unsupported topology was accepted'
fi
assert_contains "$output" 'Unsupported online CPU layout' 'unsupported topology explained'
pass 'unsupported CPU topology is refused'

arch32="$(new_fixture unsupported-architecture)"
printf 'armv7l\n' >"$arch32/proc/sys/kernel/arch"
if output="$(run_tuner "$arch32" plan 3 2>&1)"; then
  fail '32-bit Raspberry Pi tuning was accepted'
fi
assert_contains "$output" 'requires 64-bit aarch64' \
  '32-bit assumption is explained'
pass 'unsupported 32-bit Raspberry Pi path is refused'

permissions="$(new_fixture permissions)"
printf 'audio:x:29:\n' >"$permissions/etc/group"
find "$permissions/etc/security/limits.d/audio.conf" -maxdepth 0 -delete
run_tuner "$permissions" permissions-install patch >/dev/null
assert_file_contains "$permissions/etc/group" 'audio:x:29:patch' \
  'audio membership installed'
assert_file_contains "$permissions/etc/security/limits.d/95-shr-audio.conf" \
  '@audio - memlock unlimited' 'real-time policy installed'
run_tuner "$permissions" permissions-remove >/dev/null
assert_file_contains "$permissions/etc/group" 'audio:x:29:' \
  'owned membership removed'
[[ ! -e "$permissions/etc/security/limits.d/95-shr-audio.conf" ]] ||
  fail 'owned real-time policy was not removed'
pass 'owned real-time permissions install and removal'

permissions_collision="$(new_fixture permissions-collision)"
printf 'audio:x:29:\n' >"$permissions_collision/etc/group"
find "$permissions_collision/etc/security/limits.d/audio.conf" -maxdepth 0 -delete
printf '@audio - rtprio 70\n' \
  >"$permissions_collision/etc/security/limits.d/95-shr-audio.conf"
group_hash_before="$(
  sha256sum "$permissions_collision/etc/group" | awk '{print $1}'
)"
if output="$(run_tuner "$permissions_collision" permissions-install patch 2>&1)"; then
  fail 'administrator-owned limits collision was accepted'
fi
group_hash_after="$(
  sha256sum "$permissions_collision/etc/group" | awk '{print $1}'
)"
[[ "$group_hash_before" == "$group_hash_after" ]] ||
  fail 'permissions collision changed group membership before preflight completed'
assert_contains "$output" 'administrator-owned limits file' \
  'permissions collision ownership is explained'
assert_file_contains \
  "$permissions_collision/etc/security/limits.d/95-shr-audio.conf" \
  '@audio - rtprio 70' 'administrator limits file preserved'
pass 'permission policy collision is rejected before any group change'

partial_limits="$(new_fixture partial-limits)"
printf '%s\n' \
  '@audio hard rtprio 95' \
  '@audio hard memlock unlimited' \
  >"$partial_limits/etc/security/limits.d/audio.conf"
if output="$(run_tuner "$partial_limits" doctor none 2>&1)"; then
  fail 'hard-only real-time policy was accepted as complete'
fi
assert_contains "$output" 'real-time limits are incomplete' \
  'soft and hard real-time limits are both required'
pass 'hard-only real-time limits are reported as partial'

permissions_recover="$(new_fixture permissions-recover)"
printf 'audio:x:29:\n' >"$permissions_recover/etc/group"
find "$permissions_recover/etc/security/limits.d/audio.conf" -maxdepth 0 -delete
permissions_state="$permissions_recover/var/lib/shr-audio-tune"
permissions_transaction="$permissions_state/permissions-transaction"
mkdir -p "$permissions_transaction/state.before"
cp -p "$permissions_recover/etc/group" "$permissions_transaction/group.before"
printf '%s\n' "$permissions_recover/etc/group" >"$permissions_transaction/group.path"
printf 'patch\n' >"$permissions_transaction/user"
touch "$permissions_transaction/membership-added"
touch "$permissions_transaction/policy.created"
printf 'audio:x:29:patch\n' >"$permissions_recover/etc/group"
sha256sum "$permissions_recover/etc/group" |
  awk '{print $1}' >"$permissions_transaction/group.after-sha"
printf '%s\n' \
  '# Managed by shr-audio-tune. JACK clients require both limits.' \
  '@audio - rtprio 95' \
  '@audio - memlock unlimited' \
  >"$permissions_recover/etc/security/limits.d/95-shr-audio.conf"
sha256sum "$permissions_recover/etc/security/limits.d/95-shr-audio.conf" |
  awk '{print $1}' >"$permissions_transaction/policy.after-sha"
output="$(run_tuner "$permissions_recover" recover)"
assert_file_contains "$permissions_recover/etc/group" 'audio:x:29:' \
  'permission recovery restored group'
[[ ! -e "$permissions_recover/etc/security/limits.d/95-shr-audio.conf" ]] ||
  fail 'permission recovery retained a transaction-created policy'
[[ ! -d "$permissions_transaction" ]] ||
  fail 'permission recovery retained a completed transaction'
assert_contains "$output" 'permission change recovered' \
  'permission recovery reports its action'
pass 'interrupted real-time permission change is recoverable'

admin_edit="$(new_fixture later-admin-edit)"
run_tuner "$admin_edit" install 3 >/dev/null
printf '# administrator retained this edit\n' \
  >>"$admin_edit/etc/systemd/system/jack.service.d/90-shr-audio-cpu.conf"
output="$(run_tuner "$admin_edit" remove)"
assert_contains "$output" 'Manual administrator edit detected' \
  'later administrator edit reported'
assert_file_contains \
  "$admin_edit/etc/systemd/system/jack.service.d/90-shr-audio-cpu.conf" \
  'administrator retained this edit' 'later administrator edit preserved'
assert_file_not_contains "$admin_edit/boot/firmware/cmdline.txt" 'isolcpus=' \
  'owned boot token removed'
pass 'rollback/removal preserves later administrator edits'

interrupted="$(new_fixture interrupted)"
before="$(sha256sum "$interrupted/boot/firmware/cmdline.txt" | awk '{print $1}')"
mock_systemctl="$TEST_ROOT/failing-systemctl"
# shellcheck disable=SC2016
printf '%s\n' \
  '#!/usr/bin/env bash' \
  '[[ "${1:-}" == enable ]] && exit 1' \
  'exit 0' >"$mock_systemctl"
chmod +x "$mock_systemctl"
if output="$(
  SHR_TUNE_ROOT="$interrupted" SHR_TUNE_SYSTEMCTL="$mock_systemctl" \
    "$TUNER" install 3 2>&1
)"; then
  fail 'systemctl failure did not stop install'
fi
after="$(sha256sum "$interrupted/boot/firmware/cmdline.txt" | awk '{print $1}')"
[[ "$before" == "$after" ]] || fail 'failed install did not restore boot command line'
[[ ! -e "$interrupted/var/lib/shr-audio-tune/cpu" ]] ||
  fail 'failed install left a CPU ownership record'
assert_contains "$output" 'rolled back' 'failed install rollback reported'
pass 'interrupted/failed install rolls back atomically'

recover="$(new_fixture recover)"
mkdir -p "$recover/var/lib/shr-audio-tune/transaction"
cp "$recover/boot/firmware/cmdline.txt" \
  "$recover/var/lib/shr-audio-tune/transaction/cmdline.before"
printf '%s\n' "$recover/boot/firmware/cmdline.txt" \
  >"$recover/var/lib/shr-audio-tune/transaction/cmdline.path"
printf 'broken partial command line\n' >"$recover/boot/firmware/cmdline.txt"
sha256sum "$recover/boot/firmware/cmdline.txt" |
  awk '{print $1}' >"$recover/var/lib/shr-audio-tune/transaction/cmdline.after-sha"
output="$(run_tuner "$recover" recover)"
assert_file_contains "$recover/boot/firmware/cmdline.txt" 'root=PARTUUID=test' \
  'recovery restored boot command line'
assert_contains "$output" 'recovered' 'recovery action reported'
pass 'explicit recovery resumes from an interrupted install'

patchbox="$(new_fixture patchbox full patchbox)"
set_jack_state "$patchbox" enabled active 1 /lib/systemd/system/jack.service 0-3
output="$(run_tuner "$patchbox" doctor none)"
assert_contains "$output" '[retained] Patchbox owns the shared JACK service' \
  'Patchbox JACK ownership retained'
assert_contains "$output" '[optional] dedicated audio CPU is absent' \
  'Patchbox tuning remains optional'
pass 'representative Patchbox baseline retains correct owner'

thermal_fault="$(new_fixture thermal-fault)"
printf '0x50005\n' >"$thermal_fault/run/shr-audio-tune-fixture/throttled"
if output="$(run_tuner "$thermal_fault" doctor none 2>&1)"; then
  fail 'power/thermal throttle evidence was accepted as ready'
fi
assert_contains "$output" 'firmware throttle flags are 0x50005' \
  'power/thermal fault has an actionable doctor state'
pass 'power and thermal throttle evidence is surfaced'

jack_states="$(new_fixture jack-states)"
set_jack_state "$jack_states" disabled inactive 0 '' ''
output="$(run_tuner "$jack_states" doctor none)"
assert_contains "$output" 'no system JACK service owns lifecycle' \
  'disabled/absent JACK is optional'
set_jack_state "$jack_states" enabled inactive 0 /lib/systemd/system/jack.service ''
if output="$(run_tuner "$jack_states" doctor none 2>&1)"; then
  fail 'enabled but inactive JACK was accepted'
fi
assert_contains "$output" '[live-differs]' 'enabled/inactive JACK distinguished'
set_jack_state "$jack_states" enabled active 2 /lib/systemd/system/jack.service 0-3
if output="$(run_tuner "$jack_states" doctor none 2>&1)"; then
  fail 'duplicate JACK processes were accepted'
fi
assert_contains "$output" '[duplicate-service]' 'duplicate JACK owner detected'
set_jack_state "$jack_states" enabled active 1 /lib/systemd/system/jack.service 0-3
printf '%s\0' /usr/bin/jackd -R -d alsa -r 44100 -p 1024 -n 2 \
  >"$jack_states/proc/123/cmdline"
if output="$(run_tuner "$jack_states" doctor none 2>&1)"; then
  fail 'live JACK command drift was accepted'
fi
assert_contains "$output" 'live JACK command differs' \
  'configured and live JACK commands distinguished'
set_jack_state "$jack_states" disabled inactive 0 /etc/systemd/system/admin-jack.service ''
output="$(run_tuner "$jack_states" doctor none)"
assert_contains "$output" '[manual-owner]' 'external administrator JACK owner retained'
pass 'JACK enabled/disabled/active/inactive/duplicate/external states'

if SHR_TUNE_ROOT='' "$TUNER" install 3 >"$TEST_ROOT/nonroot.out" 2>&1; then
  fail 'non-root host mutation was accepted'
fi
assert_file_contains "$TEST_ROOT/nonroot.out" 'Run this operation with sudo' \
  'non-root repair command'
rg -q 'if \(\(EUID == 0\)\)' "$INSTALLER" ||
  fail 'installer root guard is missing'
rg -q 'if \(\(EUID == 0\)\); then' "$ROOT/scripts/setup.sh" ||
  fail 'setup root guard is missing'
pass 'root and non-root invocation boundaries'

fake_bin="$TEST_ROOT/fake-bin"
mkdir -p "$fake_bin"
ln -s /usr/bin/dirname "$fake_bin/dirname"
ln -s /usr/bin/pwd "$fake_bin/pwd"
ln -s /usr/bin/apt-get "$fake_bin/apt-get"
if output="$(PATH="$fake_bin" /usr/bin/bash "$INSTALLER" --plan 2>&1)"; then
  fail 'missing sudo preflight was accepted'
fi
assert_contains "$output" 'require sudo' 'missing sudo failure is precise'
find "$fake_bin/apt-get" -maxdepth 0 -delete
ln -s /usr/bin/sudo "$fake_bin/sudo"
if output="$(PATH="$fake_bin" /usr/bin/bash "$INSTALLER" --plan 2>&1)"; then
  fail 'missing apt-get preflight was accepted'
fi
assert_contains "$output" 'require Debian/Raspberry Pi OS' 'missing apt-get failure is precise'
pass 'missing sudo and dependency preflight failures'

failing_install_bin="$TEST_ROOT/failing-install-bin"
mkdir -p "$failing_install_bin"
printf '%s\n' '#!/usr/bin/env bash' 'exit 1' >"$failing_install_bin/apt-get"
printf '%s\n' '#!/usr/bin/env bash' 'exec "$@"' >"$failing_install_bin/sudo"
chmod +x "$failing_install_bin/apt-get" "$failing_install_bin/sudo"
if output="$(
  PATH="$failing_install_bin:$PATH" "$INSTALLER" --yes --no-config 2>&1
)"; then
  fail 'simulated apt interruption was accepted'
fi
assert_contains "$output" 'sudo dpkg --configure -a' \
  'installer interruption recovery names dpkg repair'
assert_contains "$output" 'package installation is idempotent' \
  'installer interruption recovery names safe retry'
pass 'interrupted package phase reports exact recovery'

rg -q --fixed-strings \
  "ask_yes_no 'Configure owned real-time audio permissions for this user?' no" \
  "$ROOT/scripts/setup.sh" || fail 'permission prompt is not safe-default no'
rg -q --fixed-strings \
  "ask_yes_no 'Apply this optional CPU/IRQ/governor profile (sudo and reboot required)?' no" \
  "$ROOT/scripts/setup.sh" || fail 'CPU tuning prompt is not safe-default no'
rg -q 'JACK was not restarted' "$TUNER" ||
  fail 'no-JACK-restart contract is missing'
if rg -n 'systemctl(_safe)?[[:space:]]+(start|restart)[[:space:]]+jack' \
  "$TUNER" "$ROOT/scripts/setup.sh" "$INSTALLER"; then
  fail 'automated path can start or restart JACK'
fi
pass 'system-changing prompts cancel safely and tests cannot operate hardware'

printf '1..%d\n' "$tests"
