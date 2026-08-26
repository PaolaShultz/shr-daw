# Step probability and conditions acceptance

This matrix owns the bounded Priority 3 contract selected in
[Sequencer workflow priorities](SEQUENCER_WORKFLOW_PRIORITIES.md). It covers
cell-owned deterministic chance and loop-aware conditions without starting
JACK, ALSA/MIDI output, a synth, playback, recording, or other hardware paths.

## Combined-pass verification

The owner-authorized non-Raspberry-Pi pass used exact Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6). Locked check,
focused probability/condition, Project/drum migration, Pattern History,
microtiming, swing, groove, REC FEEL, scheduler ownership, navigation, and UI
transaction tests passed. The complete normal suite reported 1,015 passed,
zero failed, and 13 documented ignored development, private-audition, and
performance tests.

## Product boundary

- Probability is an independent integer from 1% through 100%; 100% is the
  migrated/default value.
- Conditions are ALWAYS, FIRST, LAST/N, A:B, PRE, and FILL. LAST/N means the
  final pass in a repeating N-pass cycle. A:B means pass A in a repeating
  B-pass cycle. PRE uses the preceding note trigger in the same lane and the
  same playback pass; the start of each pass has no previous result.
- The condition is evaluated before probability. The percentage result is a
  stable function of Pattern, Arrangement step, row, lane, and one-based pass,
  so identical Project data and pass context produce identical events.
- Normal FT2 regenerates its event plan at the selected Arrangement playback
  span boundary; Live Patterns regenerate at their Pattern boundary. FIRST
  therefore fires only on pass 1; bounded cycles continue over later passes. A
  newly launched Live Pattern starts at pass 1.
- FILL is a runtime latch available on normal FT2's SOUND controller page and
  keyboard `f`; CLICK moves to FT2 Tools SYS. A change applies to the next
  playback-cycle boundary and is cleared by Stop or a new Play start. It is not
  Project data and does not dirty history.
- CELL EDIT adds CHANCE, CONDITION, COND A, and COND B to the rotary field
  sequence; the four direct-action pages remain bounded and unchanged. The
  whole draft remains one Pattern History transaction. Chance or a non-ALWAYS
  condition requires a note-on trigger; invalid drafts remain uncommitted.
- Standard MIDI File export and other context-free scheduling use deterministic
  pass 1 with FILL off. Route/engine preflight includes every conditional note
  so a later pass or Fill cannot require an engine that playback did not own.
- Project format 16 persists probability and condition. Formats 0–15 migrate
  to 100% and ALWAYS in memory without rewriting. Reusable drum-pattern format
  3 persists the same fields; formats 1–2 migrate to those defaults.

## Acceptance matrix

| ID | Acceptance | Automated evidence |
|---|---|---|
| PC-01 | FIRST, LAST/N, A:B, PRE, and FILL select the documented passes. | `sequencer::tests::step_conditions_follow_first_last_ratio_previous_and_fill_passes` |
| PC-02 | Probability is stable for identical input, varies across passes, and preflight includes conditional notes. | `sequencer::tests::probability_is_repeatable_but_varies_by_pass_and_preflight_includes_all` |
| PC-03 | Project format 16 round-trips all fields; format 15 migrates without rewriting. | `sequencer::tests::format_sixteen_round_trips_conditions_and_fifteen_migrates_to_always` plus Project migration tests |
| PC-04 | Drum format 3 round-trips all fields; formats 1–2 migrate to 100%/ALWAYS. | `drum_pattern` round-trip and migration tests |
| PC-05 | CELL EDIT commits chance plus condition as one undoable Pattern transaction; cancel/no-op behavior remains transactional. | `ui::tests::note_editor_probability_and_condition_save_as_one_history_transaction` and existing Note Edit/history tests |
| PC-06 | Controller rotary navigation reaches the four trigger fields; normal FT2 SOUND/FILL and keyboard `f` use the same latch. | navigation and UI action tests |
| PC-07 | Fill changes runtime state only and does not mutate Project data. | `ui::tests::tracker_fill_action_updates_only_the_transport_latch` |
| PC-08 | Existing timing, swing, groove, REC FEEL, scheduling ownership, navigation, and UI transaction tests remain green. | Required focused regression pass and complete normal test suite |

## Evidence limit

The non-Raspberry-Pi validation pass proves deterministic source, storage,
scheduling, navigation, and UI-state behavior only. It deliberately does not
start JACK, open ALSA sequencer ports, transmit MIDI, start a synth, run
playback/recording, make sound, or change attached hardware. Human musical feel
and physical-controller timing remain unverified here.
