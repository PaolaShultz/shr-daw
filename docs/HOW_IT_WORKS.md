# How SHR-DAW works

SHR-DAW is a small music workstation built from several deliberately separate
parts: role-separated controller/performance inputs, one managed software
engine, an FT2-style MIDI sequencer, a private WAV loop player, a synchronized
raw multitrack recorder, and an optional owned final performance bus. This
guide connects those parts and explains what the musician can do with them.
For exact configuration keys use
[Configuration and routing](CONFIGURATION.md); for the DSP and real-time
contract use [Audio graph and DSP contract](AUDIO_GRAPH.md).

## The whole signal model

The shortest useful picture is:

```text
controller / MIDI keyboard
        |
        v
SHR input router
  |                    |
  | commands/controls  +---- musical notes ----------------------+
  v                                                           |  |
screen, menus, pickup                           managed synth <-+  |
                                                or FT2 page -------+

FT2 scheduler -> each page's MIDI destination -> software or hardware instrument

managed synth audio -> direct JACK playback (graph disabled)
private WAV loop  --> direct JACK playback (graph disabled)

managed synth SOURCE/AUX --+
private WAV loop -----------+-> MASTER -> limiter -> FINAL OUT
configured stereo input ----+                +-> final WAV + playback
configured JACK sources -> fixed 18-channel meter snapshot
                        \-> shared callback timeline -> mono stems + manifest
```

The raw-stem recorder remains separate. When enabled, the final bus owns the
exact synth, loop, and one configured stereo-input pair and removes the direct
synth/loop routes transactionally. `FINAL OUT`, final WAV, and playback then
share the same post-limiter samples. They do not secretly include unrelated
JACK clients or downstream interface processing.

## Controller and performance input roles

SHR-DAW opens each exact ALSA source at most once, then classifies messages by
the configured role before they reach an instrument:

- menu buttons, the main encoder, encoder press, and the 12 mapped synthv1
  controls stay inside SHR-DAW;
- command-pad note-on, note-off, velocity-zero release, and polyphonic pressure
  are consumed only when note and optional channel both match, so releasing or
  pressing a menu pad cannot leak while the same note on another channel stays
  musical;
- performance-only inputs bypass every controller mapping; ordinary notes,
  velocity-zero releases, sustain/modulation CCs, pitch bend, channel pressure,
  polyphonic pressure, and other supported channel messages keep the musical
  path;
- a combined input consumes its mapped commands and passes unmatched musical
  messages, while a control-only input suppresses unmatched traffic; and
- pad lock can temporarily treat command pads as notes when that is wanted.

Controller learning listens only to the selected controller source. Separate
performance devices do not need special MIDI channels; channel-qualified
mappings remain available for one combined port. Active-note ownership includes
source, channel, and note, so one device's release, All Notes Off, disconnect,
or route change cannot release an identical note still owned by another.

The reviewed MiniLab factory mapping uses channel 10 for notes 36–43. Its
captured User 1 program uses the keyboard's channel 1 and is therefore not used
for commands. CC27 is DAW Shift, not an SHR pad-lock control.

On a combined input, the captured 12-byte Arturia program notification SysEx is
still forwarded by the generic musical route when an instrument is active. A
control-only input suppresses it with other unmatched traffic. It is device metadata,
but SHR deliberately does not apply a manufacturer-wide SysEx filter: the
current profile schema cannot distinguish that exact notification from other
device traffic. There is no captured evidence that synthv1 acts on this foreign
manufacturer message. An exact profile-qualified metadata rule can be added if
a future workflow needs it; unrelated SysEx must continue to pass.

## Controller clock ownership

An optional dedicated output makes SHR the MiniLab clock/transport master. It
uses the central tracker transport tempo but owns a separate exact ALSA
standard-MIDI connection that can emit only Start, Timing Clock, and Stop. It
never reuses a musical tracker page, so page notes/programs cannot feed back to
the controller and multiple pages cannot multiply pulses. Its ALSA source port
is non-exportable and every event is directly addressed to the selected port,
preventing JACK's automatic sequencer bridge from becoming a second clock
subscriber. Clock is 24 PPQN and
continues evenly across event timing and live phase-preserving tempo changes.

Clock runs whenever the feature is enabled and SHR is open, using the default
tempo before the first transport run. This lets the MiniLab detect clock before
Play; direct capture showed that Start sent before any detected clock was not
enough to launch its External-Sync arpeggiator. With the feature enabled, an
empty Pattern may run specifically for live arpeggiation. Every SHR play is a
fresh launch (`FA`), not a resume; there is no `FB` Continue or `F2` Song
Position Pointer because SHR has no pause/resume transport state. Stop and an
active clean shutdown each produce one `FC` as appropriate, while `F8` keeps
the stopped controller ready at the current tempo until SHR exits.

A controller profile describes what a physical device sends. The setup wizard
can apply a reviewed profile or learn absolute controls, either relative
encoder direction, CC/note buttons, and an encoder press without forwarding
the learning messages to a synth. Learned mappings remain private; reviewed
catalog updates are validated and published atomically. See
[Controller profiles](CONTROLLER_PROFILES.md).

The 12 synthv1 controls use pickup. After a preset or Idea loads, or after
`RESET`, mapped CC messages are blocked until the physical control reaches or
crosses the stored value. This prevents a knob position from making the sound
jump. Playback indicators compare each value with the original preset: green
is more than 0.03 below it, bright yellow is within 0.03, and red is more than
0.03 above it. Reset changes only those mapped parameters, re-arms pickup, and
does not restart the engine.

Held notes drive the Playback note/chord display and its continuous keyboard
strip. Each pitch also has its current MIDI Note On velocity shown directly
beneath its name. Note Off removes only that channel's instance; if multiple
channels hold one pitch, the display deterministically uses the highest still
held velocity. German B/H spelling is the default;
`display.note_names=english` uses A#/B spelling. Naming changes only the
display, never the MIDI notes.

## Software instruments and ownership

SHR-DAW browses three separately installed instrument hosts:

- [synthv1](https://synthv1.sourceforge.io/) for subtractive synth presets;
- [Yoshimi](https://yoshimi.github.io/) for `.xiz` instruments and banks; and
- [FluidSynth](https://www.fluidsynth.org/) for `.sf2` and `.sf3` SoundFonts.

Only one SHR-managed software engine process runs at a time. synthv1 and
Yoshimi retain one current preset. FluidSynth is the exception at the
instrument level, not the process level: its one owned process may hold several
SoundFont presets on compatible MIDI channels while producing the same one
stereo source. Loading another standalone sound may reuse or replace the owned
process; replacement sends All Notes Off, performs a clean shutdown, and
starts the next configured host. SHR-DAW records enough process identity to
stop only the engine it started. It neither layers managed backends nor kills
an unrelated synthv1, Yoshimi, or FluidSynth process opened by the user.

A managed host becomes ready only after SHR resolves one unambiguous stereo
JACK output pair for it; a MIDI JACK/ALSA port alone is not readiness. Exact
configured client names are preferred. A single uniquely prefixed client is
also accepted, which covers Yoshimi's generated names, but zero or multiple
matches and anything other than exactly two audio outputs are refused. An
exact direct connection already made by the managed synth is accepted;
otherwise the checked JACK API aborts or rolls back the owning route change.
Port connection may retry within the one startup deadline because some hosts
publish names just before their JACK client becomes active.
Managed synth MIDI selectors retain unique short-name matching for generated
ALSA destinations, while physical MIDI devices continue to require their
stable exact identities.

SHR's FluidSynth process uses JACK audio, ALSA sequencer MIDI, and its piped
command input; it does not open FluidSynth's TCP server. Startup loads the
configured SoundFonts once. Each planned channel receives only its effective
14-bit SoundFont bank and program, using a non-overlapping bank offset for each
configured SoundFont. The persisted route includes the configured SoundFont
identity as well as bank/program, so equal bank/program numbers in different
files remain different sounds. Identical route/channel pairs are deduplicated;
selecting one channel does not broadcast a program change to the other 15.
All FluidSynth parts still cross the same stereo JACK boundary. They share the
managed synth strip, source effects, meters, final-bus routing, and recording
path; there are no per-instrument EQ/compressor/aux strips, stems, or JACK
outputs. MIDI channel volume and pan remain ordinary channel messages where a
Project uses them.

Interactive setup also offers to mask the distribution's always-running
FluidSynth unit and blanket `amidiminder` patcher. This keeps a controller from
reaching the same synth or hardware destination through an unowned background
route while leaving all three engines available on demand.

Commands, client names, preset roots, SoundFonts, MIDI ports, and JACK ports
are configuration. The engine code does not assume the development hardware.
The three catalogs also remain separate: a synthv1 XML preset is not treated as
a Yoshimi instrument or a SoundFont program.

## Three different kinds of recording

SHR-DAW uses “record” for three intentionally different jobs:

1. An **Idea** captures free-time MIDI while playing a managed sound. It keeps
   event timing and instrument identity; synthv1 Ideas also keep a private
   preset snapshot and the mapped control values. `PLAY` plays that MIDI back
   through the restored instrument. An Idea is not audio.
2. FT2 **REC** quantizes notes into the selected Pattern page using that
   page's Manual, One-column, or Drum-auto allocator. Recording from stop loops
   the selected Pattern; punching in during Play keeps the current Arrangement
   schedule. Each note-on owns its exact Pattern/page/lane until the matching
   release, including across cursor moves, loops, and Arrangement boundaries,
   and auditions through the page's exact online software or hardware target.
3. **Audio recording** captures every armed exact JACK source on one shared
   callback timeline. It writes separate mono 24-bit stems and a session
   manifest. Its separate Levels overview compares the first 18 configured
   source levels at once. It records arriving audio, not the MIDI events that
   produced it.

Idea take playback runs independently of screen redraw. Stop, route changes,
replacement, panic, and exit release the exact notes still owned by that take.
Ideas publish into new private directories without replacing a same-named
Idea.

The audio callback copies a whole multichannel callback into one fixed ring or
rejects all of it; an ordinary worker performs every file operation. A unique
`*.take.part` session is published without replacement only after all mono WAVs
and the manifest finalize. Recognized interrupted stems recover only their
common whole-frame prefix and remain visibly incomplete; `.part` symlinks are
never followed. Overflow, xrun, source/JACK loss, callback mismatch, RIFF limit,
disk or finalization errors prevent a successful state. The recorder does not
provide audible software monitoring, so use safe hardware direct monitoring.

The Levels client and take client are mutually exclusive owners of the same
exact configured inputs. The meter callback computes bounded RMS/sample peaks
for 18 fixed slots and publishes them through atomics; it allocates, locks,
formats, and performs file I/O exactly zero times. UI smoothing, peak hold,
decay, and labels happen outside the callback. This metering neither duplicates
the final-bus route nor changes unrelated JACK connections. See [the complete
recorder contract](MULTITRACK_RECORDING.md).

## Projects, Patterns, pages, and columns

A **Project** is the complete tracker work saved as one `.shsong` file. It
contains:

- distinct **Patterns**;
- an **Arrangement** whose ordered steps reference Pattern IDs;
- each Pattern's tempo, meter, rows, pages, lanes, and cells;
- page/column MIDI routing and setup data;
- the optional private WAV-loop reference and placement; and
- the source, aux, and master effects state.

A Pattern is reusable musical data, while an Arrangement step is a place that
plays a Pattern. `REPEAT` adds another reference to the same Pattern, so later
edits affect every repeated use. `CLONE` or paste-new creates a separate
Pattern when the copies need to diverge. Cleanup deletes only Patterns with no
Arrangement reference and never silently rewrites the Arrangement.

Each Pattern owns one or more **pages**, and every page has four note
**columns**. All enabled pages play together. A page chooses one MIDI
destination: portable `AUTO`, an explicit software engine/instrument pair, the
configured external output, or an exact saved ALSA MIDI port. An `AUTO` page persists no
device/channel/bank/program route and resolves the current machine defaults at
playback. An explicit page's columns show channel 1–16 and program 1–128 while
storing their zero-based MIDI values, plus bank MSB/LSB, lane name, and mute
state. External-MIDI columns may share the same destination/channel only when
their master bank/program selections match, because MIDI program changes affect
the whole channel. Software pages take their preset from the saved route, so
their external bank/program fields do not impose that restriction and stored
setup messages cannot replace the route-owned bank/program.

This separation makes several useful routes possible in one Pattern: one page
can play its named software instrument, another can address a drum machine,
and another can play a hardware synth on a different port. FluidSynth pages
add a second axis: several saved presets may play through one owned process
when each MIDI channel has one stable preset. Repeating an identical
route/channel across pages does not create another engine part or collapse
lanes; every page still contributes four independent lanes. Different presets
on one channel are refused across the whole playback loop because note tails
and Pattern/Arrangement boundaries are not yet safe dynamic preset-change
points. A disconnected exact target is displayed as `OFFLINE` and never
substitutes another port; its name and notes stay in the Project. Destinations
are re-resolved on each play, so a returned interface is selected without
editing the Project. Ambiguous stable identities are reported and not guessed.

External MIDI device profiles optionally add bank labels and program names to
the column and cell program browsers. They remain JSON data, can be privately
overridden for writable user memories, and never remove raw channels 1–16 or
the musician-facing 1–128 numeric fallback. They describe rather than detect
downstream DIN hardware. See [MIDI device profiles](MIDI_DEVICE_PROFILES.md).

## FT2 Play, Record, and Edit modes with N00B

The FT2 screen has three explicit modes:

- **Play** navigates rows, pages, lanes, and Arrangement steps and starts
  transport from the cursor or Project beginning.
- **Record** performs quantized capture into the selected online Pattern page,
  whether its exact target is the Pattern-owned software instrument or hardware.
- **Edit** writes notes or chords from MIDI/computer-keyboard input. Blank,
  erase, and note-off are explicit operations, and the persistent 0–32-row
  advance determines the next cursor position. Its independent 1/1–1/128
  length selector writes the existing gate/note-off representation.

Every Pattern page separately persists a note-entry layout. Manual starts at
the musician's selected column. One column redirects every future note and
recorded release to its persisted C1–C4 anchor, making it intentionally
monophonic without changing that column's route. Drum auto performs a
deterministic, atomic four-lane allocation when notes are inserted. Alternating
core hits reuse a compact primary lane, simultaneous/quantized groups claim
distinct empty cells, and fills spill without overwriting the target row.
Automatic writes never move the visible lane cursor.

Drum auto classifies notes as core, long-tail, or other percussion, with an
optional choke group. General MIDI cymbals and hi-hats provide defaults;
unknown notes are ordinary short percussion, and a page may persist non-GM
overrides. Unrelated active cymbal lanes are excluded during placement.
Same-note retriggers and matching choke groups may reuse the relevant lane.
This is not a scheduler rule: once a cell is stored, the existing lane
scheduler interrupts the previous same-lane note for every sound.

**N00B is an independent filter switch, not a fourth mode.** It can remain on
through Play, Record, and Edit. The chosen root plus major or natural-minor
scale gates input on the selected melodic page: accepted notes retain their
exact pitch, rejected notes stay silent, and Record/Edit write only accepted
notes. Player shows the scale as a compact in-place rotary while the filter is
enabled. FT2 reuses that selection and toggles it without opening another
screen. Switching N00B never changes the underlying mode. It turns off on
percussion pages.

The selected Pattern page owns live audition. A software page loads its saved
engine/instrument pair; MIDI pages keep independent destination/channel/program routes;
and percussion pages use their channel and drum mapping. Route changes cancel
the old destination/channel before the new one is armed. An explicit FT2 route
is authoritative. Only a genuinely new, empty, unsaved default Project may
replace its factory page 1 route with the currently loaded Player instrument;
ownership of the already-running managed engine then moves to FT2 without a
restart. With no Player engine, that fresh Project loads the first available
synthv1 preset. Saved and otherwise changed Projects are never inferred from an
empty note grid and keep their routes.

Cell Edit is transactional: changes are made to a draft, `CONFIRM` publishes
the whole cell, and `EXIT` restores the original. A cell can hold a note or
note-off, inherited/explicit velocity and gate, a per-note program override,
and one command: cut, delay, retrigger, tempo, or none. Program audition uses
the selected page destination and column channel without inserting a note or
duplicating generic live thru.

Pattern Setup initially offers musically convenient meter-specific shapes:
4/4 Patterns of 8/16/32/64/128 rows and corresponding 3/4 Patterns of
6/12/24/48/96 rows.
`CONFIRM` performs NEW or CLEAR with the newly selected shape. `KEEP` performs
the same requested operation while retaining the current Pattern's shape:
NEW creates a blank Pattern with that meter/length, and CLEAR removes content
without reshaping. The interactive length chooser exposes every size from 1 through
32 rows, plus 48, 64, 96, 128, 192, and 256 rows. Groove timing remains planned
work rather than a current menu promise.

The reusable drum library contains 72 authored four-lane starting points in ten
creative genre groups. Filters choose 3/4 or 4/4 and a 2/4/8-bar phrase. Loading
changes the first percussion page's cells without replacing its MIDI target,
channels, bank, program, tempo, or Arrangement. User saves are separate
`.shdrum` files; bundled patterns are read-only. Melody-only transpose leaves
percussion pages and note-offs unchanged and refuses the whole edit if a note
would leave MIDI range.

## The managed audio graph

Without the owned graph, the managed instrument and owned loop use their exact
configured direct playback routes. With `audio.graph.enabled=true`, those two
sources and one exact configured stereo JACK capture pair move transactionally
into this route:

```text
managed instrument -> SOURCE inserts + AUX returns --+
owned WAV loop ---------------------------------------+-> stereo sum
configured capture L/R -------------------------------+
 -> MASTER rack -> master level -> linked limiter -> FINAL OUT
 -> final stereo WAV tap -> configured playback L/R
```

There are four useful placement ideas:

- A **source insert** processes the instrument in series. It is the normal
  place for tone shaping, dynamics, distortion, or an effect that belongs to
  this sound.
- An **aux send** makes a parallel copy. `PRE` takes it before source inserts;
  `POST` takes it after them. Each of AUX 1 and AUX 2 has its own send, rack,
  return gain, and meter.
- An **aux return** brings only the effected copy back into the sum. The normal
  aux editor offers Delay, Reverb, Chorus, Flanger, and Phaser and forces them
  to 100% effect/0% dry so the original instrument is not accidentally doubled.
- The **master rack** processes the complete source-plus-returns sum. It is the
  place for final corrective EQ, bus compression, overall utility changes, or
  deliberate whole-mix coloration.

Send and return levels run from -60 to +12 dB. A new aux starts with a
conservative -18 dB post-insert send. The compact controls use 3 dB steps;
sends below -60 dB show `OFF`. Each serial rack holds at most eight processors,
the complete graph at most 16, and no more than two reverbs. These limits are
rejections, not silent truncation.

### Effect possibilities

Source and master racks can use all 13 effect types:

- **Utility** trims level, pans, changes stereo width, inverts either channel,
  or mutes. It is useful for gain staging and stereo correction rather than a
  flashy sound.
- **EQ** provides a low cut, low/high shelves, two broad mid bands, and output
  trim. Use it to remove rumble, reduce boxiness or harshness, or emphasize the
  part of a sound that should speak.
- **Compressor** controls peaks and movement with threshold, ratio, knee,
  attack, release, makeup, parallel mix, and sidechain high-pass. Fast attack
  restrains transients; slower attack lets the front of a note through.
- **Distortion** offers soft cubic, hard clip, and asymmetric modes plus drive,
  bias, tone, output, and mix. They range from rounded saturation to an
  intentionally sharp edge; output trim is important for fair comparison.
- **Gate** reduces sound below a threshold with hysteresis, depth, attack,
  hold, and release. It can clean gaps or deliberately shorten a noisy/long
  texture, but aggressive settings can cut note tails.
- **Filter** is a resonant low-pass, band-pass, or high-pass with drive and
  mix. It can darken, thin, isolate a moving band, or add a resonant sweep.
- **Crusher** reduces bit depth and sample-hold rate, with optional dither and
  parallel mix, for stepped digital texture.
- **Delay** supports stereo, ping-pong, and mono-to-stereo echoes, free time or
  tempo divisions, feedback, stereo ratio, tone, wet/dry mix, and optional
  tail-on-bypass.
- **Reverb** offers room, plate, and hall voicings with predelay, decay, size,
  damping, input low cut, width, and wet/dry balance.
- **Chorus** uses a short modulated delay to add width and gentle pitch motion;
  rate, depth, stereo phase, feedback, mix, and dry level shape the result.
- **Flanger** uses a much shorter modulated delay and signed feedback for
  moving comb-filter sweeps, from subtle motion to metallic resonance.
- **Phaser** uses four or six stable all-pass stages with rate, center, range,
  feedback, stereo phase, and mix for a smoother notched sweep.
- **Tremolo/Pan** changes level or stereo position with sine, triangle, or
  smoothed-square motion, plus rate, depth, stereo phase, and output trim.

Exact names, defaults, physical ranges, and delay divisions are centralized in
[the effect schema table](AUDIO_GRAPH.md#effect-parameter-schemas). The rack UI
uses those schemas rather than a different hidden set of values.

### Bypass, tails, meters, and publication

Source/master bypass fades toward the dry signal rather than switching on one
sample. An aux cannot use that same fallback because raw send audio on a return
would double the source. If every wet generator on an aux is bypassed, its
return fades to silence. A delay with tail-on-bypass may stop accepting new
input while its already-created wet echoes drain; serial conditioning can
continue to pass an already-wet signal or feed another active wet generator.

Every processor publishes bounded input/output peak and RMS plus clip and
non-finite state. Compressor editing also exposes its detector-derived gain
reduction through a lock-free value; the LED display responds immediately to
increasing reduction and uses a fixed 250 ms release for visual stability.
Bypass publishes zero reduction. Each aux meters after its return gain. `FINAL
OUT` follows the master level and dedicated stereo-linked sample-peak limiter.
The recorder tap and playback receive the same final buffer after that meter
boundary.

The FX rack and parameter editor remain available while the graph is disabled,
so a Project can be designed silently without an audio callback to rebuild.
When the graph is enabled, every FX change that would publish a replacement
runtime plan requires stopped transport and no active recording. The complete
plan, coefficients, buffers, ports, and memory are prepared and validated away
from the real-time callback. Stable instance IDs let compatible effects retain
DSP state when moved. The callback uses fixed memory and atomics: no file
access, subprocess, logging, allocation, or locks.

The graph remains opt-in and disabled by default. The managed engine and loop
are connected directly first. The graph is activated muted, all three exact
input pairs plus its playback boundary are connected, and the four synth/loop
direct links are removed as one rollback-capable transaction before graph
output is published at a block boundary. Validation, activation, or connection
failure leaves or restores the exact prior direct links. Shutdown deactivates
the callback before restoring them, avoiding a doubled final block.

FX state is saved in the Project while the graph is disabled, but direct
playback cannot process or meter it. The graph instantiates exactly three
source kinds: managed instrument, owned loop player, and one stereo live-input
pair. It deliberately has no general strips, pan, solo, hardware insert,
per-input effect chain, or arbitrary wiring.

## Live Patterns, Loop Mix, and the final bus

Live Patterns is a sequencer-owned performance view over existing Pattern
records. Browsing is UI state only. A successful activation is scheduled at a
Pattern/bar boundary, validated against its exact targets, and published to
capture only after it actually starts. Quantized transitions retain held lanes
whose destination/channel/note remains valid; changed owners release before
new events. A different managed instrument is prepared at the boundary after
old notes release, never layered in advance.

Transient lane mute/velocity/gate/transpose shapes a runtime Pattern copy.
Stored cells and persisted lane settings remain unchanged. The state belongs
to the open Project and is dropped on Project replacement.

Every Pattern owns four optional references to privately imported mono or stereo
WAVs. This keeps its MIDI pages and Loop Mix material together without making
decoded audio into MIDI lanes. Repeated Arrangement steps share the Pattern
record; a clone copies references/settings into an independent Pattern but
never copies the WAV. The import inbox is only a browser source; each selected
file is validated and copied without replacement below the private SHR data
directory. Every populated slot saves filename, interpreted source BPM,
half/normal/double mode, non-destructive cut, whole-bar placement offset, level,
and bipolar filter.

Loop analysis is offline, outside the JACK callback. `AUTO` uses bounded pulse
and duration analysis when useful and proposes a whole-bar interpretation.
Playback remains native-speed and requires every active slot's interpreted BPM
to equal its Pattern tempo and its sample rate to match JACK. Incompatible
slots are refused rather than drifting. Each decoded WAV is bounded to
6,000,000 frames, about 125 seconds at 48 kHz.

The four fixed renderers share one owned JACK client and sum internally after
region/phase, smoothed level, and the neutral/low-pass/high-pass filter. The
callback allocates and locks nothing. In direct mode its one output pair
connects to playback. An active performance-bus transaction moves that exact
pair into the sum and removes the direct links, so there is never a parallel
doubled path. `LOOP OUT` means the complete four-slot Loop source; `FINAL OUT`
includes all three logical bus sources.

Only the active and incoming Pattern are prepared; saved Patterns add no JACK
clients, callback renderers, mixer sources, or eager decoding. Each Pattern
activation invalidates old runtime queues and publishes one fixed four-slot
renderer set through a single bounded atomic handoff. The callback swaps only
pointers; the owner thread reclaims the retired set. Outgoing audio becomes
silent at the MIDI boundary even if incoming preparation is late. Healthy
prepared slots start at the same boundary; failed ones remain silent and
faulted.

The slots share the owning Pattern's local transport origin, meter, tempo, and
bar scheduler but launch/stop independently. Every Arrangement step and Live
retrigger restarts local phase. Starting at a middle row seeks from that local
beat; preceding Arrangement steps never contribute phase. Different whole-bar
lengths retain phase. The summed Loop source
receives only its final-bus level/mute, then shares the
master, limiter, final meter, recorder, and playback with the other sources.
`REMOVE` detaches only the selected Pattern slot while keeping the private WAV.
`LIBRARY` opens the shared overlay for that slot and browses inbox and private
files without auto-preview.
Controller PLAY explicitly previews the selection. Changing selection, STOP,
Back, closing/leaving the browser, or leaving Loop Mix stops preview.
Activating an inbox entry imports and loads it; activating a
private/current/saved entry attaches and loads it. Failed preview/import keeps
the caller and selection for retry, and import failure rolls back its private
copy and Project attachment. It does not delete existing library files. The
screen shows only active, queued, muted, missing, and fault states. One bad slot
does not stop healthy slots. See [Live performance](LIVE_PERFORMANCE.md).

## Note ownership and failure behavior

MIDI notes are owned by physical destination, software route, channel, note,
column/lane, and playback source. Owners are global across Pattern pages, not
reset per four-lane page. Two lanes on different pages may therefore hold the
same note on the same destination/channel; SHR-DAW sends note-off only after
the last owner releases it. Scheduled notes and live audition use the same
ledger, so an audition release cannot cut a matching tracker note. Stop,
page/lane mute, route change, Project replacement, Idea/take stop, recorder
stop, panic, output failure, and exit deduplicate cleanup by the physical note
while retaining the all-channel engine panic. This prevents one page, lane, or
screen from cutting off another shared note.

Realtime FT2 capture adds a bounded input-owner ledger above that unchanged
playback ledger. Each note-on remembers its exact Pattern, page, lane, start
row, and generation. A note-off closes only the current generation for that
lane; an older overwritten One-column note cannot close the newer note, and
repeated identical owners require their final release. Pattern wrap and
Arrangement transitions write the release into the same global page/lane
identity. Stop, mute, panic, route/output failure, Project replacement, and
exit discard every capture owner and run the normal destination cleanup.

Missing JACK leaves browsing and external-MIDI sequencing usable. A missing
controller leaves the computer keyboard active. Audio resolves preferred,
ordered internal, then final headphone routes in memory. A missing external
MIDI target remains offline without rewriting or falling through to another
port. Ambiguous ports are reported and refused. Missing optional engines or sound banks remain visible with
an explanation. A failed graph returns to direct playback. None of those
failures authorizes SHR-DAW to rewire unrelated clients or terminate processes
it does not own.

## Project and private-data safety

Project format 8 persists the complete tracker state, exactly four optional
Loop Mix slots under each Pattern,
effects routing, per-page entry mode/anchor, drum-role/choke overrides,
explicit software engine/instrument identities, and optional external profile
metadata. Format 7's former Project-global four slots migrate in memory into
every distinct Pattern. Format 6's single WAV record migrates to slot 1 of
every Pattern. Neither migration copies audio or rewrites the file; only an
explicit save writes format 8. Format 5
and older ordinary pages gain Manual/C1 entry defaults in memory; explicitly
marked percussion pages retain their prior automatic drum entry. Format 3
remains loadable and keeps its
device/channel routes explicit.
Formats 0 and 1 migrate with empty effects; format 2 retains its source rack and
gains empty aux/master routing. Unknown newer formats, fields, malformed rack
data, unsafe paths, and over-limit structures are refused rather than partly
loaded and then written back.

Normal Project save asks again before replacing an existing file. `SAVE AS`
chooses a numbered non-overwriting copy. Rename publishes the complete new
Project before removing the old filename and refuses collisions. New Ideas,
audio recordings, imported loops, and user drum patterns likewise choose or
require unused destinations. Destructive deletion is explicit and scoped:
Pattern cleanup checks zero Arrangement references, and Pattern loop removal
keeps the WAV. The current loop browser has no file-deletion workflow.

Configuration lives below
`${XDG_STATE_HOME:-~/.local/state}/shsynth/`; private user data normally lives
below `${XDG_DATA_HOME:-~/.local/share}/shsynth/`. A repository-local launch
redirects both into ignored `user/`. Important private data includes Ideas,
Projects, recordings, imported loops, user drum patterns, learned controller
configuration, profile overrides, and uncleared presets. Public packaging uses
only the 21-presets allowlist, the authored drum data, and files named by the
cleared demo manifest. See
[Licensing and redistribution](../THIRD_PARTY.md).

## Performance information and honest limits

With the graph disabled, MTR retains its CPU and legacy managed-source display.
With the graph enabled, it shows the three source readiness/level/mute states,
master level, post-limiter final stereo meter and clip state, limiter gain
reduction, and final-recording status. Direct mode reports final-bus metering
unavailable instead of creating a hidden tap or displaying unrelated audio.

Maintainer checkpoints separately collect callback count, mean, p95, p99,
maximum, deadline misses, oversized blocks, xruns, process/core CPU, memory,
and shutdown behavior. The earlier one-source graph passed its recorded
Raspberry Pi engineering checkpoints. The three-source final bus has separate
hardware-free stress evidence; full-duplex interface acceptance remains a
future hardware test and is not implied by synthetic validation.
