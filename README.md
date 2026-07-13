# SHSynth

SHSynth is a small Rust terminal instrument appliance for three selectable
sound engines: `synthv1_jack`, Yoshimi, and FluidSynth. It browses appropriate
sounds for the selected engine, monitors and routes MIDI, records engine-neutral
MIDI ideas, drives an optional external accompaniment keyboard, captures stereo
JACK input, and remains usable in a 40×20 terminal on a 480×320 display.

One SHSynth instance owns at most one engine process. Switching engines sends
All Notes Off, stops only the child process whose identity SHSynth recorded,
and starts the selected backend. It never layers managed engines and never
searches for or kills unrelated synth processes. The older `synth-player` Bash
program remains a synthv1-only fallback; do not run both interfaces together.

## Install

On Raspberry Pi OS/Debian, the installer adds JACK/ALSA tools and all three
engines, installs the Rust 1.85 toolchain when needed, tests/builds the app, and
runs an interactive hardware-routing wizard:

```sh
./scripts/install.sh
```

The engine packages are `synthv1`, `yoshimi`/`yoshimi-data`, and `fluidsynth`.
The installer also installs the compact TimGM6mb SoundFont as a usable default.
Use `--no-deps` if engines and build dependencies are managed separately, or
`--no-config` to skip the wizard. Installed commands are `shr` (`shsynth`),
`shr-setup` (`shsynth-setup`), and the legacy `shs` (`synth-player`). Run
`shr-setup` again whenever the controller, MIDI interface, sound card, or JACK
port layout changes.

Run `shr doctor` after configuring audio/MIDI. JACK must already have a working
audio server/session. The wizard can optionally write a backed-up `~/.jackdrc`
for a selected USB or Raspberry Pi ALSA card, but it never starts or restarts
JACK. Existing JACK/session policy is kept unless that option is selected.

### Repository-local private data

For development or a self-contained personal checkout, use:

```sh
./scripts/setup-local.sh
./scripts/local.sh
```

These launchers redirect SHSynth state, ideas, songs, recordings, and private
synthv1 presets into the ignored `user/` directory. Set `SHSYNTH_USER_DIR` to
move that entire private tree without changing the scripts. Public/cleared
presets remain in `presets/synthv1/`; private or uncleared presets belong in
`user/presets/synthv1/`. Nothing under `user/` is installed or committed.

## Interface and engine selection

There are five screens:

- **Presets:** the header clearly shows the selected backend. PROG/LOOP cycle
  backward/forward through synthv1, Yoshimi, and FluidSynth. The main encoder
  continues to scroll only that engine's sound list; press it to load the
  highlighted sound. Optional engines with a missing executable or no
  configured sounds remain selectable and show an unavailable reason.
- **Playback:** play, view held notes/chords, and record MIDI. For synthv1 this
  screen shows the 12 mapped parameters; pressing the main encoder resets those
  parameters in place and re-arms pickup without restarting synthv1. Yoshimi
  and FluidSynth do not pretend to implement synthv1 parameters, so the same
  press reports that no mapped reset is available and leaves the sound alone.
- **Ideas:** inspect, load, or delete saved takes. Destructive/replacing actions
  require a second confirmation.
- **Tracker:** edit and play reusable MIDI patterns on the external Casio. It
  remains editable when the configured MIDI output is absent.
- **Stereo Recorder:** capture the configured JACK input pair to 24-bit WAV.
  Audio capture is deliberately separate from MIDI Ideas.

Yoshimi stays alive while `.xiz` instruments are loaded through its headless
command interface. FluidSynth loads configured SoundFonts once and changes
sounds with MIDI bank/program messages. Selecting a sound from another backend
performs the panic/owned-shutdown/start sequence before musical MIDI is routed
to the new engine.

Keyboard controls are arrows or `j`/`k`, Page Up/Down, Home/End, Enter,
`[`/`]` for engine selection on Presets, `B`/Escape back, `S`/Space stop, `R`
record, `P` recorded-MIDI playback, `W` Ideas, `T` Tracker, `A` stereo recorder,
and `Q` quit. Exiting and
termination signals restore the terminal, send panic, and stop the owned child.

## Controller behavior

The default MiniLab mapping is:

| Physical controls | Incoming CC | Purpose |
|---|---:|---|
| Top rotaries 1–4 | 74, 71, 76, 77 | synthv1 cutoff, resonance, filter envelope, LFO rate |
| Bottom rotaries 1–4 | 93, 18, 19, 16 | synthv1 volume, delay wet, time, feedback |
| Right sliders 1–4 | 82, 83, 85, 17 | synthv1 amplifier attack, decay, sustain, release |
| Main encoder turn | 28, relative around 64 | scroll; on Tracker, rows or TAP-held tempo |
| Main encoder press | 118, 127 press / 0 release | select/reset; Tracker EDIT BLANK/SKIP |
| Shift | 27 | toggle command-pad lock; red `LCK` means pads play as notes |

Values below 64 on relative CC 28 move up; values above 64 move down. Encoder
press release (CC 118 value 0) is consumed but does not select twice. Navigation
messages and mapped command-pad note-on/note-off are consumed before the engine
and recorder. Musical notes, sustain, modulation, bend, pressure, program and
other MIDI messages pass through the monitored route to whichever backend is
active, preserving recording and held-note/chord display.

The 12 synthv1 controls use pickup/catch after loading or resetting. A mapped
CC is blocked before synthv1 until it reaches or crosses the loaded value. The
indicators are relative to the original preset: green below −0.03, bright
yellow within ±0.03, and red above +0.03. These synthv1 indices/ranges are not
applied to Yoshimi or FluidSynth. Their physical CC messages pass normally with
one useful exception: whichever physical control maps to synthv1 Volume is
translated to standard MIDI Channel Volume CC 7 on Yoshimi and FluidSynth.
This keeps one reliable volume control across all three backends without
pretending the other synthv1 parameter mappings are portable.

MiniLab Memory 1 pads use notes 36–43. Their logical actions remain ARP, PAD,
PROG, LOOP, STOP, PLAY, SAVE, and TAP. On Presets, PROG/LOOP select the previous
or next sound engine without loading anything. On Playback, PLAY starts/stops
the most recent recorded-MIDI take; engine selection is unavailable there.

The second physical PAD button opens Tracker from Presets, Playback, or Ideas.
Once on Tracker the assignments are FILE, EDIT, LANE−, LANE+, STOP,
PLAY, SAVE, and TAP. LANE−/LANE+ cross the MELODY/DRUMS boundary, so all eight
lanes are reachable without repurposing a pad. STOP halts Casio accompaniment; pressing it again while
stopped returns to Presets. Global keyboard Shift-`S`/Space silences both
destinations. Main encoder turn moves through rows; in EDIT its press performs
BLANK/SKIP. Hold TAP while turning the encoder to change Tracker BPM without
moving the row cursor. Tap and rotary changes update the same working tempo,
shown once at bottom-right; changes also retime active Tracker playback. FILE
opens a dedicated screen where the master encoder selects songs: turn to move
and press to LOAD. Physical pads provide NEW PAT, CLEAR PAT, BACK, PLAY, SAVE,
and DELETE; two pad slots remain free for later functions. PLAY auditions the
highlighted saved song without replacing the current edit; press it again to
stop. CLEAR PAT and DELETE require confirmation, and SAVE retains its overwrite
confirmation.
While EDIT is active, TAP is labelled ERASE and clears only the selected cell;
holding it while turning the master encoder still adjusts tempo.
Pad note-on/off, encoder messages, and
navigation CCs are consumed before both MIDI destinations, MIDI Ideas, and
tracker note entry.

The raw Arturia Shift button (CC 27 by default) toggles pad lock on every screen.
With red `LCK` at top-right, notes 36–43 bypass SHSynth pad commands and pass as
ordinary musical note-on/off messages; press Shift again to restore commands.
SHSynth still does not use the controller's shifted pad layer (CC 105–108),
which overlaps internal arpeggiator/Tap Tempo behavior and is not reliable.

## Configuration and sound locations

`shr config init` creates `shsynth.conf` and `controller.conf` under
`${XDG_STATE_HOME:-~/.local/state}/shsynth/`. Existing files are preserved;
`--force` explicitly replaces both. Repository examples live under `config/`.
Old v1 keys (`synth.command`, `synth.client`, `presets.directory`, and
`midi.synth_output`) remain accepted and inherit defaults for optional engines.

`shsynth.conf` contains all executable names, client/port matches, paths, and
routes:

- `synthv1.command`, `.client`, `.presets`, and `.midi_output`;
- `yoshimi.command`, `.client`, `.midi_output`, repeatable `.preset_root`,
  repeatable `.category`, and `.presets_per_category`;
- `fluidsynth.command`, `.client`, `.midi_output`, `.gain`, and repeatable
  `.soundfont` (`gain=0.4` is a moderate increase over FluidSynth's 0.2 default);
- ordered `midi.input` matches, JACK `audio.output` routes, autoconnect flags,
  startup timeout, and the optional CPU temperature path.
- `external_midi.*` enable/output/capability/transport/timing settings;
- repeatable `capture.input=NAME|LEFT_PORT|RIGHT_PORT`, `capture.directory`,
  and `capture.ring_frames`.

The repository/installed `presets/synthv1/` directory is used when the synthv1
path is empty; `SHSYNTH_PRESET_DIR` remains a compatibility override. Yoshimi
roots are searched recursively for `.xiz`, then constrained to the configured
categories and per-category limit so system banks do not become an unusable
thousand-item flat list. There are no automatic favorites: expensive additive
or PAD instruments should be promoted only after hardware benchmarking.

FluidSynth indexes each configured `.sf2`/`.sf3` in place and enumerates its
SoundFont preset headers. The example points at Debian's
`/usr/share/sounds/sf2/TimGM6mb.sf2`; put a better installed SoundFont first or
remove TimGM later without changing code. Multiple fonts receive private bank
offsets internally so overlapping bank/program numbers remain selectable.
The browser and playback title show only the useful SoundFont preset name;
font/bank/program metadata remains internal rather than cluttering the screen.
System Yoshimi banks and SoundFonts are never copied into this repository.

`controller.conf` owns the controller input preference, encoder and pad-lock CCs, synthv1
physical-to-mapped CC table, and command-pad notes. CLI mapping commands remain:

```sh
shsynth pads list
shsynth pads input "Controller port name"
shsynth pads cc 20 74
shsynth pads set 51 rec
shsynth pads clear 51
```

The setup wizard discovers ALSA sequencer ports and physical JACK audio ports,
then writes the selected controller input, stereo engine output, optional stereo
recording input, and optional external hardware MIDI output. Every choice can
also be entered as an exact match when discovery is unavailable. It backs up
both configuration files before changing them and does not run an audible test.

## MIDI ideas

Recording captures timed musical MIDI, not audio. SAVE starts recording; STOP
finishes and saves the next numbered idea. Physical PLAY or keyboard `P` plays
the last take through the current engine. Completion/stop sends All Notes Off
on all 16 channels.

Physical FILE on Playback opens the Ideas file screen. The master encoder turn
selects a saved idea and its press LOADs it. Pads provide PREVIEW, BACK, PLAY,
SAVE, and DELETE, leaving three slots available for later functions. PREVIEW shows the selected
idea's saved details; LOAD restores its instrument and take, after which PLAY
auditions it. Destructive deletion still requires confirmation.

Ideas live under `${XDG_DATA_HOME:-~/.local/share}/shsynth/ideas/NAME/`:

- `metadata.json`: format/version, backend-aware preset identity, creation time,
  event count, and synthv1 mapped values only when applicable;
- `preset.ref`: backend and backend-specific path/bank/program identity;
- `recording.mid`: format-0 MIDI with a one-millisecond timebase;
- `preset.synthv1`: a durable snapshot for synthv1 ideas only.

Yoshimi instruments and SoundFonts remain external system/user data and are
referenced rather than copied. Old version-1 synthv1 ideas without `preset.ref`
still load. If an external sound was removed, loading its idea gives a clear
missing-source error rather than silently substituting a sound.

## Casio tracker and wiring

Connect soundcard **MIDI OUT → Casio MIDI IN**. For audio capture, connect the
Casio line/headphone outputs to the soundcard's left/right inputs. Start with
the Casio and interface input gain low: a headphone output can overload a line
input. Do not route captured input back to the Casio or speakers at high gain;
disable hardware/software monitoring paths that create feedback.

The Casio is never an owned process and never counts as a software backend.
Changing synthv1/Yoshimi/FluidSynth leaves tracker playback running. A missing
or disabled destination displays `Casio MIDI unavailable`; songs remain
editable and no software engine is affected. Live thru defaults off. If it is
enabled, only musical MIDI that survived command/navigation filtering is sent.

The installed `casio-casiotone-mt-240` profile defines two four-lane pages.
MELODY's four polyphonic lanes share MIDI channel 1 (SHSynth limits the
keyboard's six-note part to four). DRUMS' four lanes share channel 2, select
program 9 `PERCUSSION`, and use the configured sparse note map. Lanes are not
MIDI channels. Page channels, programs, gate, sparse map, and the conservative
gesture settling interval remain configurable in `shsynth.conf`.

Tracker rows 1, 9, 17, and so on (displayed as hexadecimal 00, 08, 10, etc.)
are yellow beat-start markers. Main encoder turn moves through rows;
LANE−/LANE+ move across four lanes and then onto the other page. Tab switches
the visible MELODY/DRUMS page without changing transport. During playback the
row follows the sequencer and the grid scrolls automatically. Both pages play
together regardless of which one is visible. Page Up/Down changes order.

Tracker computer keys include arrows (rows/lanes), Tab (page), Enter or EDIT
(step mode), the piano row `Z S X D C V G B H N J M`, `.` or Insert
BLANK/SKIP, `-` note OFF, Delete clear cell, `M` lane mute, Shift-`M` page mute,
`P` play from the cursor, Shift-`P` play from the start, `+`/`_` program,
`<`/`>` tempo, Shift-`N` new pattern, Shift-`C` copy pattern, Shift-`X` clear
pattern, Shift-`O` append the current pattern, Backspace remove an order entry,
`V` save, and `L` load. SAVE refuses an existing name until confirmed again.
NEW PAT creates a separate blank pattern, appends it to the song order, and
makes it current. Press BACK to edit it, then SAVE to store the complete song:
patterns 1, 2, and later patterns intentionally live together in one song file.

Songs use human-readable `SHSYNTH-SONG 2` files under
`${XDG_DATA_HOME:-~/.local/share}/shsynth/songs/`. Version 2 records explicit
page roles, channels, programs, and four lanes per page. Version-1 files load
through an in-memory conversion: old non-percussion columns become MELODY
lanes, the percussion column becomes a DRUMS lane, and the MT-240 page roles
use channels 1/2. Loading never rewrites the old file. Unknown/newer fields or
versions are refused, and confirmed save still will not overwrite an
unsupported/newer file. Publication uses a synced temporary file and atomic
rename.

On DRUMS, the computer piano row uses the configured
`external_midi.percussion_note` map when present, or starts at MIDI note 36 as
a generic fallback. The page sequences percussion notes; it does not start a
Casio panel rhythm. `external_midi.send_transport=true` adds MIDI
Start/Clock/Stop only for hardware known to accept it.

The MT-240 channel-1, channel-2, and channel-3 parts accept six, four, and two
notes respectively. SHSynth uses channel 1 for up to four melody notes and
channel 2 for up to four percussion notes; channel 3 is unused by this layout.
The controller range 60–71 maps to drum notes 36, 38, 40, 41, 43, 45, 47, 48,
50, 52, 53, and 60.

While Tracker EDIT is active, piano note-on/off messages are auditioned only
on the Casio destination for the visible page and never reach the selected
software synth. A gesture begins with the first note and commits after every
held key is released plus the configured settling interval, so slightly rolled
chords remain one entry. Up to four distinct notes retain their velocities,
are sorted low-to-high starting at the selected lane (wrapping on that page),
and advance exactly one row. Columns not written by that gesture are preserved.
A fifth
distinct note rejects the gesture with a clear status. Repeated notes and
velocity-zero note-offs are tracked correctly. Navigation and encoder messages
and the eight command pads are consumed first; with pad lock active, those pad
pitches remain playable notes. Edit-off, page changes, STOP, Back, song load,
destination loss, and exit cancel partial gestures and end audition notes.

BLANK/SKIP advances one row without modifying the four cells and wraps from
the final row to row 1. It is distinct from note `OFF` and Delete/clear. Each
cell stores note/off, velocity, optional program, and one small command:
cut, 1/16-row delay, bounded retrigger, or tempo for following rows. Bank and
program messages precede the affected note; the configured gate schedules its
note-off. Order playback loops until stopped. Lane/page mute ends only notes
owned by those lanes. Stop, song replacement, destination loss, and exit use
sustain-off, All Notes Off, and All Sound Off on the configured channels.

`shsynth casio diagnostic` is non-audible: it lists MIDI output ports, reports
the configured match and capabilities, and prints intended dry-run messages
without transmitting them. Use it before physical testing.

Physical tests require explicit authorization and should proceed one capability
at a time: single note plus All Notes Off; program; bank; several channels;
percussion; clock/start/stop; stereo input level; then a sustained recorder
overrun test. Disable each unsupported capability again.

## Stereo audio recording

The Audio Recorder screen explicitly starts/stops the first configured stereo
pair. JACK supplies float samples; its real-time callback only copies them into
a bounded, preallocated single-producer/single-consumer ring. A disk thread
clamps and writes stereo 24-bit PCM WAV. The screen reports elapsed time, sample
rate, path/size, dropped frames, recovery, and errors. Recording also checks
initial free space and reports write/ENOSPC failures.

Files are timestamped (with a sanitized optional name supported by the recorder
API) under `capture.directory`. Recording uses `.wav.part`; clean stop fixes
the WAV header, syncs it, and atomically publishes without replacing a file.
On the next start, complete frames in interrupted parts are finalized as a
unique `-recovered.wav`; too-short parts are retained and reported. Exit
attempts the same clean finalization.

The configuration is list-shaped for future stereo tracks, but the UI exposes
only one pair and SHSynth does **not** claim multitrack capture. Before enabling
simultaneous pairs, benchmark storage throughput, CPU load, JACK buffer size,
sustained duration, dropped-frame count, and channel synchronization on the
target appliance.

## Commands and runtime ownership

```sh
shsynth menu
shsynth list
shsynth status
shsynth start "synthv1:Velvet Tines"
shsynth start "Yoshimi:Fat Bass"
shsynth stop
shsynth log 80
```

`start` runs a supervised background backend; the interactive menu owns its
backend for the UI lifetime. Runtime owner identity, current sound, generated
engine configuration, and logs are under the SHSynth state directory. Before
signaling a stored PID, SHSynth verifies its process start time and executable;
legacy bare-PID files are discarded rather than trusted.

For adding cleared synthv1 sounds, see [New patches and sounds](docs/NEW_PATCHES.md).
For engine/preset licensing and the prohibited legacy archive, see
[THIRD_PARTY.md](THIRD_PARTY.md).

## Autorun

Enable only after physical testing. Use either a user systemd service or a
desktop terminal autostart entry, not both. It should run `shsynth menu`,
restart on failure with a delay, and start only after the JACK/session
environment is ready.
