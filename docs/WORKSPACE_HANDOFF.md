# Workspace handoff

This file contains only current machine state and decisions that must survive a
new thread in `$HOME/p/shsynth`. Durable repository policy is in
`AGENTS.md`; detailed helper behavior is in `docs/MAINTAINER_HELPERS.md`. Never
record credentials, GitHub device codes, or private file contents here.

## 2026-08-29 development Pi returned to four shared CPUs

The owner chose build throughput and simpler general scheduling over the
optional dedicated-JACK CPU profile on this four-core Pi. The owned
`shr-audio-tune remove` path removed CPU-3 boot isolation, IRQ housekeeping,
the performance-governor service, and the JACK affinity drop-in. The matching
private `audio.engine_cpu` value is cleared. The original boot-command-line
backup remains under the helper's owned state directory.

The already-running JACK server was not stopped or restarted and remains on
CPU 3 for the rest of the current boot. The running kernel likewise still
reports CPU 3 as isolated, so `nproc` remains 3 until reboot. Persistent boot
configuration and systemd configuration are already unpinned; after the next
normal reboot, ordinary builds and JACK share all four CPUs while JACK retains
its existing real-time priority and memory-lock policy. The optional tuning
helper remains available if measured 18-channel work later justifies restoring
a dedicated core.

## 2026-08-29 literal rotary order and legible MIDI Learn feedback

The private 293-line retry trace showed that the old order caused a real
identity error: Learn requested rotary 9's click before its turn, then called
the lower-left rotary-9 turn “rotary 2” and stored CC 114 in physical slot 2.
The owner then returned to click 9 with rotary-1 navigation, but click roles
were missing from the mapped-role replacement check. CC 115 was therefore
rejected as already used on every press instead of replacing itself.

The attempted physical-column order was rejected by the owner as needlessly
awkward. Learn now proceeds literally from rotary 1 through rotary 16. Rotary
1's click and Shift work remain beside rotary 1, and rotary 9's click follows
rotary 9's turn before rotary 10. Both click roles remain replaceable when
revisited, including the secondary synth-click alias owned by rotary 9.

The following 542-line private trace also showed why rotary 3 appeared unable
to learn. Only 120 ms after its left stream became quiet, Learn requested RIGHT;
a late left packet arrived 8–13 ms later and was immediately shown as a new
direction error. Gesture transitions, success, and retry now use a 650 ms quiet
window. Late matching packets extend that window. Per-packet proof counts no
longer replace visible feedback, so one turn produces one legible state before
the next instruction.

Exact Rust 1.97.1 passed formatting, locked check, both in-app Learn integration
tests, the numeric-order, feedback-timing, and click-9 regressions, and all 44
controller-Learn tests. Locked DEV and REL builds passed in 1m04s and 2m58s
while the running kernel still exposed only three build CPUs. The complete suite
and unrelated opt-in tests were intentionally not run for this focused
incremental repair.

## 2026-08-29 controller-only MIDI Learn recovery

The private 906-line last-session trace proved two controller-only recovery
failures. Rotary 5 reached left proof 2/3, received one opposite packet, and
entered a permanent rejected state whose visible recovery required keyboard
`R`. Rotary-1 step navigation later worked in both directions, but returning
to already-mapped rotary 4 made that step read-only: subsequent CC 76, 77, and
93 turns were all ignored even though the screen said the step was armed.

Rejected rotary attempts now wait for release and automatically re-arm the
same step. No red error prompt names a keyboard key. Navigating to an already
mapped step keeps its mapping until the first relevant replacement gesture;
that gesture then replaces the old role mapping and learns normally. Rotary 1
left/right remains the only step minus/plus gesture and needs no click.

Exact Rust 1.97.1 passed formatting, whitespace inspection, locked check, both
trace-derived controller-only regressions, and all 41 controller-Learn tests.
Locked DEV and REL builds passed. The complete suite and unrelated opt-in tests
were intentionally not run for this focused incremental repair.

## 2026-08-29 rotary-only MIDI Learn step navigation

After rotary 1's left and right directions are learned, its turns now change
the selected Learn step directly: left is one step back and right is one step
forward. No click is involved. The already-learned rotary-1 axis is the lower
navigation boundary and Review is the upper boundary. Each step change enters
the existing short input quarantine, so the same physical gesture cannot cross
multiple steps. Navigation works from both an ordinary waiting step and a
rejected rotary step and discards transient direction proof when leaving a
step. The later controller-only recovery repair makes a revisited completed
step editable by its next relevant gesture instead of leaving it read-only.

Exact Rust 1.97.1 passed formatting, whitespace inspection, locked check, the
focused no-click minus/plus navigation regression, and all 39 controller-Learn
tests. Locked DEV and REL builds passed. The complete suite and unrelated
opt-in tests were intentionally not run for this focused incremental repair.

## 2026-08-29 MIDI Learn unrelated-CC proof repair

The private last-session trace showed rotary 2 reach two of three valid left
packets on its Relative 1 CC, after which an unrelated positional CC 1 stream
forced the entire role into rejection. Once a performance rotary candidate has
started either direction proof, Learn now ignores other channel/CC streams and
retains the candidate's progress. Wrong-direction or positional values from
the candidate itself remain rejected, so the relative-only safety contract is
unchanged. A focused regression reproduces the observed two-packet proof,
unrelated 1–86 sweep, and successful completion on the original CC.

Exact Rust 1.97.1 passed formatting, whitespace inspection, locked check, the
new focused regression, and all 39 controller-Learn tests. Locked DEV and REL
builds passed; plain `shr` will use the refreshed release binary after the
currently running older process is exited normally and reopened. The complete
suite and unrelated opt-in tests were intentionally not run for this focused
incremental repair.

## 2026-08-29 MiniLab mkII Memory 2 rotary-mode repair

The owner's MIDI Learn trace showed that the next requested performance rotary
sent a positional CC 74 sweep and was correctly rejected by the relative-only
learner. A direct Linux ALSA/SysEx audit of the active MiniLab mkII Memory 2
found fourteen performance rotary slots in Absolute mode; only the CC 114
rotary was already Relative 1. The pre-change mode, channel, CC, option, and raw
replies were backed up privately below `user/state/shsynth/`.

After the owner's explicit authorization to program the controller directly,
only those fourteen rotary option fields were changed from Absolute to
Relative 1. No CC, channel, button, Shift-layer, or other memory field was
changed. An independent readback returned Relative 1 for all fifteen
performance rotary slots. No JACK, synth, audio, playback, or recording path
was started.

## 2026-08-29 MIDI Learn delayed-release and shifted-left repair

Physical review found two MIDI Learn capture failures in the current working
tree. A launcher-click CC release arriving after the opening quiet timer could
be stored as rotary 1's first command. Armed direction capture now accepts only
moving relative CC values, so delayed zero releases and neutral/reset packets
cannot become that mapping.

The MiniLab mkII direct Shift-layer disambiguator also treated a Relative 2
left packet (`127`) as a possible Shift-button press on every other packet, so
its three-packet left proof could never finish. A repeated packet from the same
channel/CC now identifies the candidate as the shifted rotary stream, retains
the first packet, and lets the proof proceed; an actual press/release remains
ignored.

Every in-app Learn entry now replaces the private
`midi-learn-last.log` below the configured state directory. It records the
requested step, internal capture state, and every received raw MIDI packet in
hex, including filtered and rejected traffic, and remains available after Save
or Cancel. This diagnostic performs file I/O only on the ordinary UI-thread
Learn path, never in the MIDI or audio callback.

Source regressions cover the delayed release, the three-packet Relative 2
shifted-left path, and exact last-session trace replacement/input retention.
Formatting and whitespace inspection passed.
The owner then explicitly authorized the combined build-and-test pass. Exact
Rust 1.97.1 (`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6) passed
locked check, all 38 controller-Learn tests, and both in-app Learn integration
tests. The final locked debug and release builds passed in 1m42s and 3m00s. The build
retained three non-fatal dead-code warnings. No app launch, live MIDI/JACK,
audio, or physical follow-up ran.

## 2026-08-28 performance-rotary direction proof

A subsequent physical retry exposed two interaction regressions around that
proof. The first line spent scarce width on `MIDI LEARN ·`, so the required
Shift gesture could be clipped. Learn now uses exactly two rows total, with no
shared transport/status footer: line 1 is the complete action-first gesture
(`SHIFT + TURN ROTARY 1 LEFT`, for example), and line 2 is only its immediate
state or recovery. Both rows use all 40 terminal columns.

The learned master rotary no longer lets delayed packets from one slow gesture
walk across several optional rotary or button roles. The later rotary-only
navigation repair supersedes the earlier decision to ignore master turns:
each deliberate turn changes exactly one step and re-enters the 120 ms input
quarantine. Waiting on an untouched Shift, rotary-9 click, or PAD role still
never changes it.

The optional Shift layer now follows the same unambiguous direction proof as
the other relative controls without requiring an unnatural continuous hold.
The visible sequence is exactly `SHIFT + TURN ROTARY 1 LEFT`, `RELEASE SHIFT`,
`SHIFT + TURN ROTARY 1 RIGHT`, `RELEASE SHIFT`. Releasing after the left action
preserves its three-packet proof; the right action begins with a fresh Shift
press and proves three opposite-direction packets on the same channel and CC.
The Shift layer's MIDI encoding is inferred independently: physical left may
arrive as low centered values, high centered values, or high/low Relative 2
values, and physical right must prove the corresponding opposite range. It is
not assumed to match the ordinary rotary. Bare `TURN`, one-sided learning, and
holding Shift across both actions are no longer valid.

Physical acceptance found that only rotary 9's **turn** behaved correctly. The
other learned performance rotaries were positional MiniLab mkII knobs which the
one-packet learner had mistaken for relative controls when their 0–127 sweep
passed through values near 64. Runtime then treated those occasional values as
signed steps, causing intermittent changes, reversals, and apparent resets.

Performance Learn now proves the same relative contract already used by the
master encoder. For every rotary 2–16 it asks for a left turn, requires three
left-direction packets, waits for that gesture to become quiet, then asks for
three right-direction packets on the same MIDI channel and CC. Both Relative 1
and Relative 2 conventions are supported. A positional, wrong-direction, or
different-control packet cannot save a mapping. The later controller-only
recovery repair supersedes the explicit retry requirement: after release the
same step automatically re-arms. The screen remains exactly two rows total and
changes its first line from `TURN LEFT` to `TURN RIGHT` during the proof.

No private controller configuration was repaired or rewritten. The currently
running process also still maps the prior release image. On the next normal
owner-controlled restart, rotaries which remain positional must be changed to
Relative 1/2 in the hardware editor and learned again; unsupported positional
knobs will now be refused honestly. Rotary 9's known-good turn is the physical
reference, not its click. The natural Shift-release follow-up passes all 36
focused Learn tests, both native two-line/40-column UI regressions, locked
check, deterministic docs generation, and the optimized release build. The
complete normal suite was intentionally not repeated under the incremental
debug gate; its preceding run passed 1,092 tests with 13 historical or audition
tests ignored before this focused interaction change. No connected MIDI/JACK
test, app restart, or audible test ran.

## 2026-08-28 Moj model-local catalog and stale-host repair

The Moj Presets list no longer sorts by translated category labels or exposes
the old global factory-file numbers. It uses the fixed model order Model D,
Six-Op PM, Strange Oscillator, Swarm Machine, Bass Matrix, then Dual Filter.
Each model has one visible letter and its own two-digit sequence: `D01`, `P01`,
`O01`, `S01`, `B01`, and `F01`. The visible sound name follows that identity.
Opening or switching to Moj Sint resets the cursor to `D01 Full Bass`; visible
letter-jump uses those same model letters. This removes the misleading initial
`16 Bass Matrix` selection caused by alphabetical category sorting.

The observed Dual Filter `START FAILED` was a source/artifact compatibility
failure, not a bad private configuration. SHR discovered schema-8 presets from
the current Moj source tree but launched a Moj release executable last built
before schema 8; that executable rejected the selected preset as unsupported.
The Moj release executable was rebuilt from current source. All 21 cleared
presets now pass its offline validator, and two production Dual Filter renders
with identical inputs are byte-identical. The active SHR and synth processes
were not stopped, restarted, connected, or exercised by the agent.

Focused Moj catalog, visible-name, letter-jump, route, and engine-replacement
regressions pass. The complete normal SHR suite passes 1,084 tests with 13
historical/opt-in tests ignored. That pass also exposed and fixed a Project
format-18 decoder range which still stopped at format 17, plus stale
relative-only/3×5 test expectations. The locked release build and the real
40×13 Presets documentation image were refreshed. The currently running SHR
process still maps the previous executable; one normal user-controlled exit
and reopen is required for the new list presentation.

## 2026-08-28 relative-only controller contract

The active controller contract has no absolute rotary mode. Rotary 1 and all
fifteen mapped performance rotaries must emit direction-only Relative 1 or
Relative 2 steps; MIDI Learn rejects positional 0–127 turns and tells the user
to change the hardware mode. The runtime no longer stores, saves, learns, or
decodes `rotary.relative`, `encoder.absolute`, or
`encoder.modified_absolute`. Those old v9 keys are migration-only input: known
misclassified MK2 mappings keep their relative identities, while unsupported
positional surfaces are dropped instead of being silently reinterpreted. The
next explicit save omits the obsolete keys.

The returned MiniLab 3's positional parameter knobs were removed from its
bundled mapping. Its relative master, buttons, and pads remain described, but
SHR does not pretend its positional knobs satisfy the new direction-only
surface. The current MiniLab mkII owner mapping remains the intended 1+15
surface. Parameter, FX, aux-send, tracker-mixer, and automation turns therefore
carry SHR's current value directly and do not expose pickup/catch status.

Owner acceptance against the previously built release exposed the exact stale
failure this contract removes: Learn saved the MiniLab mkII's repeated `63` and
`65` direction packets as absolute positions, so the master menu moved once in
each direction and then appeared trapped. Both the active private selector and
the model-owned MK2 mapping have been corrected to relative mode. Current
source has no absolute Learn path, and regression requires every repeated
identical direction packet to navigate.

The first authorized focused run then exposed a second stale-draft defect in
the MK2 Shift step: validation wrote a candidate CC before rejecting a plain
Shift-button packet, so retry could retain that rejected CC. Direct Shift
learning now validates the packet before changing the draft. A Shift press and
release leaves no mapping, while the following actual shifted turn becomes the
alternate relative CC.

The owner-authorized combined pass used exact Rust 1.97.1. Formatting and
locked check passed. All 29 controller-Learn tests, both in-app save/activate
tests, three repeated/relative encoder tests, three Home/two-line Learn tests,
the obsolete-mode migration tests, and the observed MK2 migration test passed.
The locked optimized release build passed and refreshed `target/release/shr`,
which plain `shr` launches. No app launch, MIDI/JACK transmission, synth,
audio, or fresh-binary physical-controller validation ran.

## 2026-08-28 unified 3×5 synth and aux surface

The learned MiniLab mkII performance surface is now one exact 15-rotary
contract after the separate master encoder. Moj Sint Dual Filter already had
its complete schema-8/CC20–34 host path and continues to own all fifteen slots
as synthesis controls. Synthv1 and the five older 12-control Moj models now
use slots 1–12 for their unchanged sound parameters and slots 13–15 for the
current Project's AUX 1, AUX 2, and AUX 3 send levels. Those last messages are
consumed as SHR Project controls and are never forwarded into the synth MIDI
namespace. Rotaries carry the current send in 3 dB steps. A missing/empty aux honestly renders `NO FX`
and refuses the turn until an effect exists.

Player and FT2 PARAM now share one native 3×5 renderer. Older instruments show
their twelve parameters followed by the three aux sends; Dual Filter shows its
fifteen model parameters with no aux substitution. The FX target inventory and
bounded audio graph now expose three independent wet aux buses. Project format
18 and typed graph format 2 make that expanded contract explicit; older Project
versions remain readable. The maximum graph-wide effect count remains 16 and
the reverb limit remains two.

Each compiled aux send now owns a lock-free linear-gain target. Surface turns
publish that target through a 10 ms callback ramp without graph deactivation,
allocation, locking, or callback-time dB conversion, so held notes and running
transport keep sounding. OFF retains a disabled prepared route and can come
back without a topology rebuild. Recording still refuses send changes. Aux
rack, processor, tap-point, and other structural edits retain the stopped
transport/recording publication invariant. With the graph disabled, surface
turns change Project state without touching audio.

Regression source covers the exact 12+3 and 15+0 routing split, non-forwarding
of aux turns, three-bus allocation/validation, format-18 round trip, shared
3×5 rendering, relative send steps, and Dual Filter retention
of rotary 16. Formatting and whitespace validation passed. The combined pass
recorded above compiled this work and built it into the optimized release, but
did not run the separate focused aux/audio regressions or the complete suite.
No live MIDI, JACK, synth, playback, recording, audible, or
physical-controller validation has run for this change.

## 2026-08-28 MIDI Learn interaction repair

The in-app MIDI Learn screen originally owned exactly two body lines: the current
physical action and one immediate instruction/result. The previous title,
progress fraction, isolation commentary, gesture summary, mapping counts,
required-control ledger, navigation legend, cancel prose, and save commentary
were removed. The current action-first/two-rows-total contract at the top of
this handoff supersedes the shared-row behavior from this earlier repair.

Learn now orders all rotary-1 work together: left, right, click, and optional
Shift+turn. The later numeric-order repair proceeds through rotary 2 to rotary
16 and inserts rotary 9's click immediately after its turn.
For a hardware-owned Shift layer, a second CC arriving during the Shift
gesture replaces a premature Shift-button candidate with the actual shifted
rotary CC. Once learned, that Shift identity is already reserved and cannot be
captured as rotary 9's click. Rotary-1 click saves only at the final Review
step instead of ending the session as soon as the three required controls
happen to exist. The current controller workflow requires no keyboard input.

Focused regression source covers the two-line 40×13 contract, reordered Shift
and rotary-9 capture, Shift-candidate replacement, refusal to reuse Shift as a
click, and final-step-only save. The combined check, focused tests, and release
build are recorded in the relative-only section above. No app launch, live
MIDI, JACK, synth, playback, recording, audible, or fresh-binary physical-
controller validation has run.

## 2026-08-27 MiniLab mkII direction-only encoder repair

Physical acceptance showed that the learned MiniLab mkII rotary 1 behaved as a
direction-only encoder, although MIDI Learn had saved it as absolute. The
learner had required a neutral/reset packet between the sampled left and right
values before recognizing Arturia Relative 1 or 2. It now recognizes those
direction pairs with or without the neutral packet. The relative-only
correction at the top of this handoff now also governs the MK2 hardware-owned
Shift layer.

The already-saved private MK2 mapping is corrected in memory when it has the
observed erroneous `CC112`/absolute signature; loading never rewrites that
private file. The owner confirmed that this personal MK2 setup uses
direction-only turns for every rotary. The MK2 mapping therefore retains each
physical rotary identity but discards its incoming position: left/right and
turn-speed packets carry SHR's current synth, Moj Sint, FX, automation, or
mixer value by signed steps. They bypass pickup entirely.

A passive live capture was opened without transmitting MIDI or starting audio,
but received no physical turns; the direction-only behavior is owner-provided
physical evidence rather than a captured transcript. Exact Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6) passed locked check
after rebasing the working tree onto `7fa9872` (`new controller prepare`). The
merge makes the one learned rotary 9 click serve both its physical identity and
the new Dual Filter synth action; Learn never asks for that same click twice.
The focused Learn, controller-decoder, MIDI routing, signed-carry, FX
pickup-bypass, Dual Filter click, preset, and native-render regressions passed.
The complete normal suite reported 1,080 passed, zero failed, and 13 unrelated
opt-in audio/maintainer tests ignored. The owner's explicit all-tests request
then ran those 13 with fresh destinations below ignored `user/`; all passed.
Locked debug and release builds passed in 2m09s and 2m59s. Formatting,
controller JSON, generated documentation and drift check, focused generator
tests, and whitespace validation passed. No MIDI transmission, synth, JACK,
playback, recording, audible, or physical post-build acceptance ran.

## 2026-08-27 per-model controller retention and MK2 Shift repair

The controller replacement workflow now retains every explicitly learned known
model below private state as `controller-mappings/PROFILE-ID.conf` while
`controller.conf` remains only the active selector. Startup restores the sole
connected reviewed model's private mapping before falling back to its bundled
catalog default. Explicit Learn updates the active and model-owned copies as
one recoverable operation; Cancel writes neither, and automatic device
switching never overwrites another model's retained mapping. The old MiniLab 3
mapping remains present in both its reviewed bundled profile and the existing
private timestamped backup.

Arturia documents that MiniLab mkII hardware Shift selects alternate CCs for
encoders 1 and 9. This first implementation accepted a positional alternate
CC; the relative-only correction at the top of this handoff supersedes that
mode.

The owner-authorized combined pass used exact Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6). Formatting,
locked check, 25 focused Learn tests, 12 controller-profile/automatic-switch
tests, direct MK2 Shift decoding, both in-app save/cancel regressions, and the
complete normal suite passed. The final suite reported 1,072 successful tests,
zero failures, and 13 unrelated private-audition/maintainer tests ignored.
Locked debug and release builds passed in 1m44s and 2m54s. The generated docs
site and its focused tests passed. No app restart, live MIDI capture or
transmission, JACK, synth, playback, recording, audible, or
physical-controller acceptance ran for this follow-up.

## 2026-08-27 MiniLab mkII sixteen-rotary surface

The owner chose the MiniLab mkII as the project's controller surface: sixteen
rotary turns, with clickable rotaries 1 and 9. MIDI Learn now captures rotary
1 left/right/click, rotary 9 click, and turns for rotaries 2–16. The
left/right capture originally distinguished positional 0–127 navigation from
Arturia Relative 1/2. The owner's physical acceptance later established that
this MK2 memory sends every parameter rotary as direction-only; the current
relative-only behavior is recorded at the top of this handoff. Existing instruments
use their twelve verified slots, while Dual Filter uses all fifteen and maps
rotary 9's click to its core toggle. New saves use controller profile v9
`rotary.2` through `rotary.16`; v8 `pot.1` through `pot.12` remains readable
and migrates on the next explicit save.

This is new work and remains uncommitted pending review. The owner-authorized
combined pass used exact Rust 1.97.1 (`8bab26f4f68e0e26f0bb7960be334d5b520ea452`,
LLVM 22.1.6). Formatting, locked check, 24 focused controller-learn tests, two
focused in-app/render regressions, the complete normal suite, and locked debug
and release builds passed. The final suite reported 1,067 successful tests,
zero failures, and 13 documented private-audition/maintainer tests ignored.
The explicitly requested opt-in pass then ran those 13 tests with fresh
destinations below ignored `user/`; all 13 passed and produced 60 MiB of
private review evidence. Debug and release builds completed in 1m51s and
2m52s. No physical MIDI, JACK, synth, playback, recording, audible, or hardware
acceptance ran.

## 2026-08-27 MiniLab mkII replacement discovery repair

The borrowed MiniLab 3 has been returned and the owner now has an Arturia
MiniLab mkII. Linux exposes its one input as `Arturia MiniLab mkII MIDI 1`; the
existing private controller/runtime selections still name the absent MiniLab 3
until a repaired binary next starts.

Startup controller discovery now preserves an available configured controller,
but when every saved selection is offline it automatically adopts one exact
connected endpoint only if that endpoint is the sole match for a reviewed
controller profile. It rebuilds from the reviewed profile instead of copying
the absent device's learned messages. The MK2 has a partial identity/eight-pad
profile with Arturia-manual provenance and deliberately no MIDI assignments, so
Home selects usable MIDI Learn without risking stale MK3 Stop/Play/REC/Panic
commands. Unknown replacements and multiple reviewed candidates remain
unselected.

The owner-authorized combined pass used exact Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6). Formatting, JSON,
references, whitespace, locked check, all nine controller-profile tests, the
two focused Home/keyboard Learn regressions, and the complete normal suite
passed. The suite reported 1,063 successful tests, zero failures, and 13
documented development/audition/performance tests ignored. Locked debug and
release builds passed in 2m08s and 2m51s; plain `shr` resolves through
`scripts/local.sh` to the new `target/release/shr`. No app restart, MIDI
capture/transmission, synth, JACK, playback, recording, audible, or physical-
controller verification ran.

## 2026-08-27 Moj Sint Dual Filter controller integration

The approved Moj Sint Dual Filter design is implemented across the owning
repositories. SHR recognizes the sixth Moj model and schema 8, maps its 15
continuous controls to physical rotaries 2–16, uses rotary 9's learned click as
the synth action, and keeps the master rotary exclusively on navigation. One synth
click sends the press-only core toggle; held sound is not retriggered, all pot
values remain in place, and the parameter header plus status show
`CORE: INDUSTRIAL` or `CORE: COUNTER`.

Dual Filter preset load/reset sends exact CC20–34 values followed by persisted
core state on CC36. Save New/Overwrite writes strict schema 8 with the exact
dual-filter control names and selected core. Older Moj models retain their
12-position mappings, including shared volume on position 5, and schemas 1–7
remain readable. Controller profiles/config now accept POT 1–15 and optional
`synth.press_cc`, `synth.press_note`, and `synth.press_channel`; Learn stores
the rotary 9 click under both its physical and synth-action identities.

The later owner-authorized combined pass is recorded in the direction-only
encoder repair section above. No JACK, ALSA synth launch, MIDI hardware,
physical controller, audible, or Raspberry Pi evidence was produced for the
Dual Filter merge.

## 2026-08-26 Priority 7 external USB MIDI transport sync

Priority 7 from `SEQUENCER_WORKFLOW_PRIORITIES.md` is implemented. The exact
MIDI byte, source-resolution, acquisition/filter/phase, Start/Stop positioning,
loss/reacquisition, output interaction, subsystem ownership, UI state, and
acceptance contract is in `EXTERNAL_TRANSPORT_SYNC_ACCEPTANCE.md`.

Routing now owns machine-only `SYNC` internal/external selection, one exact
stable `SYNC IN`, and Arrangement/Pattern `SYNC POS`. External mode follows
Timing Clock at 24 PPQN from only that resolved source. Seven clocks establish
tempo; the tracker supports 20.00–300.00 BPM through a 24-interval median,
bounded 1/8 smoothing, two-percent interval slew, one-eighth-pulse phase
correction, 2 ms delivery-burst tolerance, and a 500 ms loss deadline. Stop,
loss, source replacement, refusal, or bounded malformed-input fault requires
reacquisition plus a fresh Start. Continue, Song Position Pointer, clock thru,
and implicit internal fallback remain absent.

Arrangement Start is step 1/row 1; Pattern Start is the selected Pattern/row 1,
including the selected shaped Live Pattern. Playback uses the existing single
sequencer and cleanup owners. The transient playback clone substitutes the
filtered tempo and ignores Tempo commands without changing Project data or
duration. Swing, groove, microtiming, REC FEEL, lanes, probability/conditions,
PRE/FILL, retrigger, automation, Live launch boundaries, and Loop placement
retain their event-level ownership. External mode suppresses tracker clock/
Start/Stop output and fully suspends optional controller-clock pulses; the next
internal Play resumes configured output. Stopped external REC and every local
internal preview/play owner are visibly refused until the musician selects
internal sync or an acquired external Start owns transport.

Project format 17 is unchanged. Runtime configuration is version 6 and older
configuration migrates to disabled internal sync with Arrangement Start.
Routing browsing, Cancel, refusal, missing/ambiguous source, loss, Stop, and
failed validation do not write Project, Pattern History, Arrangement, Pattern,
structure, or dirty state.

The owner-authorized non-Raspberry-Pi pass used exact Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6). Formatting and
`cargo check --locked` passed. Focused parser/follower, exact-source, owner/
output, Start/Stop/restart, Routing transaction, 40x13/controller layout,
scheduler, preflight, export, transport, and count-in groups passed. The
complete `cargo test --locked` suite passed with 1,063 successful tests, zero
failures, and 13 documented private-audition, hardware, and maintainer tests
ignored. Clippy was not required by a failure or repository policy.

No JACK, ALSA sequencer port, synth, MIDI transmission, playback, recording,
audible, screenshot, physical-controller, real USB clock, or Raspberry Pi
timing evidence was produced. Real source hot-plug/replacement, USB jitter,
stuck-note cleanup, every physical controller layout, native display, Loop/
metronome phase, recording feel, and audible musical acceptance remain human
hardware work.

## 2026-08-26 Priority 6 harmonic generators

Priority 6 from `SEQUENCER_WORKFLOW_PRIORITIES.md` is implemented. The exact
musical semantics and HG-01 through HG-13 acceptance matrix are in
`HARMONIC_GENERATORS_ACCEPTANCE.md`. The existing FT2 Tools PAGE -> HISTORY ->
RHYTHM -> GEN workflow now also drafts cursor-row arpeggios with explicit
order/octaves/rate/gate/repetitions, Project-key diatonic triads with explicit
degree/inversion/close-or-open voicing/lane/rate scope, and bounded diatonic
third/fifth harmony voices with explicit lane/direction/out-of-scale policy.

Priority 6 reuses the Priority 5 cloned draft, exact affected/replacement/
collision/protected reporting, four controller pages, stopped current-Pattern
History transaction, and existing independent Pattern plus appended
Arrangement-step Clone owner. Opening, adjusting, inspecting, Repeat, Cancel,
refusal, validation failure, and no-op Apply preserve Pattern, automation,
History, dirty state, Arrangement, and FT2 cursor. HARMONY remains the separate
read-only browser.

Project format 17 and reusable drum-pattern format 4 remain unchanged. Only
ordinary generated Cells persist, so MIDI export, preflight, partial playback,
Pattern/Arrangement repeats, probability/conditions, PRE/FILL, swing/groove,
REC FEEL, lane cycles, seeded mutation, and controlled fills retain their
existing owners without playback-time harmonic regeneration.

The owner-authorized non-Raspberry-Pi pass used exact Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6). Formatting and
locked check passed without warnings. The requested focused Priority 6,
migration, Pattern History, Priority 2-5, scheduler/preflight/export/partial/
repeat, navigation/controller, native 40x13, shared-status, and HARMONY matrices
passed. The complete normal suite passed with 1,047 successful tests, zero
failures, and 13 documented development, private-audition, and performance
tests ignored. Clippy was not required by an observed failure or repository
policy.

No JACK, ALSA sequencer, synth, MIDI transmission, playback, recording,
audible, screenshot, physical-controller, Raspberry Pi timing/headroom, or
other hardware-changing evidence was produced. Musical approval remains a
human listening/controller decision.

## 2026-08-26 Priority 5 deterministic generative tools

Priority 5 from `SEQUENCER_WORKFLOW_PRIORITIES.md` is implemented. The exact
musical semantics and GT-01 through GT-14 acceptance matrix are in
`DETERMINISTIC_GENERATIVE_TOOLS_ACCEPTANCE.md`. FT2 Tools PAGE -> HISTORY ->
RHYTHM -> GEN reaches a selected-lane offline draft for Euclidean triggers,
bounded accumulator progressions, seeded pitch mutation, and percussion-only
controlled FILL cells through the existing four controller pages.

The draft retains its seed where randomness is used and shows exact affected
rows, replacements, collisions, and protected cells without changing the
Song, History, dirty baseline, Arrangement, cursor, or transport. Stopped
Apply to the current Pattern is exactly one Pattern History transaction. Apply
to Clone uses the existing independent Pattern plus explicit appended
Arrangement-step structural owner and never overwrites the source. Cancel,
refusal, validation failure, and no-op Apply are non-writing.

Priority 5 adds no persisted recipe or seed. Project format 17 and reusable
drum-pattern format 4 persist only the generated ordinary Cells, so existing
migrations remain unchanged and playback never regenerates. Context-free MIDI
export remains deterministic pass 1 with FILL off; preflight continues to scan
all conditional source triggers.

The owner-authorized non-Raspberry-Pi pass used exact Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6). Locked check; the
requested focused generator, migration, Pattern History, Priority 2-4,
scheduler ownership, export/preflight/partial/repeated playback, navigation,
controller, 40x13, and UI transaction matrices; and the complete normal suite
passed. The final suite reported 1,038 passed, zero failed, and 13 ignored
documented development, private-audition, and performance tests. Validation
shortened the launcher to `GEN` to meet the established soft-button width.
Clippy was not required by an observed failure or repository policy.

No JACK, ALSA sequencer, synth, MIDI transmission, playback, recording,
audible, screenshot, physical-controller, Raspberry Pi timing/headroom, or
other hardware-changing evidence was produced. Musical approval remains a
human listening/controller decision.

## 2026-08-26 Priority 4 independent lane playback

Priority 4 from `SEQUENCER_WORKFLOW_PRIORITIES.md` is implemented. The exact
musical semantics and LC-01 through LC-27 acceptance matrix are in
`LANE_PLAYBACK_ACCEPTANCE.md`. Every Pattern lane now owns FULL or an explicit
cycle length, 1/4X through 4X rate, and forward, reverse, pendulum, or bounded
deterministic-variation playback. Pattern tempo, row markers, automation, Loop
Mix, MIDI clock, and Arrangement duration remain Pattern-time owners; lane
drafting and Apply preserve the FT2 cursor.

FT2 Tools PAGE -> HISTORY -> RHYTHM -> CYCLE reaches the stopped-transport
draft/Apply workflow through the existing four controller pages. Successful
Apply is one Pattern History transaction; Cancel, refusal, and no-op Apply are
non-writing. Project format 17 persists lane settings and migrates formats
0–16 to FULL/1X/FORWARD. Reusable drum-pattern format 4 persists its four lane
settings and migrates formats 1–3/catalog entries to defaults while preserving
the selected percussion page's routing, names, and mutes.

The deterministic scheduler keeps lane phase on absolute Pattern time,
supports independent rates and playhead order, and cleans note owners at lane
wraps/pendulum turns, final Arrangement cleanup, mute/Stop/Panic, and Live
replacement. Ordinary Pattern boundaries preserve exact owners so long gates
and explicit releases in the following Pattern retain Priority 1–3 behavior.
Preflight scans every possible source trigger; context-free export uses pass 1
with FILL off and cannot extend the Arrangement conductor duration.

The owner-authorized non-Raspberry-Pi pass used exact Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6). Locked check; the
requested focused lane, migration, Pattern History, probability/condition,
microtiming/swing/groove/REC FEEL, scheduler ownership, export/preflight/
partial-playback, navigation/controller, and UI transaction tests; and the
complete normal suite passed. The final suite reported 1,026 passed, zero
failed, and 13 ignored documented development, private-audition, and
performance tests.

No JACK, ALSA sequencer, synth, MIDI transmission, playback, recording,
audible, screenshot, physical-controller, Raspberry Pi timing/headroom, or
other hardware-changing evidence was produced. Musical approval of the new
rate/direction choices remains a human listening decision.

## 2026-08-26 Priority 3 step probability and conditions

Priority 3 from `SEQUENCER_WORKFLOW_PRIORITIES.md` is implemented. The owning
contract and acceptance matrix are in
`STEP_PROBABILITY_CONDITIONS_ACCEPTANCE.md`. Each note-trigger Cell now stores
independent deterministic probability plus ALWAYS, FIRST, LAST/N, A:B, PRE, or
FILL. Condition evaluation precedes chance; PRE uses the preceding trigger in
the same lane and playback pass. Normal FT2 regenerates at its selected
Arrangement playback-span boundary; Live Patterns regenerate at their Pattern
boundary, so pass conditions continue beyond the first repeat.

Project format 16 persists the new fields and migrates formats 0–15 to
100%/ALWAYS in memory without rewrite. Reusable drum-pattern format 3 does the
same for formats 1–2. Context-free MIDI export uses deterministic pass 1 with
FILL off. Route/engine preflight includes every conditional note so a later
pass cannot introduce an unowned engine requirement.

CELL EDIT exposes CHANCE, CONDITION, COND A, and COND B through the existing
rotary field sequence; the four direct-action controller pages remain intact.
Normal FT2 SOUND item 4 and keyboard `f` control the next-cycle-boundary FILL
latch. CLICK moves to the previously empty FT2 Tools SYS item 3. Fill is
runtime-only, clears on Stop/new Play, and never enters Pattern history. Saving
the complete cell draft remains one undoable transaction.

The owner-authorized non-Raspberry-Pi pass used exact Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6). Locked check;
focused probability/condition, Project/drum migration, Pattern History,
microtiming, swing, groove, REC FEEL, scheduler ownership, navigation, and UI
transaction tests; and the complete normal suite passed. The suite reported
1,015 passed, zero failed, and 13 ignored development, private-audition, and
performance tests. Validation repaired stale format-15 fixtures, preserved the
four-page controller contract, and corrected later playback passes to retain
the selected starting Arrangement step.

No JACK, ALSA sequencer, synth, MIDI transmission, playback, recording,
audible, screenshot, physical-controller, Raspberry Pi timing, or other
hardware-changing evidence was produced. Priority 4 completion is recorded in
the newer section above.

## 2026-08-26 Priority 2 rhythm workflow implementation

Priority 2 from `SEQUENCER_WORKFLOW_PRIORITIES.md` is implemented in the
working tree after the bounded Pattern-history work. The pre-edit boundary and
acceptance matrix are in `RHYTHM_WORKFLOW_ACCEPTANCE.md`. Project format 15
adds independent signed cell timing in 1/96-row units plus Pattern-owned
EIGHTH/SIXTEENTH swing at 50–75%; format 14 and older load straight/on-grid
without rewrite. Reusable drum-pattern format 2 stores timing and migrates
format 1 cells to on-grid.

The elapsed scheduler applies swing, cell timing, and the legacy Delay command
only to cell events. Row markers, Pattern/Arrangement duration, MIDI clock, and
Loop beat clock remain straight. Same-lane events are ordered by their shifted
musical time, and a replaced note's generated gate release is suppressed when
it would occur after the replacement. The existing canonical MIDI timeline is
unchanged; live scheduling keeps the full fraction while SMF export rounds to
its nearest existing tick.

CELL EDIT adds independent TIME and the native grid adds `<`/blank/`>` timing
markers. HISTORY page 2 opens transactional FEEL and deterministic GROOVE
editors; successful Apply uses the Pattern-history wrapper, while Cancel,
failure, and no-op Apply do not move history. Quantized real-time REC remains
the default. Runtime-only REC FEEL uses the received MIDI callback timestamp to
choose the nearest row and bounded offset, while a completed take remains one
Undo step and retains existing note-owner cleanup.

The owner-authorized combined software pass used exact Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6). Locked check and
focused Project/drum migration, microtiming, swing, deterministic groove, REC
FEEL, scheduler ownership, navigation, and UI transaction tests passed. The
complete normal suite passed again with 1,015 successful tests, zero failures, and
13 documented ignored development, audition, and performance tests. Validation
repaired current format-15 page decoding, retained the established tracker beat
highlighting, and shortened the CAPTURE controller label to `FEEL` so it fits
the canonical 40-column soft-button width.

No screenshots, external MIDI clock measurement, Raspberry Pi timing,
physical-controller use, listening, JACK, synth, MIDI transmission, playback,
recording, audible, or other hardware-changing evidence was produced. The 75%
swing ceiling remains an implementation bound rather than musical approval.

## 2026-08-26 bounded FT2 Pattern history implementation

Priority 1 from `SEQUENCER_WORKFLOW_PRIORITIES.md` is implemented in the
working tree for review: one runtime-only bounded Pattern history owner, at
most 32 combined Undo/Redo states under the two-cell-budget plus
two-automation-budget structural cap, one Snapshot outside the stacks, and one
UI mutation wrapper. The pre-edit mutation inventory and acceptance table are
in `PATTERN_HISTORY_MUTATION_INVENTORY.md`.

Included committed cell/note/REC/Pattern tool/paste/page-route
Apply/automation/Loop Mix families enter history; Project and Arrangement
structure, global audio state, private files, runtime launch/transport state,
and Apply/Cancel drafts do not. Snapshot capture is non-dirty, Recall is
undoable, project replacement clears all runtime history, and Save retains it.
HISTORY occupies FT2 Tools PAGE item 4 with UNDO/REDO/SNAP/RECALL, dynamic
disabled controls, shared mouse/controller actions, and Ctrl+Z/Ctrl+Y/
Ctrl+Shift+Z. Context records retain Pattern/order/row/page/lane/column/mode,
automation selection, and controller page.

The document-authorized stopped-transport fallback is explicit. A Play-time
restore is refused without moving either stack; REC Undo first finishes the
take and held-note cleanup. The blocker to boundary queueing is the absence of
one scheduler transaction that can atomically publish a full authoritative
Pattern, activate managed routes and decoded Loop resources, restore context,
and report success before stack movement.

The same owner-authorized Rust 1.97.1 combined software pass completed the
Pattern-history matrix through focused model/UI/navigation/transaction tests,
the function-level mutation audit, and the complete suite result recorded
above. The pass repaired the moved opening-state ownership error in history
coalescing and stale test fixtures exposed by the new `Cell::nudge` field.
Clippy was not required by an observed failure or repository policy. Runtime
route/Loop recovery remains deterministic test-double and source evidence; the
audio, screenshot, MIDI, controller, listening, and physical limits above also
apply here.

## 2026-08-23 read-only FT2 HARMONY browser

The proposal-archive circle-of-fifths helper is now current implemented work.
FT2 Tools PAGE owns one previously empty `HARMONY` position and opens an
ordinary read-only overlay. It derives the counter-clockwise and clockwise
fifth neighbours, relative key, parallel key, and all seven diatonic triads
from the existing Project tonic and major/natural-minor mode. English/German
labels reuse the repository's canonical sharp-based note-name policy.

The overlay has no Project, persistence, note-generation, playback, MIDI, or
audio mutation path. Its launcher and direct Exit, encoder press, keyboard
H/Back/Esc/Enter, controller, and mouse paths restore the exact FT2 Tools page,
page-select mode, Project dirty state, order/Pattern/page/lane/column/cursor,
FT2 mode, and transport. All 24 keys/modes under both naming policies and the
38×10 compact fallback are covered.

The authorized combined pass used exact Rust 1.97.1
(`rustc 1.97.1 (8bab26f4f 2026-07-14)`, LLVM 22.1.6). Formatting, locked
checking, eight focused HARMONY regressions, the complete normal suite with
986 passing tests and 13 documented historical/audition/performance tests
ignored, and locked debug and release builds passed. The pinned-font self-test,
complete 143-image deterministic screenshot render/check, generated
documentation site, focused URL/reference checks, and whitespace validation
also passed. No JACK, synth, ALSA/MIDI transmission, playback, recording,
audible, or physical-hardware action ran.

## 2026-08-23 FT2 automation workflow audit and repair

The current FT2 audit repaired automation and effect-lifecycle state integrity.
Opening AUTO is read-only and NEW explicitly creates an unused target lane.
Populated lanes refuse target browsing until their nearby double-confirmed
CLEAR is used. Armed touch capture becomes safe when Arrangement playback
enters another Pattern instead of applying the selected lane index to unrelated
data, and loop-wrap capture keeps the selected point aligned. Pickup now uses
the selected automation target and real stopped cursor or playback position.

Removing an effect or confirming a replacement type also removes only that
exact effect's now-unresolvable automation lanes and reports the affected lane
and point counts; cancelling a type change retains both effect and automation.
The bounded CC publisher now cancels a pending intermediate value when the
control returns to the already-published value, preventing stale later output.

The authorized combined pass used exact Rust 1.97.1
(`rustc 1.97.1 (8bab26f4f 2026-07-14)`). Formatting, locked checking, 16
focused automation/effect-lifecycle tests, and the complete normal suite passed
with 978 tests successful and 13 documented research, audition, and performance
tests ignored. The generated documentation site reproduced deterministically
and its focused URL/reference checks passed. The explicitly requested follow-up
reran the locked check and complete normal suite, then completed locked debug
and release builds for the `shr` binary. No Clippy, screenshot batch, JACK,
synth, MIDI transmission, recording, audible, or physical-hardware action ran.

## 2026-08-16 Swarm, Bass Matrix, and shared instrument volume

The sibling Moj Sint engine now has five live models and 16 cleared starts.
The earlier typed-graph warm pad is integrated as `Swarm Machine`; the new
`Bass Matrix` is a different split-path bass instrument whose clean
half-frequency body remains outside its driven PM/metal/filter/feedback branch.
SHR recognizes strict Moj schema 7, retains schemas 1–6, and owns exact
`ENGINE → MODEL → PATCH` routing, model-qualified private saves, Player/FT2
labels, Projects, automation, pickup, RESET, failure restoration, panic, and
shutdown for both additions.

Physical controller position 5 is instrument volume wherever a managed melodic
instrument is active. The actual MiniLab profile maps its continuous CC 93 pot
to that position. synthv1 retains its verified DCA volume parameter; Moj uses
separate MIDI CC 7 and a tone-neutral 10 ms engine gain; Yoshimi, FluidSynth,
and SHR Sampler receive standard channel-volume CC 7. Player and FT2 show the
same control, loading/resetting re-arms pickup, and FT2 instrument automation
persists the route. Read-only backends cannot save this value into their source
instrument format, and their own CC smoothing is the documented backend
exception; their Project automation remains durable.

Private local configuration already points to the sibling Moj release binary
and preset root, so no `user/` mutation is required. Musical acceptance and
native Raspberry Pi callback headroom remain open listening/hardware verdicts.

The authorized combined pass used exact Rust 1.97.1
(`rustc 1.97.1 (8bab26f4f 2026-07-14)`). Formatting, locked checking, all 969
normal tests, and locked debug and release builds passed; 13 documented
research, audition, and performance tests remained ignored. No JACK, synth,
MIDI, recording, audible, or other hardware-changing test was started.

## 2026-08-05 Strange Oscillator experimental integration

The sibling Moj Sint repository now exposes Strange Oscillator as its third
live experimental model and cleared start 14. This SHR working tree recognizes Moj
preset schema 6, shows the model as `S-OSC`, routes it through the existing
`ENGINE → MODEL → PATCH` hierarchy, maps CC 20–27 to TYPE, FORM, WARP, COUPLE,
MOTION, CHAOS, COLOR, and SPACE, and preserves ADSR at CC 28–31. Model-specific
private saves, tracker parameters, automation names, and model-qualified route
identity all include the third model. Schema 1–5 Moj presets remain readable.
The local Moj command already points to the sibling optimized binary. Plain
`shr` and tty1 autoload use the optimized SHR release binary; an explicit
`SHSYNTH_BIN=target/debug/shr` override remains available for development. The
owner explicitly authorized committing and publishing both repositories for
continued experimentation before the open
musical and native-load verdicts. Publication is not production acceptance.

## Current priority and shared checkout

The early Build Week-style sprint snapshot is preserved by its historical tag,
but no hackathon application or submission was made and it does not govern the
project. Ordinary experimental development continues on `main`; do not keep or
recreate a standing `dev` branch. The temporary combined build-and-test gate in
`AGENTS.md` still applies; an active repository is not implicit permission to
compile.

The current ordered tracks are owned by `docs/EXPERIMENTAL_ROADMAP.md`: retain
the accepted workflow/install foundation, complete the owner-specified FT2
behavior without pulling random future features into scope, then implement and
physically accept simultaneous 18-channel playback and recording. Current
`0.x` package numbers are compatibility identifiers for experimental source,
not production-release milestones; public installation docs do not pin 0.4.8.

`docs/EXPERIMENTAL_DIRECTION.md` records the unscheduled product direction:
beginner- and curiosity-first music making, FT2 as the main composition and
automation workspace, Player as a deliberately simple player, optional theory
and smart assistance after play, and Moj Sint as an open experimental
instrument laboratory. The favored Moj authoring direction is a typed low-code
micro-machine format usable with or without AI. One fixed swarm graph is now
live; that does not promise runtime graph editing or a general modular system.

The current compatibility checkpoint adds SHR Sampler 0.1.2 as the fifth mutually exclusive managed
melodic backend. Its package and executable compatibility are preflighted
offline before the current owned engine is disturbed; startup, exact stereo
readiness, routing, failure publication, one-attempt restoration, All Notes
Off, unexpected exit, and shutdown remain in the shared backend lifecycle.
The complete public installer now fetches exact Moj Sint and SHR Sampler
commits, compiles SHR Drums 0.2.0 in process from its exact public commit, and
installs one allowlisted manifest-owned payload. The transactional file layer
refuses foreign collisions or locally modified managed files, rolls an
interrupted change back before retry, and preserves all private/XDG data on
update or uninstall. `scripts/validate_public_install.py` is the opt-in
hardware-free exact-source disposable validation; it never opens JACK or ALSA.

The current Input-mix work adds a live stereo/dual-mono choice to the exact
configured two-port final-bus Input. Stereo remains the fresh-runtime default.
Dual mono independently equal-power pans ports 1 and 2, initially at hard left
and hard right so its first output matches stereo; mode and matrix changes use
the existing 10 ms smoothing boundary. MTR NAV exposes mode and a
LEVEL/PAN 1/PAN 2 focus whose normal minus/plus controls follow the selected
field. Mode and focus deliberately add no dedicated computer-keyboard
shortcuts. These controls do not alter raw multitrack stems, machine JACK
mapping, or Project data. The user-authorized combined pass on 2026-08-03 used exact
Rust 1.97.1: formatting and locked check passed, SHR's complete normal suite
passed 941 tests with 13 documented research/audition/performance tests
ignored, and locked debug and release builds passed. The focused MTR NAV
documentation screenshot was regenerated and visually inspected at its native
40×13 geometry. No JACK, synth, MIDI, recording, audible, or other physical
hardware test was started.

The same 2026-08-03 repository sync fast-forwarded the clean Moj Sint sibling
by four documentation-only commits recording the completed five-kick and
snare comparison gate; production Moj sound code did not change. Its locked
all-target/all-feature normal suite passed 287 tests with 34 development-only
historical/audition/benchmark tests ignored, and its debug and release
all-target builds passed with Rust 1.97.1.

The public sound-package repair adds the four approved SHR Drums packages to
the tracked `kits/cleared-kits.txt` allowlist: Acid, Electronic House, Big Rock
(Muldjord), and Experimental Noise (Muldjord). Repository-local launchers read
that public tree directly, and installed setup selects the packaged shared-data
root unless the musician configured another kit directory. Factory kits no
longer depend on ignored `user/` state. Moj Sint's 16 factory starts remain
tracked in its sibling repository; only Playback user saves use private XDG
storage. The user-authorized 2026-08-03 combined pass ran SHR Drums' complete
all-target/all-feature normal suite: 19 tests passed, three opt-in quality
matrices remained ignored, and its locked debug and release all-target builds
passed.

Electronic House now carries the selected Moj Sint House Impact and Long
Pressure kicks as deterministic CC0 synthetic one-shots on free notes 27 and
28. Notes 33, 34, 35, and the original House Kick on 36 remain unchanged, so
the kit exposes six kicks without remapping existing material. The two new
voices use the ordinary House kit bus and note-38 snare. Package validation and
a private eight-hit rhythm render/playback passed; owner judgment of the
in-kit presentation remains open. No SHR project compilation was run under the
current build gate.

The Experimental Noise note-38 snare no longer uses the 298 Hz, 700 ms
acoustic hybrid that the owner rejected as cowbell-like. The first replacement
was also rejected because stacked hard clipping, 4.3 kHz band-pass noise, FM,
and ring modulation produced a high-pitched distant fuzz. The current revision
uses a 158 Hz short body, low-pass broadband noise, no FM or ring modulation,
restrained cubic drive, brief quiet modes, and no sample assignments; the six
now-unused snare WAVs were removed from that package. A private 124 BPM
kick/snare comparison was regenerated through the existing engine and played
through the configured soundcard. The owner accepted this revision as a real
snare, possibly stronger than Big Rock's, and explicitly kept its
rock/industrial identity out of Electronic House.

Version `0.4.4` added Moj Sint 0.2.0 as a real fourth managed backend. Presets
cycles to its bounded strict `.mojsint` catalog without launching sound; LOAD
alone starts `moj-sint --client-name ... --preset ...`. Playback renders the
eight Model D controls plus ADSR in the existing three-by-four geometry, with
Moj-specific CCs, defaults, RESET, pickup, Project/Idea/FT2 identity, and no
synthv1 XML or parameter-index reuse. The live SHR Drums sibling is now 0.2.0
and continues as the in-process fourth final-bus source beside the one managed
melodic engine. Its public format keeps legacy packages readable and adds the
optional advanced modeled-voice graph used by the public Acid recipe. This
states the source capability, not that any private compiled kit is installed.

Version `0.4.5` pairs with Moj Sint 0.2.1 and exposes all seven authored Model D
starts instead of only the idealized reference. Strict schema 3 carries bass,
lead, or filter-articulation patch identity; schemas 1 and 2 remain strict
in-memory bass migrations. Full Bass, Full Lead, Full Filter Articulation,
Matched Idealized, Matched Linear Mixer, Matched Linear Ladder, and Matched No
Drift or Feedback remain one editable instrument, each with the same eight
timbral controls plus ADSR. EVOLVE 0.5 truthfully represents authored static
oscillator character without drift; no diagnostic uses hidden state.

Version `0.4.6` moves the final stereo bus out of the optional managed-synth
owner. MTR Input now has one source-position action: `MON ON` while software
monitoring is off and `MON OFF` while it is on; there is no duplicate Input
mute. Monitoring defaults off. MON ON can activate the owned bus from only the
exact configured Input and playback pairs and never starts a synth, Loop, or
drums. Optional sources attach, disappear, and reconnect by exact owned links
without duplicate playback. The callback order is Input source level,
complete sum, master inserts, master level, fixed MASTER STRIP, limiter/final
meter, one final-WAV tap, then playback; recording receives that same final
slice. Audio Recorder `LEVELS` still opens only the unchanged 18-channel meter
overview. The private repository-local configuration declares the observed
AudioBox direct monitor active, so MON ON remains refused until the owner
physically disables that monitor and deliberately updates the declaration.
This release has deterministic software verification only; it does not claim
new JACK, audible, or hardware validation.

Version `0.4.7` pairs with Moj Sint 0.2.2 and separates the Moj Sint engine
from its synthesis model. Strict Moj schema 4 names `model_d`; schemas 1–3
migrate to it in memory. Discovery and Ideas retain typed model identity,
Project/FT2 routes use model-qualified stable instrument IDs with legacy
unqualified Model D lookup, and Playback selects its twelve labels from the
loaded model while current parameters remain on rotary 2–13 positions.

The next Moj Sint integration keeps that one managed engine and adds Six-Op PM
as its second selectable model. Strict schema 5 has model-specific patch and
macro fields; schema 4 and older remain Model D migrations. Discovery contains
seven Model D and six Six-Op PM starts. FT2 ROUTE presents `ENGINE → MODEL →
PATCH` for Moj Sint, constrains patch browsing to the selected model, and keeps
the existing whole-route Apply/Cancel transaction. Playback renders the
selected model's twelve labels. The first connected audition found
note-count-dependent artifacts because the private runtime launched an
unoptimized Moj Sint binary. Native smoke evidence and bounded chord renders
identified callback starvation rather than clipping; the runtime now launches
`target/release/moj-sint`, and the user confirmed that the reported two-note
Model D and four-note Six-Op failures are gone. This is acceptance of that
repair only; broader physical-control, routing, polyphony, and sound acceptance
remain open.

The catalog presentation repair places the in-house Moj Sint first in
Presets and software-engine selection. Moj factory names retain their stable
preset and Project route identities but display one compact number/model/name:
for example, `01 M-D Full Bass` and `08 6-OP Bell Metal`, without bracketed
model labels or repeated `Six-Op` text. The generated 40x13 Presets reference
now records that exact first-engine view. Source, focused regressions, the
complete normal suite, debug/release builds, and all 142 deterministic
screenshots passed on the native Pi. A warning-denied Clippy run still reports
38 pre-existing repository-wide lints outside this repair, and `cargo deny`
has no repository policy file so its default configuration rejects every
dependency license; those broader release-gate debts were not folded into the
catalog change. Live visual confirmation remains open until the user next
relaunches SHR.

The 2026-08-01 locked software gate used Rust 1.97.1 on AArch64. Focused Moj
schema/control/engine and Route hierarchy tests passed, then the complete
normal SHR suite passed 892 tests with zero failures and 12 existing ignored
development/audition tests in 49.07 seconds. Locked check and the explicit
debug build passed; the build completed in 1 minute 51 seconds at
`target/debug/shr`, then a final legacy-route regression refresh completed in
1 minute 19 seconds. Moj Sint's own all-target/all-feature normal suite passed
273 tests with 34 explicitly ignored historical classes, and its debug binary
validated all 13 tracked presets. No JACK, synth process, MIDI, playback,
recording, audible, or hardware action was run.

Playback `SAVE` now owns instrument-preset persistence for synthv1 and Moj
Sint. Its canonical overlay exposes explicit Overwrite, Save New, and Cancel
through controller, keyboard, encoder, and mouse dispatch. Factory, system,
public-checkout, unsupported-backend, and symlink-backed sounds remain
read-only; Overwrite on a read-only supported sound visibly redirects to the
next private `User NNN` name. Moj saves use strict schema 5 and separate Model D
and Six-Op PM directories. Synthv1 saves preserve the complete source XML while
replacing only the preset name and twelve mapped values. A successful save
refreshes Presets and FT2 ROUTE immediately, becomes the RESET baseline,
re-arms pickup, and neither restarts the engine nor releases notes. Only an
active Tracker route owner is retargeted; standalone Player saves and unrelated
Project routes are untouched. Cancel and failure preserve cursor/list state,
values, held notes, and the live session; failure keeps the overlay and prior
file intact.

The user-save acceptance gate used exact Rust 1.97.1. Two final locked SHR
default-suite runs each passed 902 tests with zero failures and 12 intentionally
ignored development/audition tests. Moj Sint's locked all-target/all-feature
suite passed 276 tests with zero failures and 34 historical render, research,
publication, and native-benchmark tests ignored. Formatting, locked checks,
fresh debug builds, and diff checks passed in both repositories. No release
build, screenshot batch, JACK, synth, MIDI, playback, recording, audible, or
physical-hardware test ran during that gate.

The locked 0.4.6 combined gate used Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`). Focused graph ownership,
monitor/UI, Recorder LEVELS, and four observed regression tests passed. The
final normal default suite passed 876 tests with zero failures and 12 ignored;
the harness took 49.93 seconds and total wall time was 50.03 seconds. Locked
`cargo check` passed in 8.51 seconds. Locked debug and release builds passed in
12.80 and 141.44 seconds at `target/debug/shr` and `target/release/shr`.
Screenshot manifests expose `DEV` and `REL` respectively; 141 deterministic
images, the pinned PSF self-test, and generated documentation drift checks
passed. `/usr/local/bin/shr` still resolves to `scripts/local.sh`, whose default
binary is this checkout's `target/debug/shr`. No ignored R&D/audition test or
physical/audio action was run.

The default Rust test gate now contains deterministic product regressions only.
Operational latency and alignment, finite and bounded audio, allocation
contracts, recovery, routing, persistence, UI behavior, and representative
adopted DSP-quality limits remain mandatory. Six development-only DSP checks
for legacy baselines, exhaustive quality/cost matrices, and a rejected
oversampling candidate are explicitly ignored alongside the six private WAV
audition renderers. Their exact scope and opt-in command live in
`docs/MAINTAINER_HELPERS.md`. The first locked default run after the split, on
2026-07-30 with Rust 1.97.1, passed 864 tests with zero failures and 12 ignored;
the harness took 49.32 seconds and total wall time including 21.05 seconds of
test-target compilation was 70.48 seconds. No JACK, synth, MIDI, playback,
recording, audible, or physical-hardware test was involved.

`shr --help` separates `effects-checkpoint` from non-audible maintenance. Its
own section states that the command starts a JACK graph, synth, and note and
requires explicit authorization. The section labels `phase2-checkpoint` as the
same audio-changing compatibility alias. The no-command description also names
the current SHR-DAW Home screen instead of the retired instrument-browser entry.

FT2's quick ROUTE overlay keeps whole-route `APPLY` and `CANCEL` commands in
the standard controller action row at native 40×13 instead of hiding Apply
below the scrolling fields or drawing custom controls into the overlay border.
Its canonical ROUTE controller page, mouse targets, and keyboard `A`/`C` share
those direct actions. Turning an active field applies valid Project and live
route changes immediately. Apply keeps the result, Cancel restores the opening
route snapshot, and Back remains field-first. An SHR Drums target now exposes
the installed drum sets through the route session's `KIT` field and persists the
applied selection with the Project, resetting only prior-kit tuning overrides
while preserving the Project key and drum effects.
FT2 Exit and computer-keyboard quit now bypass the save guard whenever the
entire Project has zero note events, regardless of unsaved route, kit, effect,
name, or other setup changes. FT2 Exit restores the clean baseline; an empty
template persists only through explicit Save. Note-bearing dirty Projects keep
the existing four-choice save guard, and dirty replacement paths remain
protected. The local launcher still deliberately uses `target/debug/shr`; its
Cargo development profile now retains assertions and debug symbols while
optimizing DSP callbacks. On the Pi 5, the rebuilt DEV artifact completed the
offline 128-frame compiler callback matrix with zero deadline misses; SHR
Drums averaged 15.16–27.66% of the 2.667 ms deadline and its worst measured
callback was 0.903 ms. The prior unoptimized DEV artifact held about 89% of one
CPU and produced thousands of `shr-drums` JACK deadline errors per minute,
while temperature, throttle, memory, swap, and I/O were healthy.
Playback keeps MIDI-take persistence on Ideas; its SOUND page uses `SAVE` for
the current synthv1 or Moj Sint instrument, and `SOUNDS` returns directly to
Presets and its visible `LOAD`. In-app and terminal MIDI Learn now capture the
complete optional encoder-Shift gesture. Holding Shift, turning left once, then
releasing records both the modifier and either the ordinary rotary CC or its
separate shifted CC. The bundled MiniLab 3 default now mirrors the owner's
current learned map: ordinary encoder CC114, click CC115, Shift CC9, shifted encoder
CC112, the twelve mapped controls, and eight channel-10 command pads. The
earlier reviewed DAW-mode CC27/CC29 pair remains a catalog-declared in-memory
compatibility variant rather than the fresh-profile default. The locked
combined repair used Rust 1.97.1 and passed the complete normal suite with 880
passing, zero failing, and 12 intentionally ignored in 51.15 seconds. The
optimized DEV artifact built successfully and its deterministic offline
callback measurement passed as recorded above. Formatting, diff checks, and
two-write deterministic documentation generation/checks passed. No screenshot
regeneration, physical controller, MIDI transmission, synth start, JACK
mutation, playback, recording, or audible check was run.

The complete first musician/operator workflow review and its persistent repair
ledger are in `docs/WORKFLOW_AUDIT_HANDOFF.md`. Its R01–R15 repair queue passed
the authorized combined acceptance pass on 2026-07-23. The ledger records the
locked debug/release builds, 662 passing tests plus four intentionally ignored
private renderers in each profile, warning-denied Clippy, all 105 screenshots,
helper checks, stress runs, connected audio checkpoints, and isolated recorder
failure/recovery drills. D01–D10 remain unanswered. P01–P08 retain their honest
owner/physical gates; machine evidence obtained for P05, P06, and P08 is
recorded without claiming user observation.

The full current-documentation reconciliation for `0.3.96` supersedes the
earlier docs-only continuation. Current musician, menu, routing,
configuration, architecture, audio, installation, controller, Project-format,
roadmap, and helper text now follow the format-6 implementation and the three
FT2 entry layouts. The private `user/docs-pruning-20260725/` material remains
unpublished reference only; it is not an active task or a public source of
truth.

Version `0.3.99` corrects FT2 Loop Mix ownership. Project format 9 retains
format 8's exactly four optional Loop Mix records under each Pattern and adds
one strict Project-global MASTER STRIP record. Format 7's four Project-global
loop slots and format 6's single slot migrate in memory into every distinct
Pattern without rewriting the source Project; all formats through 8 acquire a
neutral strip the same way. Repeated Arrangement references share one Pattern
record, while clone and paste-new copy the references/settings into an
independent Pattern. Resize retains them; confirmed CLEAR detaches them; CLEAN
and every other Pattern operation leave the shared private WAV library
untouched. Effects, final-bus routing, recorder configuration, and MASTER
STRIP settings remain Project-owned.

Ordinary Arrangement and Live Pattern boundaries now change MIDI and Loop Mix
ownership together. Each step, including a repeated Pattern reference,
restarts Pattern-local phase; middle-row and later-step starts use only the
incoming Pattern's local row, tempo, and meter. Browsing changes the editor
owner without changing sounding loops. Outgoing slots are invalidated at the
boundary, empty slots remain silent, and missing, incompatible, failed, or
late incoming slots fault independently while healthy loops and MIDI continue.
Stop, Panic, Project replacement, JACK shutdown, application shutdown, and
exit stop all owned loop state.

The runtime retains one Loop JACK client, one summed stereo source, and exactly
four active callback renderers. Only the active and one incoming Pattern are
prepared outside the callback. One fixed pending renderer set is published by
an atomic pointer swap and reclaimed by the owner thread; callback tests forbid
allocation, locking, file access, formatting, decoding, and unbounded work.
Pattern switches do not duplicate direct/final-bus routes, and the final
recorder receives the active Pattern's complete Loop sum once.

The complete `0.3.99` acceptance pass on 2026-07-26 used Rust 1.85. Locked
check and debug build passed; the complete suite ran 782 tests with 778 passing,
zero failing, and four intentionally ignored private DSP audition-pack
generators. All 132 deterministic screenshots passed exhaustive drift
validation. New populated coverage includes Pattern A, a different Pattern B,
an empty Pattern, all four slots, active/queued/stopped/muted/missing/
incompatible/faulted states, attached-loop CLEAR confirmation, Live switching,
all Loop Mix command pages, native 40×13, and compact fallback. ShellCheck,
ten cleared demo arrangements, local Markdown/image references, `git
diff --check`, Cargo metadata, the plain `shr` launcher, `DEV` rendering tests,
and the 139-note zk index passed. A loop-only transport fixture initially
failed because it did not select the new test-only batch-decode override; the
fixture was repaired and the full suite reran cleanly.

Source inspection also repaired three transition hazards before acceptance:
browsing a non-sounding Pattern can no longer apply runtime controls to the
sounding owner; identical WAV references are rebuilt with the incoming
Pattern's settings instead of reusing stale cuts or levels; and failed managed
instrument/backend replacement cannot re-arm outgoing loops. No JACK server,
synth, MIDI transmission, playback, recording, audible test, or physical
hardware test was started. Temporary visual evidence remains ignored below
`user/acceptance-pattern-loops-20260726/`.

Version `0.3.100` adds the fixed stereo MASTER STRIP and replaces the
sample-peak-only final limiter.
The single final path is MASTER rack, live master fader, INPUT, TONE, linked
GLUE, ADAA COLOR, conservative M/S IMAGE, LOUD/8× true-peak limiter, final
meter, then the identical WAV tap and JACK playback buffers. Optional stages
default bypassed; the -1.0 dBTP safety boundary does not. Fixed latency is 133
samples / 2.770833 ms at 48 kHz and 123 samples / 2.789116 ms at 44.1 kHz.
The owning ranges, algorithms, provenance, tolerance, memory, and synthetic
evidence are in `docs/MASTER_STRIP_MEASUREMENT.md`.

The MASTER STRIP acceptance pass on 2026-07-26 used Rust 1.85. Locked format,
check, debug/release builds, 18 focused tests, 788 complete-suite passes plus
four intentionally ignored private renderers, and warning-denied all-target
Clippy passed. All 141 deterministic screenshots passed exhaustive
960×624/integer-scale validation; all 301 local Markdown references and ten
cleared demo arrangements passed. The final release benchmark measured the
maximally active processor below 7% mean, 8% p99, and 20% maximum of the 64-
and 128-frame deadlines; its fixed state is 21,632 bytes with 1,056 limiter
delay bytes. One neutral non-real-time 64-frame callback was descheduled for
2.885 ms and is retained honestly in the owning evidence. Two final-mix
stresses processed 96,000 frames each at 64 and 128 frames with zero drops or
overflows and byte-identical playback/WAV PCM. No JACK server, synth, MIDI,
audible playback, hardware recording, listening approval, or physical
acceptance was performed. Synthetic WAV evidence is ignored below
`user/master-strip-validation-20260726-final/`.

Version `0.3.101` adds deterministic hundredths-of-a-BPM Project tempo and
bounded Standard MIDI File import. Project format 10 stores Pattern and tempo
command values as integer hundredths; formats 0–9 migrate their whole-BPM
values only in memory until an explicit save. Loop preparation now validates
against the prospective detected Pattern tempo and commits its private file,
attachment, runtime, and tempo only after preparation succeeds.

Current Project format 11 adds the per-page automatic Note Off choice. Format
10 loads it as ON for melodic pages and OFF for percussion pages without
rewriting the private Project; only an explicit save writes format 11.

FILES MIDI reads only regular, no-follow `.mid`/`.midi` files from the private
configured inbox. The dedicated tick-domain parser accepts SMF format 0/1 PPQN
with running status, conductor metadata, fixed 3/4 and 4/4, and 6/8 mapped to
the compound 3/4 tracker grid. Conversion retains track/channel parts, initial
bank/program, pitch, velocity, sustain-baked duration, decimal tempo maps,
four-lane allocation, overflow pages, and bar-boundary Pattern splits in a new
unsaved Project. Unsupported musical/system data is stripped and counted;
format 2, SMPTE, changing meter, malformed data, symlinks, and bounded-limit
violations are refused before Project replacement.

The repository-only `0.3.101` pass on 2026-07-27 used Rust 1.85. Locked check
and formatting passed. Focused parser/converter, decimal tempo, Project-format
0–10 migration/round-trip, native loop preparation/rollback, FILES import,
storage, and all navigation tests passed; the bundled House fixture imports as
84 BPM, compound 3/4, 160 rows, 254 starts, five pages, and zero quantized
starts. All 46 public Markdown sources and 134 local image references validated,
and `git diff --check` passed. No complete suite, Clippy, release build,
screenshot regeneration, JACK server, synth, MIDI transmission, playback,
recording, audible test, or physical hardware test was run.

Version `0.4.1` follows the accepted Raspberry Pi 5 installation milestone and
repairs three controller-first FT2-style MIDI workflows. Fresh Drums now own an
explicit discovered FluidSynth GM-drum route on channel 10 across audition,
record/edit input, and transport; route failures remain explicit and silent.
Dirty Project replacement uses the rotary `SAVE (AUTO)` / `SAVE (NAME)` /
`DON'T SAVE` / `BACK` guard, and Edit owns only contextual commands with
independent 1/1–1/128 LENGTH and 0–32 ADD selectors. The shared status row once
again retains the configured CPU temperature beside its transport glyph.

The complete `0.4.1` combined pass on 2026-07-28 used Rust 1.85. Formatting,
the locked debug build, and all 832 tests passed with 828 passing, zero failing,
and four intentionally ignored private DSP audition renderers. The generated
documentation site reproduced byte-for-byte with its pinned renderer and
`git diff --check` passed. No JACK server, synth, MIDI transmission, playback,
recording, audible test, or physical hardware test was started.

Version `0.4.2` repairs FT2 transport exit and mixed-instrument feedback, adds
per-page automatic Note Off control, defaults new percussion entry to sustained
drum hits, makes PAGE navigation page-only, and adds optional Shift+rotary
column navigation. One FluidSynth instance can still schedule independent
melodic parts and channel-10 drums; a Project that mixes FluidSynth with a
managed synthv1 or Yoshimi backend now offers an explicit, reversible
FluidSynth remap with manual sound preview instead of refusing Play silently.

The complete `0.4.2` combined pass on 2026-07-28 used Rust 1.85. Formatting,
locked check, focused FT2 regressions, and the complete suite passed with 840
passing, zero failing, and four intentionally ignored private DSP audition
renderers. `git diff --check` passed. Source and private Project inspection were
sufficient, so no JACK server, synth, MIDI transmission, playback, recording,
audible test, or physical hardware test was started.

Version `0.4.3` removes SHR Drums' mislabeled 333 ms
cross-feedback “room” line and keeps ambience in a Project-owned fixed
Reverb-then-Delay rack hosted in-process by SHR-DAW. Project format 14 persists
that rack; format 13 and older preserve their page routing and migrate
restrained family defaults in memory. DRUMS exposes `OFF`, `REVERB`, `REVERB +
DELAY`, and `DELAY`; ordinary tracker Stop drains naturally, while Panic,
Project/effect-host replacement, and shutdown clear state. Reverb is the
existing bounded diffused four-line FDN with independent pre-delay and a
finite 1.5× RT60 plus 0.4-second deadline. Delay sync labels now use the
correct quarter-note beat values. The in-process SHR Drums workspace is version
`0.1.1`; its `0.1.0` engine-compatibility floor remains deliberate.

The 2026-07-29 Rust 1.85 pass ran locked checks in both repositories and only
focused engine/factory, reverb, delay, drum-host, graph, migration, routing,
one-shot, coexistence, and FX UI tests. All finished passing after correcting
two test fixtures exposed by the new tempo mode and DRUMS target. Fresh private
review kits are ignored below
`../shr-drums/user/effects-stack-kits-20260729/`; 12 deterministic 48 kHz
stereo 24-bit comparison WAVs and `measurements.txt` are ignored below
`user/effects-stack-review-20260729/`. The owner then approved an audible
Big-Rock-only dry, reverb, delay, and combined comparison after the Rock room
was made more evident; those final six-second renders and measurements are
ignored below `user/big-rock-effect-audition-20260729-v2/`. JACK was already
running and the temporary review client connected only its own stereo outputs,
then disconnected cleanly. No JACK server, synth, MIDI transmission,
recording, release build, or physical route change was started.

The native Raspberry Pi 5 compiler A/B is complete and owned by
`docs/RUST_COMPILER_AB_2026-07-29.md`. Identical release source and lockfile
built successfully with Rust 1.85.0/LLVM 19 and Rust 1.97.1/LLVM 22. The newer
compiler produced a 7.2% smaller binary and the ordered clean build finished
40.7% sooner with 6.5% lower peak RSS, though cache order prevents attributing
the full build-time change to the compiler. Runtime was mixed: standalone SHR
Drums remained effectively unchanged, while the fixed final strip regressed
about 87% in representative mean time and complete graph/final-bus workloads
regressed 17–100% depending on boundary. All paired output was bit-identical.
The repository now pins exact Rust `1.97.1`; newer stable releases require a
deliberate pin update. Ignored raw build, runtime, output, and system evidence
remains below `user/compiler-ab-20260729/`. No JACK lifecycle, connection,
physical route, MIDI, synth, audible playback, or hardware setting changed.

The 2026-07-30 Rust 1.97.1 follow-up kept the original compiler A/B intact and
localized its final-strip regression to modulo indexing inside the 24-tap,
4×/8× true-peak interpolation loop. Cortex-A76-native code generation,
native plus `opt-level=2`, and forced inlining did not improve that boundary.
The adopted implementation instead preserves tap and floating-point
accumulation order while splitting the ring scan before and after its wrap.
Against the ordinary Rust 1.97.1 build, representative complete-strip means
improved about 43%, isolated interpolation improved 52–57%, dry graph medians
improved 43–47%, fully enabled graph medians improved 21–27%, and combined
drum-plus-melody final-bus medians improved 13–15%. Standalone drum DSP stayed
within noise. All 14 complete-graph output hashes remained bit-identical, and
a focused test locks bit-exact equivalence against the old modulo expression.
The optimized 1.97.1 strip-bearing graph is now generally within about 1–7% of
the original 1.85 controlled medians, so there is no separate old-compiler
release path. The owning explanation and exact results are in
`docs/RUST_COMPILER_AB_2026-07-29.md` and
`docs/MASTER_STRIP_MEASUREMENT.md`; ignored raw follow-up evidence is below
`user/compiler-options-20260730/` and
`user/compiler-source-options-20260730/`. Do not cosmetically collapse the two
ring ranges back into a modulo-indexed loop without rerunning the focused
benchmark and bit-exact reference test.

The requested full adoption validation used exact Rust 1.97.1 in SHR-DAW and
its live SHR Drums path dependency. SHR-DAW finished with 856 tests passed and
6 explicitly ignored private render tests; SHR Drums finished with 12 passed
and none ignored. Release all-target builds succeeded in both repositories.
The first full SHR-DAW run exposed four stale test expectations from the
earlier fourth graph source, percussion one-shot, and fifth final-bus row
changes. Their test-only repairs now derive production counts and distinguish
percussion attacks from melodic notes that own durations; no production
behavior changed. Formatting, deterministic documentation generation/check,
and diff whitespace validation passed. Ignored validation transcripts remain
below `user/adopt-modulo-free-20260730/validation/`.

The same pass repaired the non-audible `final-mix-stress` fixture after it
exposed a stale three-source allocation against the current four-source bus.
The helper now derives the production source count, supplies a distinct drum
source, and sums every source before the fixed strip and WAV equality check.
Its focused Rust 1.97.1 test passed with full PCM equality and zero
drops/overflows.

The previous ignored runtime kit directory contains copies of the approved Big
Rock (Muldjord), Experimental Noise (Muldjord), Electronic House, and Acid
packages. The tracked `kits/` tree is now the public source; the ignored copies
are no longer required by repository-local launchers. Previous local kit
entries are recoverable below
`user/kit-backups/pre-effects-stack-20260729/`. Plain `shr` resolves
`/usr/local/bin/shr` through this checkout's `scripts/local.sh` to
`target/debug/shr`; the previous installed binary is preserved as
`/usr/local/bin/shr.installed-20260727.backup`. No environment override is
required to run the approved kits.

Version `0.3.98` adds the dedicated 18-channel Levels overview without changing
Project storage. At exact 40×13, columns 1–20 show all 18 nine-segment vertical
meters as three groups of six and columns 21–40 show the active TAKE, CHANNEL,
or SYS commands. Rows 11–12 label channels and shared status alone owns row 13;
the screen omits normal controller rows only at native size. Smoothed RMS uses
the −48/−36/−30/−24/−18/−12/−6/−3/−1 dBFS ladder, with green/yellow/red
thresholds, same-colour held sample peak, clip hold, and distinct silence,
missing, and fault states. Selection never hides a channel and survives normal
navigation with its command page; Project replacement resets both. Recorder
setup/routing, the final-bus MTR, Live Patterns, Loop Mix, and future mixer
strips remain separate.

The complete `0.3.98` acceptance pass on 2026-07-26 used Rust 1.85. Locked
check and debug build passed; the complete suite ran 771 tests with 767 passing
and four intentionally ignored private DSP renderers passing separately. All
125 deterministic screenshots passed complete regeneration and 960×624
integer-pixel/drift validation. The 13 new Levels images cover native nominal,
quiet, yellow/red/clip, missing/fault, selected first/middle/last, TAKE,
CHANNEL, SYS, record, stop, and compact fallback states. ShellCheck, Python
helper syntax, all 27 isolated audio-policy cases, 300 local documentation/image
references across 47 Markdown files, and `git diff --check` passed. The zk index
rebuilt 139 notes and finds the new feature from `Project Hub.md`.

The hardware-free production recorder stress wrote 96,000 equal frames across
18 channels at 48 kHz/128 frames per callback with zero drops or overflows and
verified channel identity below the one ignored
`user/acceptance-20260726-input-monitor/` directory. The new recorder callback
publication is a fixed 18-channel atomic snapshot with bounded callback loops,
and tests prohibit allocation, locking, file access, formatting, and unbounded
loops. The meter and take clients are mutually exclusive, connect only exact
configured sources, and do not change unrelated or final-bus routes. No JACK
server, synth, MIDI transmission, audible test, recording hardware, or physical
equipment was started. Plain `shr` resolved through this checkout's
`scripts/local.sh` to `target/debug/shr`, and its screenshot manifest showed
`DEV`.

Acceptance repaired the Levels transport page to retain repository-wide Stop
and Record positions while preserving both literal Stop and Panic together on
SYS through one narrow tested exception. Test-only expectations were corrected
for finite meter-floor reset and elapsed peak decay. One combined screenshot
render/check invocation received a transient SIGTERM; independent full
regeneration and a separate exhaustive drift check then passed. Package version
was `0.3.98`; at that acceptance point `src/sequencer.rs` remained
`SONG_VERSION = 7`.

Version `0.3.97` adds two distinct FT2 performance systems. Live Patterns
browses four Patterns at a time, queues Pattern- or bar-boundary activation,
supports immediate launch and deliberate retrigger, preserves tracker
selection, and can capture only successful activation boundaries for an
explicit append/replace confirmation. Its four transient lane controls are
mute, velocity, gate, and transpose. Loop Mix owns four private WAV slots with
independent bar-queued launch/stop, mute, level, DJ filter, region, offset,
tempo interpretation, and isolated missing/fault state while retaining the
single logical Loop source in direct or final-bus routing. Project format 7
stores all four slots; format 6's single loop migrates in memory to slot 1
without inspection rewriting the file.

The complete `0.3.97` acceptance pass on 2026-07-26 used Rust 1.85. Locked
check and debug build passed; the complete suite ran 752 tests with 748 passing
and four intentionally ignored private DSP renderers passing separately. All
112 screenshots passed native 40×13 rendering and exhaustive pixel/drift
validation; ShellCheck, 27 isolated audio-policy cases, and ten deterministic
demos also passed. Connected trials used one ignored
`user/acceptance-20260726-live-performance/` directory and covered
computer-keyboard and injected connected-ALSA Pattern launch, both quantized
boundaries, queue replacement/cancel, retrigger, capture, all lane controls,
exact transformed note ownership, managed synthv1 and external MIDI routes,
four simultaneous 48 kHz stereo WAV slots, slot-local level/mute/filter/remove,
direct and final-bus routing, non-silent capture, Stop, Panic, Project
replacement, and clean signal shutdown. The final 24-bit stereo bus recording
contained all four distinguishable loop sources exactly once; removing slot 4
removed only its frequency pair while slots 1–3 and the graph remained active.
Two runtime defects found during acceptance were repaired: selected-slot
removal no longer stops unrelated slots, and multi-slot Project load now
suspends/rebuilds the final bus as one transaction instead of mistaking an
owned Loop client rebuild for source loss. Process, JACK, and ALSA state matched
the starting baseline afterward. The MiniLab 3 was connected and received
injected command-pad traffic, but it was not physically actuated and no
listening claim was made.

The complete `0.3.96` acceptance pass on 2026-07-26 used Rust 1.85. Locked
check and debug build passed; all 737 tests ran with 733 passing and four
ignored private renderers passing separately. All 107 screenshots, 27 audio
tuner fixtures, deterministic demos, schemas, source checks, and public preset
XML also passed. Live trials covered the plain `shr` launcher and `DEV` badge,
computer-keyboard and connected-ALSA tracker entry, quantized chord recording,
owned synth startup/panic/shutdown, external MIDI routing, JACK playback and
capture, non-silent audio, and 60-second 18-channel recorder and final-mix
stress. They restored the original process and route state. The MiniLab was
enumerated and subscribed but was not physically actuated; exact entry-mode,
ownership, looping, boundary, cleanup, FluidSynth multipart, shared-route, and
same-lane behavior is covered by the complete automated suite.

The complete deterministic documentation screenshot set is reconciled to the
current UI; physical approval remains the next gate for UI/controller work.
The repository-only fullscreen-EQ and exact-font pass on 2026-07-25 completed
at package version `0.3.95`: all 702 tests ran in both debug and release
profiles with 698 passing and four intentionally ignored; locked check, debug,
and release builds and warning-denied Clippy passed. All 107 screenshots use
the tty1 `Uni2-TerminusBold24x12` console font at native 40×13 geometry and
passed the exhaustive drift check. No JACK, synth, MIDI, playback, recording,
or hardware-changing test was started for this pass.

The repository-only release pass on 2026-07-22 completed at package version
`0.3.94`: all 655 tests ran with 651 passing and four intentionally ignored;
locked check, debug, and release builds and warning-denied Clippy passed; the
105-image screenshot set regenerated and passed its exhaustive drift check;
and the isolated install fixture contained only the expected public package
tree. No JACK, synth, MIDI, playback, recording, or hardware-changing test was
started for that pass.

Multiple workers use this checkout and commit their own changes independently.
Branch tips, commit messages, and clean/dirty snapshots are intentionally not
recorded here. Inspect live Git state, preserve concurrent work, commit only
your own scope, and do not wait for unrelated workers to finish; follow the
canonical collaboration rule in `AGENTS.md`.

Plain `shr` resolves to this checkout's `scripts/local.sh` through
`/usr/local/bin/shr`; `$HOME/.bash_aliases` names the same launcher. The stale
`$HOME/.local/bin/shr` link still points to the removed former checkout and
must not be used. The launcher uses `target/release/shr` unless `SHSYNTH_BIN`
is explicitly set; the release TUI shows `REL`. The separate private
`shr-release` launcher is now only a compatibility alias. Repository-local
runtime metadata selects the companion Moj Sint checkout's fresh release
binary and public preset catalog; Playback
user sounds remain in the separate ignored private data root and were not
inspected during this configuration check.

The tty1 `.bashrc` autoload waits for systemd to leave its initial `starting`
state, allows one final second for boot-console output, clears tty1, and only
then invokes plain `shr`. The wait is capped at 30 seconds so a degraded or
stalled boot cannot block autoload indefinitely; SSH and manual launches do
not wait.

## Dated DSP/JACK closure record (2026-07-22)

This section records the completed DSP closure pass. It added validated
FFT/alias analyzers; centered four-point Lagrange interpolation for delay,
chorus, and flanger; first-order ADAA on the filter cubic pre-drive; short
reverb input all-pass diffusion; focused nonlinear, interpolation, and reverb
tests; and private level-matched audition renders. Distortion retained
first-order ADAA after multi-bin characterization. The implementation and
focused provenance are in `src/dsp/`, `src/effects/`, `src/effect_schema.rs`,
`src/main.rs`, `docs/AUDIO_GRAPH.md`, and `docs/CONFIGURATION.md`.

The earlier DSP-focused offline validation was coherent: its complete suite
passed 648 tests with zero failures and four intentionally ignored private
renderers; the later checkpoint-only diagnostic/panic change passed its focused
parser test and `cargo check --locked`. Locked release builds succeeded.
Private raw evidence and audition files are in the ignored
`user/dsp-lab/20260722T151647Z/`; do not overwrite, stage, publish, or copy that
directory into tracked documentation.

The amplifier was confirmed off only for the completed connected tests in the
originating session; fresh physical work still requires fresh explicit safety
authorization. JACK was left running exactly as found at 48 kHz, 128 frames,
three periods, RT priority 95 on `hw:A96`. Starting and final snapshots both had
18 ports, zero connections, no SHR/synthv1 process, and identical routes. No
persistent audio configuration or tuning changed.

Connected release results were healthy during sustained processing:

- `soft-cubic`, 10.027 s: 3,810 callbacks, mean 53.416 us, p99 98 us,
  maximum 224.222 us, zero misses/oversized callbacks, owner/synth CPU
  3.09%/5.09%, owner/synth RSS 119,388/129,284 KiB.
- `phase4-full`, 20.050 s and eleven effects: 7,576 callbacks, mean
  437.601 us, p99 532 us, maximum 1,013.125 us, zero misses/oversized
  callbacks, owner/synth CPU 17.51%/5.44%, owner/synth RSS
  121,752/129,324 KiB, 1,860,804 bytes effect storage and 589,824 bytes graph
  buffers.
- The final five-second `soft-cubic` diagnostic had zero meter clips and
  non-finite samples, zero limiter reduction, 51.847 us mean, 96 us p99 and
  253.202 us maximum callback time.

The teardown investigation is complete. synthv1 0.9.29 in `--no-gui` mode
ignores ordinary termination signals but implements an exact JACK Session
SaveAndQuit event. SHR now sends that event only to the configured managed
client, retains owned-process termination as a fallback, and removes its
short-lived shared session directory. The complete 48-message all-channel
panic remains mandatory but is paced at 100 microseconds between synthv1
messages; an unpaced burst caused one reproducible callback miss.

Final connected release evidence after that repair:

- `soft-cubic`, 5.011 s: 1,924 callbacks, mean 50.578 us, p99 75 us,
  maximum 138.406 us, zero misses/oversized callbacks, clips, non-finite
  samples, limiter reduction, or new JACK xruns.
- `phase4-full`, 20.051 s and eleven effects: 7,568 callbacks, mean
  437.313 us, p99 539 us, maximum 808.162 us, zero misses/oversized callbacks,
  clips, non-finite samples, limiter reduction, or new JACK xruns.
- Both runs restored byte-identical JACK and ALSA route snapshots and left no
  SHR or synthv1 process. The pre-existing system JACK server remained running
  at 48 kHz, 128 frames, and three periods.

## Publishing state

Public remote: <https://github.com/PaolaShultz/shr-daw>.
GitHub CLI is installed and authenticated as `PaolaShultz` over HTTPS. This
repository's local identity is `PaolaShultz` with GitHub's numeric no-reply
address. Keep those values; if authentication expires, use `gh auth login
--hostname github.com --git-protocol https --web` and let the user complete the
device flow. The repository is public, so apply the publishing boundary in
`AGENTS.md` before any requested commit or push.

## Private runtime and public packaging

The ignored `user/` tree is the private boundary for this checkout. The local
wrappers redirect XDG state/data, presets, and the loop inbox there. Important
roots are:

- `user/state/shsynth/`: runtime/controller configuration, backups, logs, and
  generated engine state;
- `user/data/shsynth/`: Ideas, Projects, demos, recordings, loops, loop inbox,
  and drum patterns;
- `user/presets/synthv1/`: cleared copies plus private/local presets;
- `user/downloads/`: private source archives.

Never replace this boundary with hardcoded Rust paths. Setup seeds only missing
cleared content and must preserve same-named user files. The only public
packaging authorities are `presets/synthv1/cleared-presets.txt`,
`kits/cleared-kits.txt`, `loops/cleared-loops.txt`, and
`demos/cleared-demos.json`. The four approved SHR Drums packages are tracked
under `kits/`; local helpers read them there without copying factory content
into `user/`, while installed setup selects the packaged shared-data root.

The LinuxSynths archive at
`user/downloads/392Synthv1Patches.tar.gz` has SHA-256
`f4f9157cf5d245f7371a702584e28a90d1cf92b9a1eec9fa38c43fad584016ea`.
Its 392 files have no verified licence/authorship notice. They are available
for private use only and must never be committed, packaged, mirrored,
downloaded by the public installer, or described as MIT/public domain. Only the
21 manifest-cleared project presets are public and MIT. MusicRadar's optional
drum download is also private: its terms permit musical use but prohibit raw
sample redistribution.

## Current machine and hardware state

- The active development system is now a Raspberry Pi 5 Model B Rev 1.1 with
  2 GB RAM, active cooling, and a 128 GB-class bottom-mounted NVMe root. The
  exact version-0.4.2 build, DSP, synthetic writer, memory, NVMe, PMU, thermal,
  and comparison evidence is in
  `docs/PI4_PI5_PERFORMANCE_COMPARISON_2026-07-28.md`. The Pi 5 stayed at
  2.4 GHz with no firmware throttle flag, undervoltage warning, or OOM, but its
  fresh builds used zram under the 2 GB memory limit. Connected 128/64-frame
  callback comparison remains open because the sole-owner and safe-output gate
  was not met. The planned 480×320 display occupies the top GPIO position
  rather than HDMI, so the design cannot use a top-mounted M.2 HAT; its housing
  will be self-designed and printed around the measured stack.
- At the current project size, the 2 GB development target is accepted for
  serialized work. Fresh tests took 110.65 seconds at 1,501,136 KiB peak RSS;
  a fresh release took 223.14 seconds at 1,407,344 KiB peak RSS. Both completed
  without OOM, while sampled available memory fell to 179,584 KiB and
  111,744 KiB respectively and zram supplied substantial transient headroom.
  The later version-0.4.3 incremental debug build took 47.03 seconds but was
  not memory-instrumented. These facts prove practical 2 GB development with
  documented swap, not simultaneous compilation and live-audio operation or a
  measured speed difference against a 4 GB Pi 5.
- Local configuration still selects the now-absent MiniLab 3 controller until
  the repaired startup migration runs. The connected, owner-purchased
  controller is an Arturia MiniLab mkII. JACK uses
  `system:playback_1`/`system:playback_2`, AudioBox USB 96 stereo capture on
  `system:capture_1`/`system:capture_2`, and the AudioBox MIDI port as external
  output. These are private configuration values, not portable defaults.
- The previous learned mapping targets `arturia-minilab-3`; the replacement
  identity profile is `arturia-minilab-mkii` and intentionally has no messages
  before MIDI Learn. Controller and performance MIDI roles remain separate.
  The established eight-pad command layout uses
  four page pads plus semantic positions 5–8: STOP/PANIC, PLAY/LOAD/PREVIEW,
  REC/capture, and TAP. MiniLab notes 40–43 use the canonical
  `stop`/`play`/`rec`/`tap-tempo` roles; legacy item-role configuration remains
  readable. The master rotary browses content and its press selects/confirms.
  The Routing screen reports live visibility, not merely remembered
  configuration.
- The optional CPU-3 audio profile was removed on 2026-08-29 to return all four
  cores to general builds and normal JACK scheduling. Persistent boot isolation,
  IRQ housekeeping, the performance-governor service, the JACK affinity drop-in,
  and private `audio.engine_cpu` pin are absent. The current kernel and already
  running JACK process retain their old CPU-3 state only until the next normal
  reboot; do not restart JACK merely to apply this change. The helper's original
  command-line backup remains available, and `shr-audio-tune doctor none`
  reports the shared-core persistent policy ready.
- The per-user `fluidsynth.service` and system `amidiminder.service` are masked
  and stopped. `/usr/bin/fluidsynth` and the TimGM bank remain for SHR-owned
  on-demand use. Setup and tuning do not start or restart JACK.
- The MiniLab mkII is owner-purchased; the remaining project equipment is
  borrowed. Preserve physical configuration and require explicit approval
  before any JACK, synth, MIDI, recording, audible, or other physical-hardware
  test.

Rerun `scripts/setup-local.sh` only when the user requests configuration or
hardware/JACK names change. Read `docs/MAINTAINER_HELPERS.md` first.

## Decisions and open acceptance

- The competition build keeps the current bounded one-managed-source effects
  topology. The post-competition multi-strip/two-aux redesign stays in
  `docs/POST_COMPETITION_MIXER_AUX_PLAN.md`; hardware loops and full-duplex live
  input remain deferred until physical monitoring choices are made.
- `audio.graph.enabled` remains opt-in/default-false in local state. FX editing
  may validate and save routing while the graph is disabled, but only an active
  owned graph provides final metering/processing. Dated performance evidence
  belongs in the Phase 1–4 measurement documents, not here.
- The generic synchronized recorder and final stereo performance bus are
  implemented, but synthetic stress is not physical-interface or MR18
  acceptance. The first borrowed MR18 remained packed and produced no hardware
  evidence. Development and physical acceptance for simultaneous independent
  18-channel playback and 18-channel recording are deferred until the Pi 5
  clean-machine flow and the other working flows are ready; the next MR18 loan
  should span several days. Follow `docs/MR18_TEST_PLAN.md` before claiming a
  hardware pass or a checked release.
- The established tracked screenshot set now covers every normal controller
  context plus Home, MIDI Learn, and all master overlays. Keep its exact
  scenarios, font, 40×13 geometry, integer scaler, and validation contract in
  `docs/MAINTAINER_HELPERS.md`; do not hand-edit generated PNGs.
- Loop browsing is owned by FT2. Loop Browser selection is silent until its
  position-6 PLAY preview; selection change, STOP, Back/close, or leaving Loop
  Player stops that preview. Failed preview/import keeps the FT2 caller and
  selection for retry, and import failure rolls back its private copy and
  Project attachment.
- Help is temporary reference navigation and preserves the exact caller,
  controller page, FT2 mode/location/editor state, and active workflow. LAN
  Help advertises a URL only after its port has been acquired.
- Managed preset/Idea replacement validates first and restores the previous
  engine session if replacement fails. Routing failures keep both the old
  persisted/runtime route and the entered draft. Idea MIDI, FT2 pattern,
  multitrack WAV, and final WAV recording are exclusive transport owners.

The open hands-on review is non-audible and must use a new empty FT2 Project.
Keep transport/recording stopped and do not attach routes. On the physical
40×13 TTY, verify the shared 38×11 overlay at `(1,1)`, its one-cell reveal,
launcher inside the bottom border, and uninterrupted final status row;
encoder/keyboard parity and wrap behavior; silent hidden launchers; two-step
Back behavior; live ROUTE field changes followed by whole-route Cancel restoring
the opening Project and route snapshot; the Loop Library's explicit PLAY
preview, stop/rollback, and return behavior; and every entered screen, including
an MTR FX caller return, starting on controller-menu page 1. Record observed
failures before changing behavior.

A later user-authorized musical/hardware pass should exercise the
standalone/FT2 synth ownership split, N00B versus Play/REC/Edit, independent
Edit length/ADD values, routing-default confirmation, and percussion smart
column reuse. Do not start that pass merely because the overlay review is
complete. Detailed UI contracts live in `docs/CONTROLLER_INTERFACE.md`,
`docs/TRACKER.md`, and the focused routing/effects documents linked from
`docs/README.md`.

## Terminal project-note layer

This machine is operated through a TTY. Its terminal-only `zk` notebook is
rooted at `$HOME/p`, covering `shsynth`, `shr-skills`, and later project
directories below that root without copying or moving their Markdown. The
entry point is `$HOME/p/Project Hub.md`. Configuration, the rebuildable SQLite
index, and its template are below `$HOME/p/.zk/`, outside both Git
repositories.

Use:

```sh
pnotes
pnotes search words
```

The first command opens all indexed notes in an `fzf` picker; the second
full-text filters before opening the picker. The wrapper is
`~/.local/bin/pnotes`. Direct `zk` commands must use
`--notebook-dir="$HOME/p" --working-dir="$HOME/p"` when invoked from elsewhere.

Git commits do not trigger a zk-specific synchronization hook and there is no
resident indexer. The Markdown files are authoritative; zk automatically
updates its rebuildable SQLite search cache when list/edit/search commands need
it. Use `zk index --force` with the explicit notebook and working directories
after a documentation pass when deterministic index validation is required.

The index excludes `.git`, `target`, `user`, `node_modules`, and `.zk`
subtrees; keep `shsynth/user/` excluded. The notebook uses normal Markdown
links, `nano` for editing, `less` for paging, and `sed` for previews. Installed
components are ARM64 `zk` 0.15.5 at `~/.local/opt/zk-0.15.5/` and Debian `fzf`
0.38.0. There is no resident process. Plan about 25 MiB disk, 31 MiB RAM while
actively searching, and zero RAM while idle, suitable for the planned 2 GB Pi
5.

## Installed tools and current validation boundary

Exact Rust 1.97.1 is the repository pin and is installed with `gh`, `xmllint`
(`libxml2-utils`), `shellcheck`, `zk`, and `fzf`. Rust 1.85 remains locally
available only for an explicit historical comparison; it is not the
development or validation default.
The system JavaScript tools use Node.js 24.18.0 LTS from the root-owned
NodeSource `node_24.x` repository, npm 12.0.1, and the root-owned Codex CLI
0.146.0. Documentation layout checks use Chromium and ChromeDriver
150.0.7871.181 with Selenium 4.31.1. Codex startup update checks are disabled
in the private user configuration because an unprivileged updater cannot
replace that system
installation; update it deliberately with
`sudo npm install -g npm@latest @openai/codex@latest`. The restored broken
`~/.local/bin/codex` link to the former `/home/patch` checkout is retained only
below ignored `user/system-update-20260729/`; `/home/patch` and the `patch`
account do not exist on this installation. Pillow is not installed and the
retired screenshot path is not part of current validation.
Use the scoped validation policy in `AGENTS.md`; historical full
suites, release builds, benchmarks, and screenshot batches are evidence in
their dated documents, not instructions to repeat them. No current physical or
audible acceptance should be inferred from synthetic or hardware-independent
checks.
