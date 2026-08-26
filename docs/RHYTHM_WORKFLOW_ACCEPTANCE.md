# Priority 2 rhythm workflow acceptance table

Created: 2026-08-26

Status: software combined pass completed 2026-08-26

This table turns Priority 2 from `SEQUENCER_WORKFLOW_PRIORITIES.md` and Parts
2–4 of `POST_COMPETITION_RHYTHM_PLAN.md` into the first implementation
boundary. The Cargo portion is complete; screenshot, MIDI, hardware, and
listening work remains separately safety-gated.

## Combined-pass verification

The owner-authorized software pass used exact Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6). The locked check,
focused Project/drum migration, microtiming, swing, deterministic groove, REC
FEEL, scheduler ownership, navigation, and UI transaction tests passed again
after Priority 3 integration. The complete normal suite then passed with 1,015
successful tests, zero failures, and 13 documented ignored development,
audition, and performance tests.

RW-01 through RW-25 were reconciled against those focused results, the complete
suite, and source inspection. RW-15 has deterministic render assertions but no
new screenshot evidence, and RW-25 has dispatch tests but no physical-controller
evidence. External MIDI clock measurement, Raspberry Pi timing, audible groove
judgment, and the musical suitability of the 75% ceiling remain open.

## Current boundary inventory

| Area | Observed owner | Priority 2 treatment |
| --- | --- | --- |
| Cell event data | `sequencer::Cell` stores note, velocity, program, gate, and one command | Add independent signed `nudge`; retain every command unchanged. |
| Pattern feel | `sequencer::Pattern` stores tempo/meter/pages/routes/cells/automation/Loop Mix | Add straight-default swing division and amount as Pattern-owned data. |
| Project format | `sequencer::encode/decode`, current format 14 | One backward-compatible format bump; older Projects migrate in memory to zero nudge and straight feel. |
| Drum format | `drum_pattern::encode/decode`, current format 1 | One backward-compatible format bump; older files load zero nudge. |
| Musical scheduling | `sequencer::schedule_elapsed` creates steady row markers, MIDI clock, and cell messages | Swing and nudge affect cell events only; row markers, Pattern duration, MIDI clock, and Loop beat clock remain steady. |
| Canonical timeline/export | `timeline::compile` converts the elapsed schedule into a bounded tick plan | Preserve the existing timeline/automation resolution; live scheduling retains the full 1/96-row fraction and SMF export rounds to the nearest existing tick. |
| Note editing | FT2 CELL editor owns a draft and Save/Cancel | Add `TIMING`; Save commits through Pattern history, Cancel restores byte-for-byte. |
| Tracker grid | `draw_tracker` renders one compact cell line | Add one early/on-grid/late marker without widening the 40×13 grid. |
| FEEL | No current editor | Add one Pattern draft editor with division, 50–75% amount, Apply, and Cancel. |
| GROOVE | No current tool | Add deterministic preset/scope/strength draft; Apply writes exact nudge/velocity results as one undoable Pattern mutation. |
| Real-time REC | callback timestamp reaches `drain`, but `record_tracker_midi` keeps only the transport row | Keep quantized default; optional runtime `REC FEEL` derives the nearest row and bounded nudge from the callback timestamp. |
| History/recovery | Priority 1 Pattern history wrapper and stopped restore | CELL Save, FEEL Apply, GROOVE Apply, and a complete REC take each create at most one history step; failed/no-op/Cancel creates none. |
| Runtime ownership | sequencer/engine/Loop owners retain Stop, Panic, held-note, and route cleanup | Timing work must not start engines, alter routes, change transport ownership, or weaken cleanup. |

## First implementation decisions

- Cell nudge is stored in `1/96` row units and bounded to `-48..=48`.
- Pattern swing stores `EIGHTH` or `SIXTEENTH` division plus an integer
  `50..=75%` amount. `50%` is exactly straight.
- Swing moves only the alternating subdivision event boundary and returns to
  the unchanged pair/beat/Pattern boundary. Cell nudge is applied after swing;
  legacy `Delay` remains an additional late command.
- Timing is clipped only at an unavailable play-here pre-roll or the Pattern
  boundary. Stored first-row early nudges are rejected; bounded final-row late
  timing remains inside the row and is valid.
- GROOVE presets are neutral descriptive shapes: `SNARE LATE`, `HATS EARLY`,
  `ALT PUSH/PULL`, `END DRAG`, and `END PUSH`. They are deterministic and do
  not claim cultural authenticity.
- REC FEEL is runtime-only and opt-in. Quantized REC remains the default.
- The 75% swing ceiling is an implementation bound, not a musical approval.
  Human listening may lower it before release but must not silently change the
  stored meaning of already-published values.

## Combined-pass acceptance table

| ID | Scenario | Required result | Planned evidence |
| --- | --- | --- | --- |
| RW-01 | New/old Cell | Default is zero nudge; `-48`, `0`, and `48` validate; values outside the bound fail. | Model tests. |
| RW-02 | Project format 14 load | Loads byte content without rewrite; every cell is zero nudge and every Pattern is straight. | Migration fixture and no-write test. |
| RW-03 | Current Project round trip | Nudge coexists with Cut, Delay, Retrigger, Tempo, velocity, program, and gate; Pattern feel round-trips exactly. | Encode/decode tests. |
| RW-04 | Drum format 1 load/current round trip | Old drums gain zero nudge in memory; current drums retain deliberate timing with commands. | Drum codec tests. |
| RW-05 | Exact microtiming | Early/on-grid/late notes schedule at the represented fraction from 20–300 BPM. | Focused scheduler tests. |
| RW-06 | Boundary timing | No event escapes Pattern start/end; play-here has no unavailable pre-roll; final cleanup remains bounded. | Scheduler boundary tests. |
| RW-07 | Crossed same-lane events | Events are resolved in musical-time order; replacement cannot emit a stale release before its note-on or leave a stuck note. | Focused ownership test. |
| RW-08 | Program ordering | Program/bank messages precede their note at the identical shifted time. | Scheduler ordering test. |
| RW-09 | Gate/retrigger/off | Gate begins at shifted note-on; generated releases, retriggers, explicit Off, Cut, Stop, and Panic remain bounded. | Scheduler and transport tests. |
| RW-10 | Straight equivalence | Straight Pattern event times and steady transport equal the pre-feature contract. | Existing timing fixtures plus focused regression. |
| RW-11 | Swing ratios | Eighth/sixteenth alternating positions match configured 50–75% ratios and every pair returns to its straight boundary. | Elapsed-time scheduler tests. |
| RW-12 | Clock separation | MIDI clock stays even at 24 PPQN; row cursor, WAV/Loop beat clock, Pattern duration, and Arrangement duration do not wobble or drift. | Timeline/clock/long-repeat tests. |
| RW-13 | Tempo interaction | Pattern tempo, Tempo commands, play-here, and live refresh preserve the selected feel without boundary drift. | Focused timing tests. |
| RW-14 | CELL editor | Timing displays `ON GRID`, `EARLY … ms`, or `LATE … ms`; reset returns exactly to zero; Apply is one history step; Cancel/no-op creates none. | UI/state tests. |
| RW-15 | Grid marker | Early/on-grid/late cells remain readable at native 40×13 and compact fallback; shared status row is unchanged. | Render assertions and later screenshots. |
| RW-16 | FEEL draft | Opening is non-mutating; amount/division edits stay draft-only; Apply is one undoable Pattern mutation; Cancel restores exact context. | UI transaction/history tests. |
| RW-17 | GROOVE scope | Cell/lane/page/Pattern scope changes only matching note-on cells and reports affected hits. | Deterministic model/UI tests. |
| RW-18 | GROOVE determinism | Same preset/scope/strength/input produces byte-identical nudge/velocity output; zero-strength/no-op creates no history. | Model tests. |
| RW-19 | Groove boundaries | First-row push and final-row drag never create an out-of-Pattern stored nudge. | Boundary tests. |
| RW-20 | Quantized REC | Default recording continues to store zero nudge and unchanged note ownership/release rows. | Existing REC regressions. |
| RW-21 | REC FEEL | Callback residual chooses nearest row and bounded nudge; beyond half-row selects the adjacent in-Pattern row rather than a misleading extreme. | Timestamp-position tests. |
| RW-22 | REC take history | One complete take is one Undo step; finishing/Undo retains held-note cleanup; failed/empty take changes no history. | UI/history tests. |
| RW-23 | Copy/clone/paste/drum/load/export | Timing and feel survive every existing Pattern-preserving operation; new material defaults straight/on-grid. | Focused operation/codec tests. |
| RW-24 | Recovery/ownership | Failure, Cancel, no-op, Stop, Panic, route failure, unavailable target, and project replacement preserve existing recovery and ownership contracts. | Focused state tests and source inspection. |
| RW-25 | Input parity | Controller, keyboard, mouse, and encoder dispatch the same CELL/FEEL/GROOVE/REC FEEL actions and restore exact return context. | Navigation/UI tests. |

## Deferred evidence

- External MIDI clock measurement, Raspberry Pi timing, screenshots, physical
  controller use, audible groove judgment, and the 75% musical ceiling remain
  unclaimed until separately exercised under the repository safety gate.
