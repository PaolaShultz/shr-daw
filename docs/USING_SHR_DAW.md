# Using SHR-DAW

SHR-DAW is a Raspberry Pi groovebox and song-sketching workstation built
around a 40×13 terminal. It supports a control surface, MIDI keyboards, the
computer keyboard, and terminal pointer input. It is meant for quick musical
work, not as a desktop production DAW.

## From an idea to a sketch

1. Start a Project or load one from **FILES**.
2. Choose a software instrument or a routed external MIDI instrument.
3. Play freely, record a MIDI Idea, enter tracker notes, add drums, or attach
   Pattern-owned WAV loops.
4. Build an Arrangement or perform the existing Patterns through **LIVE**.
5. Add only the effects and final processing the sketch needs.
6. Record the final stereo performance or synchronized raw stems.
7. Share the resulting files with tools outside SHR-DAW.

The intended idea-to-sketch target is roughly 10 to 15 minutes. That is a
design goal, not a measured guarantee or a promise of a finished production.

## Boundaries

SHR-DAW has no desktop timeline, plugin windows, waveform editor, free-wiring
matrix, unlimited mixer, offline song renderer, upload service, or messaging
system. It does not replace Ardour, Ableton Live, Reaper, Bitwig, or another
full production workstation.

The program helps you play, sequence, arrange, shape, and record your own
choices. Proposed musical assistants remain in
[Future improvements](FUTURE_IMPROVEMENTS.md); they are not current features.

## Terms used in the guides

- **FT2** means the vertical Pattern editor inspired by FastTracker II. SHR-DAW
  is not an FT2 clone and does not read XM files.
- **MTR** is the on-screen performance meter and final-bus control surface.
- **JACK** is the JACK Audio Connection Kit, the low-latency audio server and
  connection graph. **ALSA** is the Advanced Linux Sound Architecture layer
  used for Linux MIDI and audio devices.
- A MIDI **CC** is a Control Change message. **PPQN** means pulses per quarter
  note and describes MIDI clock resolution.
- **LUFS** measures perceived loudness relative to full scale. **dBTP** means
  decibels true peak.
- **XDG directories** are the standard Linux locations used for private
  configuration and data.
- **Owned** means SHR-DAW started or created something and may safely change or
  stop it. An **exact route** is one saved endpoint that SHR never guesses.

## Instruments and Playback

The installed SHR-DAW sound system presents five melodic instrument families
in Presets:

- synthv1 presets;
- Yoshimi `.xiz` banks;
- FluidSynth `.sf2` and `.sf3` SoundFonts;
- Moj Sint `.mojsint` Model D, Six-Op PM, Strange Oscillator, Swarm Machine,
  and Bass Matrix presets;
- SHR Sampler `.shrinst` packages.

Browsing is silent. `LOAD` is the only managed start or replacement action.
Only one SHR-managed melodic engine runs at a time. The loaded instrument
continues when you leave Presets or Playback and stops on replacement, Panic,
shutdown, or an FT2 route that needs a different backend.

Playback shows held notes, decimal MIDI strike velocity, chord names, a
keyboard-state strip when space permits, and 12 controls for the active
backend. `SAVE` offers Overwrite, Save New, and Cancel for synthv1 and Moj Sint.
Factory and system sounds stay read-only; saving them creates the next private
`User NNN` sound for that engine and Moj model. The saved values become the
current RESET baseline without restarting the engine. The new sound appears
immediately in Presets and under its Moj model in FT2 ROUTE. When the running
sound belongs to an FT2 route, only that active owner is retargeted; saving a
standalone Player sound never rewrites unrelated Project routes. Cancel or a
failed save preserves list/cursor state, values, held notes, and the live
session. Unsupported engines remain visibly read-only. Idea capture and saving
remain on the Ideas screen. `SOUNDS` returns to Presets and its visible `LOAD`.

Playback's PLAY page also owns the optional external-sync controller
arpeggiator. `PLAY` sends Start even when no MIDI take exists, `RECORD` starts
the same clock before capture, `STOP` ends it without unloading the instrument,
and `TAP` changes tempo without silently starting transport.

Moj Sint keeps its own preset format and controls. Its 16 authored starts form
five editable model families. SHR Sampler packages are read-only and use their
own strict format; the installed project-authored factory package is a neutral
first-load sound. SHR Drums is separate from the managed melodic engine and
runs in process. See [SHR-DAW instruments and drums](INSTRUMENTS_AND_DRUMS.md)
for the complete sound-system guide: Moj controls and saves, Sampler
validation, Drums kits, routing, ownership, recovery, and public provenance.

## Explore with N00B

Playback N00B filters live melodic input to a selected root plus major or
natural-minor scale. Allowed notes keep their pitch; other notes stay silent.
The screen still shows the normal notes, chords, velocities, keyboard, and
controls.

FT2 uses the same scale on melodic pages in Play, Record, and Edit. Record and
Edit write only accepted notes. Moving to a percussion page turns the filter
off. The Project stores its tonic and scale mode; SHR does not infer a key from
audio or a finished Arrangement.

The practical loop is simple:

```text
press -> hear -> see -> change -> compare -> ask why
```

## Workspaces

Home opens Software Synths, FT2, Recorder, Performance, MIDI Learn, Routing,
Effects, Ideas, and Help. The main encoder browses; press it to select. Back
returns one level. Controller MIDI never quits the application.

Shift plus the main rotary changes a second reversible browse axis only where
one exists, such as Preset engine, FT2 column, Live lane, Loop slot, drum
genre, FX target, or MASTER STRIP section. It stays inert where an accidental
turn could trigger transport, confirmation, or a destructive action.

Ordinary overlays preserve their caller and discard unconfirmed drafts. FT2
ROUTE is the live-audition exception: valid active-field choices change the
Project and live route at once, Apply keeps them, and Cancel restores the route
snapshot from when the overlay opened.

Use these focused guides for exact actions:

- [Screen and menu manual](MENU_MANUAL.md) for every screen and controller
  page;
- [Tracker guide](TRACKER.md) for FT2 editing, routing, Patterns, Arrangement,
  drums, loops, and files;
- [Controller interface](CONTROLLER_INTERFACE.md) for the complete physical
  action contract;
- [Configuration and routing](CONFIGURATION.md) for machine settings and
  persisted route fields.

## Live Patterns, Loop Mix, and Ideas

Live Patterns lets you browse without launching, then queue activation at a
Pattern or bar boundary. It can also capture successful activations for an
explicit Append or Replace confirmation. Its lane mute, velocity, gate, and
transpose controls are temporary and do not rewrite note cells.

Each FT2 Pattern owns up to four private WAV loop references. Loop Mix browsing
does not launch audio. Arrangement and Live boundaries switch the MIDI and WAV
owners together. A bad slot is isolated, and SHR does not time-stretch files.

Ideas preserve free-timed MIDI. A synthv1 or Moj Sint Idea includes its private
preset snapshot; other backends keep their instrument reference. SHR Sampler
stores the package's stable ID and configured path without copying its sample
data. Loading an Idea restores its sound before playback.

See [Live performance](LIVE_PERFORMANCE.md) for boundary timing, capture,
failure behavior, and realtime limits.

## Automation, click, and MIDI export

FT2 **AUTO** records compact Pattern-owned control curves independently from
notes and cell commands. Arm only the lane you mean to write. Continuous
controls ramp to the next point; switches, choices, modes, divisions, and
bypass step at their point. Play Here and loops chase the effective value, and
physical knobs keep pickup protection when automation takes or releases
ownership.

Keep **CLICK** on when recording from stop: SHR-DAW accents beat one, displays
one bar of `4 3 2 1 → REC`, and begins capture at row zero. Punching into a
playing Arrangement starts immediately. The click is internal audio and never
reaches an instrument or external MIDI output.

FILES **EXPORT** analyses, then confirms, a non-overwriting format-1 MIDI file
for the whole Arrangement. Tempo, meter, parts, setup, notes/gates, and portable
CC automation use the same musical-tick interpretation as playback. Loop audio
and SHR-only effect automation are counted as omissions.

## Effects and final sound

Effects contains bounded source, aux, drums, and master processing. The
Project owns rack order, parameters, routing, the fixed DRUMS
Reverb-then-Delay rack, and the fixed MASTER STRIP.

With the optional graph disabled, ordinary source, aux, and master rack edits
change Project data but do not process direct audio. The DRUMS rack still
processes in-process drums on their direct path. With the graph active, stop
transport and recording before changing graph structure. MASTER STRIP value
changes can be auditioned during playback but are refused during final
recording.

The exact placement and safety rules live in [How SHR-DAW
works](HOW_IT_WORKS.md), [Audio graph and DSP contract](AUDIO_GRAPH.md), and
[Fixed stereo MASTER STRIP](MASTER_STRIP_MEASUREMENT.md).

## Recording and meters

Recorder writes armed exact JACK sources as synchronized mono 24-bit WAV stems
with one timeline and manifest. **LEVELS** opens the fixed 18-channel overview.
A missing assigned source blocks take start until it is reassigned or disarmed.
Interrupted recognized takes remain available for bounded recovery.

Performance owns the optional final bus and one 24-bit stereo recording of the
same post-strip samples sent to playback. Input software monitoring starts off
and uses one `MON ON` or `MON OFF` action. Enable it only after checking for
direct hardware monitoring, or the input may be heard twice.

All horizontal meters use circular `●` LEDs. Green is the safe range; yellow
and red appear only at their active thresholds. A brighter circle holds the
peak.

Read [Synchronized multitrack recording](MULTITRACK_RECORDING.md) and
[Final stereo performance bus](FINAL_PERFORMANCE_BUS.md) before relying on a
recording or monitoring setup.

## Command line

Common inspection and setup commands are:

```sh
shr menu
shr list
shr status
shr doctor
shr start "synthv1:Velvet Tines"
shr stop
shr log 80
shr ideas list
shr pads auto [PORT_MATCH]
shr pads learn [PORT_MATCH]
shr config paths
shr config init [--force]
```

`shr config init` preserves existing files unless `--force` is given.
`shr casio diagnostic` is a legacy-named, non-transmitting route report.
Command-line Idea playback restores the Idea's instrument and stops on Ctrl+C.

The complete command inventory is in `shr --help`. Maintenance stress commands
belong to [Maintainer helper scripts](MAINTAINER_HELPERS.md).
`effects-checkpoint` is different: it starts a prepared JACK graph and synth,
sends a low-gain note, and measures a bounded run. Run it only with
explicit authorization. Its setup contract is in
[Configuration and routing](CONFIGURATION.md#owned-audio-graph).
