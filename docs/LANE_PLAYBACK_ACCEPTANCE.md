# Priority 4 lane playback acceptance

Created: 2026-08-26

Status: software combined pass completed 2026-08-26

This document owns the bounded Priority 4 contract selected in
[Sequencer workflow priorities](SEQUENCER_WORKFLOW_PRIORITIES.md). It defines
independent lane cycles, rates, and playback direction without changing the
FT2 edit cursor or the Pattern's Arrangement duration.

## Musical semantics

- Every Pattern lane owns three playback settings: cycle length, rate, and
  direction. Fresh and migrated lanes use `FULL`, `1X`, and `FORWARD`.
- `FULL` follows the current Pattern row count. An explicit cycle length is
  `1..=Pattern rows`; shrinking a Pattern clamps only explicit lengths that no
  longer fit. Pattern growth leaves explicit polymeters unchanged.
- Rates are `1/4X`, `1/2X`, `1X`, `2X`, and `4X`. Slow rates advance one lane
  step every four or two Pattern rows. Fast rates emit two or four evenly
  spaced lane steps inside one Pattern row. Lane phase is anchored to absolute
  Pattern time, so Play Here skips earlier lane steps instead of restarting the
  lane at the selected row. The next transport repeat starts at Pattern row 0.
- `FORWARD` reads `0..length-1`; `REVERSE` reads `length-1..0`.
  `PENDULUM` reads both directions without repeating either endpoint. Lengths
  one and two remain well-defined. `VARIATION` uses a stable seeded permutation
  that visits every source row exactly once per lane cycle. Its seed includes
  Pattern identity, Arrangement position, lane, playback pass, and cycle, so
  the same Project/context is repeatable while later passes can evolve.
- A lane wrap is a note-ownership boundary. Each pendulum turn is also an
  ownership boundary. A held melodic note is released before the next section;
  an attack cannot be nudged backward across that boundary and a final attack
  cannot be nudged beyond it. An ordinary Arrangement Pattern boundary keeps
  the existing exact lane owner so a valid long gate or explicit OFF in the
  next Pattern is not truncated. The final Arrangement boundary releases any
  remaining owner. Percussion remains one-shot but Stop/Panic and route cleanup
  retain their existing all-notes-off ownership.
- Gate and Retrigger use the emitted lane-step duration. Cell nudge and legacy
  Delay scale with that same lane-step duration. Pattern swing applies at the
  unchanged transport-row boundaries; fast intermediate steps remain evenly
  spaced inside their containing row. This preserves all `1X` Priority 2
  timing exactly while making non-`1X` timing bounded and intelligible.
- Probability and conditions are evaluated in emitted lane order. FIRST,
  LAST/N, A:B, and FILL retain Pattern-pass meaning; lane wraps do not create a
  new Pattern pass. PRE follows the preceding emitted note trigger in that lane
  and resets at each Pattern boundary. Probability includes the emitted
  occurrence for repeated source rows while migrated `FULL/1X/FORWARD` lanes
  retain the Priority 3 result.
- Tempo commands remain attached to their stored Pattern rows and affect the
  transport timeline once. They are never repeated, reversed, or varied by a
  lane playhead. Pattern swing, automation, Loop Mix, MIDI clock, row markers,
  meter, and Pattern/Arrangement duration remain Pattern-time owners.
- Context-free MIDI export renders deterministic pass 1 with FILL off through
  the same canonical timeline as runtime scheduling. Preflight scans every
  source note trigger regardless of conditions or the pass-1 variation so a
  later pass cannot introduce an unowned software route.

## Workflow and transactions

- FT2 Tools **PAGE** -> **HISTORY** -> **RHYTHM** -> **CYCLE** opens the lane
  editor for the currently selected Pattern/page/lane. Opening and draft
  changes do not move the row, page, lane, or column cursor and do not dirty the
  Project.
- The editor uses the existing four controller pages. `LEN-`, `LEN+`, `RATE`,
  and `DIR` edit the draft; `STOP`, `APPLY`, `RESET`, and `CANCEL` own the
  transaction. Keyboard and mouse dispatch the same actions. No fifth
  controller page or hidden controller mode is added.
- `APPLY` requires stopped transport. A Play/REC/Live attempt keeps the draft
  and Pattern unchanged and points to nearby `STOP`; this makes note cleanup a
  visible precondition instead of changing direction inside a sounding lane.
  Successful Apply is exactly one Pattern History transaction. Cancel, Reset
  followed by Cancel, invalid data, and no-op Apply add no history entry.
- Lane settings are Pattern data. Lane/page/Pattern copy and clone preserve
  them. Lane/page paste and reusable drum-pattern load commit them through the
  existing Pattern History wrapper. Arrangement structure, Live shaping, and
  runtime mute remain outside this new transaction.

## Persistence and migration

- Project format 17 stores cycle length, rate, and direction on each
  `pattern_lane` record. Formats 0-16 migrate in memory to
  `FULL/1X/FORWARD` without rewriting the source file.
- Reusable drum-pattern format 4 stores the four lanes' playback settings.
  Formats 1-3 and the compact bundled catalog migrate to defaults. Save copies
  settings from the selected percussion page; load applies settings with the
  four lane cells while preserving route, lane name, and mute state.
- Invalid lengths, rates, directions, duplicate/missing current-format drum
  lane records, and future formats are refused before replacing current data.

## Acceptance matrix

| ID | Scenario | Required result |
|---|---|---|
| LC-01 | Fresh/migrated lane | `FULL/1X/FORWARD`; existing scheduling is byte- and time-equivalent. |
| LC-02 | Explicit length | Only the first N source rows participate; each lane wraps independently without changing Pattern markers or duration. |
| LC-03 | Pattern resize | Shrink clamps explicit lengths; growth preserves polymeter; FULL tracks the new size. |
| LC-04 | Rates | 1/4X, 1/2X, 1X, 2X, and 4X advance at exact bounded Pattern-row fractions. |
| LC-05 | Forward/reverse | Source-row order is exact and deterministic at every rate. |
| LC-06 | Pendulum | Endpoints occur once and turns are deterministic for lengths 1, 2, and greater. |
| LC-07 | Variation | Each cycle is a full permutation, stays in range, varies by documented context, and repeats byte-for-byte for identical context. |
| LC-08 | Play Here/repeat | Partial playback keeps absolute Pattern phase; first repeat restarts at row 0 without a zero-time loop. |
| LC-09 | Arrangement duration | Row markers, tempo map, MIDI clock, Loop clock, and end boundary equal the same Pattern without lane settings. |
| LC-10 | Tempo commands | A mapped source cell never duplicates/reverses a Pattern tempo command. |
| LC-11 | Timing interaction | Nudge, swing, Delay, gate, and Retrigger obey the documented lane-step and transport-grid ownership. |
| LC-12 | Priority 3 interaction | Conditions, probability, PRE, and FILL remain deterministic over wraps, rates, directions, and later Pattern passes. |
| LC-13 | Wrap/turn ownership | Held notes release at lane wraps and pendulum turns; boundary nudge cannot create a stale owner. |
| LC-14 | Pattern/Arrangement ownership | Ordinary Pattern steps preserve exact cross-boundary owners and releases; same-lane incoming attacks interrupt safely; final Arrangement cleanup releases any remainder. |
| LC-15 | Mute/Stop/Panic | Muting a lane/page and Stop/Panic clean only the existing owned notes through canonical paths. |
| LC-16 | Live activation | Retrigger/switch cleans outgoing owners; only an identical incoming first note may transfer, and a blank/different lane releases. |
| LC-17 | Project migration | Format 16 and representative older fixtures migrate in memory; format 17 round-trips all settings and rejects malformed/future values. |
| LC-18 | Drum migration | Formats 1-3/catalog default; format 4 round-trips and load/save preserves the four lane settings transactionally. |
| LC-19 | Pattern History | One Apply is one Undo item; Undo/Redo restores exact lane settings and context; Cancel/no-op/refusal moves neither stack. |
| LC-20 | Copy/paste/clone | Lane, page, Pattern, and drum operations preserve or default settings at their existing ownership boundary. |
| LC-21 | Cursor/Arrangement invariants | Draft, Apply, Cancel, Undo, and Redo do not move the selected FT2 row/page/lane/column or edit Arrangement structure. |
| LC-22 | Controller reachability | CYCLE and all editor actions are reachable on every supported layout through exactly four pages; keyboard/mouse share dispatch. |
| LC-23 | Native/compact UI | Values and stopped-transport consequence fit 40x13 and compact fallback without touching the shared status row. |
| LC-24 | Preflight | Every possible source note route is visible even when pass 1/rate/direction/variation does not emit it. |
| LC-25 | Export | Repeated exports are byte-identical and match runtime pass 1 event order/ticks without extending the conductor track. |
| LC-26 | Scheduler bound | Invalid data and event-limit overflow fail before transport ownership changes. |
| LC-27 | Priority 1-3 regression | History, timing, groove, REC FEEL, probability/conditions, navigation, scheduler ownership, and UI transactions remain green. |

## Evidence boundary

The owner-authorized pass used exact Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6). `cargo check
--locked`, the requested focused migration/history/rhythm/scheduler/export/
navigation/UI tests, and the complete normal suite passed. The final suite
reported 1,026 passed, zero failed, and 13 documented ignored development,
private-audition, and performance tests. LC-01 through LC-27 were reconciled
against those results and final source inspection.

The authorized validation is software-only on a non-Raspberry-Pi machine. It
must not start JACK, open ALSA sequencer ports, start a synth, transmit MIDI,
run playback/recording, make sound, take screenshots, or change hardware.
Physical-controller reach, native Pi timing/headroom, and musical approval of
the rate/direction choices remain separate evidence.
