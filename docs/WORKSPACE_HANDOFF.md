# Workspace handoff

Updated: 2026-09-05

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
| SHR-DAW package | 0.4.8 |
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
| SHR Drums 0.2.0 | exact Git revision compiled into `shr`; no child process |
| Moj Sint 0.2.3 | exact Git revision installed as a managed external process |
| SHR Sampler 0.1.2 | exact Git revision installed as a managed external process; accepted runtime range `>=0.1.2,<0.2.0` |

SHR source supports Moj Sint schema 9 and seven models, including monophonic
Pressure Chain with Deep Cascade, Body Tap, and Cross Feed starts (24 cleared
starts in the companion source catalog). Pressure Chain exposes all eight
timbre values plus amp ADSR; position 5 is SWEEP and 13–15 remain AUX sends.
Its one stereo return uses the existing Project instrument strip. The
installer pins the compatible schema-9 engine and all 24 cleared starts.
The subsequent authorized build pass refreshed this checkout’s debug and
release binaries and the companion Moj host. It did not reinstall external
payloads or restart running applications.

The non-audible integration pass used rustc 1.97.1 (8bab26f4f, LLVM 22.1.6)
on AArch64. Formatting, locked checks, 18 focused Moj regressions, and the
normal SHR suite passed (1,128 tests, 14 ignored). Moj's normal all-target
suite and focused live DSP tests also passed. Controller/listening and
real-time hardware acceptance remain for the coordinated human session.

[How SHR-DAW works](HOW_IT_WORKS.md) owns the component process, MIDI, audio,
configuration, lifecycle, validation, failure, and redistribution boundaries.

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
