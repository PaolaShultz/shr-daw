
![SHR-DAW](docs/images/shr-daw-header.jpg)

SHR-DAW is a compact Raspberry Pi music workstation for a 40×13 terminal,
optional MIDI gear, software instruments, FT2-style sequencing, WAV loops,
effects, and JACK recording.

It grew from a personal need to play synths and capture ideas quickly while on
the move or jamming with friends. Its purpose is exploration. A finished
production was never the goal, although it can record a rough demo to send to
friends.

Read the complete public documentation at
[paolashultz.github.io/shr-daw](https://paolashultz.github.io/shr-daw/).

> [!WARNING]
> SHR-DAW is experimental. Back up Projects and user data, and begin audio
> testing at a low monitoring level.

## Features

- Play synthv1, Yoshimi, FluidSynth, or Moj Sint through one safely managed engine.
- Build routed multi-page Patterns, Arrangements, drum parts, and private WAV
  loop performances in the FT2 workspace, including quantized Live Patterns
  and a four-slot Loop Mix.
- Save free-timed MIDI Ideas, synchronized raw JACK stems, or the protected
  final stereo performance mix.
- Use the computer keyboard, mouse, or a configured four-, five-, or
  eight-button controller.

See [Using SHR-DAW](docs/USING_SHR_DAW.md) for musical workflows and
[How it works](docs/HOW_IT_WORKS.md) for routing, ownership, storage, and
failure boundaries.

## Install and run

On Patchbox OS, Raspberry Pi OS, or Debian:

```sh
./scripts/install.sh
shr-setup
shr doctor
shr
```

JACK is optional for browsing and external-MIDI sequencing, but required for
software-instrument audio, WAV loops, effects, and audio recording. SHR-DAW
does not start or restart JACK. Continue with [First run](docs/FIRST_RUN.md) or
the full [installation guide](docs/INSTALLATION.md).

For a repository-local development checkout:

```sh
cargo build --locked
./scripts/setup-local.sh
./scripts/local.sh
```

The local helpers keep configuration and user data below ignored `user/` and
launch this checkout's visibly marked `DEV` binary.

## Screenshot tour

### Software instruments

![Preset browser showing synthv1 sounds](docs/images/shr-daw-presets.png)

Browse the separate synthv1, Yoshimi, FluidSynth, and Moj Sint catalogs.

### Playback

![Playback screen with held notes, velocities, and mapped controls](docs/images/shr-daw-playback.png)

Play the loaded sound, inspect notes and chords, shape mapped controls, and
capture MIDI Ideas.

### FT2 Pattern editor

![FT2 Pattern editor with four lanes of note data](docs/images/shr-daw-ft2-pattern.png)

Edit routed melodic or percussion pages and arrange reusable Patterns.

### Live Patterns

![Live Patterns screen with selected, playing, queued, and lane-shaping states](docs/images/shr-daw-live-patterns.png)

Browse without launching, queue or retrigger at Pattern/bar boundaries, shape
four MIDI lanes live, and optionally capture successful launches.

### Loop Mix

![Four-slot Loop Mix with playing, queued, muted, and fault states](docs/images/shr-daw-ft2-loop.png)

Each FT2 Pattern owns four private native-rate WAV references. Arrangement and
Live Pattern changes switch MIDI and loops together; launch/stop, smoothed
level, and bipolar filtering remain available per slot.

### Audio recorder

![Synchronized multitrack recorder with armed and missing inputs](docs/images/shr-daw-audio-recorder.png)

Map exact JACK inputs and record one callback-aligned take as separate mono
stems.

### 18-channel input levels

![All 18 recording inputs shown as three groups of six vertical meters](docs/images/shr-daw-input-monitor.png)

Compare all 18 recording levels at once while keeping setup, routing, and the
final-bus mixer separate.

### Performance bus

![Final performance bus with source, limiter, meter, and recording status](docs/images/shr-daw-performance-meter.png)

Control and record the opt-in four-source final bus, or inspect the passive
meter view while the graph is disabled.

### MASTER STRIP

![Fixed stereo MASTER STRIP with six sections and mastering meters](docs/images/shr-daw-master-strip.png)

Shape and meter the final stereo mix through a fixed, Project-owned mastering
path with protected true-peak output.

The [screen and menu manual](docs/MENU_MANUAL.md) contains the complete visual
tour without duplicating its controls here.

## Documentation

- [First run](docs/FIRST_RUN.md) and [Using SHR-DAW](docs/USING_SHR_DAW.md)
- [Tracker guide](docs/TRACKER.md) and [screen and menu manual](docs/MENU_MANUAL.md)
- [Live performance](docs/LIVE_PERFORMANCE.md)
- [Configuration and routing](docs/CONFIGURATION.md) and
  [controller interface](docs/CONTROLLER_INTERFACE.md)
- [How it works](docs/HOW_IT_WORKS.md), [audio graph](docs/AUDIO_GRAPH.md), and
  [multitrack recording](docs/MULTITRACK_RECORDING.md)
- [Complete documentation index](docs/README.md)

## Built with Codex

SHR-DAW was a pre-existing personal project that was meaningfully extended
during OpenAI Build Week using GPT-5.6 through Codex CLI directly on the target
Raspberry Pi. Codex accelerated Rust implementation, ALSA/JACK/MIDI diagnosis,
controller setup, original preset and rhythm design, safety review, validation,
and documentation. The creator chose the product and musical direction,
supplied and operated the hardware, judged the sound, and controlled public
release. The [development story and dated baseline](docs/BUILD_WEEK.md) describe
that collaboration and distinguish earlier work from Build Week additions.

## Licence

SHR-DAW is MIT licensed. Included presets, demos, rhythms, and WAV loops have
their own documented clearance boundaries; read [THIRD_PARTY.md](THIRD_PARTY.md)
before packaging or adding sounds.
