# Workspace handoff

Updated: 2026-09-11

This is the short current-state record for work in this checkout. Source code
and machine-readable files are authoritative. Durable policy lives in
`AGENTS.md`, helper behavior in [Maintainer helper scripts](MAINTAINER_HELPERS.md),
and dated implementation records in
[Development history](DEVELOPMENT_HISTORY.md).

Do not add branch tips or clean/dirty snapshots here. This checkout is shared,
so inspect live Git state before editing, staging, or committing.

## Current versions and formats

| Owner | Current value |
| --- | --- |
| SHR-DAW package | 0.4.9 |
| Rust toolchain | exact 1.97.1 from `rust-toolchain.toml` |
| Project `.shsong` | format 19 |
| Reusable drum pattern `.shdrum` | format 4 |
| Audio graph/effect data | graph format 2, effect format 1 |
| MIDI Idea metadata | format 3 |
| Multitrack session manifest | format 1 |
| MASTER STRIP | format 1 |
| Runtime configuration template | version 6 |
| Controller configuration | version 9 |

Formats 0 through 18 migrate in memory to Project format 19 with optional
[instrument channel strips](CHANNEL_INSERTS.md) initially empty/OFF. Loading or
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
| Moj Sint 0.2.4 | exact Git revision installed as a managed external process |
| SHR Sampler 0.1.3 | exact Git revision installed as a managed external process; accepted runtime range `>=0.1.2,<0.2.0` |

SHR source supports Moj Sint through schema 10 and eight models, including
monophonic Pressure Chain and Open303 (28 cleared companion starts).
Open303 adds its own native envelope controls and four starts. Pressure Chain
retains eight timbre values plus ADSR; Open303 uses eleven native controls and
volume. Both keep AUX on physical rotaries 14–16 and the existing Project instrument strip.
The authorized September 9 pass refreshed all checkout debug/release targets.
The configured checkout host/catalog and the normal release launcher now
provide Open303 after a normal exit/reopen; running processes were preserved.

The non-audible integration pass used rustc 1.97.1 (8bab26f4f, LLVM 22.1.6)
on AArch64. Formatting, locked checks, 18 focused Moj regressions, and the
normal SHR suite passed (1,128 tests, 14 ignored). Moj's normal all-target
suite and focused live DSP tests also passed. Controller/listening and
real-time hardware acceptance remain for the coordinated human session.

[How SHR-DAW works](HOW_IT_WORKS.md) owns the component process, MIDI, audio,
configuration, lifecycle, validation, failure, and redistribution boundaries.

## Open303 integration, 2026-09-09

The owner requested Open303 and a few presets. Source now supports the eighth
Moj model, schema-10 Open303, its two preset-owned filter types and A01–A04
Rubber Bass, Accent Wire, Hollow Slide, and Soft Pluck (28 total Moj starts).
The existing catalog, Project route, private save/reset, and instrument strip
retain its identity. It is monophonic and displays the same `M` marker.

Its eleven native controls plus CC7 volume now use the shared 4×4 surface
documented in [Instruments and drums](INSTRUMENTS_AND_DRUMS.md#moj-sint-sounds).
Physical rotaries 13–16 hold Volume and three AUX sends. Native envelope timings replace generic ADSR labels only for
Open303. The installer retains the separate Open303 MIT and Ooura FFT notices.
Companion source owns the native preparation, bounded note stack, and render
safety. Existing models and private files are preserved.

The installer now pins the compatible public Moj engine with all 28 starts.
Moj's normal suite passes 359 tests (36 ignored), focused Open303 release tests
and native sanitizer/allocation/spectral checks pass, and all factory presets
validate with deterministic paired release renders. SHR formatting, source
inspection, Python syntax and read-only compatibility checks pass. The owner subsequently authorized the full build. All debug and release
targets in both checkouts build with exact Rust 1.97.1 on AArch64. SHR
passes its locked check and 1,141 normal tests (14 historical tests ignored).
A stale routing-recovery test expected obsolete status prose; it now checks
the current save-error prefix while retaining all direct state assertions.
Existing compiler warnings remain. The fresh SHR release discovers all four
Open303 starts in an isolated catalog. All 28 Moj presets validate, and the
four new starts produce identical paired release renders. Existing local
configuration already selects the companion checkout host/catalog; no private
configuration, presets, or independent installed payloads were changed.
No running process or hardware was restarted.

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
1/AUX 2/AUX 3 occupy physical rotaries 13–16. Previously displaced Moj controls
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
Existing compiler warnings remain. Moj Sint's matching pass also completed:
367 normal tests passed with 36 historical cases ignored, warning-denied
Clippy and audit/deny passed, and DEV/REL host builds pass their help checks.
All 28 cleared presets validate and produce identical finite paired one-second
release renders. Moj's handoff owns the exact render probe and hash. No live
app, synth, JACK, MIDI, playback, recording, private configuration, or hardware
state was changed by this pass. Normal exit/reopen selects the rebuilt binaries.

The September 10 synth follow-up adds the monophonic `M` to Presets, identifies
changed Project areas in the exit dialog, and protects Home mouse exit. The
pinned Moj source retriggers live mono envelopes on each press while retaining
pitch glide and non-retriggering held-note fallback. The combined pass used
exact Rust 1.97.1 on AArch64: locked checks, 40 focused Project tests, both
preset-list tests, and the full normal SHR suite pass (1,157 passed, 14 ignored).
Moj passes 362 normal tests (36 ignored), its focused retrigger/native contracts,
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

The September 8 title change marks Moj Sint Pressure Chain with one inverted
`M` cell in Playback and FT2 PARAM. Other model titles have no voice marker.
Long names reserve room for the marker inside the existing title area.
Formatting and source/whitespace checks passed. Existing native-size UI tests
now cover the marker, long-name clipping, and its absence on other Moj models;
these tests and compilation subsequently passed in the September 9 combined pass.

The September 5 full pass used rustc 1.97.1 (8bab26f4f, LLVM 22.1.6) on
AArch64. Formatting, locked all-target/all-feature checks, 1,128 normal Rust
tests, 11 Python helper tests, and 29 isolated audio-policy checks passed.
All debug and release targets build; existing dead-code warnings remain.
The 14 opt-in historical/exhaustive tests stayed ignored.

Moj Sint passed 349 normal tests (35 ignored), warning-denied Clippy, audit,
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
demo, Moj Sint, and SHR Sampler cleared manifests. `THIRD_PARTY.md` owns
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
