# Sequencer workflow priorities

Created: 2026-08-26

Status: owner-directed product priority; Priorities 1–7 are implemented and
verified in their linked acceptance matrices

## Purpose and decision

This document compares SHR-DAW's current FT2 workflow with modern hardware
step sequencers, records the accepted hardware boundary, and preserves the
ordered software workflow priorities.

The first priority was **bounded FT2 Pattern Undo/Redo plus one explicit Pattern
Snapshot/Recall**. Priority 2 added timing and groove work, and Priority 3 added
step probability/conditions. Priority 4 added independent lane cycles, speed,
and direction; Priority 5 added deterministic generative tools. Priority 6
extended that same draft with internal arpeggio, chord, and harmonizer
generation. Priority 7 added exclusive external USB MIDI transport sync. The
original comparison and ordering remain here as the product decision record.

## Product and hardware boundary

- Do not add GPIO circuits, direct CV/Gate electronics, analog clock hardware,
  or another custom electronics path to the Raspberry Pi. The existing machine
  and controller hardware are the platform.
- A hardware-facing function remains eligible only when SHR-DAW can provide it
  in software through an ordinary USB device, such as USB MIDI clock input or
  USB MIDI Polyphonic Expression. It must not require modifying the Pi.
- An off-the-shelf USB device may translate MIDI outside SHR-DAW, but SHR-DAW
  should continue to own MIDI semantics rather than special-case the device's
  electronics.
- Direct CV/Gate and analog-clock output are therefore outside the product
  direction. They are not deferred software priorities.
- Pure software workflow improvements take precedence over expanding physical
  integration.

## Current SHR-DAW foundation

Observed in current source and focused documentation:

- FT2-style step editing and row-quantized real-time recording accept notes and
  chords through Manual, One-column, and Drum-auto entry.
- A cell stores note, velocity, program, gate, independent signed timing,
  probability, one loop-aware condition, and one Cut, Delay, Retrigger, or
  Tempo command.
- Patterns own tempo, 3/4 or 4/4 meter, pages, routes, lane setup, automation,
  cells, and four Loop Mix slots. Structural tools can reach any row count from
  1 through 256.
- The Arrangement chains Pattern references. Live Patterns add immediate,
  Pattern-quantized, and bar-quantized launch, retrigger, cancellation, capture,
  and transient lane shaping.
- Sparse Pattern automation records and edits instrument controls, external
  MIDI CC, and effect parameters with target-owned Step or Ramp curves.
- Project save/load, Pattern clone/copy/paste, lane/page clipboards, destructive
  confirmations, route/editor cancellation, and the dirty-Project guard already
  protect specific boundaries.
- Four Pattern-owned WAV slots, drum-groove discovery, MIDI import/export,
  count-in, metronome, scale filtering, and MIDI clock output are implemented.

The result is already a composition, arrangement, automation, loop, and
recording workstation. The important comparison is not whether SHR-DAW has a
sequencer; it is how safely and fluidly a musician can explore inside it.

## Researched gaps, in priority order

### 1. Bounded Pattern Undo/Redo and Snapshot/Recall

**Implemented in the bounded Priority 1 slice.** The exact mutation inventory,
transaction boundaries, stopped-transport fallback, automated matrix, and
non-Raspberry-Pi evidence limit are in
[Pattern History mutation inventory](PATTERN_HISTORY_MUTATION_INVENTORY.md).

Before Priority 1, transactions protected only the operation presently open;
after an edit was confirmed the musician had no general way to step back, try
an alternative, or restore a captured Pattern state. This section retains the
rationale and original implementation contract behind the completed workflow.

The first implementation is deliberately Pattern-scoped. It covers the
selected Pattern's tempo, meter, pages, routes, cells, automation, and Loop Mix
settings. It does not pretend that Project replacement, Arrangement edits,
Project-global effects, mixer/final-bus data, saving, or private files are part
of the first history model.

Squarp Hapax documents Undo/Redo and Snapshot, while Polyend Play documents
sixteen Undo/Redo layers and a saved Pattern reset. Their value here is the
recovery motion, not their exact button layout or history size.

### 2. Independent microtiming, swing, groove, and timing-aware REC

**Implemented in the bounded Priority 2 slice.** The exact timing units,
persistence migration, swing/groove/REC FEEL semantics, scheduler ownership,
automated matrix, and non-Raspberry-Pi evidence limit are in
[Rhythm workflow acceptance](RHYTHM_WORKFLOW_ACCEPTANCE.md).

Before Priority 2, SHR-DAW quantized REC to rows and Delay could move a cell
late only by occupying its single command slot. The completed workflow adds
independent signed timing, Pattern swing, deterministic groove application,
and optional runtime `REC FEEL`. This section retains the evidence behind that
choice.

Digitakt II, Hapax, Polyend Play, Circuit Tracks, and KeyStep Pro provide
microtiming, swing, unquantized capture, or equivalent time-shift controls.

### 3. Per-step probability and conditions

**Implemented in the bounded Priority 3 slice.** The exact semantics,
persistence migration, editor/runtime boundary, automated matrix, and
non-Raspberry-Pi evidence limit are in
[Step probability and conditions acceptance](STEP_PROBABILITY_CONDITIONS_ACCEPTANCE.md).

Useful first conditions are percentage chance, first pass, last pass, `A:B`
loop count, previous-result dependency, and Fill-only. They let a short Pattern
evolve without cloning many nearly identical Patterns.

Elektron documents probability, previous/neighbor, first/last, `A:B`, and Fill
conditions. OXI One, Hapax, and Polyend Play provide related chance and logic
systems. SHR-DAW now stores deterministic probability and one loop-aware
condition in each Cell; this section retains the evidence behind that choice.

### 4. Independent lane cycles, speed, and playback direction

Implemented as the bounded workflow defined in [Priority 4 lane playback
acceptance](LANE_PLAYBACK_ACCEPTANCE.md). Each lane owns FULL or an explicit
cycle length, five exact rates, and forward, reverse, pendulum, or bounded
deterministic-variation playback. Pattern-time owners—including tempo,
Arrangement duration, row markers, swing, automation, and Loop Mix—remain
unchanged, and editing does not move the FT2 cursor. The workflow reuses the
four existing controller pages and commits one stopped-transport Pattern
History transaction.

### 5. Deterministic generative tools

**Implemented in the bounded Priority 5 slice.** The exact Euclidean,
accumulator, seeded-mutation, and controlled-FILL semantics; collision and
transaction ownership; persistence behavior; automated matrix; and
non-Raspberry-Pi evidence limit are in
[Deterministic generative tools acceptance](DETERMINISTIC_GENERATIVE_TOOLS_ACCEPTANCE.md).

The completed workflow generates a visible selected-lane draft, reports
affected/colliding/protected cells, retains a runtime seed where randomness is
involved, and requires Apply, Apply to Clone, or Cancel. Successful results are
ordinary Pattern cells; playback never invokes a hidden generator. Hapax, OXI
One, Torso T-1, and Polyend Play retain their role as comparison evidence.

### 6. Internal arpeggiator, chord generator, and harmonizer

**Implemented in the bounded Priority 6 slice.** The exact cursor-row arpeggio
source/order/octave/rate/gate/repetition semantics, Project-key triad degree/
inversion/voicing/lane placement, diatonic harmonizer interval/voice/
out-of-scale policy, range and collision rules, shared draft ownership,
automated matrix, and non-Raspberry-Pi evidence limit are in [Priority 6
arpeggio, chord, and harmonizer generators](HARMONIC_GENERATORS_ACCEPTANCE.md).

The completed first version is offline and writes ordinary cells only. It does
not introduce a live arpeggiator, chord-following transport owner, hidden
playback regeneration, or another generator system. OXI One, Hapax, Torso T-1,
and KeyStep Pro retain their role as comparison evidence.

### 7. External transport sync through USB MIDI

**Implemented in the bounded Priority 7 slice.** One exact configured USB MIDI
source may own 24 PPQN Timing Clock plus Start/Stop, with bounded acquisition,
tempo/phase tracking, visible loss/refusal/reacquisition, exclusive suppression
of SHR clock output, and unchanged event-level timing owners. The exact
positioning, output interaction, failure behavior, automated matrix, protocol
provenance, and non-Raspberry-Pi evidence limit are in [Priority 7 external
transport sync acceptance](EXTERNAL_TRANSPORT_SYNC_ACCEPTANCE.md). Song Position
Pointer, Continue, and clock thru remain explicitly outside the first version.

### 8. Expressive MIDI and MPE through USB MIDI

Live MIDI can pass expressive channel messages, but FT2 persistence and MIDI
import do not retain pitch bend, pressure, or per-note MPE expression. Hapax is
an example of editable MPE sequencing. This is eligible over USB MIDI but lower
priority because the existing instruments, workflow, and compact display do not
yet establish a broad MPE need.

## Explicitly not treated as missing basics

- **Song construction:** SHR-DAW already has Pattern Arrangement and Live
  Pattern capture. A future Playlist above complete Projects is a separate set
  workflow, not a missing step-sequencer core.
- **Ratchets:** Cell Retrigger already supplies bounded repeated notes. Later
  probability or shaped-repeat work may extend it without relabeling it absent.
- **Parameter locks in general:** Pattern automation already provides Step and
  Ramp values for instrument, MIDI CC, and effect targets. A faster per-step
  entry motion may be useful later, but the broad musical capability exists.
- **Sampling, loops, effects, and recording:** SHR-DAW already owns these larger
  workstation functions. This priority pass must not duplicate them.
- **Direct CV/Gate or analog clock:** outside the accepted Raspberry Pi hardware
  boundary.

## Priority 1 implementation contract

### Intended musician motion

Before:

1. The musician confirms an edit.
2. The Project becomes dirty.
3. Cancel no longer applies after leaving that editor.
4. Recovering an earlier musical state requires manual reconstruction, a saved
   Project reload, or an ad hoc clipboard copy.

After:

1. Every supported committed Pattern mutation creates one understandable Undo
   step.
2. **UNDO** restores that Pattern and its editing context; **REDO** reapplies the
   state that Undo replaced.
3. A new committed edit after Undo clears Redo.
4. **SNAP** captures one explicit runtime Pattern state without dirtying the
   Project. **RECALL** restores it as an ordinary undoable mutation.
5. Loading or creating another Project clears history and Snapshot explicitly.

The obvious actions must each have one result. Undo is not Cancel, Snapshot is
not Save, and Recall is not Project reload.

### First-version scope

Include committed changes to one existing Pattern from:

- cell/note editing, blank, erase, note-off, and step/chord entry;
- one completed real-time recording take;
- Pattern tempo, meter, length, SIZE, clear, transpose, and drum-pattern load;
- lane/page paste and Pattern paste-over;
- page/route changes once their existing Apply transaction commits;
- automation lane/point creation, editing, recording, and clear; and
- Pattern-owned Loop Mix attachments and settings.

Treat one user gesture as one history step. Continuous encoder movement,
automation capture, a chord entry, and a REC take must not create hundreds of
microscopic Undo entries.

Exclude from the first version:

- Project New/Load/Import, Save/Save As, rename, and deletion;
- Pattern creation, clone, paste-new, and unused-Pattern deletion because they
  change Project structure rather than one existing Pattern;
- Arrangement mutations;
- Project-global effects, aux/master routing, MASTER STRIP, recorder, and final
  bus configuration;
- runtime Live Pattern shaping, launch queues, Loop playback position, mutes,
  held notes, and transport state;
- private files and engine preset files; and
- edits still inside an editor's existing draft/Apply/Cancel transaction.

Those exclusions must be described as first-version boundaries, not silently
ignored commands. Later Project-structure history can use separate typed
entries if real use establishes the need.

### Data model

Add one focused history owner, preferably outside `ui.rs`:

```text
PatternHistory
  undo: deque<PatternHistoryState>
  redo: deque<PatternHistoryState>
  snapshot: optional<PatternSnapshot>

PatternHistoryState
  pattern_id
  pattern
  edit_context
  label

PatternSnapshot
  pattern_id
  pattern
  edit_context
```

Each stack stores one prior state, not a before-and-after pair. Undo pops the
prior state, pushes the current Pattern and context to Redo, then swaps in the
prior state. Redo performs the inverse. This halves retained data compared with
storing two complete Patterns per entry.

`edit_context` should retain Pattern/Arrangement selection, row, page, lane,
column, FT2 mode where safe, automation selection when relevant, and the
controller page needed to return coherently. Clamp only when the restored
Pattern genuinely lacks the old row/page/lane.

Bound history by both entry count and structural weight. Start with at most 32
states and a conservative total budget equivalent to two maximum Project cell
budgets plus two maximum Project automation-point budgets. Evict the oldest
Undo state first; Redo uses the same combined budget. Keep exactly one explicit
Snapshot outside the stacks. Measure representative and maximum Pattern memory
on the Pi before treating the bound as accepted.

History is runtime state and must not change `.shsong` format. Saving changes
the existing clean baseline but does not erase useful history. Because dirty
state is already equality against the clean baseline, Undo back to the saved
Song should naturally show `SAVED`; Redo away from it should show `DIRTY`.

### Mutation boundary

Do not scatter raw `history.push()` calls after mutations. Add one helper that
captures a pre-mutation Pattern and commits it only when the operation succeeds:

```text
with_pattern_history(label, gesture, mutation) -> result
```

The helper must:

1. resolve the exact Pattern and capture context;
2. run the existing validation/transaction without publishing history yet;
3. compare the final Pattern with the opening Pattern;
4. push one Undo state and clear Redo only when the Pattern changed;
5. leave history untouched on refusal, Cancel, validation failure, allocation
   failure, engine/route failure, or a no-op; and
6. coalesce repeated events that belong to one explicit gesture or capture.

Draft editors retain their current Cancel behavior. Their final Apply/Confirm
enters history once; internal knob movement does not.

### Restore and transport behavior

- Snapshot capture never changes transport, notes, runtime loops, Project dirty
  state, or selection.
- A stopped Undo/Redo/Recall restores immediately through the same validated
  Pattern/runtime publication paths used by current edits.
- During active FT2 Play, a restore queues for the next safe Pattern boundary
  and shows `UNDO Q`, `REDO Q`, or `RECALL Q`. It must not replace sounding
  routes or Loop state mid-note. A later history command replaces the queued
  restore; Back cancels it without changing either stack.
- During REC, Undo first ends the current recording take as one committed
  history step, then the requested Undo removes that take at the next safe
  boundary. It must not discard an uncommitted held-note owner.
- A restore that needs another managed instrument or Loop preparation uses the
  existing preflight, ownership, All Notes Off, replacement, rollback, and
  failure-restoration contracts. History stacks move only after successful
  activation. Failure keeps the current Pattern sounding and the same Undo or
  Redo available for retry.
- Panic and Stop retain their existing literal behavior and never become
  history operations.

If the current scheduler cannot safely queue a complete Pattern restoration at
a boundary without a large redesign, the first implementation may require
stopped transport. That fallback must be explicit in the UI and documentation,
and the new thread must record the exact technical blocker rather than silently
stopping playback.

### Interface

Add an FT2 **HISTORY** child reachable from one currently unused slot on the
FT2 Tools `PAGE` controller row. Keep existing page/lane mute actions in place.

The child screen's first controller page is:

| Item 1 | Item 2 | Item 3 | Item 4 |
| --- | --- | --- | --- |
| `UNDO` | `REDO` | `SNAP` | `RECALL` |

The fourth page remains canonical `SYS` with Panic, Help, and Exit. Unavailable
Undo, Redo, or Recall controls are visibly disabled. The body shows only:

- next Undo label or `UNDO —`;
- next Redo label or `REDO —`;
- Snapshot Pattern number and capture label or `SNAP —`; and
- queued boundary restore when present.

Keyboard uses conventional `Ctrl+Z` for Undo and `Ctrl+Y` plus `Ctrl+Shift+Z`
for Redo. Snapshot and Recall use the same action dispatcher from the HISTORY
screen. Mouse, keyboard, and controller must call identical actions. Returning
to FT2 restores the exact prior page/lane/column/row and controller page.

### Historical Priority 1 implementation sequence

1. **Inventory mutations.** Enumerate every `self.song`/Pattern mutation in
   `ui.rs`, classify it as included, excluded, draft-only, structural, or
   runtime-only, and turn the classification into a test table before editing.
2. **Add the history model.** Implement bounded Undo/Redo stacks, the single
   Snapshot, context records, structural-cost accounting, new-edit Redo
   invalidation, Project-replacement reset, and focused model tests.
3. **Add one mutation wrapper.** Route simple stopped cell edits through it
   first. Prove success, no-op, cancellation/failure, Undo, Redo, and saved
   baseline behavior before expanding coverage.
4. **Cover included mutation families.** Add step entry, gestures, REC take
   coalescing, SIZE/transpose/paste/drum load, route/page transactions,
   automation, and Loop Mix one family at a time. Do not refactor unrelated UI.
5. **Integrate safe restoration.** Reuse current sequencer publication, managed
   engine, Loop preparation, route rollback, held-note, and All Notes Off paths.
   Implement boundary queueing only if those ownership contracts remain exact.
6. **Add the HISTORY screen.** Add navigation actions, controller menus,
   keyboard/mouse dispatch, disabled states, compact 40x13 rendering, Help, and
   exact return-context behavior.
7. **Update focused documentation.** Update `TRACKER.md`, `CONTROLLER_INTERFACE.md`,
   `HELP.md`, `MENU_MANUAL.md`, and generated documentation/screenshots only for
   the implemented behavior.
8. **Verify in proportion to the active repository gate.** During the current
   incremental phase, use formatting, source inspection, focused static/data
   checks, and `git diff --check`. Do not run Cargo build/check/test, Clippy,
   screenshot batches, JACK, MIDI, synth, playback, recording, or audible tests
   until the owner explicitly authorizes the combined pass.

This sequence is retained as implementation history. Priority 1 is published;
its accepted stopped-transport fallback and remaining hardware evidence limit
are recorded in `WORKSPACE_HANDOFF.md`.

### Acceptance matrix

- Undo/Redo one cell, chord, blank, erase, note-off, and one coalesced REC take.
- Undo/Redo route/page, automation, Loop Mix, resize, transpose, paste-over, and
  drum-load changes without touching unrelated Patterns or Project-global data.
- New edit after Undo clears Redo; failed/no-op/cancelled edits do not.
- Undo back to the clean baseline shows saved; Redo shows dirty; Save does not
  corrupt history.
- Snapshot capture is non-dirty and non-audible; Recall is undoable; replacing
  the Project clears Snapshot and both stacks.
- History capacity evicts oldest states deterministically and remains bounded
  for maximum cells, pages, setup data, automation, and long Loop paths.
- Restored context preserves Pattern, Arrangement step, row, page, lane, column,
  relevant mode, and controller page, clamping only to restored bounds.
- Draft Cancel and restore failure keep the Pattern and history byte-for-byte;
  retry remains adjacent.
- Play/REC boundary behavior never duplicates notes, loses note-offs, layers
  engines, starts an unrequested synth, applies a queued restore twice, or
  changes unrelated JACK/ALSA routes.
- Keyboard, controller, and mouse parity; native 40x13 and compact fallback;
  unchanged shared status row.
- Old Projects remain byte-for-byte compatible because history is not persisted.

## Primary hardware-sequencer sources

- [Elektron Digitakt II User Manual, OS 1.15](https://www.elektron.se/wp-content/uploads/2025/06/Digitakt-2-User-Manual_ENG_OS1.15_250625.pdf)
- [Squarp Hapax product and feature specification](https://squarp.net/hapax/)
- [Squarp Hapax manual](https://squarp.net/hapax/manual/)
- [OXI One MKII product and sequencing modes](https://oxiinstruments.com/oxi-one)
- [Polyend Play manual](https://polyend.com/manuals/play/)
- [Novation Circuit Tracks user guide](https://userguides.novationmusic.com/hc/en-gb/articles/26207128061458-Using-Circuit-Tracks-Synths-MIDI-Tracks-and-Drums)
- [Arturia KeyStep Pro product specification](https://www.arturia.com/store/hybrid-synths/keystep-pro)
- [Torso T-1 technical specification](https://torsoelectronics.com/t1/technical-specifications)

These sources establish that the comparison features exist in representative
hardware sequencers. They do not establish that copying every feature or
interaction would fit SHR-DAW. The ordering above is an SHR-DAW product decision
based on its current code, compact controller workflow, Raspberry Pi limits,
and the owner's stated priorities.
