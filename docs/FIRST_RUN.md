# First run

You can start with a Raspberry Pi and a terminal. A MIDI keyboard, control
surface, audio interface, mixer, and dedicated display are optional.

## Configure and start

After installation, run:

```sh
shr-setup
shr doctor
shr
```

`shr-setup` keeps controller input and musical keyboard input as separate
choices. It also asks about note spelling, MIDI output, JACK Audio Connection
Kit (JACK) playback, audio capture, and optional CPU tuning. The wizard seeds
four starter loops and ten cleared demo Projects. You may also choose a private
MusicRadar loop download after reading its redistribution limit.

Run setup again when a controller, interface, sound card, or port layout
changes. Remembered hardware choices are not replaced just because a device is
temporarily disconnected. If the preferred playback pair is missing, SHR tries
the configured fallbacks in order and reports which one it used.

`shr doctor` is a strict check of the complete setup. Missing JACK therefore
produces a failing result even though the preset browser and external MIDI
tracker can still open. Software instruments, WAV loops, effects, and audio
recording require JACK. SHR-DAW never starts or restarts JACK.

Doctor groups its report into `CORE / EDITOR`, `MIDI`, `JACK AUDIO`, and
`AUDIO TUNING`. It does not change policy or services. Each problem includes
the relevant inspection or recovery command. The optional CPU policy and its
rollback are documented in [Raspberry Pi audio-system
optimization](AUDIO_SYSTEM_OPTIMIZATION.md).

The effects graph starts disabled. Software instruments, SHR Drums, and loops
first use their configured direct routes. Read [How SHR-DAW
works](HOW_IT_WORKS.md) before enabling the final bus or Input software
monitoring.

## Play something

- Use the computer keyboard to navigate and enter notes. The tracker note keys
  are `Z S X D C V G B H N J M`.
- A configured MIDI keyboard adds velocity, chords, and live recording. Its
  musical messages bypass controller commands.
- A configured control surface provides the four-page menus, main encoder,
  pads, and mapped synth controls.
- In the FastTracker II (FT2)-style tracker, open **FILES** to load a seeded
  demo Project. Its portable `AUTO` routes use the destinations and channels
  configured on this machine.
- Open **LIVE** to perform existing Patterns without changing the saved
  Arrangement. Open **LOOP** for the selected Pattern's four WAV slots.

`?` and F1 open contextual Help. The [Using SHR-DAW](USING_SHR_DAW.md) guide
continues from here without repeating every screen action.

## Terminal size

The native layout is 40 columns by 13 rows. SHR adapts to the terminal cell
size and reports when the window is too small. The installer does not change
the console font, display resolution, desktop, window borders, scaling, or
fullscreen mode.

Pixel resolution alone does not determine the available row and column count.
Adjust your terminal settings if fewer than 40 columns or 13 rows fit.

## Run a development checkout

For a repository-local setup:

```sh
cargo build --locked
./scripts/setup-local.sh
./scripts/local.sh
```

The local helpers keep configuration, logs, Projects, Ideas, recordings,
downloads, loops, and private presets below ignored `user/`. They preserve
existing private files and do not install packages or start JACK.
`setup-local.sh` configures the checkout. `local.sh` launches its
`target/debug/shr`, identified as `DEV` in the TUI. Set `SHSYNTH_USER_DIR` to
use another private root.

## Unusual hardware or recovery

The installer and setup wizard are the normal path. For an uncommon controller,
complex route, or recovery problem, follow the
[Codex-assisted setup brief](CODEX_ASSISTED_SETUP.md). It keeps hardware
inspection, audible tests, and system changes behind explicit permission.

After installing and signing in to Codex CLI, start that brief from the
checkout:

```sh
codex -C . "$(cat docs/CODEX_ASSISTED_SETUP.md)"
```

Known USB controllers are matched during setup. Unknown devices can use the
non-audible MIDI learner; learned mappings remain private. See
[Automatic controller setup and MIDI learn](CONTROLLER_PROFILES.md).

For a larger rig, continue with [Physical connections](CONNECTIONS.md).
