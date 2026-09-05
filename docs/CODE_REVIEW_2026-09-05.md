# Repository code review — 2026-09-05

This report records **21 findings: 12 P1 and 9 P2**. It is a repair backlog,
not a release sign-off. The original review findings are preserved below.
Repairs for all 21 were implemented in `e6726d87dd745a0560561a06cd9510594f931307`.
They remain **pending regression validation**, not closed; see the
[implementation record](#implementation-record).

Reviewed source: commit `4bda7603ab6b929dcae5aa68f8f125b77c801066`
(`Record complete debug and release validation pass`). Source links and line
numbers below refer to that revision; use the named functions when later
changes move them. No application, helper, configuration, or test code was
changed during this review.

## Scope and evidence

The repository-wide review inventoried the first-party Rust modules, scripts,
build/install entry points, public schemas, and their tests. Manual inspection
followed production callers through state changes, error handling, ownership,
and persistence. It concentrated on data loss, recovery, real-time safety,
MIDI routing, and differences between saved state and running behavior.
This is a source review, not a claim that every execution path was exercised.

| Area | Review coverage |
| --- | --- |
| Application and TUI | Startup/exit, working-screen layout and navigation, controller dispatch, Project lifecycle, editor drafts and history, routing application, screenshot construction |
| Musical model and transport | Song/Pattern validation and serialization, Arrangement expansion, conditions and lane playback, live performance, tempo maps, automation, MIDI import/export |
| MIDI and instruments | Input subscriptions, mapped controls and pickup, note ownership and cleanup, external destination resolution, clock ownership, managed process lifecycle, preset/model discovery and saves |
| Audio | JACK registration/lifetime, graph compilation/publication, effect controls and bypass, final bus/master processing, loop loading/publication, drum host, meters, recording writers and recovery |
| Offline editors | Rhythm, harmony, generators, Arrangement assistant, Pattern history, controller/profile parsing and learning |
| Delivery and public assets | Local/setup/install paths, manifest transactions, audio-service helper, public payload/provenance checks, documentation and screenshot generation, existing test boundaries |

Evidence labels:

- **Source-confirmed** means the failure follows from the cited implementation
  and an identified caller or interleaving. It does not mean a Rust executable
  reproduced the failure during this review.
- **Fixture-reproduced** means the real Python installer functions demonstrated
  the failure with disposable files under `/tmp`, without root or system writes.

Validation performed: source/caller inspection; inspection of relevant existing
tests; disposable installer reproductions for CR-17, CR-18, and CR-19; report
source-link/line checks and `git diff --check`. The installer overlap fixture
uses a deterministic interleaving, not a timing-dependent stress run.

**Intentionally not run:** Rust compilation, Cargo check/test, Clippy, the
normal production test suite, historical/exhaustive tests, benchmarks, audition
or evidence renderers, screenshots, and live JACK/MIDI/audio tests. The current
repository rule requires a separate explicit combined build-and-test request.
The review did not open private runtime files or operate borrowed hardware.
Existing validation recorded elsewhere is not fresh evidence for this report.
External engine repositories, dependency implementations, acoustic quality,
and hardware behavior were not independently audited.

## Priority index

P1 means prioritize before relying on the affected operation: data preservation,
ownership, sound/control correctness, or process reliability is compromised.
P2 means a concrete correctness or recovery defect for a narrower trigger.
No P0 failure was established. Priority describes impact; the evidence column
describes how the finding was established.

| ID | Priority | Finding | Evidence |
| --- | --- | --- | --- |
| [CR-01](#cr-01) | P1 | Renaming a saved Project discards unsaved edits | Source-confirmed |
| [CR-02](#cr-02) | P1 | Configuration rollback can delete an unreadable original | Source-confirmed |
| [CR-03](#cr-03) | P1 | Multitrack recovery cannot resume partially finalized stems | Source-confirmed |
| [CR-04](#cr-04) | P1 | Audio callbacks alias mutable DSP state with owner-thread reads | Source-confirmed |
| [CR-05](#cr-05) | P1 | Drum and graph effect IDs collide in the automation registry | Source-confirmed |
| [CR-06](#cr-06) | P1 | Saved external-MIDI settings never update the sequencer | Source-confirmed |
| [CR-07](#cr-07) | P1 | MIDI inputs remain live while their owned notes are released | Source-confirmed |
| [CR-08](#cr-08) | P1 | Timeline limits are enforced after oversized event expansion | Source-confirmed |
| [CR-09](#cr-09) | P2 | Project and drum-file size checks happen after full reads | Source-confirmed |
| [CR-10](#cr-10) | P2 | Final-mix startup can re-arm a recorder whose writer failed | Source-confirmed |
| [CR-11](#cr-11) | P2 | AUX bypass automation can introduce a dry return | Source-confirmed |
| [CR-12](#cr-12) | P2 | Imported in-Pattern tempo changes take effect one row late | Source-confirmed |
| [CR-13](#cr-13) | P2 | An Idea can be saved larger than the loader accepts | Source-confirmed |
| [CR-14](#cr-14) | P2 | Routing cannot recover when no MIDI router was constructed | Source-confirmed |
| [CR-15](#cr-15) | P2 | Invalid runtime configuration prevents `shr stop` | Source-confirmed |
| [CR-16](#cr-16) | P1 | Screenshot generation can read private state and transmit MIDI clock | Source-confirmed |
| [CR-17](#cr-17) | P1 | Installer recovery overwrites intervening repairs | Fixture-reproduced |
| [CR-18](#cr-18) | P1 | Installer removal follows replaced parent directories | Fixture-reproduced |
| [CR-19](#cr-19) | P1 | Overlapping installers corrupt the manifest/file relationship | Fixture-reproduced |
| [CR-20](#cr-20) | P2 | Installer recovery lacks durable ordering for power loss | Source-confirmed |
| [CR-21](#cr-21) | P2 | Interrupted JACK-service installation leaves unrecognized owned files | Source-confirmed |

## Findings

<a id="cr-01"></a>
### CR-01 — P1 — Renaming a saved Project discards unsaved edits

**Source:** [ui.rs:12276](../src/ui.rs#L12276), `commit_project_rename`;
[sequencer.rs:3057](../src/sequencer.rs#L3057), `rename_project`.

**Trigger and result:** Load or save a Project, edit notes, routing, effects,
or Arrangement without saving, then confirm a rename. The helper reloads the
old file from disk and changes only its name. The UI replaces `self.song` with
that older Song and calls `mark_project_clean`. Current edits disappear and the
UI no longer warns that they were unsaved. Even a rename that keeps the same
sanitized filename takes this path. The unsaved-Project branch correctly
clones the current Song, making behavior depend on whether it has a filename.

**Repair direction:** Rename a validated candidate built from the current Song.
Keep the existing no-replacement publication guarantees and preserve editor
context. Only mark clean after the current contents have actually been saved.

**Regression acceptance:** Rename a saved, dirty Project with changed cells,
order, key, and effects; assert all edits survive both in memory and on disk.
Cover same-stem rename, destination collision, and failed publication.

<a id="cr-02"></a>
### CR-02 — P1 — Configuration rollback can delete an unreadable original

**Source:** [ui.rs:1883](../src/ui.rs#L1883), `restore_config_file` and
`persist_routing_transaction`; [controller_learn.rs:1987](../src/controller_learn.rs#L1987),
`restore_file` and `save_learned_for_state`.

**Trigger and result:** An existing configuration file is unreadable but its
directory is writable. Both transactions take their original snapshots with
`fs::read(...).ok()`, treating every read error as absence. A subsequent backup
failure invokes rollback; `None` plus `path.is_file()` causes removal of the
original file. A save that never successfully began can therefore delete the
user's configuration. Rollback write/remove errors are also discarded, so
claims of restoration are not verified.

**Repair direction:** Treat only `NotFound` as absence. Finish reading all
originals before mutation, preserve read/backup failures, and surface rollback
failures without claiming the old route was restored.

**Regression acceptance:** Inject permission/read errors separately from
absence in routing and learned-profile saves. Original bytes must remain
unchanged; a failed restore must return a distinguishable recovery error.

<a id="cr-03"></a>
### CR-03 — P1 — Multitrack recovery cannot resume partially finalized stems

**Source:** [audio_recorder.rs:1404](../src/audio_recorder.rs#L1404),
`write_session`; [audio_recorder.rs:2344](../src/audio_recorder.rs#L2344),
`recover_session_directory`.

**Trigger and result:** Terminate a recording writer after it renames one
`*.wav.part` stem to `*.wav`, before publishing the take directory. Recovery
requires a `.part` file for every manifest track and immediately fails on the
already-renamed stem. The same failure occurs after all stem renames but before
directory publication, or if recovery itself is interrupted between renames.
The audio may still exist, but automatic recovery cannot complete it.
`recover_interrupted` also puts failed paths in the same returned collection as
successful recoveries, allowing callers to report them as recovered recordings.

**Repair direction:** Make finalization resumable from validated combinations
of final and temporary stems. Handle conflicts explicitly, preserve the common
frame-count contract, and return separate recovered/failed outcomes.

**Regression acceptance:** Inject interruption after each stem rename, after
manifest publication, and before directory publication. Retry recovery twice;
all surviving common frames must remain readable and no failed take may be
counted as successfully recovered.

<a id="cr-04"></a>
### CR-04 — P1 — Audio callbacks alias mutable DSP state with owner-thread reads

**Source:** [audio_graph_client.rs:1401](../src/audio_graph_client.rs#L1401),
`process_callback`; [audio_graph_client.rs:900](../src/audio_graph_client.rs#L900),
`effect_meter`; [audio_graph_runtime.rs:647](../src/audio_graph_runtime.rs#L647),
`effect_meters_by_id`, and `process` at line 736;
[drums_host.rs:242](../src/drums_host.rs#L242), `kit_id`, and `process_callback`
at line 351.

**Trigger and result:** With JACK active, the process callback constructs
`&mut CallbackData` and mutably processes the graph/engine. The owner can
simultaneously traverse `callback.plan.nodes` and `EffectSlot` values to find
meters, or borrow `callback.engine` to obtain the drum kit ID. Published meter
values being atomic does not make traversing their mutable owning objects
safe. The callbacks' pinning comments establish storage lifetime but do not
establish exclusive access. This is an unsafe Rust aliasing defect, not a
claim that a crash was observed. Rust's reference rules require exclusivity
for a live mutable reference; JACK invokes registered callbacks independently
of the application's UI work.
[Rust reference](https://doc.rust-lang.org/reference/behavior-considered-undefined.html),
[JACK callback API](https://jackaudio.org/api/group__ClientCallbacks.html).

**Repair direction:** Separate callback-owned mutable DSP storage from shared
control/status handles. Cache kit identity and clone meter/control handles
before activation; UI reads must not traverse active DSP objects. Use an
explicitly justified interior-mutability boundary for callback-exclusive state.

**Regression acceptance:** Audit every callback argument and owner accessor.
Exercise the ownership model in a hardware-free concurrency harness, with an
appropriate Rust aliasing checker where supported. Preserve callback allocation
and blocking constraints; ordinary atomic-value tests alone are insufficient.

<a id="cr-05"></a>
### CR-05 — P1 — Drum and graph effect IDs collide in the automation registry

**Source:** [sequencer.rs:754](../src/sequencer.rs#L754), default drum rack;
[audio_graph.rs:428](../src/audio_graph.rs#L428), `next_effect_id`;
[effects/mod.rs:148](../src/effects/mod.rs#L148), `EffectControlRegistry`;
[timeline.rs:332](../src/timeline.rs#L332), scheduled effect construction.

**Trigger and result:** A new Song gives drum Reverb/Delay IDs 1 and 2. Adding
ordinary graph effects allocates from the source/master/AUX racks only, so IDs
1 and 2 are reused. Song validation validates drums separately. Persisted
automation identifies both rack and ID, but scheduling drops the rack and the
shared control registry keys only by ID. Registering either host overwrites
the other's entry. Matching effect kinds can receive each other's automation;
different kinds fail schema checks. Clearing one owner can also leave the
other owner's ID pointing at the wrong, stale control.

**Repair direction:** Carry a complete effect identity through scheduling and
publication, or establish true Project-wide ID allocation with migration for
already-valid colliding Projects. Do not merely reject existing files without
a preservation path.

**Regression acceptance:** In a fresh Song, create graph Reverb/Delay alongside
drum Reverb/Delay. Automate each independently, vary host registration order,
and clear/restart either host. Values must reach only the specified rack.

<a id="cr-06"></a>
### CR-06 — P1 — Saved external-MIDI settings never update the sequencer

**Source:** [ui.rs:3000](../src/ui.rs#L3000), `confirm_routing_edit`;
[ui.rs:2003](../src/ui.rs#L2003), sequencer construction;
[sequencer.rs:4290](../src/sequencer.rs#L4290), `start_with_clock`;
[sequencer.rs:6184](../src/sequencer.rs#L6184), `resolve_midi_route`.

**Trigger and result:** Change the configured external output from device A
to valid device B and save Routing. The UI persists the new config and
reconfigures MIDI inputs, but the sequencer worker retains the configuration
cloned at application construction. It has no corresponding configuration
update command. Playback can continue or start using A while Routing displays
B. Disabling external MIDI during playback likewise does not update that
owner. The existing “next start” notices cover audio and controller clock,
not this stale sequencer configuration.

**Repair direction:** Include the sequencer/destination owner in the routing
transaction. Define and enforce the stop boundary, release old destinations,
then publish the new configuration with rollback or explicit pending state.

**Regression acceptance:** With mocked outputs A and B, apply a routing change
and restart playback in the same App. All subsequent events must use B; old
notes must be cleaned up. Cover disable/re-enable and activation failure.

<a id="cr-07"></a>
### CR-07 — P1 — MIDI inputs remain live while their owned notes are released

**Source:** [engine.rs:598](../src/engine.rs#L598), `reconfigure_inputs`;
[engine.rs:723](../src/engine.rs#L723), `Drop for MidiRouter`;
[engine.rs:2087](../src/engine.rs#L2087), `connect_midi_input`.

**Trigger and result:** Reconfiguration stops the topology monitor, drains
owned notes, and delivers releases before clearing input connections. An input
callback can accept or deliver another note between the drain and connection
closure. Its note-off never arrives after that subscription closes, and no
second drain cleans it up. Drop has the same ordering: connections survive the
explicit cleanup until automatic field destruction. Stopping the topology
monitor does not quiesce the MIDI callbacks.

**Repair direction:** Quiesce and close input producers before draining their
note ownership, while keeping destination handles alive for the final release.
Account for callbacks already between state calculation and output delivery.

**Regression acceptance:** Use barriers to pause a callback around note
ownership/delivery while reconfiguration or Drop runs. No note-on may arrive
after final cleanup; output and tracker note ownership must both end empty.

<a id="cr-08"></a>
### CR-08 — P1 — Timeline limits are enforced after oversized event expansion

**Source:** [sequencer.rs:3222](../src/sequencer.rs#L3222),
`schedule_elapsed_with_conditions`; [timeline.rs:155](../src/timeline.rs#L155),
`compile_with_conditions`, and `effect_events` at line 284.

**Trigger and result:** Song validation caps stored cells and Arrangement
entries separately. Repeating one valid 256-row, four-lane dense Pattern 4,096
times expands more than four million note-ons alone, despite storing only
1,024 cells. Releases, row markers, retriggers, and automation add more.
The scheduler builds and sorts this entire vector before the caller checks
the 1,048,576-event limit. Effect automation is also expanded before the
combined limit check. A valid small Project can therefore stall or exhaust
memory during play/preflight/export before the intended refusal is reached.
No oversized allocation was attempted during review.

**Repair direction:** Enforce a shared budget during expansion, including
markers, setup, note releases, lane subdivisions, and automation samples.
Checked preflight estimates can reject obvious excess early, but every append
path still needs a bound. Keep refusal before large allocations and sorting.

**Regression acceptance:** Use a low injected budget with repeated Patterns
and dense automation; assert expansion aborts at the budget. Keep this focused
test fast. Large memory/time measurements belong in an opt-in benchmark.

<a id="cr-09"></a>
### CR-09 — P2 — Project and drum-file size checks happen after full reads

**Source:** [sequencer.rs:3045](../src/sequencer.rs#L3045), `load`, and overwrite
inspection at line 3031; [sequencer.rs:2281](../src/sequencer.rs#L2281), `decode`;
[drum_pattern.rs:132](../src/drum_pattern.rs#L132), `load_path`.

**Trigger and result:** Select an oversized regular `.shsong` or user drum
file. `fs::read_to_string` first reads the complete file; only then does decode
check its 16 MiB or 256 KiB limit. The advertised parser limits therefore do
not bound loading memory or time. Overwrite validation repeats the same
unbounded Project read. A mistakenly copied large file or oversized sparse
file can freeze or terminate the UI instead of producing a bounded error.

**Repair direction:** Read through a regular-file, bounded reader and check
the actual bytes consumed, not only a pathname's preliminary metadata. Use the
same boundary for load, rename, and overwrite validation.

**Regression acceptance:** Maximum-sized input, one byte over, and a large
sparse file must produce bounded results without allocating the file's full
size. Reject inappropriate file types before blocking reads.

<a id="cr-10"></a>
### CR-10 — P2 — Final-mix startup can re-arm a recorder whose writer failed

**Source:** [audio_recorder.rs:413](../src/audio_recorder.rs#L413),
`FinalMixRecorder::start`; [audio_recorder.rs:283](../src/audio_recorder.rs#L283),
`FinalMixCapture::capture`.

**Trigger and result:** The writer thread fails immediately, for example when
creating its temporary WAV. It stores `writer_running = false` and mode IDLE.
The caller, after spawning that thread, unconditionally stores ARMED. If the
failure wins this race, the next callback changes ARMED to ACTIVE and queues
frames with no writer. Capture does not consult the writer-running/fault flags
before enqueueing. Reaping the finished worker does not clear the resurrected
mode or pending frames, so subsequent starts can remain blocked even though
the UI has observed the writer error.

**Repair direction:** Establish a startup handshake/state transition that
cannot overwrite terminal failure. Ensure failed startup leaves a reusable
recorder with no producer writing into an undrained ring.

**Regression acceptance:** Force writer creation failure before the caller
finishes startup. Feed a synthetic callback, inspect terminal state/ring, then
retry successfully with the same recorder. No JACK client is needed.

<a id="cr-11"></a>
### CR-11 — P2 — AUX bypass automation can introduce a dry return

**Source:** [audio_graph_runtime.rs:195](../src/audio_graph_runtime.rs#L195),
`aux_bypass_mode`, and `set_effect_bypass` at line 695;
[effects/mod.rs:639](../src/effects/mod.rs#L639), `consume_control`.

**Trigger and result:** Put an active wet generator, such as Reverb, on an
AUX and automate its bypass. Compilation chooses `DryPassthrough` for an active
effect. The explicit graph bypass API updates AUX state and recomputes whether
bypass should emit silence or a wet tail. Automation bypass instead calls the
slot directly with its cached mode. The newly bypassed wet return consequently
passes dry input, adding another copy of the source to the mix. State-dependent
interactions with other AUX wet generators also use stale topology information.

**Repair direction:** Consume bypass changes at the graph owner and update
the affected AUX bypass modes together, with bounded callback work. Parameter
control publication must preserve the same bypass semantics as structural edits.

**Regression acceptance:** Compare automated bypass with explicit graph bypass
for a single wet generator, tail on/off, and two-generator AUX chains. After
the transition, a bypassed wet-only return must not introduce dry source audio.

<a id="cr-12"></a>
### CR-12 — P2 — Imported in-Pattern tempo changes take effect one row late

**Source:** [midi_import.rs:1127](../src/midi_import.rs#L1127),
`place_tempo_commands`; [sequencer.rs:3254](../src/sequencer.rs#L3254), row timing;
[timeline.rs:348](../src/timeline.rs#L348), `musical_maps`.

**Trigger and result:** Import a tempo change exactly on an interior row
boundary. Import writes `Command::Tempo` into that row. The sequencer computes
the row duration before applying its tempo command, and the canonical tempo
map places the change at the following row boundary. Thus an event reported
as exactly quantized plays and exports one tracker row late. Row-zero changes
use `pattern.tempo` and avoid this defect.

**Repair direction:** Translate MIDI tempo boundaries into the tracker's
actual command semantics, including the preceding-row placement and Pattern
boundary cases. Keep collision handling and the import report truthful.

**Regression acceptance:** Import a file with a tempo change at an interior
row, compile/export it, and compare absolute change ticks and note times with
the input. Also cover Pattern starts and multiple changes near a boundary.

<a id="cr-13"></a>
### CR-13 — P2 — An Idea can be saved larger than the loader accepts

**Source:** [recording.rs:14](../src/recording.rs#L14), `MAX_IDEA_MIDI_BYTES`;
[recording.rs:59](../src/recording.rs#L59), `Recorder::capture`;
[recording.rs:175](../src/recording.rs#L175), Idea MIDI load;
[recording.rs:236](../src/recording.rs#L236), `save`.

**Trigger and result:** Record a sufficiently long or dense stream of musical
MIDI/CC messages. Capture grows an unbounded event vector. Save validates
message shape and timestamps but writes the encoded SMF without a size limit.
It can report a successful save larger than the loader's strict 16 MiB limit;
that Idea then cannot be reopened through the application. The capture memory
growth also precedes any opportunity to report an oversized save.

**Repair direction:** Share a save/load resource budget and enforce it while
capturing, with an explicit visible stop or preservation strategy. Reserve
space for final cleanup messages. Do not silently truncate the performance.

**Regression acceptance:** Exercise just-under/over-limit recordings with a
small injected budget. Every successful save must be loadable, and reaching
the cap must preserve already-accepted events and report what happened.

<a id="cr-14"></a>
### CR-14 — P2 — Routing cannot recover when no MIDI router was constructed

**Source:** [engine.rs:412](../src/engine.rs#L412), `MidiRouter::start`;
[ui.rs:16912](../src/ui.rs#L16912), `app.midi_router = router.ok()`;
[ui.rs:3046](../src/ui.rs#L3046), Routing activation closure.

**Trigger and result:** Launch with MIDI autoconnect disabled, or with an
initial router-construction error. The App retains `None` and fallback shared
handles. Enabling/correcting Routing only invokes `reconfigure_inputs` through
`as_mut().map(...).transpose()`. With `None`, that succeeds without opening
anything. The saved settings look accepted, but MIDI input remains unavailable
until a full application restart. The startup error specifically directs the
musician to Routing, which cannot perform this recovery.

**Repair direction:** Support constructing/recovering the router using the
App's current shared output, pickup, learning, and tracker owners. A newly
created router must not silently create disconnected replacement handles.

**Regression acceptance:** Start in the keyboard fallback, make a valid input
available, and apply Routing. Verify note/controller delivery through the
existing App owners without restart. Cover another failed attempt followed by
a successful retry.

<a id="cr-15"></a>
### CR-15 — P2 — Invalid runtime configuration prevents `shr stop`

**Source:** [main.rs:104](../src/main.rs#L104), `real_main` command dispatch;
[config.rs:475](../src/config.rs#L475), `RuntimeConfig::load`;
[engine.rs:2932](../src/engine.rs#L2932), `stop_managed`.

**Trigger and result:** A managed daemon is running and `shsynth.conf` becomes
invalid or unreadable. `shr stop` first loads that configuration and discovers
preset catalogs. Configuration failure returns before the command reaches
`stop_managed`, although stopping owned processes needs only the state
directory. This removes the normal recovery command precisely when setup is
broken. Log access is unnecessarily gated by the same initialization path.

**Repair direction:** Dispatch state-only stop/log operations before runtime
configuration and catalog loading. Retain exact process-ownership checks.

**Regression acceptance:** With a malformed configuration and a mocked owned
daemon, `stop` must reach cleanup; with an unrelated process it must still
refuse to signal it. Reading logs must not require valid instrument setup.

<a id="cr-16"></a>
### CR-16 — P1 — Screenshot generation can read private state and transmit MIDI clock

**Source:** [main.rs:104](../src/main.rs#L104), configuration/catalog loading;
[ui.rs:26531](../src/ui.rs#L26531), `readme_screenshots_json`;
[ui.rs:26911](../src/ui.rs#L26911), `screenshot_app`;
[ui.rs:1995](../src/ui.rs#L1995), App clock construction;
[loop_player.rs:73](../src/loop_player.rs#L73), `new_with_external_owner`, and
`run_controller_clock` at line 417;
[render-readme-screenshots.py:196](../scripts/render-readme-screenshots.py#L196).

**Trigger and result:** Run the screenshot command with controller clock
enabled, internal sync, and a matching output available. The renderer inherits
the process environment; `real_main` reads actual configuration and preset
catalogs. The screenshot builder passes that configuration to the normal App
constructor without disabling its clock owner. The resulting worker sends
Timing Clock on timeouts even without transport Play. This can open/transmit
to a real MIDI port during documentation rendering. Private catalog/profile
discovery and un-overridden configuration also undermine isolation and
determinism. No real MIDI transmission was attempted during review.

This contradicts the helper's explicit promise that no MIDI port or private
user file is involved ([helper contract](MAINTAINER_HELPERS.md#tui-screenshot-renderer-render-readme-screenshotspy)).

**Repair direction:** Build screenshots from an explicit fixture configuration
and inert dependencies. Dispatch before private discovery, and make it
impossible for screenshot construction to create hardware clock/router owners.
Clearing only one current configuration flag would leave the broader isolation
problem unresolved.

**Regression acceptance:** Supply enabled hardware settings and private
discovery sentinels through the environment, using mock I/O. Rendering must
perform zero private reads, port opens, or sends and produce identical frames
across host configurations. Do not verify this by sending to real equipment.

<a id="cr-17"></a>
### CR-17 — P1 — Installer recovery overwrites intervening repairs

**Source:** [managed_install.py:174](../scripts/managed_install.py#L174),
`recover`; pending resource construction in `apply` at line 279.

**Trigger and result:** Interrupt an upgrade, repair an affected file, then
rerun the installer or recover. Recovery removes every journaled target and
restores its old backup without checking the target's current fingerprint.
It overwrites the intervening repair. It also removes a target before checking
whether its required backup exists, so a missing backup can turn a recovery
error into deletion of the remaining file.

**Observed fixture:** Install `v1`, inject an upgrade failure using
`SHR_INSTALL_FAIL_AFTER=1`, replace the target contents with
`administrator repair`, then call `recover`. The target contents became `v1`.
All files were disposable fixtures, not installed system files.

**Repair direction:** Journal expected before/after fingerprints and preflight
all backups and current resources before destructive recovery. Preserve
intervening edits and return an actionable conflict. Integrate with CR-18/19.

**Regression acceptance:** Interrupt at every mutation boundary; change a
target or remove a backup before recovery. No intervening content may be
deleted, and failed preflight must leave all current resources unchanged.

<a id="cr-18"></a>
### CR-18 — P1 — Installer removal follows replaced parent directories

**Source:** [managed_install.py:320](../scripts/managed_install.py#L320),
`uninstall`; `recover` at line 174; `_check_ancestors` at line 75.

**Trigger and result:** After installation, replace a managed file's parent
directory with a symlink to another directory. Planning checks resource
ancestors, but uninstall and recovery check only the installer-state ancestry.
File fingerprinting and deletion follow the replaced intermediate directory.
If the unrelated file has the expected bytes/mode, uninstall treats it as
owned and removes it. Recovery can similarly delete or overwrite through the
redirected parent.

**Observed fixture:** After installing a disposable `usr/local/bin/shr`, replace
its `bin` directory with a symlink to a sibling fixture containing an identical
file. `uninstall` deleted the sibling's `shr` file outside the selected install
root. The fixture's installer-state directory remained ordinary throughout.

**Repair direction:** Enforce resource-parent ownership checks for recovery
and removal as well as planning. Prefer operations relative to verified
directory descriptors so a subsequent parent swap cannot bypass preflight.

**Regression acceptance:** Substitute symlink parents for installed resources
and journal targets; both operations must refuse without touching their
destinations. Include parent substitution between validation and mutation.

<a id="cr-19"></a>
### CR-19 — P1 — Overlapping installers corrupt the manifest/file relationship

**Source:** [managed_install.py:257](../scripts/managed_install.py#L257), `apply`;
`recover`, `_read_current`, and `uninstall` in the same module.

**Trigger and result:** Two install/recovery/removal operations share one root
and prefix. There is no process lock. A second apply treats the first active
transaction's pending journal as an interrupted install and rolls it back.
The first process can then resume with assumptions and backups that no longer
exist, overwriting the second installation or publishing a mismatched manifest.

**Observed fixture:** Start from files `a=v1`, `b=v1`. During a v2 apply, after
`a` has been replaced/verified and just before copying `b`, interleave a full
v3 apply, then resume v2. The first apply ends with `FileNotFoundError`; the
published manifest says v2, while installed contents are `a=v3`, `b=v2`.
The manifest fingerprint for `usr/local/bin/a` does not match the actual file.
The interleaving called real helper functions with only a copy-boundary hook.

**Repair direction:** Acquire one process lock for the complete ownership
transaction, including recovery, validation, mutation, manifest publication,
and cleanup. Distinguish an active owner from an interrupted transaction.

**Regression acceptance:** Repeat this interleaving with two subprocesses and
barriers. The second operation must wait or refuse before mutation. Also cover
apply versus uninstall and recovery after a lock-owning process dies.

<a id="cr-20"></a>
### CR-20 — P2 — Installer recovery lacks durable ordering for power loss

**Source:** [managed_install.py:27](../scripts/managed_install.py#L27),
`_atomic_json`; `_copy_resource` at line 144; journal and commit sequence in
`apply` at line 279.

**Trigger and result:** Power loss or a kernel crash during apply can leave
live-file mutations without a durable journal/backup, or a durable manifest
without durable payload contents. JSON temporary files are fsynced, but their
rename's containing directory is not. Resource/backup copies are not fsynced;
the destination is unlinked before replacement, and pending state is removed
without a durable commit barrier. Process-exception recovery tests do not
establish these guarantees. This is a source-confirmed durability gap; no
power-loss experiment was performed. Linux explicitly requires directory
synchronization to persist directory entries separately from file contents.
[Linux fsync documentation](https://man7.org/linux/man-pages/man2/fsync.2.html).

**Repair direction:** Specify and implement durable ordering: backups and their
directories, journal publication, atomic target replacements, durable manifest,
then journal retirement. Preserve old regular-file entries until replacements
are ready, and handle fsync failures as transaction failures.

**Regression acceptance:** Add focused fault-injection checks for synchronization
ordering and errors. Any filesystem crash experiment must use a disposable
filesystem and remain opt-in; never power-cycle the working machine for this.

<a id="cr-21"></a>
### CR-21 — P2 — Interrupted JACK-service installation leaves unrecognized owned files

**Source:** [audio-performance.sh:733](../scripts/audio-performance.sh#L733),
`install_jack_service`; publication at line 835; `remove_jack_service` at
line 861.

**Trigger and result:** Terminate `jack-install` after it installs the service
or config, but before installing the ownership manifest. Its ordinary command
failure branches roll back, but this path has no pending journal or active
signal-recovery transaction. On retry, the installed files are treated as
administrator-owned conflicts. `jack-remove` sees no manifest and reports that
no managed service is installed. A partial installation cannot be recovered
through either advertised command. Interruption during removal can likewise
leave a manifest whose missing companion files block another removal attempt.

**Repair direction:** Publish durable pending ownership before live files,
and provide idempotent recovery for partial install/removal while preserving
later administrator edits. Reuse the helper's established ownership principles
without starting or stopping JACK implicitly.

**Regression acceptance:** In the helper's redirected filesystem/mock-systemctl
harness, terminate after each file publication/removal and retry. Recovery must
recognize exactly its own files, preserve unrelated files, and complete without
operating a real service.

## Repair sequencing and closure

The index is the authoritative set of findings from this review. Keep the IDs
stable when implementing fixes; record the fixing commit and actual regression
evidence against each entry. A code change alone does not close a finding.
If later evidence disproves an entry, retain the ID with the reason.

Suggested order, grouped to avoid conflicting changes to the same owner:

1. Preserve user work and make recovery commands dependable: CR-01, CR-02,
   CR-03, CR-10, CR-15.
2. Isolate documentation rendering and repair audio/control ownership:
   CR-16, CR-04, CR-05, CR-11.
3. Repair runtime MIDI transitions: CR-06, CR-07, CR-14.
4. Bound loading/scheduling and correct musical round trips: CR-08, CR-09,
   CR-12, CR-13.
5. Repair installer ownership as one coherent transaction design: CR-17,
   CR-18, CR-19, CR-20; then apply those lessons to CR-21's separate helper.

The regression checks above are proposed future work, not tests added or run
by this review. Keep focused unit, contract, recovery, and safety regressions
in the normal suite. Keep exhaustive interleavings, large-memory benchmarks,
power-loss experiments, audition generation, and evidence renderers opt-in.
During the current incremental phase, compilation and test execution still
require the explicit combined-pass authorization in `AGENTS.md`.

Remaining validation limits are not additional defect claims: real JACK
shutdown/reconnection behavior, MIDI-device-specific timing, sustained disk
pressure, and audio quality require later authorized evidence. Passing existing
tests or fixing these 21 entries would not by itself prove those properties.


## Implementation record

Repair commit: `e6726d87dd745a0560561a06cd9510594f931307`.
All IDs below refer to that implementation commit. The original source links
above continue to refer to the reviewed revision, as stated at the beginning.

| Findings | Implemented repair | Regression/source evidence prepared |
| --- | --- | --- |
| CR-01 | Rename saves the current Song, including same-stem edits | Extended Project rename regression checks current cells/order in memory and on disk, plus existing invalid-name/collision cases |
| CR-02 | Snapshot errors stop before backup/mutation; recovery errors propagate | Routing rollback regression, learned-save read-failure regression, and shared snapshot/restore checks |
| CR-03 | Preflight mixed final/temporary stems; separate recovered and failed outcomes | New recovery test covers every stem-publication boundary and repeat recovery; existing unsafe-manifest/symlink checks updated |
| CR-04 | Callback-only DSP uses explicit interior mutability; owner caches meters and kit ID before activation | Callback ownership/accessor audit and new hardware-free concurrent meter-read test; aliasing-checker execution remains pending |
| CR-05 | Graph and drums retain separate control-owner identities through scheduling/publication | Existing owner-clearing test now deliberately collides IDs; timeline test asserts drum owner survives scheduling |
| CR-06 | Stopped sequencer acknowledges configuration replacement and retires old destinations | New same-worker configuration regression; mocked physical destination delivery remains pending |
| CR-07 | Close/join producers before draining notes, including partial activation failure | Reconfigure/Drop caller audit; local midir ALSA `close_internal` and Drop confirm thread join precedes return; barrier-controlled MIDI delivery execution remains pending |
| CR-08 | Every schedule/automation event append checks its budget before allocation growth or sorting | New low-budget repeated-Pattern and remaining-effect-budget regressions |
| CR-09 | Descriptor-checked regular-file reads enforce byte limits before decode | New maximum/over-limit, sparse-file, and nonregular-input regression; load/rename/overwrite callers audited |
| CR-10 | Writer arms capture after startup; terminal failure cannot be overwritten; producer quiescence permits ring cleanup | New forced-startup-failure/callback/retry regression; existing writer-failure fixtures retained |
| CR-11 | Graph consumes bypass before DSP; late bypass publication remains pending for the next callback | New automated/explicit AUX comparison with one and two generators; existing wet-tail/bypass regressions retained |
| CR-12 | Interior tempo commands precede their effective row; conflicting quantized boundaries refuse conversion | New imported-tempo/canonical-timeline boundary regression; existing import quantization tests retained |
| CR-13 | Capture reserves cleanup within a shared save/load budget and visibly stops with accepted events preserved | New injected-budget capture/cleanup/SMF decode regression; save computes encoded size before allocation/publication |
| CR-14 | Startup fallback retains an inactive router with the same shared App handles | New inactive-owner retention regression; Routing success fixture supplies an inert router; hardware recovery execution remains pending |
| CR-15 | Stop/log dispatch precedes runtime config and preset discovery | Source/caller audit; exact owned-process cleanup remains unchanged |
| CR-16 | Early fixture-only screenshot dispatch, discovery-free constructor, inert clock, compiled drum catalog | New enabled-hardware/discovery-isolation constructor regression; screenshot scenario calls audited for private discovery; no screenshots generated |
| CR-17 | Recovery checks all backups and before/after resource fingerprints before mutation | New intervening-repair and missing-backup fixtures |
| CR-18 | Recovery/removal use no-follow pinned parent descriptors, including state | New redirected-parent and post-preflight parent-swap fixtures |
| CR-19 | One process lock covers the complete transaction; planning stays read-only | New competing-process apply/remove/recover and lock-owner-death fixtures |
| CR-20 | Durable backup, journal, atomic resource, manifest, and retirement ordering | New fsync-failure and durable-commit/interrupted-retirement fixtures; no power-loss experiment |
| CR-21 | Durable pending service ownership and idempotent partial install/removal recovery | Added redirected-filesystem fixtures for every publication/removal boundary and intervening administrator edits |

Validation actually performed for the repair: Rust formatting/parse checks,
Python `py_compile`, Bash syntax, ShellCheck on both changed shell files,
source/caller inspection (including the installed midir dependency's shutdown
contract), and `git diff --check`. These passed. None of the added or retained
regression tests has been executed for this repair.

The repository's explicit combined-pass restriction remains in force. Rust
compilation, Cargo check/test, Python and shell regression execution, Clippy,
the normal production suite, historical/exhaustive tests, benchmarks, image
rendering, and live JACK/MIDI/audio/hardware checks were intentionally skipped.
The next authorized combined pass should run locked check and the focused
persistence, recorder recovery/startup, callback/control, routing, timeline,
import, capture-budget, and screenshot-isolation regressions, plus
`scripts/test_managed_install.py` and `scripts/test-audio-performance.sh` in
their disposable mocked roots. Full-suite or hardware validation still requires
the separate authorization defined in `AGENTS.md`.
