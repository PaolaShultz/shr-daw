# A A B A Arrangement assistant

Created: 2026-09-02

Status: implemented and validated

This document owns the smallest bounded Phase 6 Arrangement-assistance slice
from [Future musical sketch helpers](FUTURE_MUSICAL_HELPERS.md). It adds one
transparent template and no general composer, Pattern generator, playback
owner, or persisted recipe.

## Workflow and ownership

- ARRANGE **FORM** -> **AABA** captures the Pattern referenced by the selected
  Arrangement step as `A`. It does not move the FT2 cursor or change transport.
- `B` initially has no value. Encoder turn or **B-**/**B+** explicitly chooses
  one ID from the Project's existing sorted Pattern IDs; the assistant never
  infers contrast, role, similarity, or musical meaning.
- The fixed draft is exactly `A A B A`. It lists all four section/reference
  pairs, each resolved Pattern row count, four total steps, the exact summed
  rows, and any unset, missing, or invalid Pattern.
- Draft construction and browsing are runtime-only. They do not write Song,
  Arrangement, Pattern data, automation, Pattern History, clean baseline,
  dirty state, routing, transport, or FT2 cursor/context.
- With transport stopped, **APPEND** adds all four references after the current
  Arrangement as one validated order transaction. **REPLACE** validates the
  same four references and invokes the existing unsaved-Project guard before
  replacing only `Song.order` when the Project is dirty. **CANCEL** and Exit
  restore the opening Arrangement selection and controller page state.
- The transaction owner prepares and validates the complete prospective order
  before swapping it. It never clones, edits, creates, or deletes a Pattern.
  There is no Project-format change and playback consumes only the resulting
  ordinary Pattern references.
- Missing/unset Patterns, an invalid Pattern, append overflow, Project
  validation failure, active transport, guard Back, Cancel, and allocation
  failure retain the exact prior Arrangement, Pattern data, History, dirty
  state, transport, and FT2 context. A refused draft remains visible for
  correction.

## Controller and keyboard

The existing ARRANGE screen keeps four pages. Its previously empty third page
now contains the one **AABA** launcher. While the draft is open, the same screen
uses exactly four contextual pages: **FORM** with B-/B+, **APPLY** with
Stop/Append/Replace/Cancel, one empty page, and canonical **SYS** with
Panic/Help/Exit. Four-button selection, five-button cycling, eight-button
direct selection, pointer input, and the shared dispatcher use the same action
table. Keyboard `F` opens the assistant, Left/Right selects B, `A` appends, `R`
requests replacement, and `C`, `B`, or Esc cancels.

## Acceptance matrix

| ID | Required evidence |
|---|---|
| AA-01 | Opening captures the selected Arrangement Pattern as A, leaves B unset, and writes nothing. |
| AA-02 | Explicit B selection walks only existing sorted Pattern IDs and deterministically produces A A B A. |
| AA-03 | The native draft renders every section/reference, per-Pattern rows, four total steps, exact total rows, and unset/missing/invalid state without touching shared row 13. |
| AA-04 | APPEND adds exactly four references after the prior Arrangement in one order-only transaction; Pattern objects and History remain exact. |
| AA-05 | REPLACE produces exactly A A B A only after the existing dirty guard; guard Back keeps the full draft and prior Project/context. |
| AA-06 | CANCEL/Exit restores the opening Arrangement selection, controller page/mode, Song, clean baseline, dirty state, History, transport, and FT2 cursor. |
| AA-07 | Missing/unset/invalid Patterns, maximum-step overflow, Project validation failure, and active transport refuse without a partial write or transport change. |
| AA-08 | The ARRANGE launcher and contextual actions remain reachable through exactly four controller pages and the shared keyboard/controller/mouse dispatcher. |
| AA-09 | Project schema and Pattern data are unchanged; save/load, scheduling, preflight, and export continue to consume ordinary Arrangement references only. |

## Validation evidence

The combined software pass used exact Rust 1.97.1. Formatting and locked check
passed. The focused Arrangement assistant filter passed all 10 matching model,
transaction, controller-routing, and UI regressions. The requested ROLL filter
also passed, although its broad `roll` substring matched 117 tests; the four
ROLL-specific generator, scheduling, and UI ownership regressions were among
them and then each passed again by exact test name. The final complete normal
production suite passed 1,114 tests with 13 opt-in tests ignored.

The first complete-suite run found one stale MIDI Learn display regression
that still waited 200 ms despite the production 650 ms settling contract. Its
test clock now waits 700 ms, the isolated regression passes, and the complete
suite passes cleanly. Historical research, audition generation, benchmarks,
hardware tests, release builds, Clippy, JACK, synth, external MIDI, audible
playback, and recording were intentionally not run.
