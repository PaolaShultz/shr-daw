# Workspace handoff

Updated: 2026-10-02

This is the short current-state record for work in this checkout. Source code
and machine-readable files are authoritative. Durable policy lives in
`AGENTS.md`, helper behavior in [Maintainer helper scripts](MAINTAINER_HELPERS.md),
and dated implementation records in
[Development history](DEVELOPMENT_HISTORY.md).

Do not add branch tips or clean/dirty snapshots here. This checkout is shared,
so inspect live Git state before editing, staging, or committing.

## CI notifications

[Failure/recovery issue threads](CI.md#failure-and-recovery-notifications) track
current default-branch CI incidents and explicitly close them after recovery.

## Expanded drum palette

Power Studio is a cleared 30-voice / 171-sample pack owned and released by
[SHR Drums](https://github.com/PaolaShultz/shr-drums/blob/main/packs/POWER_STUDIO.md).
This checkout ships the same package, with full mixed CC BY 4.0/CC0 source
credits. All 120 factory rhythms use its enlarged palette; older internal kits
receive explicit supported-note substitutions on factory LOAD. Saved user
patterns remain exact. Extended note classes preserve user overrides.
The kit selector and fill workflow are documented in
[Tracker](TRACKER.md#drum-pattern-library-and-transpose); current musical data
and software coverage are in [the pattern audit](DRUM_PATTERN_AUDIT.md).

## Current versions and formats

| Owner | Current value |
| --- | --- |
| SHR-DAW package | 0.4.9 |
| Rust toolchain | exact 1.97.1 from `rust-toolchain.toml` |
| Project `.shsong` | format 20 |
| Reusable drum pattern `.shdrum` | format 4 |
| Audio graph/effect data | graph format 3, effect format 1 |
| MIDI Idea metadata | format 3 |
| Multitrack session manifest | format 1 |
| MASTER STRIP | format 1 |
| Runtime configuration template | version 6 |
| Controller configuration | version 9 |

Formats 0 through 19 migrate in memory to Project format 20 with AUX return
limiters OFF. Formats before 19 also initialize optional
[instrument channel strips](CHANNEL_INSERTS.md) empty/OFF. Loading or
inspection never rewrites a Project. Unknown newer formats and malformed or
over-limit structures are refused before replacement.

Plain `scripts/local.sh` and the installed `shr` command select
`target/release/shr`, which shows `REL`. Development launches must set an
explicit `SHSYNTH_BIN=.../target/debug/shr` override; that binary shows `DEV`.

## Maintained component set

Machine-readable ownership remains in `Cargo.toml` and
`install/compatibility.json`:

| Component | Relationship |
| --- | --- |
| SHR Drums 0.2.1 | exact Git revision compiled into `shr`; no child process |
| SHR Synth 0.2.4 | exact Git revision installed as a managed external process |
| SHR Sampler 0.1.3 | exact Git revision installed as a managed external process; accepted runtime range `>=0.1.2,<0.2.0` |

SHR source supports SHR Synth through schema 11 and nine models, including
monophonic Pressure Chain and Open303, plus Perceptual (38 cleared companion starts).
Open303 adds its own native envelope controls and four starts. Pressure Chain
retains eight timbre values plus ADSR; Open303 uses eleven native controls and
volume. Both keep AUX on physical rotaries 14–16 and the existing Project instrument strip.
The authorized September 9 pass refreshed all checkout debug/release targets.
The configured checkout host/catalog and the normal release launcher now
provide Open303 after a normal exit/reopen; running processes were preserved.

The non-audible integration pass used rustc 1.97.1 (8bab26f4f, LLVM 22.1.6)
on AArch64. Formatting, locked checks, 18 focused SHR Synth regressions, and the
normal SHR suite passed (1,128 tests, 14 ignored). SHR Synth's normal all-target
suite and focused live DSP tests also passed. Controller/listening and
real-time hardware acceptance remain for the coordinated human session.

[How SHR-DAW works](HOW_IT_WORKS.md) owns the component process, MIDI, audio,
configuration, lifecycle, validation, failure, and redistribution boundaries.

## Player-only FX/AUX exit, 2026-11-07

The owner reported a Project-save prompt labeled FX/AUX after playing synths in
Player, with no Project to save. Player's live racks share Song storage; rack
or send edits therefore previously guarded exit even in the untouched startup
session. The offline regression reproduces that FX-only case. Ordinary note,
sound-selection and synth-control events have no Project-write path in the
checked flow; the exact input responsible for the reported rack difference is
not established.

Quit now permits an otherwise untouched, untitled, never-saved startup session
to exit with FX/AUX-only changes. Comparison against the complete startup Song
(with configured routing/FX defaults) ignores only insert rack, AUX routing and
drum rack. Name, notes, tempo, routing, arrangement, master strip and other
Project fields still protect exit; any loaded/saved Project also stays guarded.
The normal dirty flag and in-memory settings are preserved. Project replacement
still guards FX edits, and nothing is automatically saved. The Player-only
no-write regression now includes Perceptual. No audio or running app is touched.
On exact Rust 1.97.1/AArch64, the new regression failed before the fix; all 14
focused Player tests and all 1,261 normal tests pass afterward (14 historical
cases intentionally ignored). Locked checking, formatting and deterministic
documentation generation pass. Agent builds use `CARGO_INCREMENTAL=0` and the
normal target directory; test temporary paths stay in a task-owned ignored area.
The release executable is refreshed for normal exit/reopen; the running app is
preserved. No private configuration or saved Project is rewritten.

## Perceptual saw-source rework, 2026-11-07

The owner resumed the musical task after the note-interaction fix. The companion
revoices all ten Perceptual starts: detuned saw pairs, saw/pulse, an octave-pulse
blend, rough stepped ramps, and a broader missing-fundamental cluster. E06–E10
retain their five different swirl graphs and slow Rate law, with distinct
factory envelopes. Body on those five now controls topology behavior only;
Bright opens the source tone filter. No schema, host protocol, UI code or control
surface change is needed. Existing private Perceptual sounds inherit the source
change. The instrument guide describes sources, pot meanings and loading.

The checkout configuration already loads the companion release host/presets;
the installer pin is advanced with this change. App code is unchanged, so no
SHR app rebuild is necessary. Synth validation and native offline timing belong
in its handoff. Four installer-helper tests and deterministic documentation
generation passed for this metadata/documentation update. No running app,
audio, MIDI, hardware or recording was started
or reconfigured. Listening acceptance remains with the owner.

## Overlapping-note parameter investigation, 2026-11-07

The companion pin includes the focused SHR Synth correction: starting a second
note no longer restarts the shared pitch-update clock of held voices. Offline
regressions preserve the first voice's complete state in all ten Perceptual
starts, including wheels and moving controls. No filter/envelope reset was
found, and the tiny reproduced pitch change is not a confirmed explanation for
the user's audible click. The user declined recording; no audio or running
process was changed. Factory tones remain unchanged; the requested saw/rough-saw
character is still unresolved. Synth DEV/REL hosts were rebuilt and its normal
suite, strict Clippy, audit/deny and all-38 deterministic render comparison pass.
This metadata-only SHR change needs no app rebuild; the checkout configuration
already selects the rebuilt companion release host. The synth handoff owns the
diagnostic details and remaining acceptance boundary.

## Slow Perceptual swirls, 2026-11-07

The owner authorized five playable sounds, integration, full builds and checks,
without audio/hardware operation. The Perceptual reader/writer now retains
Counter Tide, Vowel Orbit, Phase Bloom, Spectral Carousel and Tidal Memory.
They extend the schema-11 topology vocabulary, with unchanged macro fields,
model identity, pickup/reset, Project route and physical mapping. Older readers
reject the new IDs; the installer pins the matching companion revision.

E06–E10 use pot 6 Rate for logarithmic 0.005–2 Hz filter movement, pot 7 Motion
for depth, Position for cutoff and Focus for resonance. Body changes each
wiring. ADSR stays on row 3; Volume/AUX 1–3 stay on row 4. The instrument guide
owns the mapping and companion `docs/PERCEPTUAL.md` owns DSP behavior.
The local configuration already points to the companion checkout host/catalog;
normal exit/reopen loads the rebuilt binaries and refreshes the preset list.

Exact Rust 1.97.1 on AArch64 passes the full normal all-target/all-feature
suite: 1,259 tests, zero failures, 14 historical tests intentionally skipped.
The topology regression first failed on the unknown IDs, then passed for all
ten identities, including live values, volume, save/load and physical CC order.
The locked check and formatting pass. Four Python installer contracts and
two documentation-generator policy tests pass. The companion handoff records
372 passing synth tests, strict Clippy, audit/deny and exact paired renders of
all 38 presets, including unchanged output for the earlier 33.

Warning-denied DAW Clippy reports 26 existing lints in unchanged code (recorder,
sequencer, UI and older tests, plus the existing schema-range expression).
No lint suppression or unrelated refactor was added. The synth's strict
Clippy passes. This is a documented validation limitation, not a clean DAW
Clippy result. Both DEV and REL application builds pass against the final
companion pin, as do their version/help checks and the focused Rust installer
contract. The configured REL catalog discovers all five new factory starts.
Builds used `CARGO_INCREMENTAL=0` and normal target directories; optimized DAW
test-harness builds were discontinued after the normal suite had passed, with
application binaries built directly using `cargo build --locked --bin shr`
and its `--release` equivalent. No running process was replaced.
Temporary renders/scratch and redundant test-build output are removed at
closeout; maintained factory presets, source/tests/docs and DEV/REL binaries
are retained.
No audio process, JACK, MIDI, controller, playback or hardware test is part
of this task.

## Perceptual integration, 2026-11-07

The owner authorized a new engine and full builds. SHR Synth's Perceptual model
adds E01–E05 Micro Delay, Spectral Split, Dispersion, Notch Motion and Virtual
Pitch. Schema 11 retains topology on load/save; native CC20–31 map to eight
tone controls plus ADSR, with CC7 volume and the existing three AUX slots.
Held-note control, pickup/reset and Project identity use the ordinary SHR Synth
path. The [instrument guide](INSTRUMENTS_AND_DRUMS.md#perceptual-synthesis)
owns the control layout; companion `docs/PERCEPTUAL.md` owns DSP and listening
limits. The installer manifest pins the matching companion source.

Validation is on x86_64 with exact rustc 1.97.1. Locked checking, formatting,
all-target/all-feature DEV and REL builds, both binary version checks and REL
help pass. The final REL catalog discovers all five new starts through the
checkout-local launcher. All seven model/surface integration tests, the new
five-topology save/load test and four installer contracts pass. The initial pass
found eight pre-existing EQ/effect/FT2 failures; the follow-up repair below
resolves them. No native Pi performance, physical-controller acceptance or
listening claim is made.
The new checkout-local runtime configuration selects the companion release
host and factory catalog. No existing private configuration was overwritten,
and no live app, JACK, MIDI, playback or hardware was started.

## Startup FX regression repair, 2026-11-07

Startup FX defaults are now included in the exact untouched-Project comparison.
A fresh FT2 Project can adopt the Player instrument without restarting it even
when the default master compressor or a restored FX snapshot is present.
Saved Projects and subsequent FX, note, Arrangement or routing edits still
prevent automatic retargeting. See [the tracker contract](TRACKER.md).

Six older EQ/effect tests assumed an empty master rack and/or reused its effect
ID. Their fixtures now allocate globally unique IDs and assert that existing
startup effects survive adding a master effect. The default compressor and
production graph validation are preserved. A new failing-before/passing-after
regression covers both built-in and restored FX defaults and seven protected
edit/save cases; the original eight failures and all 429 UI tests pass.

The locked full all-target/all-feature suite passes 1,259 tests with zero
failures and 14 historical tests intentionally ignored on exact Rust 1.97.1,
x86_64. Locked checking, formatting and all-target/all-feature DEV/REL builds
pass; both binaries report their version and the REL catalog retains all five
Perceptual sounds. The documentation generator's
existing local-path rejection was fixed by replacing the GigPies plan's
machine-specific checkout path with its public repository URL; generation and
both URL-policy tests pass without relaxing the security checks.
No audio, MIDI, hardware or live process was started or replaced.

## Open303 integration, 2026-09-09

The owner requested Open303 and a few presets. Source now supports the eighth
SHR Synth model, schema-10 Open303, its two preset-owned filter types and A01–A04
Rubber Bass, Accent Wire, Hollow Slide, and Soft Pluck (28 total SHR Synth starts).
The existing catalog, Project route, private save/reset, and instrument strip
retain its identity. It is monophonic and displays the same `M` marker.

Its eleven native controls plus CC7 volume now use the shared 4×4 surface
documented in [Instruments and drums](INSTRUMENTS_AND_DRUMS.md#shr-synth-sounds).
Physical rotaries 13–16 hold Volume and three AUX sends. Native envelope timings replace generic ADSR labels only for
Open303. The installer retains the separate Open303 MIT and Ooura FFT notices.
Companion source owns the native preparation, bounded note stack, and render
safety. Existing models and private files are preserved.

The installer now pins the compatible public SHR Synth engine with all 28 starts.
SHR Synth's normal suite passes 359 tests (36 ignored), focused Open303 release tests
and native sanitizer/allocation/spectral checks pass, and all factory presets
validate with deterministic paired release renders. SHR formatting, source
inspection, Python syntax and read-only compatibility checks pass. The owner subsequently authorized the full build. All debug and release
targets in both checkouts build with exact Rust 1.97.1 on AArch64. SHR
passes its locked check and 1,141 normal tests (14 historical tests ignored).
A stale routing-recovery test expected obsolete status prose; it now checks
the current save-error prefix while retaining all direct state assertions.
Existing compiler warnings remain. The fresh SHR release discovers all four
Open303 starts in an isolated catalog. All 28 SHR Synth presets validate, and the
four new starts produce identical paired release renders. Existing local
configuration already selects the companion checkout host/catalog; no private
configuration, presets, or independent installed payloads were changed.
No running process or hardware was restarted.

## 2026-09-28 companion rename

SHR Synth uses repository, checkout, command and install directory `shr-synth`,
and current configuration uses `shr_synth.*`. The reader accepts prior keys,
backend labels and channel bindings; `.mojsint` presets and model identities
remain compatible. The installer pins the renamed source and supplies the old
command alias for existing private configurations. The local legacy checkout
link preserves saved absolute paths and the already-running application.
The running older application retains readable legacy keys in its private
configuration, with updated executable/catalog paths. A subsequent save by the
rebuilt application writes the canonical keys.
The amp and recorder checkouts are `shr-tone-over-9000` and `shr-rec`.

The rename passes all 1,222 normal Rust tests (14 historical tests ignored),
including legacy configuration/backend and channel-strip identity regressions,
plus 21 Python helper tests, including payload layout and the compatibility
command alias. The generated documentation site is deterministic.
The selected compiler is exact Rust 1.97.1. The REL application builds; existing
compiler warnings remain. All 28 companion factory presets render byte-identically
between the old and renamed release hosts, with finite output.

## Current sequencer contracts

### Bounded ROLL

ROLL is a percussion-only draft tool under FT2 Tools `PAGE -> HISTORY ->
RHYTHM -> GEN`. Its cursor-anchored span cannot exceed the Pattern. Amount is
one through eight total pulses. EVEN uses the existing Retrigger command when
several pulses share a row. ACCENT and CRESCENDO write deterministic ordinary
rows with bounded explicit velocities.

Opening ROLL selects the visible NEW CLONE policy. Apply then delegates to the
existing stopped independent-Pattern transaction, leaves the source exact, and
adds one final Arrangement reference. EMPTY ONLY and REPLACE NOTE are explicit
current-Pattern alternatives. Browsing, setting changes, Repeat, and inspection
do not write. Cancel or any refusal preserves Pattern data, Arrangement,
History, dirty state, transport, routing, and cursor context.

### A A B A Arrangement assistant

ARRANGE FORM captures A from the Pattern referenced by the selected
Arrangement step. B begins unset and must be chosen explicitly from existing
sorted Pattern IDs. The draft is exactly A A B A; it neither creates nor edits
a Pattern.

APPEND atomically adds four references after the current order. REPLACE uses
the existing unsaved-Project guard before replacing only the order. CANCEL,
Back, invalid bounds, missing Patterns, validation failure, and refused
replacement preserve the exact Song, Pattern data, Pattern History, dirty
state, transport, and FT2 cursor/context.

The controller uses the existing four-page action table and dispatcher. The
keyboard opens FORM with `F`, browses with Left/Right, applies APPEND with `A`,
requests guarded REPLACE with `R`, and cancels with `C`, `B`, or Esc.

The focused contracts and acceptance matrices live in
[deterministic generative tools](DETERMINISTIC_GENERATIVE_TOOLS_ACCEPTANCE.md)
and the [Arrangement assistant](ARRANGEMENT_ASSISTANT_ACCEPTANCE.md).

## Latest software evidence

The September 28 audio follow-up adds a private, bounded
`audio-diagnostics.log` for graph JACK xruns, SHR callback deadline/size faults,
and protection resets in the five AUX effect kinds. The callback accumulates
counters; the UI/daemon owner writes grouped observations and current control
targets. Normal distortion/bypass is not a fault. See
[Audio incident diagnostics](HOW_IT_WORKS.md#audio-incident-diagnostics).
The reported hot-AUX glitch remains unattributed without a captured event.

The owner authorized the full test/build pass. Exact rustc 1.97.1
(8bab26f4f, AArch64, LLVM 22.1.6) passes the locked check, the focused fault
allocation checks, all 1,219 normal Rust tests, 20 Python helper tests, and 31
isolated audio-policy cases. Fourteen historical/exhaustive tests remain
ignored. The first full run hit the preset guard because the shared temporary
root contains a `.git` marker; rerunning with `TMPDIR` below ignored `user/`
passes without weakening that guard or changing the shared temporary root.
The all-target/all-feature DEV build, locked REL application build, and both
binaries’ version/help checks pass. Existing compiler warnings remain.
Normal exit/reopen selects the rebuilt REL binary; no live process was replaced.
The documentation site is regenerated from the updated focused guides, with
stable Markdown finding anchors repairing an older review’s broken site links.
No live app, JACK, synth, MIDI, playback, recording, or controller LED command
was started.

[Controller pad LED feedback](CONTROLLER_LED_FEEDBACK.md) records the researched
MiniLab mkII colour/off SysEx protocol and a proposed first integration with
the canonical menu/action table. No LED driver is enabled. Pad release,
bank/Shift behaviour, output ownership, and exit restoration still require
an explicitly authorized hardware trial before enabling that feature.

The September 14 stuck-note follow-up fixes Player scale changes suppressing
held-note releases. The reported N00B-off incident led to a separate SHR Synth host
fix: callback-budget excess now defers MIDI events instead of dropping them,
and queue loss stops the owned instrument with an explicit fault. SHR Synth's handoff
owns the failing-before/passing-after overload evidence; the heard incident
remains unattributed without live event evidence.

Exact rustc 1.97.1 passed both locked checks, 46 focused SHR engine tests and
18 focused SHR Synth host/engine tests, including allocation and finite-silence checks.
The owner then authorized rebuilding all: locked all-target/all-feature DEV
and REL builds pass in both checkouts, as do both SHR version/help and SHR Synth host
help checks. Existing SHR warnings remain. Full suites, historical tests,
Clippy and hardware tests were not run in this focused pass. No running app,
JACK, synth, MIDI, playback or recording was restarted. Normal exit/reopen
selects the rebuilt checkout binaries.

The September 11 end-of-day pass used exact rustc 1.97.1 (8bab26f4f, LLVM
22.1.6), AArch64. Formatting, locked all-target/all-feature checks, all 1,200
normal tests, 20 Python helper tests and 31 isolated audio-policy cases passed.
Clippy completed with existing warnings. All DEV/REL targets built and both
SHR binaries passed version/help checks. Fourteen historical tests stayed
intentionally ignored.

SHR Synth now reuses idle voices, then the oldest released note, before taking
a held note when every voice is held. This protects a held Swarm bass from
high-note release tails without increasing the voice count. Its focused
regression and 368 normal tests passed (36 historical cases ignored), as did
warning-denied Clippy, audit/deny, all DEV/REL builds and host help checks. All
28 cleared presets validate and paired release renders are finite and exact;
SHR Synth’s handoff owns the probe details. No live app, JACK, synth, MIDI, audio
playback, recording or hardware was started or changed by this pass. Normal
exit/reopen selects the rebuilt checkout binaries.

The September 11 recording follow-up keeps Player MIDIREC (lowercase `r`) and
adds AUDIO → WAVSTOP / DETAILS / WAV REC (Shift-R). MIDI and final stereo
capture can run together; WAVSTOP affects only audio, while Player STOP ends
its MIDI/arp transport and stereo take. Home RECORDER now opens the simple
stereo view, with final-output L/R meters, elapsed time, filename and faults.
DETAILS returns to its caller without ending capture. TOOLS → RAW REC retains
the old multitrack recorder while standalone `shr-rec` remains a foundation.
Explicit WAV REC can activate the configured final bus; failed activation
preserves MIDI capture. Raw recording and stereo recording remain exclusive.
Formatting, locked checking and all 1,192 normal tests pass on exact Rust
1.97.1; 14 historical tests remain ignored. The release build and version/help
checks pass; normal exit/reopen selects the rebuilt binary. Existing compiler
warnings remain. No hardware/audio session was run.


The September 11 synth surface change uses four rows of four in
Player and FT2 PARAM. Rotary 1 edits the first parameter; click toggles visible
NAV for menu-page selection, and another click returns to editing. Volume/AUX
1/AUX 2/AUX 3 occupy physical rotaries 13–16. Previously displaced SHR Synth controls
return; Dual Filter exposes all amp stages together and retains filter attack,
sustain and release as preset-owned values. The one information row keeps
chord/note names and the enabled scale; Player Shift-turn changes that scale.
The focused instrument/controller guides own the complete mappings.

The authorized September 11 pass used rustc 1.97.1 (8bab26f4f, LLVM 22.1.6)
on AArch64. Locked checking, formatting, all 1,189 normal Rust tests, 20 Python
helper tests, and 31 isolated audio-policy cases passed. Fourteen historical
Rust tests remained intentionally ignored. One new render-test fixture needed
an active instrument before asserting AUX labels; its focused rerun and the
complete suite pass. DEV and REL builds and their version/help checks pass.
Existing compiler warnings remain. SHR Synth's matching pass also completed:
367 normal tests passed with 36 historical cases ignored, warning-denied
Clippy and audit/deny passed, and DEV/REL host builds pass their help checks.
All 28 cleared presets validate and produce identical finite paired one-second
release renders. SHR Synth's handoff owns the exact render probe and hash. No live
app, synth, JACK, MIDI, playback, recording, private configuration, or hardware
state was changed by this pass. Normal exit/reopen selects the rebuilt binaries.

The September 10 synth follow-up adds the monophonic `M` to Presets, identifies
changed Project areas in the exit dialog, and protects Home mouse exit. The
pinned SHR Synth source retriggers live mono envelopes on each press while retaining
pitch glide and non-retriggering held-note fallback. The combined pass used
exact Rust 1.97.1 on AArch64: locked checks, 40 focused Project tests, both
preset-list tests, and the full normal SHR suite pass (1,157 passed, 14 ignored).
SHR Synth passes 362 normal tests (36 ignored), its focused retrigger/native contracts,
and warning-denied Clippy. Helpers pass 20 Python tests and 31 audio-policy
cases. Fresh DEV/REL binaries for both applications build and pass non-audible
executable checks. Existing SHR compiler warnings remain; no app, host, MIDI,
or audio was started for acceptance.

The September 10 controller Learn pass used exact Rust 1.97.1 on AArch64.
The locked check, all 57 controller regressions, and the complete normal Rust
suite passed: 1,153 tests, with 14 historical/rendering/benchmark tests ignored.
The helper suites passed 20 Python tests and 31 isolated audio-policy cases.
DEV and REL application binaries both build with the Shift and navigation/save
fixes; existing compiler warnings remain. The fresh REL binary successfully
reads the complete recovered private controller mapping. The running application
was closed cleanly at the owner's request; JACK was preserved and no synth or
playback was started.

The September 8 title change marks SHR Synth Pressure Chain with one inverted
`M` cell in Playback and FT2 PARAM. Other model titles have no voice marker.
Long names reserve room for the marker inside the existing title area.
Formatting and source/whitespace checks passed. Existing native-size UI tests
now cover the marker, long-name clipping, and its absence on other SHR Synth models;
these tests and compilation subsequently passed in the September 9 combined pass.

The September 5 full pass used rustc 1.97.1 (8bab26f4f, LLVM 22.1.6) on
AArch64. Formatting, locked all-target/all-feature checks, 1,128 normal Rust
tests, 11 Python helper tests, and 29 isolated audio-policy checks passed.
All debug and release targets build; existing dead-code warnings remain.
The 14 opt-in historical/exhaustive tests stayed ignored.

SHR Synth passed 349 normal tests (35 ignored), warning-denied Clippy, audit,
and licence/source checks. A release-only test allocator declaration was
corrected; its five focused regressions pass in debug and release. Both build
profiles complete. The fresh release host validates all 24 cleared presets,
and paired offline renders of all three Pressure Chain topologies match
byte-for-byte. No running host, JACK, MIDI, hardware, playback, or recording
was started or restarted; listening and real-time headroom remain unverified.

The September 2 combined pass used exact Rust 1.97.1.
Formatting, locked check, all ten Arrangement-assistant regressions, the
117-test generator-related filter, the four exact ROLL regressions, and the
complete normal suite passed. The clean suite result was 1,114 passed, zero
failed, and 13 opt-in tests ignored.

A later full build pass repeated locked check and the complete normal suite,
then produced both canonical AArch64 artifacts. DEV took 2m14s with 1,537,568
KiB peak RSS. REL took 2m59s with 1,548,464 KiB peak RSS. Neither artifact was
launched.

All 13 opt-in tests later passed offline and serially. Eleven historical,
exhaustive, callback-cost, and private-renderer tests passed together. The two
create-only drum renderers passed when each received a fresh nonexistent
destination. Their generated evidence remains below ignored `user/` and must
not be staged or published.

On 2026-09-04, the managed JACK generator was changed to select JACK2
synchronous mode before the ALSA backend. The 29-test isolated audio-policy
suite, ShellCheck, and shell syntax passed. A live 48 kHz/128-frame/two-period
run reported 128 capture plus 128 playback frames. A representative two-NAM
plus cabinet chain ran for five minutes at roughly 26–34% JACK DSP load with
zero xruns and 51–55°C observed temperature. This is JACK graph-latency and
scheduling evidence, not a physical analogue loopback measurement.

Hardware tests, Clippy, JACK, synth processes, external MIDI, audible playback,
recording, listening, and Raspberry Pi callback/headroom acceptance were not
part of those passes.

## Machine and safety state

The October 1 Bluetooth trial uses PipeWire/WirePlumber with the distribution's
PipeWire JACK libraries selected system-wide. The old standalone `jack.service`
and the trial BlueALSA services are disabled; the original JACK service and
command files remain intact. The musician's PipeWire services start with the
user manager and support headless Bluetooth. Exact device choices, configuration
backup and rollback notes remain below ignored `user/audio-bluetooth-20261001/`.
Do not restart the old JACK service alongside this audio owner.

Routing now discovers typed JACK playback destinations and offers separate
stereo AUDIO IN / AUDIO OUT choices. The October 2 source follow-up applies
AUDIO OUT live through the shared final bus and adds paired-device BT DEVICE /
BT LINK controls. AUDIO IN changes still require reopening SHR. Player, drums
and loops now activate the same final bus under normal audio autoconnection;
loop replacement retains that bus and final capture. Exact playback links are
restored when a disconnected output returns. The authorized October 2 build
passes the locked check and 73 focused routing, Bluetooth, final-bus, loop,
monitoring and engine-lifecycle regressions with exact rustc 1.97.1 (8bab26f4f,
AArch64, LLVM 22.1.6). All-target/all-feature DEV and the REL application build,
formatting, and both binaries' version/help checks pass. The subsequent warning
cleanup removes unused code, limits test helpers to test builds and asserts the
timeline test's Result. Its all-target/all-feature check, 77 focused tests and
refreshed DEV/REL application builds pass without compiler warnings; both
binaries pass version/help checks. Full-suite, historical, Clippy and hardware checks were not run
in this focused pass. Normal exit/reopen selects the rebuilt REL binary. No live
app, audio process or connection was restarted or changed.
An explicit empty graph input means NONE, while an omitted key retains the
legacy capture preference. Software playback/effects/final recording no longer
require a physical capture device. Input monitoring still requires the exact
chosen pair. Audio-only saves preserve MIDI settings and do not require offline
MIDI devices to reconnect. Exact Rust 1.97.1 passes the locked check, 1,230 normal tests
(14 historical tests ignored), and the REL build. A live twelve-second
Bluetooth synth/final-capture trial with the USB audio profile disabled passes
with no xruns, dropped frames or callback deadline misses. Reconnection and a
six-second effects trial also pass. Physical listening remains the user's check.

The checkout directory is now `~/p/shr-daw`. The former `~/p/shsynth` is a
compatibility symlink for existing processes and saved absolute paths. Local
launchers, current runtime path settings, sibling-project references, and the
zk notebook now use the new directory. Keep the compatibility link while old
processes, build artifacts, or private saved paths still depend on it.

The development machine is a Raspberry Pi 5 Model B Rev 1.1 with 2 GB RAM,
active cooling, and an NVMe root. The current physical controller is an
Arturia MiniLab mkII. Remaining project equipment is borrowed.

The optional dedicated CPU profile was removed to return all four cores to
general scheduling. A kernel and JACK process that were already running at
removal can retain the old CPU assignment until the next normal reboot. Do not
restart JACK merely to apply that change.

Do not start JACK, synths, ALSA/MIDI transmission, playback, recording, or any
audible or hardware-changing workflow without explicit user permission.
Configuration names, routes, logs, and device-specific state are private and
must not be copied into documentation.

## Public and private boundary

Every tracked file is public. The ignored `user/` tree and XDG state/data
locations contain private configuration, logs, Ideas, Projects, recordings,
downloads, loops, routes, learned mappings, presets, kits, and evidence.
Do not inspect, stage, summarize, or publish them during ordinary repository
work.

Public payloads are limited to entries named by the synth preset, kit, loop,
demo, SHR Synth, and SHR Sampler cleared manifests. `THIRD_PARTY.md` owns
licence, provenance, and redistribution statements. Setup may seed missing
cleared files but must preserve same-named musician files.

## Genuine open work

- Run the MR18 procedure only when the user authorizes the borrowed-hardware
  session. Synthetic recorder tests do not prove 18-channel hardware capture.
- Complete non-audible physical 40x13 overlay and navigation review before any
  separate musical or audible acceptance pass.
- Keep screenshot generation explicit. Do not hand-edit generated PNGs, and
  follow the exact helper contract before refreshing them.

Current user workflows belong in [Using SHR-DAW](USING_SHR_DAW.md), the
[Tracker guide](TRACKER.md), and the
[controller interface](CONTROLLER_INTERFACE.md). Plans and proposals are
indexed separately in [SHR-DAW documentation](README.md) and do not override
implemented behavior.
