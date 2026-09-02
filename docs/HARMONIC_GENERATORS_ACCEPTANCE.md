# Priority 6 arpeggio, chord, and harmonizer generators

Created: 2026-08-26

Status: software combined pass completed 2026-08-26

This document owns the first bounded Priority 6 contract selected in
[Sequencer workflow priorities](SEQUENCER_WORKFLOW_PRIORITIES.md). It extends
the existing Priority 5 `GENERATOR`; it does not add another generator,
transport owner, playback mode, or hidden recipe.

## Shared workflow and ownership

- FT2 Tools **PAGE** -> **HISTORY** -> **RHYTHM** -> **GEN** remains the only
  launcher. **ARP**, **CHORD**, and **HARMONIZER** join the existing Euclidean,
  accumulator, mutation, and controlled-FILL tools.
- The current Pattern, page, lane, and cursor row are captured as the explicit
  source and placement context when the Generator opens. Generator controls do
  not move that FT2 cursor.
- Opening, browsing, adjusting, inspecting, repeating, or cancelling changes
  only the visible runtime draft. Pattern cells, automation, Arrangement,
  Pattern History, the clean baseline, dirty state, transport, routing, and
  structural state remain unchanged.
- The Generator keeps exactly four controller pages. **SHAPE** owns tool,
  length down/up, and the tool's primary amount. **DETAIL** owns the two
  directions of the secondary value, one bounded control, and collision
  policy. **VALUE** owns the final bounded value down/up, Repeat, and Inspect.
  **APPLY** owns Stop, Apply, Clone, and Cancel. Four-button page selection,
  five-button page cycling, eight-button direct page selection, keyboard, and
  mouse all dispatch those same actions. There is no fifth page.
- The body always names the source, target, complete musical settings, exact
  affected/replacement/collision/protected counts, any out-of-scale skips, and
  affected rows. A refusal names its reason and count where applicable while
  keeping the source unchanged.
- Apply to the current stopped Pattern swaps the exact inspected draft through
  one Pattern History transaction. An equal/no-op draft creates no History
  entry and does not dirty the Project.
- Apply to Clone requires stopped transport and uses the existing independent
  Pattern plus appended Arrangement-step structural owner. It never overwrites
  the source Pattern and never enters Pattern History.
- A refused commit keeps the draft available. Cancel, stopped-transport
  refusal, source/range/scope validation failure, structural failure, and
  no-op Apply leave Pattern History, Project dirty state, Pattern data,
  Arrangement, and cursor context unchanged.

## Shared cell and collision rules

- A proposal is one exact `(row, lane, Cell)` write into a cloned Pattern.
  Candidate order is stable row then lane order. `affected` counts changed
  cells, `replacements` counts accepted writes over existing note triggers,
  `collisions` counts refused destination writes, and `protected` is the
  collision subset which Replace may never overwrite.
- **EMPTY ONLY** remains the default. **REPLACE NOTE** may replace only an
  existing `Note::On` cell with no command. This is the explicit permission to
  replace that complete destination cell, including its velocity, gate,
  probability, condition, and microtiming. Note Off and every command-bearing
  destination remain protected and are reported.
- Identical proposals are no-ops. Pattern automation is never a proposal
  target and is preserved byte-for-byte.
- Draft construction validates the complete source, MIDI range, destination
  lanes, and Pattern-end placement before accepting any proposal. It never
  extends Pattern or Arrangement duration and never returns a partial draft
  after a range or scope refusal.

## Deterministic arpeggio

### Source and ordering

- The explicit source is every `Note::On` cell on the selected page at the
  cursor row, read in lane order. At least one note is required. Equal MIDI
  pitches collapse to the first lane's source cell; other source cells are
  never changed.
- **AS LANE** retains that first-occurrence lane order. **UP** sorts pitches
  low to high, **DOWN** sorts high to low, and **UP/DOWN** traverses low to high
  then back through the interior without repeating either endpoint. One- and
  two-note sources have the corresponding bounded sequence.
- The octave count is `1..=3`. Octave one is the source register; later
  octaves add 12 semitones to each source tone before ordering. If any required
  copy exceeds MIDI 127, the complete draft is refused with the exact range
  refusal count.

### Placement, gate, and repetition

- Rate is exactly 1, 2, 4, or 8 rows per generated step. The source row is
  inspectable input, not an implicit destination: the first output is at
  `source row + rate` in the selected lane.
- Length means `1..=8` complete repetitions. Each repetition restarts the same
  finite ordered family from its first note. No state crosses draft rebuilds,
  Pattern playback passes, or Pattern repeats.
- Every required output row must exist. If the last output would exceed the
  current Pattern, the complete draft is refused; Pattern duration is not
  changed and a partial final repetition is not produced.
- Gate is explicit at 25, 50, 75, or 100 percent. A generated trigger retains
  its source tone's resolved velocity and optional program, uses the selected
  gate, and resets command, microtiming, probability, and condition to None,
  on-grid, 100%, and ALWAYS.

## Diatonic chord generator

### Scale, degree, register, and quality

- The source is the Project's explicit chromatic tonic plus MAJOR or natural
  MINOR scale. Degree is `1..=7`; quality is not guessed or separately
  overridden. The generator takes scale degrees `degree`, `degree + 2`, and
  `degree + 4`, wrapping diatonically into the next octave, so major, minor,
  and diminished triads follow the stored Project scale exactly.
- Register comes from the selected cursor cell when it is `Note::On`: its
  twelve-note C register owns the generated root. With no selected note, the
  deterministic fallback is the C4 register beginning at MIDI 60. The screen
  identifies whether the cell or fallback supplied the register.
- Inversion is ROOT, FIRST, or SECOND. First inversion raises the lowest close
  voice by one octave; second inversion raises the two lowest close voices.
- **CLOSE** assigns the resulting three ascending tones. **OPEN** raises the
  middle close-position voice one octave and re-sorts the three voices. This
  is one exact, bounded open-triad rule rather than a broad voicing engine.
- Any required pitch above MIDI 127 or below MIDI 0 refuses the complete draft
  with the exact range refusal count.

### Lane and row scope

- The selected lane is voice 1 and the next two lanes are voices 2 and 3 on
  the same page. Starting on lane 3 or 4 is refused; allocation never wraps to
  another page and never moves the cursor.
- Length means `1..=8` chord repetitions. Rate is exactly 1, 2, 4, or 8 rows;
  repetition `n` is placed at `cursor row + n * rate`. The complete placement
  must fit the current Pattern.
- Each voice is an ordinary default trigger with the page velocity. It has no
  cell program or command, inherits the Project gate, is on-grid, has 100%
  probability, and is ALWAYS. Existing destination data is handled only by
  the shared collision policy.

## Bounded diatonic harmonizer

- The source is the selected lane from the cursor row through a `1..=256` row
  span bounded by the Pattern end. The target is one explicitly selected other
  lane on the same page; selecting the source lane as target is refused.
- Interval is a diatonic THIRD or FIFTH in the Project's stored MAJOR or
  natural MINOR scale. Voice is ABOVE or BELOW. For an in-scale note, the
  generator walks exactly two or four scale steps in that direction while
  retaining octave position.
- Out-of-scale policy is explicit. **REFUSE** counts every out-of-scale source
  trigger and refuses the complete draft. **SKIP** leaves those source rows
  unproposed and reports the exact skipped count. It never silently snaps a
  chromatic note to another pitch.
- If any required in-scale harmony voice falls outside MIDI 0..=127, the
  complete draft is refused with the exact range refusal count.
- Every source `Note::On` proposal copies the complete Cell and changes only
  its MIDI note. Velocity, program, gate, command, microtiming, probability,
  and condition therefore remain exact. Source Note Off cells are copied
  exactly to the target lane so the generated voice retains explicit cleanup.
  Empty and command-only source cells create no proposal. Source cells are
  never changed.

## Persistence, reuse, export, and playback

- Priority 6 adds no persisted recipe, mode, or seed. At this feature's
  baseline it did not change Project format 17 or reusable drum-pattern format
  4. Old Projects retain
  their current in-memory migrations and are not rewritten by inspection.
- Apply stores only ordinary Cells. Save/load, Pattern clone/copy/paste,
  reusable drum-pattern boundaries, partial playback, repeated playback, and
  Arrangement reuse consume those cells without calling the Generator.
- Melodic Priority 6 cells do not enter reusable drum-pattern files. Existing
  percussion Pattern data and Priority 5 FILL cells retain format-4 behavior.
- MIDI export and preflight see the concrete generated cells through their
  existing all-source and deterministic-pass owners. Probability, conditions,
  PRE/FILL, swing, groove, REC FEEL, lane cycles, mutation seeds, controlled
  fills, scheduler note ownership, and cleanup are not reinterpreted.
- The existing read-only **HARMONY** browser remains unchanged and separately
  launched from FT2 Tools PAGE. `GEN` names its current Project key in CHORD and
  HARMONIZER drafts; it does not turn HARMONY into an editor or add a second
  launch path.

## Acceptance matrix

| ID | Required evidence |
|---|---|
| HG-01 | Arpeggio source extraction covers one note, four-lane chords, duplicate pitches, lane order, missing source, and non-writing refusal. |
| HG-02 | UP, DOWN, UP/DOWN, and AS LANE order; one through three octaves; 1/2/4/8-row rate; 25/50/75/100 gate; complete repetitions; repeatability; MIDI and Pattern-end refusal. |
| HG-03 | Chord degree and quality across every major/minor degree; source/fallback register; all inversions; CLOSE/OPEN voicing; selected three-lane allocation; MIDI bounds. |
| HG-04 | Chord rate/repetition placement, Pattern-end refusal, collision/replacement/protected behavior, and byte-identical repeated drafts. |
| HG-05 | Harmonizer THIRD/FIFTH, ABOVE/BELOW, target lane, scale walking, REFUSE/SKIP policy, MIDI-range refusal, source scope, Note Off copying, and complete field preservation. |
| HG-06 | Drafts report exact affected cells/rows, replacements, collisions, protected cells, out-of-scale skips, and counted range refusals. |
| HG-07 | Open, every parameter change, Inspect, Repeat, Cancel, refusal, validation failure, and no-op Apply preserve Pattern, Arrangement, History, dirty state, cursor, and automation. |
| HG-08 | Current Apply is exactly one Pattern History transaction with exact Undo/Redo; Clone leaves the source exact and uses one independent Pattern plus appended Arrangement step. |
| HG-09 | Baseline Project format 17 and reusable drum-pattern format 4 remain unchanged by these generators; ordinary generated cells round-trip and old formats retain existing migration behavior. |
| HG-10 | Priority 2-5 timing, probability, conditions, PRE/FILL, swing, groove, REC FEEL, lane cycles, seeded mutation, and controlled fill retain their exact behavior. |
| HG-11 | Scheduler ownership, Note Off/cleanup, preflight, MIDI export, partial playback, Pattern repeat, and repeated Arrangement references consume stored cells only. |
| HG-12 | GEN and every new parameter remain reachable on exactly four pages for four-, five-, and eight-control layouts; keyboard/mouse share dispatch; native 40x13 rendering preserves the shared status row and FT2 cursor. |
| HG-13 | The read-only HARMONY browser remains read-only, separately launched, and context-preserving. |

## Evidence limits

The authorized non-Raspberry-Pi software pass used exact Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6). Formatting and
locked check passed without warnings. The HG-01 through HG-13 focused model,
draft/refusal, Pattern History, clone, Project/drum migration, Priority 2-5,
scheduler/preflight/partial/repeated-playback, MIDI export, navigation,
four-controller-layout, native 40x13, shared-status, and HARMONY-preservation
tests passed. The complete normal suite then passed with 1,047 successful
tests, zero failures, and 13 documented development, private-audition, and
performance tests ignored. Clippy was not required by an observed failure or
repository policy.

Software tests can prove deterministic drafts, ordinary-cell persistence,
transaction boundaries, navigation, rendering geometry, scheduling, export,
and preflight without opening audio or MIDI devices. Musical usefulness,
physical-controller feel, Raspberry Pi timing/headroom, listening, and live
hardware behavior require a separate authorized human/hardware pass and must
not be inferred from software evidence.
