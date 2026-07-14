# How SHR-DAW works

This document explains the behavior behind the small interface. For normal
setup and use, start with [First run](FIRST_RUN.md) and
[Using SHR-DAW](USING_SHR_DAW.md).

## Software instruments

SHR-DAW controls three separately installed open-source synth engines:

- [synthv1](https://synthv1.sourceforge.io/), a subtractive polyphonic synth;
- [Yoshimi](https://yoshimi.github.io/), a software synthesizer derived from
  ZynAddSubFX;
- [FluidSynth](https://www.fluidsynth.org/), a SoundFont 2 synthesizer.

Their code and factory sounds are not part of SHR-DAW. SHR-DAW starts them as
external processes and connects their configured MIDI and JACK ports. See
[THIRD_PARTY.md](../THIRD_PARTY.md) for authorship, licences, and packaging
details.

Only one managed software instrument runs at a time. Before changing engines,
SHR-DAW sends All Notes Off and stops the process it started. It does not stop
other synth processes owned by the user.

## MIDI routing and safety

The controller has one route into SHR-DAW. Command pads, the encoder, and
mapped controls are consumed inside the program. Musical messages pass to the
selected software instrument or tracker destination.

Tracker pages own their notes separately. Stop, mute, route changes, song
replacement, and exit release only the notes affected by that action. If two
lanes hold the same note on the same device and channel, the note is released
after its last lane owner ends.

All hardware names, ports, channels, commands, and paths belong in
`shsynth.conf`, `controller.conf`, MIDI profile data, or a song. They are not
compiled into the Rust program.

## Controller pickup

The 12 mapped synthv1 controls use pickup. After a preset or idea loads, or the
controls are reset, incoming mapped CC messages are blocked until the physical
control reaches or crosses the loaded value. This prevents a knob from causing
a sudden jump.

The Playback screen compares each value with the original preset. Green means
more than 0.03 below the original, bright yellow means within 0.03, and red
means more than 0.03 above it. Reset changes only these 12 parameters and
re-arms pickup; it does not restart the synth.

## Audio recording

The JACK audio callback moves samples into a fixed ring buffer. A normal disk
thread writes them as a 24-bit stereo WAV file. Keeping disk work out of the
real-time callback protects audio responsiveness.

An interrupted recording stays as `.wav.part`. SHR-DAW attempts recovery when
the next recording starts. Live input is not sent from JACK capture back to
JACK playback; use hardware direct monitoring when available.

## Data and configuration

User configuration is stored below:

```text
${XDG_STATE_HOME:-~/.local/state}/shsynth/
```

The main files are:

- `shsynth.conf` for engines, sound paths, MIDI, JACK, recording, and tracker
  output;
- `controller.conf` for the controller input, encoder, buttons, and 12 mapped
  controls.

User data is stored below `${XDG_DATA_HOME:-~/.local/share}/shsynth/`:

- `ideas/` contains MIDI ideas;
- `songs/` contains tracker songs;
- recordings use the configured `capture.directory`;
- MIDI device profiles can be added under `midi-devices/`.

The repository contains 21 cleared synthv1 presets. A private preset bank can
be selected with `synthv1.presets` or `SHSYNTH_PRESET_DIR`. Yoshimi banks and
SoundFonts are read where they are installed instead of being copied into the
project.

For all keys and routes, see [Configuration and routing](CONFIGURATION.md).
