# Future improvements

Started: 2026-07-18

Status: unscheduled proposal archive

This file records useful extensions that are deliberately not part of the
current behavior. They are not required for separate FT2 pages to sequence
multiple hardware instruments simultaneously.

These entries do not enter active work automatically. The
[experimental roadmap](EXPERIMENTAL_ROADMAP.md) owns development order; an
item here becomes a current requirement only when the owner explicitly moves
it there.
Use the musician guides and architecture documents in [the documentation
index](README.md) for current behavior. This page is a proposal archive.

## Six-button touch performance console

Provide an optional touchscreen performance surface for situations where the
musician wants to play live while SHR handles accompaniment, recording, or
mixing and the MIDI controller should remain available for the instrument.
The surface is a master overlay over the current TUI: opening it preserves the
underlying screen and Project context, and closing it returns to that exact
place.

The interaction is built around six large, reachable buttons. Three positions
remain stable—`BACK`, literal `STOP`, and `MORE`—while the other three expose
the most important actions for the current procedure. `MORE` moves between
small action pages without changing the underlying procedure. Each button
shows its current action and immediate state clearly enough for stage use;
advanced preparation, routing, naming, and detailed editing remain in the
ordinary TUI.

Candidate procedures include:

- start, pause, resume, and stop accompaniment while playing live;
- launch and capture Live Patterns or control Pattern-owned Loop Mix;
- arm, start, stop, and review synchronized multitrack recording;
- run soundcheck and inspect levels before a performance;
- control the final mix, recording, and a small set of performance-safe
  effects; and
- recover directly from a missing source, stopped owner, or recording fault
  without losing the current Project context.

The six positions are a physical interaction contract, not six fixed global
commands. A procedure may change the three contextual labels, but stopping and
leaving must remain predictable. The design still needs touchscreen musician
testing for reach, accidental activation, feedback visibility, interruption,
and recovery while the other hand is occupied by an instrument.

## Future smart musical assistance — unscheduled

Smart assistance should remove repetitive musical work while leaving the
musician in control of the musical decision.

Possible helpers include:

- optional key or harmony inference with visible uncertainty;
- chord-progression suggestions and user-confirmed progression generation;
- arpeggiated fills and filler parts that follow the selected harmony;
- bass notes that follow chord changes, with rhythm suggestions informed by
  kick timing;
- context-aware accompaniment;
- arrangement suggestions that help complete a short sketch; and
- other explicitly reviewable helpers for repetitive musical work.

None of these helpers exists or belongs to the current work track today. They
must not act without clear review and confirmation. An idea enters active
scope only when the owner moves it into the experimental roadmap with an action,
result, state boundary, and acceptance path.

The owner-directed interaction, theory/algorithm research, FT2 `SIZE` design,
circle of fifths, arpeggio families, drum fills/rolls, and staged Arrangement
assistant are developed in [Future musical sketch helpers](FUTURE_MUSICAL_HELPERS.md).

Current key and tuning behavior is narrower and already useful. The musician
selects the Project tonic and major or natural-minor mode, and N00B filters
live melodic input against that scale. SHR Drums already supports `OFF`,
`FOLLOW KEY`, and `MANUAL` per-piece tuning; `FOLLOW KEY` follows the stored
Project key. Only automatic key inference and higher-level tuning suggestions
belong to this future section.

## Playlist above Song

SHR-DAW currently loads one saved `.shsong` Project at a time. That Project is
the musician's song: it owns one Arrangement, its Patterns, routes, sounds,
loops, automation, effects, and recording state. The current `SONG` control in
FT2 navigates Arrangement steps inside that Project. It does not navigate
separate song files.

The intended hierarchy is:

```text
Playlist                                          future
└── Song / Project (.shsong)                        current
    ├── Arrangement ──── references Pattern IDs          current
    └── Patterns
        └── Pattern
            ├── tracker Pages
            │   └── four columns / lanes and their cells
            ├── four-slot Loop Mix workspace
            └── automation lanes
```

The Arrangement orders references to Patterns; it does not own or copy them.
Each tracker Page belongs to one Pattern. A note Page owns one destination,
four column setups, four lanes of cells, entry behavior, mute state, and Page
metadata. Loop Mix is shown as a musician-facing Page in FT2, but its four WAV
slots are stored directly under the Pattern. Automation lanes are also
Pattern-owned and may target controls associated with a Page.

A future Playlist should be a small ordered list of saved Projects. Each song
must remain a complete, independently loadable `.shsong` file. The Playlist
must not merge song data, move Pattern ownership above the Project, or rewrite
a song merely because its order changes.

What exists now:

- Project files can be created, named, saved, copied, loaded, and protected
  from accidental loss;
- each Project has an Arrangement that orders reusable Pattern references;
- stop, panic, route replacement, Project load, and shutdown clean up owned
  notes and audio; and
- Projects retain their own routes, instruments, loops, effects, automation,
  tempo changes, and recording configuration.

What does not exist:

- a Playlist file format, storage location, or migration contract;
- an ordered list of Project references with previous/next song movement;
- a Playlist screen, controller page, or performance status;
- missing-song locate, skip, remove, and retry behavior;
- dirty-song protection when moving to another Playlist entry;
- next-song route and instrument preflight;
- automatic, timed, gapless, or overlapping song transitions; or
- Playlist import, export, copying, or portability rules.

Do not add Playlist controls to the current FT2 `SONG` overlay, tracker body,
or controller rows. Those screens are already responsible for editing and
performing one Project. A later design should use a separate child screen and
return to the same Playlist row after a song is opened, edited, played, or
cancelled. The entry point for that child is still undecided.

The safe first version should treat song changes as explicit transactions. It
must protect unsaved work, stop the outgoing transport, release notes and
owned routes, validate the next Project, and either complete the load or keep
the previous song and Playlist position recoverable. A missing or incompatible
song should remain visible in the list rather than causing another file to be
loaded by guesswork.

The main product choice is still open: whether Playlist is only a set list for
manual song selection or may also advance automatically during performance.
Automatic advance, per-entry repeats or notes, preloading, and gapless audio
belong to later decisions. None is implied by recording the hierarchy now.

Acceptance must cover create, reorder, save, reload, open, return, cancel,
dirty-song handling, missing and incompatible files, route failure, stop,
panic, repeated song changes, and shutdown. Existing Project files and all
current 40×13 Song/Arrangement controls must behave exactly as they do today.

## Future Page operations

Pages are already real Pattern-owned objects. Today the musician can select a
Page, add one in the full-screen Tracks manager, change its destination and
four column setups, choose its entry behavior, mute it, and edit its cells.
The FT2 clipboard can copy or paste a lane or a four-lane Page block, but that
clipboard carries cells only. It does not copy the Page name, route, column
setup, entry behavior, drum classification, setup messages, mute state, or
automation targets.

The future Page manager should add operations for:

- renaming a Page;
- duplicating a complete Page inside the current Pattern;
- reordering Pages without changing their musical content;
- removing a Page with an explicit data count and confirmation;
- clearing Page cells while retaining its route and setup; and
- copying or moving a complete Page between Patterns.

Cross-Project Page transfer is still an open choice. It would need explicit
handling for sounds, MIDI destinations, device profiles, drum kits, and any
other reference that may not exist in the receiving Project.

A complete Page operation must carry or deliberately remap the Page name,
target, four channel/bank/program setups, velocity, percussion and entry
settings, drum classification overrides, setup messages, enabled state, lanes,
and cells. Automation lanes that target the Page must follow it during reorder
or duplication. Removal must show how many automation points would also be
removed or detached. Pattern-level Loop Mix slots and unrelated automation
must stay untouched.

Patterns may have different row counts, meters, and tempos. Copy or move to a
different Pattern must preflight cell rows and automation positions. If data
would fall outside the destination, the screen must show exact affected counts
and offer an explicit supported result or Cancel. It must not truncate cells
or automation silently.

Structural Page changes must be transactional. If the Page is sounding, the
operation must release its notes and route owners before publication. Reorder
must keep the cursor on the same logical Page. Duplicate should select the new
copy. Remove should choose a nearby surviving Page, and a Pattern must retain
at least one tracker Page. Cancel, validation failure, route failure, or an
incompatible destination must leave the Pattern and cursor unchanged.

Do not add these operations to the quick `PAGE` overlay or the normal FT2
controller rows. The existing full-screen Tracks manager is the owner because
it already handles Page creation, routing drafts, Done, and Exit rollback. A
later design can add a focused operations child there while preserving the
selected Page, column, row, transport, and return location. Exact button
placement and whether sounding structural edits require Stop or a safe Pattern
boundary remain open design decisions.

Acceptance must cover empty and populated Pages, first/middle/last positions,
the last surviving Page, copied routes that are online or offline, shared
software routes, external MIDI setup, drum Pages, Page-targeted automation,
playback interruption, cancellation, save/reload, and old Projects.

## Safe fallback for unknown USB MIDI devices

When a USB MIDI input is connected without a saved or reviewed controller
profile, SHR should eventually offer or apply a useful fallback mapping instead
of leaving the device entirely unmapped. Reviewed profiles must remain the
preferred source, and fallback discovery must never silently overwrite the
user's controller configuration.

This needs a deliberately conservative design. Arbitrary notes and CCs must not
accidentally become transport, record, panic, or navigation commands. Musical
notes must continue to pass through unless the user deliberately assigns them
as channel-qualified commands. Safe continuous-control discovery should be
separate from command-button assignment: a knob or encoder can be proposed from
observed continuous traffic, while transport and other command buttons require
clear review or explicit learning before activation.

## Raspberry Pi 5 headroom pass

The Raspberry Pi 5 and NVMe baseline is complete. Remaining unscheduled work
may compare dependency and library footprint, private-cache benefits from
real-time core placement, and effect state and callback cost before making any
new optimization claim.

The proposal keeps one effects rack. Effects that later pass fixed low-state
and low-callback-cost gates may receive the compact `» PRESTO` mark; unmarked
effects remain normal first-class choices. No hardware result, marker, library
change, or schedule is implied today. The complete boundaries and planned
experiment matrix are in the [Raspberry Pi 5 headroom and footprint
plan](PI5_HEADROOM_PLAN.md).

## Irregular Patterns, swing, and groove timing

Arbitrary Pattern shortening/growing, independent early/late hits,
Pattern-wide swing, deterministic groove tools, and timing-aware tracker REC
are implemented. Their current contracts live in [the tracker
manual](TRACKER.md), [rhythm workflow acceptance](RHYTHM_WORKFLOW_ACCEPTANCE.md),
and the owner-directed FT2 Edit `SIZE` work in
[Future musical sketch helpers](FUTURE_MUSICAL_HELPERS.md). Only optional
formal odd-meter/grouping metadata from the [legacy rhythm
proposal](POST_COMPETITION_RHYTHM_PLAN.md) remains an unscheduled idea.

## Unreasonable but useful challenges

These are moonshots, not release promises. They exist because a tiny music box
should occasionally attempt something delightfully excessive, and because a
good stunt can expose weaknesses that polite test material never finds.

### The Space Shuttle challenge: decode a Danny Carey performance

Privately import a short, legally obtained excerpt from a Tool recording that
features Danny Carey, then ask SHR-DAW to analyze the full mix and build an
editable drum track from what it hears. The name is intentionally unserious;
the engineering challenge is not.

The experiment should:

1. detect likely drum transients without assuming a steady 4/4 grid;
2. propose tempo regions, irregular phrase ends, and groupings such as
   `2+2+3`, while showing uncertainty rather than inventing certainty;
3. separate kick, snare, hat/cymbal, and other-percussion candidates into the
   four tracker lanes;
4. retain estimated velocity and early/late timing instead of flattening the
   performance onto a rigid grid;
5. let the user audition, correct, shorten, grow, regroup, and simplify the
   result; and
6. derive a new, playable SHR-DAW groove that demonstrates what the analysis
   taught us without requiring the source recording during playback.

This challenge depends on the arbitrary-length, microtiming, swing, groove,
and expressive-capture work in the [post-competition rhythm
plan](POST_COMPETITION_RHYTHM_PLAN.md). Offline analysis must remain separate
from the real-time audio callback and must have bounded input length, memory,
and CPU use. A mixed commercial master may prevent reliable instrument
separation, so a useful partial transcription with visible confidence is a
valid result; pretending it is exact is not.

The imported audio and an exact derived transcription remain private below the
user-data boundary unless their redistribution rights are established. They
must not be committed, packaged, embedded in a demo, or presented as project
content. A public result must use newly authored/cleared audio and a genuinely
original groove rather than redistributing the Tool excerpt or a note-for-note
copy of the performance.

Success is not “replace Danny Carey.” Success is that SHR-DAW can inspect one
famously demanding rhythmic performance, explain its best hypothesis in plain
language, turn that hypothesis into editable tracker data, and help a musician
make something new. If the little box survives the Space Shuttle challenge,
ordinary drum loops should feel like a pleasant afternoon.

## Audio effects graph: inserts, sends, and returns

The current narrow performance bus now sums exactly the managed instrument,
SHR Drums, owned loop, and one configured two-port Input in stereo or dual mono before the master,
dedicated limiter, final meter, recorder, and playback. The broader proposed
migration to a bounded multi-strip mixer with genuinely shared multi-source aux buses is in the
[post-competition mixer and shared-aux plan](POST_COMPETITION_MIXER_AUX_PLAN.md).
It also records the current dry/wet behavior, the audio-source boundary behind
tracker lanes, Project migration, and recording taps. The two narrow meter and
aux-bypass findings from that routing audit have also been repaired. The final
bus does not implement the broader strip/aux design.

The managed graph now includes the essential source inserts, delay/modulation,
three reverb voicings, two independently scaled pre/post aux sends and returns,
and an ordered master rack. It retains strict Project persistence, stopped
structural publication, compact editors, and meters. Evidence is in the
[Phase 2 insert-effects measurement](PHASE2_AUDIO_GRAPH_MEASUREMENT.md) and
[Phase 3/4 effects measurement](PHASE3_4_AUDIO_GRAPH_MEASUREMENT.md). The
four-source path has hardware-independent evidence, while full-duplex physical
interface acceptance remains deliberately deferred.

### Product idea

Effects should be reusable audio processors that can be placed deliberately in
the signal path, not hard-wired decorations on one synth. The routing model
should eventually support:

- an ordered **source insert chain**, such as synth → filter → drive → output;
- a **master insert**, such as all SHR-DAW sources → compressor/EQ → output;
- a shared **aux send/return**, where multiple sources retain their dry path
  while feeding a 100%-wet delay or reverb at independent send levels; and
- an optional **external hardware insert/send**, where a spare interface output
  feeds a pedal/rack processor and a capture input returns it; and
- a **live input strip**, where a physical JACK capture pair becomes a
  first-class source that can pass through the same inserts, sends, master,
  monitor, and recording choices as software sources while playback continues.

Insert and send are musically different. An insert replaces the source path and
usually exposes a wet/dry or bypass control. A send copies some of the source to
a shared effect while the dry signal continues to the master; its return must be
mixed exactly once. The UI and Project format should use those words rather than
hiding both behaviors behind a generic “effect” switch.

### Current architecture boundary

SHR-DAW now owns a bounded four-source stereo sum. The managed source's dry
path and two wet returns meet SHR Drums, the complete internally summed
four-slot Loop Mix, and the configured live-input pair, then pass through the
master, final limiter/meter/recorder and playback. The raw synchronized
multitrack recorder remains a separate workflow.

Loop Mix settings now belong to each FT2 Pattern, while one fixed four-renderer
client serves only the active Pattern and one bounded incoming preparation.
That ownership correction does not imply the proposed
[Playlist above Song](#playlist-above-song), companion mode, a standalone
Pattern library, cue/headphone routing, time-stretching, more mixer strips, or
additional audio buses. Those remain separate future product decisions, not
architectural follow-ons.

The graph uses internal preallocated mixer, send-tap, and return nodes rather
than relying on implicit JACK summing. That makes independent send/return gain,
pre/post placement, return metering, and exactly-once mixing explicit and
testable. The final bus adds smoothed level per source and master level, MUTE
for Synth/Loop/Drums, and one MON ON/OFF action for Input. A fuller mixer would
still be needed for pan on the other sources, solo, per-input inserts, or
shared aux sends, none of which is current product scope.

Primary source: [JACK 2 `jack_port_get_buffer` API
contract](https://github.com/jackaudio/jack2/blob/develop/common/jack/jack.h),
which specifies appropriate mixing for multiple inbound connections.

The graph owner must connect and disconnect only SHR-owned ports, refuse
ambiguous endpoints, restore a safe graph after client loss, and never alter
unrelated JACK connections.

### Free wiring and first-class inputs

The long-term model should be a validated audio patch bay rather than separate
special cases for synth, loop, and recorder:

- **sources:** managed engine, WAV loop, physical capture input, and owned
  effect/hardware returns;
- **processors:** gain/pan, meters, insert effects, send taps, wet effects, and
  optional master processing; and
- **sinks:** physical playback, pre-effect recording, post-effect recording,
  and explicitly configured hardware sends.

A proper live-input client would register stereo JACK inputs connected from the
configured capture ports and stereo JACK outputs that enter the same processor
graph as the other sources. With a full-duplex device and JACK configuration,
capture and playback can run simultaneously: an external synth, microphone, or
hardware return can be processed, monitored, and recorded through SHR routing.
That behavior must be proven on the actual device rather than inferred from the
presence of input and output names.

“Free wiring” should mean that users may compose any valid acyclic route from
available sources, processors, and sinks. It must not mean that SHR silently
accepts a feedback cycle, connects an output to itself, creates two monitor
paths, or rewires unrelated JACK clients. Validate the proposed graph before
publishing it, reject unsafe cycles and ambiguous ports, and switch from the old
graph to the new graph with a bounded mute/fade strategy so partial connection
failure does not leave a loud, doubled, or silent path.

Input monitoring must explicitly distinguish:

- **hardware/direct monitoring**, which bypasses SHR processing and has the
  lowest device latency;
- **software monitoring**, which routes capture through SHR effects and back to
  playback; and
- **record-only input**, which captures without returning audio to playback.

Enabling hardware and software monitoring together can double the dry input or
create feedback in an external loop. The UI and Project must make the selected
mode visible. Recording should explicitly choose pre-insert, post-insert, or
master output instead of silently changing what is captured.

### Implemented foundation and remaining choices

Earlier revisions of this plan compared per-source, JACK-summed, and owned-mix
topologies and proposed a first small effect set. That selection work is now
historical: the current bounded implementation uses an owned exactly-once sum,
13 effect types, source/master serial racks, two wet-only aux racks, a
post-master meter, and transactional direct fallback. The authoritative
current behavior and limits live in the [audio graph contract](AUDIO_GRAPH.md),
not in this future-work page.

The choices still open here are genuinely future ones: how independently owned
loop, live-input, and hardware-return sources become mixer strips; how
monitoring and recording taps remain unambiguous; whether external hardware
inserts are safe and worthwhile; and how a validated free-wiring UI fits 40×13.
Chain order, bypass/tails, client loss, Project migration, and publication must
retain the current safety guarantees during that expansion. Objective DSP and
performance measurements can establish engineering fitness, but final
low-gain, level-matched musical curation remains a human listening decision.

### Raspberry Pi metric plan

Desktop development can accelerate unit tests and DSP prototyping, but only
release-mode Raspberry Pi measurements count for the product claim. Establish
an idle/bypass baseline, then measure 1, 2, and 4 instances where the topology
allows it. At minimum record:

- JACK sample rate, period size, periods, and the callback time budget
  (`period_frames / sample_rate`);
- callback mean, p95, p99, and maximum duration using lock-free counters read
  outside the callback;
- JACK xruns and deadline misses over a sustained run;
- process/core CPU, isolated audio-core utilization, RSS, and bounded effect
  memory;
- added algorithmic latency and, for an authorized hardware loop, measured
  round-trip latency;
- sustained simultaneous capture/playback behavior, input-to-output latency,
  and whether direct monitoring creates a doubled path;
- peak/RMS before and after, NaN/non-finite protection, clipping, and feedback
  containment;
- bypass/reorder/load discontinuities and click risk;
- sample-rate changes, client loss/reconnect, panic, stop, and clean shutdown;
  and
- the final demo graph under the exact song workload rather than a silent
  microbenchmark alone.

At 48 kHz, a 128-frame JACK period is about 2.67 ms; 64 frames is about 1.33
ms, and 256 frames about 5.33 ms. Those are total callback deadlines, not CPU
budgets available entirely to one effect. Report observed settings and results
rather than promising latency in advance.

### Real-time acceptance gates

- No allocation, locks, file I/O, subprocess calls, logging, or panics in the
  JACK callback.
- Fixed/bounded buffers, finite-value guards, denormal handling where needed,
  parameter smoothing, and safe feedback limits.
- No connection to an effect means a predictable dry path; a crashed effect
  must not leave destructive feedback or an unrecoverable silent graph.
- Bypass and shutdown are click-conscious and release every owned JACK resource.
- Project/config migration is versioned and atomic; unknown newer formats are
  refused.
- The 40×13 workflow exposes only the controls needed to understand and perform
  the chain.
- Free-wiring publication is transactional: validate the complete graph first,
  then connect it without leaving a partial, cyclic, or doubled route.

### PC/Pi split

If DSP work begins on a development PC, keep it in a separate Git
branch/worktree so it cannot destabilize the submission checkout. Record that
development split truthfully. A feature may enter SHR-DAW only after the same
locked formatting, tests, warning-denied Clippy, optimized build, non-audible
graph tests, and authorized performance measurements pass on the Raspberry Pi.

## External MIDI routing

### Optional multi-target live thru

FT2 playback already routes every page to its own `(MIDI output, channel)`, so
two instruments on separate physical MIDI outputs may use the same receive
channel without interfering. Step-edit audition intentionally follows only the
selected page, while normal live thru follows the single configured external
output.

A future opt-in live-routing layer could send or split controller performance
input across several page targets. It must retain exact target/channel/note
ownership, consume command pads, prevent doubled routes, and send correct note
offs during target changes, stop, panic, and disconnects. The default should
remain a single destination so enabling a second interface never layers synths
unexpectedly.

### Stable identity for identical USB-MIDI adapters

Exact ALSA MIDI output names distinguish different interfaces today. Two
different named ports work independently, but identical adapters can expose
indistinguishable names. SHR-DAW now refuses ambiguous exact or partial matches
instead of selecting the first one.

A future device-alias system could bind user-facing names such as `CASIO OUT`
and `D-50 OUT` to stable USB/ALSA card and port identity, preserve those aliases
across reconnects, while preserving the current refusal to guess. It should
remain configuration data rather than adding hardware names to Rust constants.
