# SHR-DAW documentation

README is the short product overview. This index routes each kind of detail to
one maintained home so setup instructions, implementation contracts,
measurements, history, and future plans do not become repeated walls of text.

For current behavior, begin in **Start and use**, then follow implementation
details into **Install and configure** or **Architecture and safety**.
Measurement pages are dated evidence from particular checkpoints, not a second
current specification. Development records preserve what was true during the
build. Planned-work pages describe proposals and do not override current code,
configuration, or the architecture contracts.

## Start and use

- [First run](FIRST_RUN.md) — configure hardware and open SHR-DAW.
- [Using SHR-DAW](USING_SHR_DAW.md) — musical purpose, the realistic
  idea-to-sketch workflow, non-goals, instruments, effects, recording, and
  commands.
- [SHR-DAW instruments and drums](INSTRUMENTS_AND_DRUMS.md) — one installed
  sound system with melodic instruments, Moj models and saves, Sampler, Drums
  kits, routing, ownership, and recovery.
- [Complete screen and menu manual](MENU_MANUAL.md) — deterministic images
  covering every populated 40×13 screen, contextual editor, visible Levels
  command page, and controller menu page, with explanations.
- [In-app help](HELP.md) — the compact help text shown by `?` or F1.
- [Tracker guide](TRACKER.md) — FT2 editing, pages, routing, Arrangement,
  drums, loops, and Project files.
- [Live performance](LIVE_PERFORMANCE.md) — Live Pattern launch/capture,
  transient lane shaping, and Pattern-owned four-WAV Loop Mix transitions.
- [Public-domain demo songs](DEMO_SONGS.md) — tempos, keys, parts, restyle
  ideas, clearance records, and installed discovery.
- [Controller interface](CONTROLLER_INTERFACE.md) — complete four-page action
  inventory and hardware navigation contract.
- [Physical connections](CONNECTIONS.md) — MIDI and audio wiring examples.

## Install and configure

- [Installation](INSTALLATION.md) — dependencies, install/uninstall boundaries,
  repository-local evaluation, JACK, and optional CPU tuning.
- [Raspberry Pi 5 NVMe installation](PI5_NVME_INSTALL.md) — safe imaging,
  headless customisation, first-boot proof, and boot recovery.
- [Raspberry Pi audio-system optimization](AUDIO_SYSTEM_OPTIMIZATION.md) —
  Patchbox/stock baselines, reviewed settings, ownership, rollback, doctor
  states, and primary-source provenance.
- [Configuration and routing](CONFIGURATION.md) — every runtime setting and
  persisted route.
- [MIDI device profiles](MIDI_DEVICE_PROFILES.md) — external-instrument bank
  and program names.
- [Controller profiles](CONTROLLER_PROFILES.md) — automatic matching and the
  non-audible MIDI learner.
- [Codex-assisted setup](CODEX_ASSISTED_SETUP.md) — optional assistance for
  unusual hardware and recovery.

## Architecture and safety

- [How SHR-DAW works](HOW_IT_WORKS.md) — end-to-end MIDI/audio routing,
  instrument ownership, Ideas, FT2 Projects, loops, recording, the full effect
  palette, graph safety, persistence, and honest current limits.
- [Audio graph and DSP contract](AUDIO_GRAPH.md) — Project effects data, exact
  parameter schemas, real-time limits, routing publication, meters, bypass,
  tails, topology limits, and curation gates.
- [Final stereo performance bus](FINAL_PERFORMANCE_BUS.md) — input-only
  activation, optional-source ownership, fixed mastering boundary, monitoring
  safety, final WAV capture, and hardware acceptance.
- [Synchronized multitrack recording](MULTITRACK_RECORDING.md) — exact JACK
  source mapping, fixed 18-channel RMS/peak overview, shared callback timeline,
  mono stems, manifests, recovery, and the non-audible stress helper.
- [MR18 acceptance plan](MR18_TEST_PLAN.md) — readiness gates and a printable,
  safety-first 18×18 full-duplex 48 kHz hardware procedure.
- [Three-minute multitrack presentation](MULTITRACK_PRESENTATION.md) — truthful
  hardware and synthetic versions with exact on-screen evidence.
- [Third-party software and sounds](../THIRD_PARTY.md) — licences, credits,
  provenance, and redistribution rules.
- [New patches and sounds](NEW_PATCHES.md) — synthv1 schema and authoring
  workflow.
- [SHR Drums](https://github.com/PaolaShultz/shr-drums): public source for the
  in-process drum engine; its
  [package format](https://github.com/PaolaShultz/shr-drums/blob/main/FORMAT.md)
  and
  [sample provenance](https://github.com/PaolaShultz/shr-drums/blob/main/SOURCES.md)
  live with that repository.
- [Moj Sint](https://github.com/PaolaShultz/moj-sint): public managed synth host,
  strict preset schema, and 16 allowlisted factory starts.
- [SHR Sampler](https://github.com/PaolaShultz/shr-sampler): public strict
  sample-package engine and project-authored cleared factory instrument.

## Measurements and audits

- [Phase 1 dry graph](PHASE1_AUDIO_GRAPH_MEASUREMENT.md) — owned-routing and
  bit-exact fallback checkpoint.
- [Phase 2 insert effects](PHASE2_AUDIO_GRAPH_MEASUREMENT.md) — deterministic
  processor evidence and Raspberry Pi measurements.
- [Phase 3/4 effects and buses](PHASE3_4_AUDIO_GRAPH_MEASUREMENT.md) — time and
  modulation effects, reverb, aux/master routing, and consolidated curation.
- [Fixed stereo MASTER STRIP](MASTER_STRIP_MEASUREMENT.md) — exact parameters,
  DSP provenance, true-peak/loudness contract, latency, and repeatable
  hardware-independent timing evidence.
- [Raspberry Pi 4 and Pi 5 performance comparison](PI4_PI5_PERFORMANCE_COMPARISON_2026-07-28.md)
  — dated build, DSP, memory, NVMe, PMU, thermal, and comparison evidence at
  version 0.4.2.
- [Rust compiler A/B on Raspberry Pi 5](RUST_COMPILER_AB_2026-07-29.md) —
  controlled Rust 1.85.0 versus 1.97.1 build and callback results, plus the
  exact adopted toolchain pin.
- [Preset audit](PRESET_AUDIT.md) — cleared public synthv1 bank review.
- [Drum-pattern audit](DRUM_PATTERN_AUDIT.md) — bundled rhythm structure,
  limitations, and listening shortlist.

## Development record

- [Maintainer helper scripts](MAINTAINER_HELPERS.md) — parameters, side
  effects, safety boundaries, and design decisions for every repository helper
  and the related Make targets.
- [Early development sprint record](BUILD_WEEK.md) — chronology, model
  provenance, division of work, target-native workflow, and evaluation; no
  hackathon application or submission was made.
- [Učenje kroz istraživanje](UCENJE_KROZ_ISTRAZIVANJE.md): the creator's
  Croatian essay about learning through musical exploration.
- [Feature and quirk matrix](BUILD_WEEK_FEATURE_MATRIX.md) — subsystem-level
  access, persistence, failure, safety, architecture, and test inventory.
- [Workflow audit and repair handoff](WORKFLOW_AUDIT_HANDOFF.md) — complete
  musician/operator workflow coverage, first repair queue, deferred decisions,
  evidence gaps, and persistent fix tracking.
- Sequencer acceptance: [Pattern History](PATTERN_HISTORY_MUTATION_INVENTORY.md),
  [rhythm workflow](RHYTHM_WORKFLOW_ACCEPTANCE.md),
  [step probability/conditions](STEP_PROBABILITY_CONDITIONS_ACCEPTANCE.md),
  [independent lane playback](LANE_PLAYBACK_ACCEPTANCE.md), and
  [deterministic generative tools](DETERMINISTIC_GENERATIVE_TOOLS_ACCEPTANCE.md),
  [arpeggio, chord, and harmonizer generators](HARMONIC_GENERATORS_ACCEPTANCE.md),
  and [external USB MIDI transport sync](EXTERNAL_TRANSPORT_SYNC_ACCEPTANCE.md) —
  bounded contracts, automated matrices, and honest evidence limits.
- [Workspace handoff](WORKSPACE_HANDOFF.md) — current checkout, local hardware,
  private/public boundary, and validation state for maintainers.

`WORKSPACE_HANDOFF.md` describes one development machine and is not an end-user
setup guide.

## Planned work

- [Experimental product direction](EXPERIMENTAL_DIRECTION.md) — beginner-first
  music making, FT2 ownership, open Moj Sint models, low-code micro-machines,
  and smart help without feature promises.
- [Experimental development roadmap](EXPERIMENTAL_ROADMAP.md) — current work
  order, evidence boundaries, and the rule that keeps unrelated ideas out of
  the current track without treating `0.x` as production releases.
- [Sequencer workflow priorities](SEQUENCER_WORKFLOW_PRIORITIES.md) — current
  hardware-sequencer comparison, USB-only hardware boundary, ranked software
  gaps, and the bounded Pattern Undo/Redo plus Snapshot implementation plan.
- [Future improvements](FUTURE_IMPROVEMENTS.md) — unscheduled smart musical
  assistance, deferred routing and product ideas, and the deliberately
  unreasonable challenges.
- [Future musical sketch helpers](FUTURE_MUSICAL_HELPERS.md) — FT2 SIZE,
  circle-of-fifths notes, arpeggio algorithms, drum fills/rolls, and staged
  Arrangement assistance.
- [Raspberry Pi 5 headroom and footprint plan](PI5_HEADROOM_PLAN.md) —
  completed platform baseline followed by the later dependency/footprint,
  real-time-core, and PRESTO experiments.
- [Legacy mixer and aux proposal](POST_COMPETITION_MIXER_AUX_PLAN.md) —
  multi-strip mixer and shared-aux migration, retained from the early sprint.
- [Legacy rhythm proposal](POST_COMPETITION_RHYTHM_PLAN.md) — arbitrary Pattern
  length, microtiming, swing, groove tools, and optional formal meter.
