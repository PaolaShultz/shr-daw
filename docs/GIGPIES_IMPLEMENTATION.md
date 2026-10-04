# shr-daw in GigPies: activation plan

Planning baseline **2026-10-04 / GP-2026-10-04.1**. All new tasks below are
**planned**, not implemented by this document. [Central inventory](https://github.com/PaolaShultz/gigpies/blob/main/docs/MODULE_IMPLEMENTATION_MAP.md) ·
[Agreed contracts](https://github.com/PaolaShultz/gigpies/blob/main/docs/MODULE_CONTRACTS.md). Existing product roadmaps remain authoritative for
unrelated work; this plan owns only the GigPies integration increments below.

## Objective and boundary

Own workstation/tracker/instrument graph and its existing recorder. GigPies may inspect narrow DSP/controller patterns with provenance; do not migrate the band mixer into DAW or remove its recorder as part of this plan.

## Source and evidence reviewed

Repository: `/home/shome/p/shr-daw`. Inspected HEAD: `d93447d1ae391e7910c54965f886ed39baedab1c`.
Clean at inspection; recheck before editing. This dated observation is not a future ownership claim.

Owning documents: README.md, AGENTS.md, docs/WORKSPACE_HANDOFF.md, AUDIO_GRAPH.md, CONTROLLER_INTERFACE.md, MULTITRACK_RECORDING.md.

Source inspected: `src/audio_graph.rs::{GraphDefinition,RecordingTap}`, `src/audio_recorder.rs::{AudioRecorder,recover_interrupted}`, `src/controller*.rs`, Cargo.toml and install/compatibility.json.

Implemented workstation graph and raw/final recording, exact Git-pinned SHR Drums library and managed Synth/Sampler processes. The graph depends on workstation state and is not a drop-in GigPies mixer. No mocks are being adopted as product engines.

These are source inspection and previously recorded results, not fresh builds or
physical acceptance. The planning session runs documentation checks only.

## Milestones and tasks

Reference only; no GigPies runtime dependency. First useful milestone is intentionally deferred. Only an explicit GP-07 extraction/reference request naming a concrete missing function and provenance may activate DAW-01. Existing GigPies adapted equations already have attribution; avoid new work unless a real consumer needs it.

Task states are execution dependencies: READY has no missing software provider;
WAITING names its precise prerequisite; DEFERRED has an activation condition.
Source delivery and build reservation are additional launch prerequisites on a
peer. Every row has one owner, the repository named in its Owner column. A later
task starts only after the previous artifact is reviewed, never merely delivered.

| Task / priority / state | Owner | Work area, inputs and required artifact | Output and measurable acceptance |
|---|---|---|---|
| DAW-01 / P3 / DEFERRED | shr-daw | Only an explicit GP-07 extraction/reference request naming a concrete missing function and provenance may activate DAW-01. Existing GigPies adapted equations already have attribution; avoid new work unless a real consumer needs it. Existing named source files only after activation; provider contract must be accepted first. | When activated, read named existing source/contracts, provide a narrow ownership/licence/behavior mapping and synthetic vectors to GigPies. Any requested DAW change must have its own bounded scope; do not remove standalone features. |

## Validation and failure behavior

Documentation/format/source checks only under current AGENTS build restriction. Build-producing commands require a later explicit combined-build/test authorization; then use the exact focused commands selected by that authorized task. Do not run full suite/Clippy/release from this plan. This exception overrides the generic build example in the launch card.

Preserve exact routes, recorder readiness/recovery and All Notes Off in owner code; no new cross-show connection or process lifecycle is activated. Failed reference fit stays reference-only.

Historical research, auditions, exhaustive matrices, long soaks, full-show renders
and physical/combined-load checks are intentionally outside the normal software
milestones unless their protected behavior changes. Retain their owning documented
on-demand commands; no private media download or test hardware side effect.
Independent builds retain lockfiles and existing repository editions; this plan
does not upgrade dependencies/editions or replace existing intra-repository workspace
paths. The ban is on new sibling-repository path dependencies.

## Resources, review and recovery of work

No initial node/build reservation. On activation use a freed software lane with one jobs=1 build slot and exact reviewed source pin. Provisional first-harness budget ≤1 GiB compiler RSS and ≤512 MiB new output; native DSP builds need fresh inventory. No cache/media mirroring or background bench.

Independent fallback: none; leave deferred until a real consumer and scoped authorization exist. No unbounded render or research assignment.
Before builds check free space and target size; below 20 GiB free or above 5 GiB
output is a review, not permission to delete another task's cache. No reduced
coverage/debug information to make a budget appear to pass.

Handoff: exact changed files, commit plus patch hashes or bounded source manifest
if uncommitted, contract IDs/versions and provider-fixture hashes, commands/results,
intentional skipped classes, remaining limits and next task/owner. Stage only named
owned changes if a later implementation session commits; no public push is implied.
Receiving owner reviews independently and writes an immutable private-ledger
acknowledgement. Interrupted work stays visible with last completed acceptance
criterion; never reset/stash/clean another session or replay an uncertain mutation.

## Activation boundary

Only an explicit GP-07 extraction/reference request naming a concrete missing function and provenance may activate DAW-01. Existing GigPies adapted equations already have attribution; avoid new work unless a real consumer needs it. No runtime change or compatibility claim is made by writing this plan.

## Implementation launch prompt

Host/cwd assignments and source preparation are in GigPies PARALLEL_WORK_PLAN.md.
This is a prompt for a later user-started session; no implementation worker has
been started by the planning pass.

```text
Read docs/GIGPIES_IMPLEMENTATION.md in shr-daw, check current source and owning instructions,
and verify whether its activation condition is satisfied. There is no READY
implementation task for this repository in the initial GigPies wave. Do not
invent one. If still deferred, report that fact and stop. If activated by an
explicit later GigPies task, implement only the named bounded task after its
contract/ownership review; preserve other sessions and unrelated work. No sibling
writes, unilateral contract changes or sibling path dependencies. Use Rust 1.97.1,
Cargo.lock and CARGO_INCREMENTAL=0 with the shared build slot and cargo -j 1;
respect any stricter owning build restriction. No physical audio/MIDI/DMX,
playback, devices, services, shared load, publication or deployment. Never mark
mocked/incomplete work done; report exact dependency mismatches and keep progress
in this plan. There is no busywork fallback; remain deferred.
```

## Progress

- 2026-10-04: source and owner documents inspected; plan written. Implementation
  tasks remain in the states above. Physical evidence retains its original limits.
