# Pattern History Mutation Inventory and Test Table

This is the pre-implementation inventory for Priority 1 in
`SEQUENCER_WORKFLOW_PRIORITIES.md`. It classifies production mutations in
`src/ui.rs` by ownership boundary. Test IDs are the required first-version
acceptance table; they are not permission to broaden history beyond the named
Pattern scope.

## Combined-pass verification

The owner-authorized software pass completed on 2026-08-26 with exact Rust
1.97.1 (`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6). The locked
check, focused Pattern-history model/UI/navigation/transaction tests, and the
complete normal suite passed again after Priority 4 integration and bounded
in-scope repair. The final suite reported 1,026 passed, zero failed, and 13
documented ignored development, audition, and performance tests.

PH-01 through PH-19 were reconciled against the focused results, complete-suite
regressions, and the function-level mutation audit below. Route and Loop
publication/recovery evidence remains deterministic test-double and source
evidence; no JACK, synth, MIDI, playback, recording, audible, screenshot, or
physical-controller check was run.

## Mutation inventory

| Source mutation family in `ui.rs` | Classification | First-version history boundary |
| --- | --- | --- |
| Cell blank/erase/off, note entry, step entry, chord entry, note-editor Save | included | One successful cell edit, step, chord, or note-editor Save; note-editor Cancel remains outside history. |
| Note-editor route/note/value changes before Save | draft-only | The editor owns transient values. Capture only the final Save if the Pattern changes. |
| Tracker REC note-on/note-off writes, including external input and automation capture during the take | included | One completed take from REC start through its clean finish. A refused or empty take creates no entry. |
| Pattern tempo, meter, row length/SIZE, clear, transpose, and cleared drum-pattern load | included | One successful command. A validation failure or unchanged value creates no entry. |
| Lane paste, page paste, and Pattern paste-over | included | One successful paste. Pattern paste-new is structural and excluded. |
| Page entry layout, page/lane mute, program, bank, and other persistent page settings | included | One successful command. Repeated encoder turns may coalesce as one continuous gesture. |
| Lane cycle length, playback rate, and direction | included | One stopped-transport CYCLE Apply. Draft, Cancel, refusal, invalid data, and unchanged Apply create no entry. |
| Page Manager page add/delete/reorder/route/settings while open | draft-only | The existing whole-song draft remains authoritative. Capture one Pattern entry only after Apply validates and commits; Cancel creates no entry. |
| FT2 ROUTE target/setup edits and live audition adjustments while open | draft-only | Capture the opening Pattern before the draft. Ordinary Apply creates one entry only after validated publication and runtime activation succeed; Cancel/rollback creates none. |
| Mixed-engine remap transaction | structural | It can change multiple Patterns plus engine ownership, so it remains under its existing confirmation/rollback transaction and outside Pattern history. |
| Automation lane create, target change, point add/edit/delete, record, and clear | included | One successful command or continuous point/recording gesture. Failed capture and unchanged values create no entry. |
| Pattern Loop candidate commit/import, remove, region, source BPM, offset, filter, level, and other persistent attachment settings | included | One successful Pattern edit after decode/preflight succeeds. Repeated control movement may coalesce. |
| Live-shaped Pattern clones, queued Pattern/order changes, loop preparation buffers, slot playback/mute, held notes, transport and playback cursor | runtime-only | Never serialized into history and never restored from it. |
| Initial empty-project page adoption and internal drum-kit runtime preparation | runtime-only | Startup/runtime preparation is not a musician Pattern edit. |
| Pattern create, clone, paste-new, unused delete, and Arrangement insert/delete/duplicate/move | structural | Explicitly excluded from Pattern history. Existing structural ownership remains unchanged. |
| New/Load/Import/Save/Save As/Rename/project delete and clean-exit baseline replacement | excluded | Project lifecycle operations clear or replace history as appropriate but never create Pattern entries. |
| Project key/noob scale, drum-kit identity/tuning, inserts, sends, aux/master/drum racks, master/record/final bus, effect removal and effect-automation cleanup | excluded | Project-global state is outside Pattern history. |
| Preset browsing/import/save/adoption and any private file below `user/` | excluded | Preset and private-file ownership is outside Pattern history. |

### Exact production mutation-owner checklist

This checklist is the function-level audit of `ui.rs`; helper calls such as
`current_pattern_mut`, `current_page_mut`, `current_column_mut`, and direct
`patterns.get_mut` access are listed under the function that owns the change.

| Mutation owner(s) | Classification |
| --- | --- |
| `save_project_for_guard_as`, `save_song_file`, `save_song_as`, `commit_project_rename` | excluded project save/name lifecycle |
| `restore_clean_project_for_exit`, `new_project`, `commit_midi_import`, `load_song_named` | excluded Project replacement; clear all Pattern history and Snapshot after successful publication |
| `apply_live_route_adjustment`, `assign_route_snapshot`, `restore_live_route_snapshot`, route-field branches of `activate_overlay`/`overlay_back`/`close_overlay` | draft-only route audition and rollback |
| ordinary `confirm_route_overlay` | included once after Apply and runtime activation; mixed-engine remap branch is structural |
| `prepare_first_tracker_instrument`, `ensure_internal_drum_kit` | runtime-only initial/active route preparation; global kit mirror is not a Pattern edit |
| note-editor `select_note_editor_field`, `adjust_note_editor`, `confirm_note_editor_field`, `cancel_note_editor_field`, `cancel_note_editor` | draft-only live audition/rollback |
| `save_note_editor` | included cell plus page/column commit |
| `tracker_skip`, `tracker_erase`, `tracker_note_off`, `write_edit_notes`, `commit_tracker_gesture_after` | included cell/note/step/chord family |
| `record_tracker_midi_at`, `record_tracker_note_on`, `record_tracker_note_off`, `finish_tracker_recording` | included once as one completed REC take |
| `apply_pattern_resize`, `apply_pattern_clear`, `clear_pattern_now`, `transpose_pattern`, `load_drum_pattern` | included Pattern SIZE/clear/transpose/drum family |
| `paste_lane`, `paste_page_block`, `paste_pattern_over` | included paste family |
| `new_pattern`, `create_pattern`, `create_pattern_keep_shape`, `create_pattern_from`, `clone_pattern`, `paste_pattern_new`, `delete_unused_pattern` | structural and excluded |
| `arrangement_append_current`, `arrangement_insert_current`, `arrangement_remove_step`, `arrangement_duplicate_step`, `arrangement_move_step`, `repeat_order`, `delete_order`, `move_order` | structural Arrangement mutations and excluded |
| `open_page_manager`, `add_tracker_page`, `confirm_page_field`, `cancel_page_field`, `turn_page_manager`, `cancel_page_manager`, entry-layout `activate_overlay` branch | draft-only until Page Manager Apply |
| `confirm_page_manager` | included once after validation and successful route activation |
| `toggle_tracker_page_mute`, `toggle_tracker_lane_mute`, `set_tracker_tempo`/`apply_tracker_tempo`, `change_program`, `change_bank` | included persistent Pattern/page settings |
| `open_lane_playback`, lane draft adjustments, `apply_lane_playback`, `cancel_lane_playback` | draft-only until stopped Apply; Apply is one included lane-settings entry |
| `adjust_playback_noob_scale` | excluded Project-global key mutation |
| `new_automation_lane`, `cycle_automation_target`, `add_automation_point`, `adjust_automation_value`, `delete_automation_point`, `toggle_automation_curve`, `clear_automation_lane`, `capture_automation_control`, `capture_external_automation` | included automation family; repeated capture/value input coalesces |
| `commit_loop_candidate`, `remove_pattern_loop`, `adjust_loop_slot_level`, `adjust_loop_slot_filter`, `neutral_loop_slot_filter`, `auto_align_loop`, `adjust_loop_offset_bars`, `adjust_loop_source_bpm`, `cycle_loop_bpm_mode`, `adjust_loop_region`, Loop Library `activate_overlay` branch | included Pattern Loop attachment/settings after preflight succeeds |
| `tracker_record_song`, `shaped_live_song`, live queue/launch/shape functions, `activate_pattern_loops`, `stage_pattern_loops`, `command_loop_slot`, `cancel_loop_slot_queue`, `toggle_loop_slot_mute` | runtime-only clones, preparation, playback, queue, and mute state |
| mixed-engine remap `begin_*`/`advance_*`/`finish_*`/`cancel_*` | structural multi-Pattern/runtime transaction and excluded |
| `commit_master_strip`, `commit_fx_routing`, effect add/remove/type/move/bypass, aux-send functions, `remove_effect_automation` | excluded Project-global audio graph and dependent cleanup |
| `adopt_saved_user_preset` and preset save/import functions | excluded preset/private-file ownership |
| `restore_pattern_state` | history runtime publication itself; not a new mutation entry |

## Acceptance test table

| ID | Normal path | Recovery/negative path | Required proof |
| --- | --- | --- | --- |
| PH-01 | One included cell edit adds one Undo item and marks the project dirty. | Blank-on-blank, refused input, and failed mutation add nothing. | Undo label and depth change only for the real edit. |
| PH-02 | Undo restores the exact prior Pattern; Redo restores the exact edited Pattern. | Undo/Redo with an empty stack is disabled and changes nothing. | Pattern equality plus order, row, page, lane, column, controller page, and screen-return context. |
| PH-03 | A new edit after Undo clears Redo. | Navigation or another no-op after Undo does not clear Redo. | Redo availability follows the successful mutation boundary. |
| PH-04 | History retains at most 32 entries and stays within the shared undo/redo weight budget. | Oldest Undo states are evicted first; the current Pattern is never discarded. | Entry count and combined structural weight assertions. |
| PH-05 | Snapshot captures the current Pattern and edit context without changing transport or dirty state. | Recall with no snapshot is disabled and changes nothing. | Snapshot label/Pattern shown; clean baseline remains clean after capture. |
| PH-06 | Recall restores the snapshot and itself becomes Undoable. | Recall of an identical Pattern/context is a no-op and adds no history. | Undo after Recall restores the pre-Recall Pattern. |
| PH-07 | Consecutive edits of one continuous control gesture produce one Undo step. | Moving focus or performing another mutation ends coalescing. | Tempo/automation/Loop representative gesture tests. |
| PH-08 | Page Manager Apply records one entry after validation. | Cancel, failed validation, and unchanged Apply record none. | Existing Apply/Cancel draft and selection behavior remains intact. |
| PH-09 | Ordinary ROUTE Apply records one entry only after runtime activation succeeds. | Cancel or activation rollback leaves both Pattern and history unchanged. | Route/runtime ownership and rollback assertions remain intact. |
| PH-10 | One completed REC take records one entry; Undo first finishes REC and then restores safely. | Empty/cancelled/refused take records nothing. | Held notes are cleaned and prior transport return behavior is preserved. |
| PH-11 | Automation create/edit/record/clear is undoable at the named gesture boundary. | Capture failure and unchanged point values create no entry. | Point/lane equality and stable selection after restore. |
| PH-12 | Pattern Loop attachment/settings restore only after decode/preflight succeeds. | Missing/invalid media or failed runtime preparation leaves history unmoved. | Attachment data, prepared-loop ownership, and rollback state agree. |
| PH-13 | HISTORY page 1 is exactly UNDO/REDO/SNAP/RECALL; page 4 keeps Panic/Help/blank/Exit. | UNDO, REDO, and RECALL are visibly disabled when unavailable and cannot dispatch. | Keyboard, controller, and mouse use the same action path. |
| PH-14 | Ctrl+Z, Ctrl+Y, and Ctrl+Shift+Z dispatch Undo/Redo in the FT2 workspace. | Active Apply/Cancel drafts retain ownership; shortcuts do not bypass them. | Key-event tests cover modifiers and draft refusal. |
| PH-15 | Stopped transport restores immediately through validated Pattern publication. | During Play, restore is refused with a concise stop-transport message and stack positions remain unchanged; during REC, Undo finishes the take first. | No partial Song/runtime publication and no history movement on refusal. |
| PH-16 | Undo back to the saved Pattern clears dirty; Redo makes it dirty again. | Snapshot capture alone never changes dirty. | Equality against the existing clean baseline, not a separate history flag. |
| PH-17 | Project replacement clears undo, redo, and snapshot. | Save/Rename without Pattern replacement does not manufacture history. | No history state leaks between projects. |
| PH-18 | Structural, global, draft-only, runtime-only, and private mutations remain absent from history. | Their existing transaction, queue, route, transport, and ownership behavior is unchanged. | Representative exclusion tests and source-level mutation audit. |
| PH-19 | One stopped CYCLE Apply restores exact lane settings through Undo/Redo without moving the cursor. | Cancel, unchanged Apply, invalid settings, and Play/REC refusal leave both stacks unmoved. | Lane-settings equality, stack depths, and row/page/lane selection assertions. |

The first implementation uses the document-authorized stopped-transport
fallback for restore during Play. The existing scheduler can queue live Pattern
selection and shaped runtime clones, but it has no boundary transaction that
atomically replaces an authoritative full Pattern, activates route and Loop
resources, restores editor context, and reports success for history-stack
movement. Adding that scheduler transaction would be a larger redesign. A
restore therefore succeeds only while stopped; Undo during REC first completes
the current take and then restores it while stopped.
