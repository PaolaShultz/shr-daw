# Priority 5 deterministic generative tools

Created: 2026-08-26

Status: implemented and software-validated through ROLL on 2026-09-02

This document owns the bounded Priority 5 contract selected in
[Sequencer workflow priorities](SEQUENCER_WORKFLOW_PRIORITIES.md). The first
version is an offline, selected-lane draft workflow. It creates ordinary FT2
cells and never becomes another transport, scheduler, or hidden playback mode.

## Shared workflow and ownership

- FT2 Tools **PAGE** -> **HISTORY** -> **RHYTHM** -> **GEN** opens the Generator draft
  for the current Pattern, page, lane, and cursor row. The row span starts at
  that cursor and is bounded by the Pattern end; length is `1..=256` and can
  never exceed the remaining rows.
- Opening, browsing tools, changing any setting or seed, inspecting the draft,
  using Repeat, and leaving with Cancel change only runtime draft state. They
  do not alter the Song, dirty baseline, Pattern History, Arrangement,
  transport, page/lane/column/cursor, routing, or audio ownership.
- The 40x13 screen shows the source Pattern/page/lane, row span, complete
  settings, retained seed when used, exact affected/replacement/collision/
  protected counts, and the affected row numbers. This is the inspectable
  draft; there is no audition or automatic playback command in this pass.
- The existing four controller pages are **SHAPE**, **DETAIL**, **VALUE**, and
  **APPLY**. They expose tool/length/amount, tool-specific offset or phase,
  mutation density, Roll shape/depth, collision policy,
  the context-named value down/up (the seed for Mutation/Fill), Repeat/Inspect,
  and Stop/Apply/Clone/Cancel. Keyboard, mouse,
  four-button page selection, five-button page cycling, and eight-button direct
  selection dispatch the same actions. No fifth page or hidden controller mode
  is added.
- Apply to the current Pattern requires stopped transport and swaps the exact
  validated draft into that Pattern through one Pattern History transaction.
  ROLL's visible NEW CLONE policy routes Apply to Clone instead; choosing EMPTY
  ONLY or REPLACE NOTE restores the ordinary current-Pattern Apply meaning.
  Apply to Clone also requires stopped transport and uses `Song::append_pattern`
  exactly like the existing Clone operation: the source Pattern remains
  untouched, one independent Pattern and one explicit final Arrangement step
  are added, and structural work stays outside Pattern History. The clone is
  selected while row/page/lane/column remain unchanged.
- Cancel, transport refusal, missing source, invalid scope, validation failure,
  structural refusal, and an unchanged Apply leave History, dirty state,
  Pattern data, and Arrangement byte-for-byte unchanged. A refused commit keeps
  the draft for recovery. Successful Apply and Clone retain the recipe and seed
  in runtime so **REPEAT** can rebuild it against the current source; replacing
  the Project clears that runtime memory.

## Draft data and collision policy

- A proposal is one exact `(row, lane, Cell)` write. The draft is a full cloned
  Pattern with accepted proposals applied, plus a report. `affected` counts
  writes that differ from the source, `replacements` counts those writes over a
  non-default note cell, `collisions` counts skipped proposals, and `protected`
  is the collision subset that Replace is never allowed to overwrite.
- **EMPTY ONLY** is the default. It accepts only default destination cells.
  **REPLACE NOTE** may replace a `Note::On` cell only when its command is
  `None`; Note Off and every command-bearing cell are protected. Identical
  proposals are no-ops, not affected cells or collisions.
- ROLL alone opens with **NEW CLONE** because its source cell is necessarily an
  existing note. It previews with the same replaceable-note rules as REPLACE
  NOTE, but Apply delegates to the existing independent Clone transaction.
  Cycling policy makes EMPTY ONLY and REPLACE NOTE explicit current-Pattern
  choices; leaving ROLL restores the ordinary EMPTY ONLY default.
- Seeded Mutation is deliberately different: it changes only eligible existing
  `Note::On` cells whose command is `None`. Empty, Note Off, and command-bearing
  cells are protected and never become mutation targets. Its selected changed
  cells are reported as replacements; collision policy does not broaden its
  scope.
- Draft generation validates all bounds before returning a draft. Candidate
  order and reporting are stable row order. Identical Pattern data, selection,
  settings, and seed produce equal Patterns and byte-identical canonical
  Project encodings after Apply.

## Musical semantics and bounds

All note-producing tools take their source note from the selected cursor cell.
That cell must contain `Note::On`; its note, optional velocity, program, and
gate form the template. Generated triggers deliberately reset command, timing,
probability, and condition to `None`, on-grid, 100%, and ALWAYS, except that a
controlled Fill is explicitly FILL-only and EVEN Roll uses the existing
Retrigger command when needed. The page default velocity is used when the
source has no explicit velocity.

### Euclidean

- Length is the number of one-row steps and amount is `0..=length` pulses.
- Step `i` is active when `((i * pulses) mod length) < pulses`; rotation moves
  that finite mask right by `0..length-1`. This is the repository's exact
  maximally-even/Bresenham convention and always produces the requested pulse
  count, including zero and full density.
- Seed and mutation density do not participate. The collision policy decides
  whether each active proposal is accepted.

### Accumulator

- Every row in the span receives one proposed note. The source note is the
  accumulator reset value. Amount is a wrap span of `1..=48` semitones; offset
  is the signed increment `-12..=12`; phase is the initial accumulator
  phase `0..length-1`.
- Positive increments wrap in `[source, min(127, source + span)]`; negative
  increments wrap in `[max(0, source - span), source]`; zero repeats the source.
  Modular arithmetic is applied before the MIDI note is written, so every
  draft remains in `0..=127`. Opening or repeating the tool always resets from
  the source and phase; it has no cross-run hidden state.

### Seeded Mutation

- The span is scanned in row order. Density is `0..=100%`. A stable integer
  mixer keyed by seed, row, and lane selects eligible cells; a second stable
  draw chooses a non-zero pitch delta within amount `1..=12` semitones from the
  in-range candidates.
- Only the MIDI note changes. Velocity, program, gate, timing, probability,
  condition, and every other cell field remain exact. The same source and seed
  repeat exactly; a seed change deterministically produces another selection
  or delta sequence without editing the Pattern.

### Controlled Fill

- Fill requires the selected page to be percussion. Length is the selected
  phrase-end span and amount is `0..=length` hits. A stable seeded permutation
  selects exactly that many distinct rows; phase rotation moves the finite selection
  within the span.
- Each accepted trigger copies the source note template, uses a bounded linear
  build from the source/page velocity toward 127 across selected hit order, and
  stores `condition=FILL`. It therefore follows the existing Pattern-pass FILL
  latch, PRE ownership, probability ordering, note cleanup, and next-boundary
  semantics; generation itself never arms FILL.
- Fill never writes another lane or melodic page and never changes Pattern or
  Arrangement duration.

### Roll

- Roll requires a percussion page and an existing note at the selected cell.
  Length is a bounded span beginning at the cursor; amount is exactly `1..=8`
  total pulses. Pulse rows are distributed deterministically from the first to
  the last row of the span. When there are more pulses than rows, EVEN divides
  them across those rows with ordinary one-pulse notes or existing
  `Retrigger(2..=8)` commands, so every pulse remains inside its owning row.
- EVEN keeps the source/page velocity. A one-row EVEN roll is the direct
  within-row case: one pulse is an unchanged ordinary note and two through
  eight pulses store `Retrigger(2..=8)`. No per-pulse contour is invented.
- ACCENT and CRESCENDO use one ordinary trigger per selected pulse row and
  therefore never store Retrigger. ACCENT alternates the source velocity with
  a quieter velocity. CRESCENDO rises linearly from the quieter velocity to
  the source velocity. Depth is `1..=63`; both endpoints remain in `1..=127`.
  Shaped pulse count is bounded by the selected row span because the current
  Cell schema cannot represent different velocities inside one row.
- Roll copies only the selected percussion note template and resets timing,
  probability, and condition exactly like the other note-producing tools. It
  never changes drum mapping, kit, routing, effects, Pattern length, or another
  lane. Identical settings repeat exactly; the retained seed is shown but not
  used because this first Roll contract has no random choice.

## Persistence, reuse, export, and playback

- Priority 5 and ROLL add no persisted recipe or seed and therefore do not
  change current Project format 18 or reusable drum-pattern format 4. Existing
  migrations remain unchanged; inspecting or loading never rewrites a file.
- Apply persists concrete ordinary Cells through the existing Project encoder.
  Apply to Clone persists the same cells in an independent existing Pattern
  record. Save/load/clone/copy/paste and repeated playback do not regenerate
  anything.
- Saving a reusable drum pattern copies generated percussion cells, including
  FILL conditions, ROLL Retrigger commands, timing, probability, and lane-cycle
  data through the existing format. Loading copies those cells through its
  existing selected-page ownership; no recipe, seed, routing, kit, effect, or
  Arrangement state is smuggled into the reusable file.
- Context-free MIDI export remains deterministic pass 1 with FILL off, so
  FILL-only generated triggers are absent from that export. Preflight retains
  its existing all-source-trigger scan, including generated conditional cells.
  Partial and repeated playback use the stored cells and the Priority 2-4
  scheduler without a generator callback or new note owner.

## Acceptance matrix

| ID | Required evidence |
|---|---|
| GT-01 | Euclidean lengths 1 and 256, zero/full/intermediate pulse counts, rotation, scope bounds, exact placement, and repeatability. |
| GT-02 | Accumulator positive/negative/zero increments, reset, phase, upper/lower MIDI wrap, length bounds, and repeatability. |
| GT-03 | Mutation seed stability and change, 0/100/intermediate density, lane/span scope, pitch range, field preservation, and protected cells. |
| GT-04 | Fill exact seeded selection, seed/rotation change, velocity bound, percussion refusal, Pattern bounds, and stored FILL interaction. |
| GT-05 | Draft reports exact affected rows, replacements, collisions, protected collisions, and identical drafts/encodings for identical inputs. |
| GT-06 | Opening, every adjustment, seed change, Repeat, Cancel, refusal, validation failure, and no-op Apply preserve Pattern History, dirty state, cursor, and structural state. |
| GT-07 | Current Apply is exactly one undoable/redoable History transaction; source/draft equality gives no transaction. |
| GT-08 | Apply to Clone leaves the source exact, uses one independent appended Pattern/Arrangement step, preserves cursor fields, and rolls back on structural refusal. |
| GT-09 | Current Project format 18 and legacy migration round trip generated cells without a rewrite-on-load or generator schema change. |
| GT-10 | Reusable drum-pattern format 4 round trips generated timing/probability/FILL cells and lane settings; loading changes only the selected percussion page. |
| GT-11 | Probability, conditions, PRE/FILL, microtiming, swing, groove, REC FEEL, and independent lane cycles retain their Priority 2-4 behavior. |
| GT-12 | Scheduler ownership, note cleanup, export, preflight, partial playback, and repeated playback consume only stored cells. |
| GT-13 | Navigation exposes GEN and all actions through exactly four pages on supported controller layouts; native 40x13 rendering preserves the shared status row and FT2 cursor. |
| GT-14 | Keyboard, mouse, controller, Apply/Clone/Cancel, refusal, and return paths share the same UI transaction owners. |
| GT-15 | ROLL covers one/eight pulses, one-row Retrigger, odd/multi-row distribution, final-row bounds, percussion refusal, explicit ACCENT/CRESCENDO velocities, and byte-stable repeatability. |
| GT-16 | ROLL opens with NEW CLONE, Apply delegates to the existing structural owner, current-Pattern policies remain explicit, reports stay exact, and source/Arrangement/cursor/History owners survive every refusal or Cancel. |

## Evidence limits

The 2026-09-02 authorized non-Raspberry-Pi software pass used exact Rust 1.97.1.
Locked check, the complete 117-test generator-related filter, four exact ROLL
regressions, and the complete normal suite passed. The final suite reported
1,114 passed, zero failed, and 13 documented ignored development,
private-audition, and performance tests. This covers GT-01 through GT-16,
including draft-only behavior, bounded ROLL generation, controller routing,
Clone ownership, and rollback on refusal. A later full build pass repeated the
locked check and the same complete normal-suite result. Clippy was not required
by an observed failure or repository policy.

Software tests can prove deterministic drafts, storage, transactions,
navigation, rendering geometry, scheduling, export, and preflight without
starting audio or MIDI. Musical usefulness, physical-controller feel,
Raspberry Pi timing/headroom, listening, and live hardware behavior remain
separate human/hardware acceptance and must not be inferred from this pass.
