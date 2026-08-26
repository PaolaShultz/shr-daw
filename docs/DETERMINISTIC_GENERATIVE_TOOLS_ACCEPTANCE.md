# Priority 5 deterministic generative tools

Created: 2026-08-26

Status: software combined pass completed 2026-08-26

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
  mutation density, collision policy,
  the context-named value down/up (the seed for Mutation/Fill), Repeat/Inspect,
  and Stop/Apply/Clone/Cancel. Keyboard, mouse,
  four-button page selection, five-button page cycling, and eight-button direct
  selection dispatch the same actions. No fifth page or hidden controller mode
  is added.
- Apply to the current Pattern requires stopped transport and swaps the exact
  validated draft into that Pattern through one Pattern History transaction.
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
controlled Fill is explicitly FILL-only. The page default velocity is used
when the source has no explicit velocity.

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

## Persistence, reuse, export, and playback

- Priority 5 adds no persisted recipe or seed and therefore does not change
  Project format 17 or reusable drum-pattern format 4. Formats 0-16 retain
  their existing migrations; inspecting or loading never rewrites a file.
- Apply persists concrete ordinary Cells through the existing Project encoder.
  Apply to Clone persists the same cells in an independent existing Pattern
  record. Save/load/clone/copy/paste and repeated playback do not regenerate
  anything.
- Saving a reusable drum pattern copies generated percussion cells, including
  FILL conditions, timing, probability, and lane-cycle data through the
  existing format. Loading copies those cells through its existing selected-page
  ownership; no recipe, seed, routing, kit, effect, or Arrangement state is
  smuggled into the reusable file.
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
| GT-09 | Project format 17 and legacy migration round trips generated cells without a rewrite-on-load or schema change. |
| GT-10 | Reusable drum-pattern format 4 round trips generated timing/probability/FILL cells and lane settings; loading changes only the selected percussion page. |
| GT-11 | Probability, conditions, PRE/FILL, microtiming, swing, groove, REC FEEL, and independent lane cycles retain their Priority 2-4 behavior. |
| GT-12 | Scheduler ownership, note cleanup, export, preflight, partial playback, and repeated playback consume only stored cells. |
| GT-13 | Navigation exposes GEN and all actions through exactly four pages on supported controller layouts; native 40x13 rendering preserves the shared status row and FT2 cursor. |
| GT-14 | Keyboard, mouse, controller, Apply/Clone/Cancel, refusal, and return paths share the same UI transaction owners. |

## Evidence limits

The authorized non-Raspberry-Pi software pass used exact Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6). Locked check,
the GT-01 through GT-14 focused algorithm/migration/History/scheduler/export/
preflight/navigation/controller/rendering transaction matrices, and the
complete normal suite passed. The final suite reported 1,038 passed, zero
failed, and 13 documented ignored development, private-audition, and
performance tests. Focused validation shortened only the RHYTHM launcher from
`GENERATE` to `GEN` so it meets the established 40-column soft-button width;
the entered screen remains `GENERATOR`. Clippy was not required by an observed
failure or repository policy.

Software tests can prove deterministic drafts, storage, transactions,
navigation, rendering geometry, scheduling, export, and preflight without
starting audio or MIDI. Musical usefulness, physical-controller feel,
Raspberry Pi timing/headroom, listening, and live hardware behavior remain
separate human/hardware acceptance and must not be inferred from this pass.
