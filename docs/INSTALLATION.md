# Installation

SHR-DAW supports two coherent Raspberry Pi paths. On Patchbox OS it retains the
distribution's shared JACK service and working real-time policy, then offers
only missing SHR-specific CPU isolation. On a clean 64-bit Raspberry Pi OS Lite
installation it installs the required packages, can add missing real-time
permissions, creates a user JACK command only when no system service or live
`jackd` process owns JACK, and offers the same reviewed CPU profile. Detection
comes before mutation, and every system-changing setup choice defaults to no.

Current physical audio and MIDI evidence comes from Patchbox OS based on Debian
12 (Bookworm). The clean Raspberry Pi OS Lite 64-bit path has isolated fixture
coverage but still needs the release roadmap's fresh-image and physical audio
acceptance on the Raspberry Pi 5. Record the exact image and version used rather
than treating “Lite” as a reproducible version identifier. The
[release roadmap](RELEASE_ROADMAP.md) owns the gate; the
[Pi 5 plan](PI5_HEADROOM_PLAN.md) owns the state comparison.

The supported family is Debian-based Linux. Rust 1.85, Cargo, a C build
toolchain, `pkg-config`, Python 3, ripgrep, ALSA development/runtime tools, and
JACK2 are required to build and diagnose the complete installation. A running
JACK server is optional for browsing and editing but required for
software-instrument audio, WAV-loop playback, and multitrack recording.
synthv1, Yoshimi, and FluidSynth/TimGM are separate optional sound engines at
runtime; the default installer includes all three so their catalogs are useful
immediately. MIDI controllers, external instruments, audio interfaces, and a
480×320 display are optional hardware. On that display the current fixed TTY
layout is 40×13 cells; installation does not change its font.

## Install

From the project directory, run:

```sh
./scripts/install.sh
```

The installer:

- previews its phases, refuses root invocation, and verifies `apt-get` and
  `sudo` before its first package change;
- after one default-no grouped prompt, installs build, JACK, and ALSA tools;
- installs synthv1, Yoshimi, FluidSynth, and the small TimGM SoundFont without
  recommended desktop frontends or the much larger FluidR3 bank;
- stops and masks the package-enabled per-user FluidSynth daemon while leaving
  the FluidSynth executable available to SHR;
- detects the current login's `rtprio` and `memlock`; if they are inadequate,
  a separate default-no prompt can add the user to `audio` and create a
  helper-owned limits file only when no distribution policy already suffices;
- installs/selects the official Rust 1.85 toolchain when the current Cargo is
  older, runs the locked tests, and builds the locked release version;
- installs commands, templates, the 21 allowlisted presets, four allowlisted
  CC0 48 kHz loops, ten manifest-cleared demo Projects plus MIDI files,
  device/controller profiles, drum data, documentation, and
  all 95 menu-manual images below the selected prefix (normally `/usr/local`);
- opens the routing wizard.

Before changing packages or services, the installer prints the enabled phases
and the exact per-user FluidSynth masking consequence. `--plan` performs the
dependency preflight and exits without packages, policy, builds, installation,
or setup. `--yes` is the explicit non-interactive acceptance for the grouped
prompts; without it, a non-terminal invocation refuses to mutate the system.
If interactive setup is interrupted, its phase summary distinguishes
completed, possibly partial, and not-started work and prints exact rerun or
recovery commands for recorded side effects. If installation stops after
package mutation begins, it names `sudo dpkg --configure -a`, the idempotent
rerun, the exact FluidSynth unmask, and permission-ledger recovery as applicable
instead of attempting to roll package-manager state back.

After package consent, the dependency installer masks the exact per-user
`fluidsynth.service` that its package enables. At the start of interactive
routing, setup checks that mask and detects the system-wide
`amidiminder.service` blanket MIDI patcher. When either known conflict remains,
a separate default-no choice stops and masks only those exact units. It does not
uninstall FluidSynth, stop JACK, disconnect arbitrary routes, or prevent SHR
from launching its own FluidSynth process when a SoundFont sound is loaded. The
prompt is skipped when both units are absent or already masked.

To deliberately restore those distribution services later:

```sh
systemctl --user unmask fluidsynth.service
sudo systemctl unmask amidiminder.service
```

Unmasking permits them to run again; start or enable them separately only when
their automatic audio/MIDI behavior is actually wanted.

Use `--no-deps` to keep the installer from installing system packages. Use
`--no-config` to skip the routing wizard. Preview or explicitly accept the
grouped prompts with:

```sh
./scripts/install.sh --no-deps
./scripts/install.sh --no-config
./scripts/install.sh --plan
./scripts/install.sh --yes
```

## Installed commands

- `shr` opens SHR-DAW and provides its command-line tools.
- `shr-setup` opens the routing wizard.
- `shr-audio-tune` manages optional Raspberry Pi audio CPU tuning.
- `shs` and `synth-player` are compatibility names for `shr`. They use the same
  Rust engine ownership, routing, and shutdown path as the main command.

The product and Cargo package are named `shr-daw`. The main command is `shr`.
Existing `shsynth` configuration and data paths are kept for compatibility.

## Repository-local evaluation

Contributors can build and inspect the checkout without installing files:

```sh
PATH=/home/patch/.rustup/toolchains/1.85.0-aarch64-unknown-linux-gnu/bin:$PATH cargo build --locked
SHSYNTH_STATE_DIR=/tmp/shr-daw-judge-state target/debug/shr config init
SHSYNTH_STATE_DIR=/tmp/shr-daw-judge-state target/debug/shr list
python3 scripts/render-readme-screenshots.py --check
```

This path does not start JACK or transmit MIDI. Delete the explicit temporary
state directory afterward. For a persistent private development checkout,
`./scripts/setup-local.sh` and `./scripts/local.sh` redirect configuration,
Projects, Ideas, recordings, loops, and private presets below ignored `user/`.
They copy missing public presets, starter loops, and demo Projects without
replacing private files. Build the debug binary first; neither helper installs
packages or builds the program. `local.sh` launches this checkout's
`target/debug/shr`, which carries the visible `DEV` badge.

## Upgrade and uninstall boundaries

Rerunning `./scripts/install.sh` builds the locked current checkout and replaces
installed program/shared documentation files. Existing XDG configuration,
controller learning, Projects, Ideas, loops, and recordings are not removed or
reset. Package installation, service masking, real-time policy, and CPU tuning
are idempotent. Run `shr-setup` only when routes or hardware need to change.

For a default `/usr/local` source installation, remove installed SHR-DAW files
from this checkout with:

```sh
sudo make uninstall
```

This removes the installed commands, public presets, profiles, rhythms, and
documentation. It deliberately preserves user data under
`${XDG_STATE_HOME:-~/.local/state}/shsynth/` and
`${XDG_DATA_HOME:-~/.local/share}/shsynth/`, repository-local `user/`, system
packages, JACK policy, and setup backups. Optional CPU/audio tuning is also a
separate explicit system change; inspect/remove it with `shr-audio-tune` before
uninstalling the command if desired. Never delete those retained directories
unless their Projects, Ideas, recordings, loops, and private presets have been
reviewed and backed up.

The Makefile install/uninstall file boundary was validated in an isolated
`DESTDIR`: 21 allowlisted public presets and only manifest-cleared demos were
installed, no `user/` path was included, and staged uninstall removed only
staged product files.

## JACK

JACK must be running before loading a software synth, playing WAV loops, or
recording audio. The browser and external-MIDI tracker can start without JACK.

On Patchbox, setup detects and retains the shared `jack.service` and its
`/etc/jackdrc`; it does not create a competing owner. When no system service or
live `jackd` process owns JACK, setup can create a backed-up `~/.jackdrc` for
the musician's next explicit JACK start. It never enables, starts, stops, or
restarts JACK. Choose a sample rate that matches the WAV loops you intend to
use, normally 48000 Hz for the installed cleared loops. Three periods is the
safe USB default; lower latency should be earned with xrun measurement.

## Optional dedicated audio CPU

On a Raspberry Pi with at least four cores, the setup wizard can reserve one
CPU for JACK and the one software synth managed by SHR-DAW. The wizard asks
before making this system-wide change and defaults to no. If accepted, it runs
the managed tuning helper immediately; boot-time isolation takes effect only
after the user reboots.

The purpose is predictable real-time scheduling under demanding simultaneous
playback and recording, such as an 18-input/18-output session. Keeping normal
tasks and most interrupts off the audio CPU reduces the chance that a compiler,
desktop task, or unrelated device delays JACK at the wrong moment. The tradeoff
is deliberate: normal work has one fewer CPU until the profile is removed and
the Pi is rebooted. Rust linking is largely serial, so it is often the longest
build stage either way; parallel compilation can be somewhat slower with one
of four CPUs reserved. Stopping JACK alone does not return an isolated CPU to
general scheduling.

The optional profile:

- pins JACK and the managed synth to the selected CPU;
- uses `isolcpus=domain,managed_irq,CPU` and keeps default IRQ affinity on the
  housekeeping CPUs;
- adds `nohz_full` and RCU callback offload only when the installed kernel was
  built with their required options;
- enables a managed `performance` governor service for the next boot without
  changing the live governor during setup;
- records exact boot tokens and file hashes in `/var/lib/shr-audio-tune/`;
- refuses to replace CPU isolation settings it did not create.

Preview, diagnose, recover, or remove the managed settings with:

```sh
shr-audio-tune plan 3
shr-audio-tune status
shr-audio-tune doctor 3
sudo shr-audio-tune recover
sudo shr-audio-tune remove
```

After removing them, clear `audio.engine_cpu` in `shsynth.conf` and reboot.
CPU isolation leaves fewer cores for normal system work. It can improve audio
scheduling, but it cannot prevent every xrun caused by hardware, firmware, or
an unsuitable JACK buffer size.

The full setting-by-setting decision matrix, Patchbox versus stock baseline,
rollback rules, rejected folklore, and primary-source provenance are in
[Raspberry Pi audio-system optimization](AUDIO_SYSTEM_OPTIMIZATION.md).

Continue with [First run](FIRST_RUN.md).
