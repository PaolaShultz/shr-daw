# Experimental development roadmap

Status: working order for an experimental project, not a production-release
schedule.

This roadmap owns the current development order. Detailed behavior remains in
the focused product, installation, FT2, and hardware documents linked below.

## Lifecycle and compatibility identifiers

All current SHR-DAW source, `0.x` package versions, and tags are experimental.
The package version and the component versions in
`install/compatibility.json` identify compatible source and installed files;
they do not declare a production release, product tier, or completion level.
Public documentation follows the current source and does not hard-pin one
experimental tag as “the release.” Historical tags remain reproducible
snapshots.

A production release will be an explicit future decision with its own version,
support boundary, installation target, evidence, and announcement. Until then,
progress is described by behavior and evidence rather than release ceremony.

## Scope rule

Work on the current track before pulling from later or speculative plans.
An observed defect or missing recovery path that blocks the current work is in
scope. A new feature, redesign, optimization, or interesting experiment is not
in scope unless the owner explicitly moves it into the current track.

The owner supplies product intent that has not yet been written down. Do not
infer missing FT2 requirements from nearby ideas or implement random entries
from [Future improvements](FUTURE_IMPROVEMENTS.md). Record an intended action,
result, state boundary, and acceptance path before implementing it.

## Established foundation — trust the existing product

Outcome: every existing menu entry is in the intended place and every current
workflow works as intended on the supported compact UI and a clean normal
Raspberry Pi OS Lite installation.

Status: the Raspberry Pi 5 installation, doctor, managed-JACK, silent-engine,
and repository validation evidence passed. This is an experimental foundation,
not a production release.

The evidence boundary requires:

- every reachable menu entry and controller-menu item checked for its intended
  screen, page, order, label, and return location, using keyboard/controller
  parity where both are supported;
- every currently documented workflow checked through normal completion,
  cancellation or Back, repeated use, failure and retry, interruption or mode
  change, and preservation of existing Projects, configuration, selection, and
  other state that the action should not change;
- the complete installer and setup flow checked from a fresh 64-bit Raspberry
  Pi OS Lite image rather than treating Patchbox OS as the target platform;
- the exact Pi 4 development-system state captured as the comparison baseline,
  including OS/kernel, packages, services, boot/audio tuning, JACK, toolchain,
  storage, power/cooling, and relevant hardware configuration;
- the exact Pi 5 image and starting state recorded, with every dependency,
  prompt, restart, configuration decision, failure, retry, and successful
  return to the install/setup path;
- existing managed system optimizations evaluated on the new system and
  applied through their owner only when compatible and useful, never copied
  blindly from Pi 4 boot or service files; and
- focused documentation corrected to match the accepted behavior and platform.

Keep raw state captures, logs, routes, host/network identifiers, serials, and
runtime configuration below ignored `user/`. Promote only cleared, relevant
platform and measurement facts into public documentation.

The platform procedure and comparison fields live in the
[Pi 5 plan](PI5_HEADROOM_PLAN.md). Passing installation does not itself prove
audio hardware, musical quality, or 18×18 full duplex.

## Current focus — complete the intended FT2 workflow

Outcome: FT2 has the complete functionality the owner intends, without the
current short-wired or partial flows and without unrelated future ideas
obstructing that work.

The current base already stores Loop Mix under its owning Pattern and switches
MIDI/loops together. This track must preserve that boundary; the future
[Playlist above Song](FUTURE_IMPROVEMENTS.md#playlist-above-song), companion
mode, a standalone Pattern library, cue routing, time-stretching, extra mixer
strips, and additional buses are not implied by it.

The exact functionality inventory is still owner input. Until it is stated and
captured, this roadmap deliberately does not invent it. For each supplied item:

1. record the intended musician action and visible/audible result;
2. identify how the current partial path differs;
3. preserve the required cursor, lane, column, page, route, mode, and Project
   state, making genuinely exclusive modes replace one another;
4. provide nearby cancellation, failure, and retry behavior without losing
   work; and
5. verify normal, repeated, interrupted, saved/reloaded, and existing-state
   paths that apply.

The first owner-selected software workflow item is bounded FT2 Pattern
Undo/Redo plus one explicit Pattern Snapshot/Recall. Its current-behavior
evidence, hardware boundary, scope, recovery contract, and staged implementation
plan are in [Sequencer workflow priorities](SEQUENCER_WORKFLOW_PRIORITIES.md).
It precedes the later microtiming, probability, generative, and harmony ideas;
it does not authorize Raspberry Pi electronics work or direct CV/Gate support.

Completion means the owner-approved FT2 inventory, its focused
tests and hands-on checks, and matching current documentation. Planned rhythm,
mixer, analysis, or other ideas are not blockers unless the owner adds them to
that inventory.

## Later track — 18×18 full-duplex multichannel audio

Outcome: SHR can play 18 independent output channels while synchronously
recording 18 input channels through one multichannel interface, and that path
is physically proven rather than inferred from synthetic tests.

The native 18-channel Levels overview is already implemented as capture
preparation: all inputs remain visible with fixed RMS/held-peak meters and
missing/fault distinction. It does not complete this track because there
is still no 18-output path or physical 18×18 acceptance evidence.

The evidence boundary requires:

- the 18-output path implemented with exact configured JACK destinations and
  bounded live-audio behavior;
- the existing 18-input recorder integrated without weakening synchronized
  publication, recovery, or source-identity guarantees;
- hardware-independent playback identity, capture identity, failure, retry,
  and combined-load checks completed before borrowing the mixer;
- progressive capture-only, playback-only, and simultaneous 2×2 through 18×18
  checks on the Raspberry Pi 5 and MR18;
- exact bidirectional channel identity, zero required xrun/drop/overflow/fault
  counters, safe disconnect/reconnect and teardown, and a sustained 18×18 soak;
  and
- the borrowed mixer scene and physical safety state restored after testing.

The authoritative physical procedure and result sheets are in the
[MR18 acceptance plan](MR18_TEST_PLAN.md). The track is not marked checked
until that physical evidence passes.
