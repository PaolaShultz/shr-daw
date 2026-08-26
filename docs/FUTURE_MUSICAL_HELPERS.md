# Future musical sketch helpers

Created: 2026-08-02

Status: FT2 Edit SIZE, read-only HARMONY, and the bounded Priority 5/6
Generator implemented; remaining roll and Arrangement helpers are
owner-directed proposals, not scheduled promises

This plan covers small, inspectable helpers that turn a short musical idea into
editable FT2 material. The helpers should remove repetitive work, not decide
what the song is. Every generated result must be previewable, deterministic,
and accepted by the musician before it replaces Project data.

## Product boundary

The first useful system is deliberately not a general AI composer. It works
with the Project already in memory and produces ordinary Patterns,
Arrangement references, cells, velocities, gates, and commands. That keeps the
result understandable and editable with the existing tracker.

The shared interaction should be:

1. choose the current Pattern, page, lane, or Arrangement as the explicit
   scope;
2. choose one named operation and a small number of musical parameters;
3. build a draft without changing playback or the saved Project;
4. show the affected row/event count and any data that would be replaced;
5. preview only on an explicit Preview command;
6. Apply, apply to a new clone, or Cancel; and
7. persist the concrete result, not a hidden generator that changes on every
   playback.

Randomized helpers must store or display their seed. Running the same helper
with the same input, settings, and seed must produce the same cells. Defaults
should favor `NEW CLONE` or `EMPTY ONLY`; replacing existing notes is a
separate explicit choice.

Priority 5 now owns the implemented shared draft plus selected-lane Euclidean,
accumulator, seeded-mutation, and controlled-FILL tools. Its exact scope,
collision policy, Apply/Clone ownership, persistence, and acceptance matrix are
in [Priority 5 deterministic generative tools](DETERMINISTIC_GENERATIVE_TOOLS_ACCEPTANCE.md).
Priority 6 extends that same draft with the bounded arpeggio, Project-key triad,
and diatonic harmonizer semantics in [Priority 6 arpeggio, chord, and
harmonizer generators](HARMONIC_GENERATORS_ACCEPTANCE.md). Those focused
contracts supersede the earlier fill, shared-draft, and first-arpeggio proposals
below wherever they overlap; roll and Arrangement assistance remain future
proposals.

## FT2 Edit `SIZE` page

### Existing foundation

Observed in current source:

- `Pattern.rows` is the actual duration and already validates `1..=256`;
- Project encoding stores the exact row count, so no format bump is required;
- scheduling already visits empty rows and loops at the final Pattern row;
- every Pattern row spans every page and lane, so size is Pattern-wide rather
  than page-local;
- Arrangement steps reference a Pattern record, so changing that Pattern
  changes every reference without rewriting the order;
- Pattern-owned Loop Mix settings survive the existing resize path; and
- current setup overlays already offer row counts through 256, and Edit now
  owns the quick structural SIZE operations below.

The existing post-competition `LENGTH` proposal used a separate transactional
editor. The owner-supplied `SIZE` motion below supersedes that Part 1 surface;
the later microtiming, swing, groove, and meter phases in that plan remain
useful.

### Menu placement

The controller has exactly four directly selectable menu pages and `SYS` must
remain the fourth. The recommended Edit pages are therefore `EDIT`, `SET`,
`SIZE`, and `SYS`. `SIZE` replaces Edit's duplicate `SELECT` page: PAGE and
ROUTE remain one Exit away on normal FT2 `SELECT`, and leaving Edit preserves
the Pattern/page/lane/column/cursor.

| SIZE item | Result |
|---|---|
| `HALF` | Keep one half and make it the complete Pattern |
| `ROW-` | Delete the current row and shift later rows up |
| `ROW+` | Insert one empty row after the cursor |
| `DOUBLE` | Append an empty or copied second half |

The labels fit the established four-button row. Keyboard and mouse dispatch
must call the same actions.

### Exact transforms

`HALF`

- Requires an even row count of at least two. Odd lengths show `HALF NEEDS
  EVEN ROWS` and change nothing.
- Inspect all cells on all pages, not only visible notes on the selected page.
- If the Pattern has any non-default cell, open `KEEP TOP` / `KEEP BOTTOM` /
  `CANCEL` and show the number of non-default cells that each choice discards.
  This keeps the top/bottom choice explicit even when only one half is
  populated. If the Pattern is completely empty, keep the top half directly.
- Keeping top clamps the cursor to the new final row. Keeping bottom subtracts
  the removed half from the cursor so the same musical row remains selected.

`ROW-`

- Refuses at one row.
- Deletes the cursor row across every page/lane and shifts later rows upward.
- An empty row deletes immediately. A populated row requires one confirmation
  with the exact non-default-cell count.
- The cursor stays at the same numeric row, clamped only when the old last row
  was deleted. Page, lane, and column do not move.

`ROW+`

- Refuses at 256 rows.
- Inserts one all-default row after the cursor across every page/lane and
  shifts later rows downward.
- The cursor remains on the original row; a musician can press repeatedly to
  open space after the same event.

`DOUBLE`

- Refuses above 128 rows because the result would exceed 256.
- An empty Pattern doubles with empty appended rows and no prompt.
- A Pattern with any non-default cell asks `COPY NOTES` / `EMPTY HALF` /
  `CANCEL`. `COPY NOTES` appends an exact copy of all rows, pages, cells,
  velocities, gates, programs, and commands. `EMPTY HALF` appends default
  cells.
- The cursor and selected page/lane/column remain unchanged.

All four actions stop FT2 Play/REC before commitment, retain tempo, meter,
routes, page setup, lane state, Project key, effects, Loop Mix settings, and
Arrangement references, and mark the Project dirty. A failed allocation or
validation leaves the Pattern byte-for-byte unchanged. Because direct SIZE
actions do not have room for a persistent Apply/Cancel page, populated loss
must be confirmed before mutation and the normal dirty-Project guard remains
the recovery boundary.

### Suggested model helpers

Keep transformations below the UI so one set of tests owns their data rules:

```text
halve_rows(pattern, Top | Bottom) -> Pattern
remove_row(pattern, row) -> RemovedRowSummary
insert_row_after(pattern, row) -> Pattern
double_rows(pattern, Copy | Empty) -> Pattern
row_non_default_count(pattern, range) -> usize
```

Each helper should validate first and mutate a clone or a fully prepared row
vector before swapping it into the Song. UI code owns prompts, status text,
transport stop, and cursor mapping; the model helper owns exact cell movement
and the 1–256 bound.

### Focused acceptance

- Exercise lengths 1, 2, odd, even, 128, 129, 255, and 256.
- Cover the top/bottom prompt for top-only, bottom-only, and both-half data
  across multiple pages; an empty Pattern needs no prompt.
- Count Note On, Note Off, tempo, program, and other command-only cells as data.
- Verify cancellation and allocation/validation failure preserve the original.
- Verify shared Arrangement references, copy/clone/save/load, Play from cursor,
  Pattern repeat, Loop Mix restart, REC wrap, and final-row scheduling.
- Verify keyboard/controller/mouse parity and exact cursor preservation.

## Circle of fifths

Status: implemented as the read-only FT2 Tools `HARMONY` browser.

`Circle of fifths` is the normal English term; `cycle of fifths` is also used.
It orders pitch classes by perfect fifth: C, G, D, A, E, B, F♯/G♭, D♭, A♭,
E♭, B♭, F, then back to C. Clockwise major keys add sharps and
counter-clockwise keys add flats. Each major key shares its key signature with
a relative minor a minor third below.

For SHR-DAW it is useful as a compact relationship browser, not as a promise
that adjacent keys or chords automatically make a good song. The implemented
first helper remains read-only:

```text
F  <-  C  ->  G       IV <- I -> V
Dm     Am     Em       ii   vi   iii
relative: A minor     signature: no ♯/♭
```

The current Project stores a chromatic tonic plus major or natural-minor mode.
`HARMONY` derives the two fifth neighbours, relative key, parallel key, and
seven diatonic triads without new persistence. Changing the Project key
remains an explicit N00B-scale action. English/German spelling follows runtime
configuration. The reviewed enharmonic choice is SHR's existing canonical
sharp-based pitch-name table, so the browser agrees with current note and
chord labels instead of introducing a second flat/sharp policy.

Reference: [Open Music Theory: key signatures and the circle of fifths](https://open-musictheory.github.io/docs/fundamentals/key-signatures/).

## Arpeggio pattern helpers

Status: the bounded UP/DOWN/UP-DOWN/AS-LANE offline slice is implemented in the
shared Generator. The larger pattern-family catalog below remains background
for possible later owner-directed work, not current behavior.

### Deterministic pattern families

Given sorted chord tones `p[0..n)` and optional octave copies, useful patterns
are small index generators:

- `UP`: `0, 1, ... n-1`;
- `DOWN`: `n-1, ... 1, 0`;
- `UP/DOWN`: reflect at the ends without repeating them;
- `UP/DOWN HOLD`: repeat endpoints for a more stepped phrase;
- `AS PLAYED`: retain note-on order;
- `OUTSIDE IN` and `INSIDE OUT`: alternate extremes or center-neighbors;
- `STEP s`: advance `(index + s) mod n`; require `gcd(s, n) = 1` to visit
  every tone before repeating;
- `ROTATE r`: start any family at a different phase;
- `SEEDED SHUFFLE`: one reproducible permutation per cycle; and
- `EUCLIDEAN k/n`: use a maximally even onset mask while a chosen pitch family
  supplies the notes.

Octave range, gate, velocity contour, octave displacement, rests, and rhythm
are orthogonal settings. Do not bake them into dozens of opaque mode names.
The Euclidean algorithm is especially suitable for onset distribution, but it
does not supply genre, accents, pitch order, or musical taste by itself.

References: [Toussaint, “The Euclidean Algorithm Generates Traditional Musical Rhythms”](https://archive.bridgesmathart.org/2005/bridges2005-47.html)
and [Nierhaus, *Algorithmic Composition*](https://link.springer.com/book/10.1007/978-3-211-75540-2).

### First implementation

Start offline, not as another live transport owner:

1. take notes from an explicit held chord, selected cells, or Project-key
   triad;
2. choose target page/lane range, row span, subdivision, family, octave range,
   gate, velocity shape, and phase;
3. generate a draft into empty cells or a new Pattern clone;
4. report collisions and out-of-MIDI-range notes rather than silently dropping
   them; and
5. Apply concrete cells.

This reuses the scheduler, note ownership, routing, and Project persistence.
A later live arpeggiator would need its own exact Note On/Off ownership,
latency, transport, controller-clock, stop/panic, and route-change design; it
should not be inferred from the offline helper.

## Drum fills and rolls

### What exists now

The drum library already expands a one-bar seed and adds a deterministic final
four-row fill. That implementation is deliberately narrow: its rules are
hard-coded by broad label group, its scope is fixed to phrase end, and the
result is applied during library expansion rather than previewed as a separate
musician action.

Cells also support `Retrigger(1..=8)`. The scheduler divides one row into that
many pulses and preserves bounded gate/note cleanup. All pulses currently share
one velocity, so a true within-row crescendo cannot be represented by the
existing command alone.

### Feasible first helper

Add a `FILL` draft action for a selected percussion page with:

- length: one beat, half bar, or one bar;
- family: snare build, kick pickup, hat burst, sparse break, or mapped-drum
  walk;
- density and velocity direction;
- placement: ending at cursor, Pattern end, or selected Arrangement boundary;
- collision policy: `NEW CLONE` (default), `EMPTY ONLY`, or explicit `REPLACE`;
  and
- deterministic variant/seed.

Add a separate `ROLL` action for the current percussion cell or selected row
span. Same-velocity within-row rolls can use Retrigger immediately. Accented or
crescendo rolls should initially use multiple ordinary rows with explicit
velocities. A later per-pulse velocity contour would require a command/storage
extension and must not silently change old `Retrigger` meaning.

Drum roles must come from Project/configured percussion mapping or the selected
kit's declared semantics. Do not hard-code a borrowed device's note map into
Rust. Fills should never alter melodic pages, routing, kit selection, effects,
or other Patterns unless the user chooses a new clone and Arrangement insert.

### Acceptance

- Preview/cancel leaves the Pattern unchanged; Apply matches the preview.
- Collision counts are exact for every affected row and lane.
- Roll timing remains inside its row and Stop/Panic produces no stuck note.
- 1- and 8-pulse bounds, final Pattern row, odd lengths, tempo extremes, and
  disabled automatic Note Off are covered.
- Generated variants are reproducible from the recorded seed.
- Musical quality and mapped-drum identity still require an authorized human
  listening/controller pass.

## Arrangement assistance

Arrangement should be hierarchical: song → sections → phrases/bars → Pattern
steps. A useful helper asks the musician to label or choose roles such as
Intro, A/Verse, B/Chorus, Bridge/Break, and Outro, then proposes references to
existing Patterns. It should not infer semantic certainty from a few notes.

### Recommended algorithm order

1. **Templates and constraints.** Offer small forms such as `A A B A`,
   `INTRO A B A B OUTRO`, or a build/drop template. Fit existing Pattern
   lengths, require a selected anchor Pattern, cap total bars, and show the
   result before Apply. This is transparent and cheap.
2. **Feature-based variation.** Measure symbolic density, active pages/lanes,
   pitch register, velocity, drum activity, and repetition. Suggest mute,
   thinning, octave, fill, or clone operations to make section energy differ.
3. **Dynamic programming or constraint search.** Score candidate section
   sequences for requested length, contrast, reuse, transition cost, and a
   final cadence while satisfying hard limits. Keep the cost breakdown visible.
4. **Grammar.** A small hierarchical grammar can expand `SONG` into sections
   and phrases while preserving long-range repetition. This is a better fit
   for explainable macro form than a note-level Markov chain.
5. **Markov/learned models only later.** They can suggest local transitions,
   but need licensed training data, provenance, style/copying safeguards, and
   stronger long-range controls. They are not needed for the first useful
   assistant.

Arrangement under musical constraints can become computationally hard, so the
product needs bounded candidates rather than an unbounded “best song” search.
References: [Moses and Demaine, “Computational Complexity of Arranging Music”](https://arxiv.org/abs/1607.04220)
and [Marmoret, Cohen, and Bimbot, barwise hierarchical structure and dynamic programming](https://arxiv.org/abs/2311.18604).

### Safe Arrangement result

The first assistant should output an Arrangement draft made from existing
Pattern references plus explicitly named clones. It shows total bars/rows,
section labels, every cloned or transformed Pattern, and which original
Patterns remain untouched. `APPEND`, `REPLACE`, and `CANCEL` are distinct;
Replace uses the existing dirty-Project protection. Preview follows the draft
without committing it, and failure returns to the exact prior order/Pattern/
page/lane/column/cursor.

## Implementation sequence

| Phase | Work | Why first/next |
|---|---|---|
| 1 (implemented) | FT2 Edit SIZE model helpers and page | Immediate manual sketch speed; storage already supports it |
| 2 (implemented) | Shared generated-draft, collision summary, retained seed, inspect, Apply/Clone/Cancel, plus bounded Euclidean, accumulator, mutation, and FILL tools | One non-writing draft and explicit transaction owners for the first deterministic helpers |
| 3 (implemented) | Read-only circle-of-fifths/HARMONY browser | Useful theory support with no generated-data risk |
| 4 (implemented) | Offline arpeggio plus Project-key chord and harmonizer generators | Small deterministic algorithms over existing cells in the shared draft |
| 5 (partial) | Controlled FILL implemented; roll drafts remain future work | FILL uses ordinary conditional cells; rolls still need their own bounded contract |
| 6 | Template/constraint Arrangement assistant | Builds on clone/variation operations and explicit previews |

Each phase is independently shippable. None should change the audio callback,
start hardware, or require a Project-format bump unless a later retrigger
velocity contour or new harmony metadata is deliberately adopted.

## Remaining open owner decisions

- Should a future roll draft use the current Generator's `EMPTY ONLY`
  current-Pattern default, or default to an independent clone?
- Which two or three Arrangement templates match the musician's actual sketch
  workflow? Start with those, not a large genre menu.
