# How SHR-DAW works

SHR-DAW is a small music workstation built from several deliberately separate
parts: role-separated controller/performance inputs, one managed software
engine, in-process SHR Drums, a FastTracker II (FT2)-style MIDI sequencer, a
private WAV loop player, a synchronized raw multitrack recorder, and an
optional owned final performance bus. This guide connects those parts and
explains what the musician can do with them.
For exact configuration keys use
[Configuration and routing](CONFIGURATION.md); for the DSP and real-time
contract use [Audio graph and DSP contract](AUDIO_GRAPH.md).

In architecture pages, **owned** means started or created by SHR-DAW and
therefore safe for it to change or stop. An **exact route** is a saved endpoint
that is never guessed. To **publish** a graph plan means making a validated,
bounded plan active for the audio callback.

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
screen, menus, relative controls                managed synth <-+  |
                                                or FT2 page -------+

FT2 scheduler -> each page's MIDI destination -> software or hardware instrument

managed synth audio -> direct JACK playback (graph disabled)
SHR Drums audio ----> direct JACK playback (graph disabled)
private WAV loop  --> direct JACK playback (graph disabled)

managed synth SOURCE/AUX --+
private WAV loop -----------+-> MASTER rack -> live fader -> MASTER STRIP
SHR Drums ------------------+
configured input 1/2 -------+-> stereo or dual-mono pan -> FINAL OUT + final WAV + playback
configured JACK sources -> fixed 18-channel meter snapshot
                        \-> shared callback timeline -> mono stems + manifest
```

The raw-stem recorder remains separate. The application owns the final bus
independently of its optional synth, SHR Drums, and Loop sources. Input MON ON
can therefore activate only the exact two-port Input and playback pair; it
does not launch missing sound sources. Present optional direct routes move into
the bus transactionally and reconcile when sources disappear or return.
`FINAL OUT`, final WAV, and playback then share the same post-strip samples.
They do not secretly include unrelated JACK clients or downstream interface
processing.

## Controller and performance input roles

SHR-DAW opens each exact ALSA source at most once, then classifies messages by
the configured role before they reach an instrument:

- menu buttons, the main encoder, encoder press, and the 15 relative
  performance rotaries stay inside SHR-DAW; the loaded backend decides whether
  those positions control synthesis or Project aux sends;
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
for commands. The current learned Shift CC9 is not an SHR pad-lock control;
the earlier compatible DAW-mode Shift is CC27.

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
empty Pattern may run specifically for live arpeggiation. Playback PLAY also
starts the controller without requiring a recorded take; RECORD starts it for
capture, STOP ends it without unloading the sound, and TAP changes tempo but
does not imply Start. Every SHR play is a fresh launch (`FA`), not a resume;
there is no `FB` Continue or `F2` Song Position Pointer because SHR has no
pause/resume transport state. Stop and an active clean shutdown each produce
one `FC` as appropriate, while `F8` keeps the stopped controller ready at the
current tempo until SHR exits.

A controller profile describes what a physical device sends. The setup wizard
can apply a reviewed profile or learn direction-only rotary messages,
encoder direction, CC/note buttons, and an encoder press without forwarding
the learning messages to a synth. Learned mappings remain private; reviewed
catalog updates are validated and published atomically. See
[Controller profiles](CONTROLLER_PROFILES.md).

The synth controls use signed relative steps. After a preset or Idea loads, or
after `RESET`, turns continue from the stored value, so stale hardware position
cannot make the sound jump. Playback indicators compare each value with the original preset: green
is more than 0.03 below it, bright yellow is within 0.03, and red is more than
0.03 above it. Reset changes only those mapped parameters and
does not restart the engine.

Held notes drive the Playback note/chord display and its continuous keyboard
strip. Each pitch also has its current MIDI Note On velocity shown directly
beneath its name. Note Off removes only that channel's instance; if multiple
channels hold one pitch, the display deterministically uses the highest still
held velocity. German B/H spelling is the default;
`display.note_names=english` uses A#/B spelling. Naming changes only the
display, never the MIDI notes.

## Software instruments and ownership

The musician-facing behavior of the complete SHR-DAW instrument system is
collected in
[SHR-DAW instruments and drums](INSTRUMENTS_AND_DRUMS.md). This section owns the
lower-level process and route boundaries.

The whole-system installation presents five melodic instrument families
through one SHR-DAW workflow. Their runtime hosts are:

- [synthv1](https://synthv1.sourceforge.io/) for subtractive synth presets;
- [Yoshimi](https://yoshimi.github.io/) for `.xiz` instruments and banks;
- [FluidSynth](https://www.fluidsynth.org/) for `.sf2` and `.sf3` SoundFonts;
- Moj Sint for strict `.mojsint` Model D, Six-Op PM, Strange Oscillator, Swarm Machine, Bass Matrix, and Dual Filter presets; and
- SHR Sampler for strict preloaded `.shrinst` sample packages.

Only one SHR-managed software engine process runs at a time. synthv1 and
Yoshimi retain one current preset. FluidSynth is the exception at the
instrument level, not the process level: its one owned process may hold several
SoundFont presets on compatible MIDI channels while producing the same one
stereo source. Loading another standalone sound may reuse or replace the owned
process; replacement sends All Notes Off, performs a clean shutdown, and
starts the next configured host. SHR-DAW records enough process identity to
stop only the engine it started. It neither layers managed backends nor kills
an unrelated synthv1, Yoshimi, FluidSynth, Moj Sint, or SHR Sampler process
opened by the user.

Moj Sint is started with `--client-name` and `--preset`, publishes exactly
`out_l`/`out_r`, accepts timbre/ADSR on its established CCs, and accepts shared
instrument volume on CC 7. SHR verifies the configured port names and owns only
the child it started. The browser never launches it; LOAD is the transaction
boundary.

SHR Sampler is preflighted with its machine-readable version and strict
offline package validator before the current engine is disturbed. It is then
started with `--client-name` and `--instrument`, publishes exactly the two
configured outputs, and exposes one configured ALSA input. It does not connect
itself. A missing executable/package, malformed package, incompatible version,
validation timeout, startup failure, or unexpected exit becomes a visible
managed-backend fault. Failed replacement gets one attempt to restore the
previous owned session; no second melodic child remains layered.

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
The fresh FT2 Drums page resolves the General MIDI drum preset from that
discovered metadata, stores its explicit FluidSynth identity, and gives all
four columns zero-based channel 9. Live input and transport prepare that exact
bank/program on channel 10 before sending `0x99` note-ons. A missing or failed
selection stays explicit, offline, and silent instead of falling through to a
previous part, Player sound, channel 1, or external MIDI. Saved Project routes
and channels are never replaced by this fresh-Project default.
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
The five catalogs also remain separate: synthv1 XML, Yoshimi instruments,
SoundFont programs, `.mojsint` files, and `.shrinst` packages never borrow one
another's parsers or controls.

## Maintained component repositories

Three maintained repositories supply SHR-DAW components. Their boundaries are
part of the installation and recovery contract.

| Component | Runtime boundary | Component ownership | SHR-DAW ownership |
| --- | --- | --- | --- |
| [SHR Drums](https://github.com/PaolaShultz/shr-drums) | Rust library compiled into `shr`; there is no drum child process | Format 1 `.shrkit` validation, bounded sample decode, voice rendering, and the offline `shr-kit` compiler | Pattern timing, MIDI note dispatch, JACK publication, effects, kit selection, and public kit allowlists |
| [Moj Sint](https://github.com/PaolaShultz/moj-sint) | One managed `moj-sint` process with an ALSA input and stereo JACK output | Preset schema, synthesis models, MIDI controls, audio rendering, and the factory preset manifest | Exact command/preset configuration, process identity and shutdown, route connection, replacement rollback, private saves, and Project state |
| [SHR Sampler](https://github.com/PaolaShultz/shr-sampler) | One managed `shr-sampler` process with an ALSA input and stereo JACK output | Format 1 package parsing, integrity checks, decoded samples, voice rendering, live host, and the cleared factory package | Version/package preflight, exact command/instrument configuration, process identity and shutdown, route connection, replacement rollback, and Project state |

The current machine-readable pins are exact commits: SHR Drums
`0199297b3efd160a67e3f47df64a6bf418c20df2`, Moj Sint
`693ad165271ae04bc2da6746642b87af1875b553`, and SHR Sampler
`9f2115f5fcc25d6ffa82a7106ee069cad47ce592`. `Cargo.toml` owns the SHR Drums
dependency; `install/compatibility.json` owns installer revisions and accepted
runtime ranges. The Moj Sint pin contains the 16-start catalog through Bass
Matrix. SHR-DAW source can also host schema 8 and Dual Filter, but the installer
will not provide those five newer starts until its compatibility pin changes.

Each component refuses malformed or unsupported owned data before replacing a
working session. SHR-DAW isolates a drum-kit failure to that source and keeps
healthy sources running. For external instruments, it attempts one restoration
of the previous owned session after replacement fails and never stops a
matching process it did not start.

Public installation copies only files named by the relevant cleared manifest.
Private presets, kits, samples, packages, renders, Projects, and recordings stay
outside all four repositories. Developers should read the
[SHR Drums package format](https://github.com/PaolaShultz/shr-drums/blob/main/FORMAT.md),
[Moj Sint documentation index](https://github.com/PaolaShultz/moj-sint/blob/main/docs/README.md),
and [SHR Sampler host architecture](https://github.com/PaolaShultz/shr-sampler/blob/main/docs/HOST_ARCHITECTURE.md).
Musicians should start with
[SHR-DAW instruments and drums](INSTRUMENTS_AND_DRUMS.md), which describes the
shared load, play, save, and recovery workflow.

## Three different kinds of recording

SHR-DAW uses “record” for three intentionally different jobs:

1. An **Idea** captures free-time MIDI while playing a managed sound. It keeps
   event timing and instrument identity; synthv1 and Moj Sint Ideas also keep
   a private preset snapshot and backend-specific mapped control values, while
   SHR Sampler Ideas keep only the stable package ID and configured public path
   without copying sample content. `PLAY`
   plays that MIDI back
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
replacement, panic, and application termination release the exact notes still owned by that take.
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

Home, Player, FT2, Effects, and Performance are views of one live in-memory
session. Navigation preserves Project data and dirty state, Pattern history,
FT2 context, loaded engine and pickup, audio routes, and running transport or
recording. FT2 Exit is navigation, including for zero-note setup Projects;
it never restores the clean baseline. Saving persists that session across
termination. Quit and explicit Project replacement guard all dirty work.
STOP ends its musical transport/take; PANIC cleans up notes without releasing
engine or final-bus ownership. Full shutdown remains exclusive to termination
and explicit incompatible replacement.

A **Project** is the complete tracker work saved as one `.shsong` file. It
contains:

- distinct **Patterns**;
- an **Arrangement** whose ordered steps reference Pattern IDs;
- each Pattern's tempo, meter, rows, pages, lanes, and cells;
- page/column MIDI routing and setup data;
- the optional private WAV-loop reference and placement; and
- the source, aux, and master effects state.

The current saved hierarchy stops at Project. SHR loads one Project at a time,
and the FT2 `SONG` control navigates that Project's Arrangement steps. There is
no Playlist object above it. The proposed higher layer is documented in
[Playlist above Song](FUTURE_IMPROVEMENTS.md#playlist-above-song); it adds no
current screen, file, or runtime behavior. Patterns already own tracker Pages,
and the missing whole-Page edit operations are recorded separately under
[Future Page operations](FUTURE_IMPROVEMENTS.md#future-page-operations).

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
For software pages, “online” additionally means the managed engine has the
exact saved route selected on that page's runtime channel; resolving a label
alone is not readiness.

External MIDI device profiles optionally add bank labels and program names to
the column and cell program browsers. They remain JSON data, can be privately
overridden for writable user memories, and never remove raw channels 1–16 or
the musician-facing 1–128 numeric fallback. They describe rather than detect
downstream DIN hardware. See [MIDI device profiles](MIDI_DEVICE_PROFILES.md).

## FT2 ownership

Play, Record, and Edit are separate modes over the same Pattern data. Each
Pattern page owns four lanes, its route, entry layout, automatic Note Off
choice, and live-audition destination. Record quantizes performance input.
Edit writes deliberate cells with independent note length and row advance.
N00B is a scale filter layered over those modes, not another mode.

Manual, One column, and Drum auto change only future entry. They do not rewrite
existing cells or move the visible cursor. Cell Edit remains transactional:
Confirm publishes the complete cell and Exit restores the original.
Probability and loop-aware conditions are Cell data. The scheduler evaluates
conditions before a deterministic percentage gate. Normal FT2 rebuilds the
event plan at its selected Arrangement playback-span boundary; Live Patterns
rebuild at their Pattern boundary. Route/engine preflight includes every
conditional trigger, while context-free MIDI export uses pass 1 with Fill off.

An explicit page route is authoritative. A genuinely new, empty, unsaved
Project may adopt the current Player instrument for page 1 without restarting
it; saved or otherwise changed Projects keep their own routes. Route changes
release the old destination before the new one is armed.

Pattern setup, drum-library loading, transpose, and Arrangement operations
change Project records without changing private source libraries. The exact
musician workflow and field behavior belong to the [Tracker
guide](TRACKER.md). The [Project storage sections](CONFIGURATION.md#tracker-pages)
define what is persisted.

## The managed audio graph

Without the owned graph, the managed instrument, SHR Drums, and owned loop use
their exact configured direct playback routes. `audio.graph.enabled=true`
starts the bus automatically; MTR Input MON ON can start it explicitly with
only one exact configured stereo JACK capture pair and the playback pair.
Whichever optional sources are present move transactionally into this route:

```text
managed instrument -> SOURCE inserts + AUX returns --+
owned WAV loop ---------------------------------------+-> stereo sum
SHR Drums --------------------------------------------+
configured capture L/R -------------------------------+
 -> MASTER rack -> live master level
 -> fixed INPUT/TONE/GLUE/COLOR/IMAGE/LOUD strip -> FINAL OUT
 -> final stereo WAV tap -> configured playback L/R
```

There are four useful placement ideas:

- A **source insert** processes the instrument in series. It is the normal
  place for tone shaping, dynamics, distortion, or an effect that belongs to
  this sound.
- An **aux send** makes a parallel copy. `PRE` takes it before source inserts;
  `POST` takes it after them. Each of AUX 1, AUX 2, and AUX 3 has its own send, rack,
  return gain, and meter.
- An **aux return** brings only the effected copy back into the sum. The normal
  aux editor offers Delay, Reverb, Chorus, Flanger, and Phaser and forces them
  to 100% effect/0% dry so the original instrument is not accidentally doubled.
- The **master rack** processes the complete source-plus-returns sum. It is the
  place for final corrective EQ, bus compression, overall utility changes, or
  deliberate whole-mix coloration.
- The fixed **MASTER STRIP** follows the live master level. It provides saved
  mastering gain/cleanup, broad tone, linked full-band glue, declared harmonic
  colour, conservative image width, and protected true-peak output in one
  non-reorderable order. It is Project-global rather than Pattern-owned.

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
  damping, input low cut, width, and wet/dry balance. Pre-delay is independent
  of decay; the diffuse FDN has no single room/echo repeat.
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

Internal drums own one fixed Reverb-then-Delay Project rack before their direct
or graph output boundary. Bypassing the two slots exposes `OFF`, `REVERB`,
`REVERB + DELAY`, and `DELAY` without changing routing. Tracker Stop drains
this rack naturally; Panic, Project replacement, route-host replacement, and
shutdown clear it.

Every processor publishes bounded input/output peak and RMS plus clip and
non-finite state. Compressor editing also exposes its detector-derived gain
reduction through a lock-free value; the LED display responds immediately to
increasing reduction and uses a fixed 250 ms release for visual stability.
Bypass publishes zero reduction. Each aux meters after its return gain. `FINAL
OUT` follows the fixed strip and its stereo-linked 8× true-peak limiter. It
distinguishes sample peak, dBTP, GLUE/limiter reduction, correlation, and
LUFS-M/S/I. The recorder tap and playback receive the same final buffer after
that meter boundary. Exact strip controls and latency are in
[Fixed stereo MASTER STRIP](MASTER_STRIP_MEASUREMENT.md).

The FX rack and parameter editor remain available while the graph is disabled,
so a Project can be designed silently without an audio callback to rebuild.
When the graph is enabled, every FX change that would publish a replacement
runtime plan requires stopped transport and no active recording. The complete
plan, coefficients, buffers, ports, and memory are prepared and validated away
from the real-time callback. Stable instance IDs let compatible effects retain
DSP state when moved. The callback uses fixed memory and atomics: no file
access, subprocess, logging, allocation, or locks.

MASTER STRIP values and section bypasses are different from rack structure:
they are smoothed atomic updates and may be auditioned during playback. They
are rejected during a final recording. Whole-strip comparison keeps the same
delay and true-peak protection, and never overwrites the edited values.

The graph remains opt-in and disabled by default. The managed engine, internal
drums when active, and loop are connected directly first. The graph is
activated muted, its four stereo inputs plus playback boundary are connected,
and the owned direct links are removed as one rollback-capable transaction before graph
output is published at a block boundary. Validation, activation, or connection
failure leaves or restores the exact prior direct links. Shutdown deactivates
the callback before restoring them, avoiding a doubled final block.

FX state is saved in the Project while the graph is disabled, but direct
playback cannot process or meter it. The graph instantiates exactly four
source kinds: managed instrument, SHR Drums, owned loop player, and one
two-port live Input. That Input can preserve stereo or independently pan its
two ports in dual mono. The graph deliberately has no general strips, pan for
other sources, solo, hardware insert, per-input effect chain, or arbitrary
wiring.

## Live Patterns, Loop Mix, and the final bus

Live Patterns is a sequencer-owned performance view over existing Pattern
records. Browsing is UI state. Successful activations occur at validated
Pattern or bar boundaries. Temporary lane shaping changes a runtime copy and
is dropped when the Project is replaced.

Each Pattern owns four optional references to private WAVs. The fixed renderers
share one owned JACK client and sum to one logical Loop source. Only the active
and incoming Pattern are prepared. The callback publishes a fixed renderer set
without allocation, locks, decoding, or file access. A failed slot stays silent
without stopping healthy slots or MIDI.

Arrangement steps and Live retriggers restart Pattern-local phase for MIDI and
loops together. In direct mode the Loop output connects to playback. An active
final-bus transaction moves that same output into the sum and removes the
direct links, so the path is never doubled. Removing a slot detaches its
Project reference and keeps the private WAV.

See [Live performance](LIVE_PERFORMANCE.md) for launch, preview, capture, and
boundary controls. See [Audio graph and DSP contract](AUDIO_GRAPH.md) for the
callback and routing limits.

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

Project format 18 persists the complete tracker state, integer-hundredths
Pattern/command tempos, exactly four optional
Loop Mix slots under each Pattern,
effects routing including the internal-drum rack, one Project-global fixed
MASTER STRIP, per-page entry
mode/anchor, automatic Note Off choice, drum-role/choke overrides,
explicit software engine/instrument identities, optional external profile
metadata, Project tonic/mode, selected drum kit, drum tuning, and bounded
Pattern-owned sparse automation lanes, Pattern swing, independent signed
1/96-row cell timing, deterministic probability/condition per note trigger,
and independent lane cycle length, rate, and direction.
Format 15 and older Projects gain 100%/ALWAYS trigger defaults. Format 14 and older Projects gain straight/on-grid rhythm
defaults; Format 13 and older Projects gain empty automation in memory. Format 7's
former Project-global four slots migrate in memory into
every distinct Pattern. Format 6's single WAV record migrates to slot 1 of
every Pattern. Formats 0–8 gain a neutral strip in memory. No migration copies
audio or rewrites the file. Formats 0–16 gain FULL/1X/FORWARD lane playback.
Format 18 expands the bounded aux inventory to three without changing any
older saved route; only an explicit save writes format 18. Format 12
keeps its routing and gains safe family drum-effect defaults in memory. Format 10
infers the Note Off choice from the percussion flag. Format 5
and older ordinary pages gain Manual/C1 entry defaults in memory; explicitly
marked percussion pages retain their prior automatic drum entry. Format 3
remains loadable and keeps its
device/channel routes explicit.
Formats 0 and 1 migrate with empty effects; format 2 retains its source rack and
gains empty aux/master routing. Unknown newer formats, fields, malformed rack
data, unsafe paths, and over-limit structures are refused rather than partly
loaded and then written back.

Opening the automation editor does not create a lane. Lane creation and
confirmed clearing are explicit, populated lanes cannot lose points through
target browsing, and capture is disarmed before an Arrangement boundary could
redirect a lane index into another Pattern. Effect removal and confirmed type
replacement atomically discard only the automation lanes whose exact
rack/effect identity can no longer resolve; cancelling the type change keeps
both the effect and its automation.

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
only cleared preset and component manifests, authored drum data, and files
named by the cleared demo manifest. See
[Licensing and redistribution](../THIRD_PARTY.md).

## Performance information and honest limits

With the graph inactive, MTR retains its CPU and legacy managed-source display.
With the graph active, it shows four source readiness/level states, MUTE for
Synth/Loop/Drums, the one MON ON/MON OFF action for Input, master level,
post-strip sample/true-peak and loudness state, linked gain reduction,
correlation, and final-recording status. Direct mode reports final-bus metering
unavailable instead of creating a hidden tap or displaying unrelated audio.

Maintainer checkpoints separately collect callback count, mean, p95, p99,
maximum, deadline misses, oversized blocks, xruns, process/core CPU, memory,
and shutdown behavior. The earlier one-source graph passed its recorded
Raspberry Pi engineering checkpoints. The fixed four-node final bus has separate
hardware-free stress evidence; full-duplex interface acceptance remains a
future hardware test and is not implied by synthetic validation.
