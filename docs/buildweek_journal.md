# Build Week development journal

Keep entries short. State what is confirmed, and do not turn roadmap ideas into
completed work.

## 2026-07-13 — Initial workstation, sounds, and first release

- Subsystems: engine hosting, MIDI/JACK setup, controller, presets, ideas,
  tracker, and recorder.
- Completed: inspected ports and configuration, guided physical MIDI/audio
  routing, mapped the MiniLab controls, and established loadable inputs,
  outputs, presets, and devices alongside the initial Rust release.
- Decisions/challenges: hardware names stayed in configuration; pickup, All
  Notes Off, owned-process shutdown, and private preset boundaries protected
  live use and public release.
- Sound design: authored Velvet Tines, then 20 more original synthv1 parameter
  designs; XML/schema validation passed, while listening remains a human task.
- Human context: the first public commit was the release the creator dedicated
  to their uncle, who died while the software was being released. It marks a
  personal release moment, not the beginning of coding.
- Model evidence: a privacy-preserving local metadata review found 144 recorded
  pre-commit turns in this checkout across 12 Codex CLI session files; every
  turn names `gpt-5.6-sol`. Raw logs and Session IDs remain private.
- Timeline: this is prior project work, not a Build Week extension; exact
  eligibility timestamps are retained in the audit rather than made the human
  story.
- Milestone: first public SHSynth release with 21 cleared presets, dedicated to
  the creator's uncle.

## 2026-07-14 — Setup automation and expanded DAW workflow

- Subsystems: installer, controller learn, tracker routing, device profiles,
  live recording, loops, and help.
- Completed: turned the observed input/output setup into a reusable wizard,
  added non-audible MIDI learning, structured external-device program data,
  and documented controller-to-engine and sound-card-to-instrument wiring.
- Decisions/challenges: unknown controllers inherit no commands; exact routes
  remain user data; command MIDI is consumed while musical MIDI passes safely.
- Milestone: reframed as SHR-DAW with configurable pages, FT2 modes, private WAV
  loops, and a device-neutral setup path.

## 2026-07-16 — Tracker, storage, and presentation polish

- Subsystems: Pattern architecture, routing, loop alignment, project storage,
  screenshots, and documentation.
- Completed: widened Patterns and Arrangement, matched tempo to analyzed loop
  beats, hardened file/routing behavior, and produced consistent 40x20 visuals.
- Decisions/challenges: offline devices retain exact routes; destructive file
  actions avoid referenced or colliding data; loop tempo changes do not imply
  time-stretching.
- Milestone: largest refactor to date, with the core demo workflow represented
  in README screenshots.

## 2026-07-18 — Rhythm library and Build Week record

- Subsystems: rhythm editing, reusable drum patterns, planning, and development
  documentation.
- Completed: authored 72 MIDI drum grooves across ten genres and two meters,
  added save/load/filter workflow, and documented Codex's non-code navigation,
  hardware setup, routing, preset/device work, rhythm design, and sound design.
- Decisions/challenges: bundled grooves are read-only and user saves remain
  private; conventional rhythm ideas were translated into editable MIDI data,
  but musical taste still requires user listening and curation.
- Milestone: established reproducible Build Week metrics and an ongoing journal
  without activating the sampling, DSP, or native-synth roadmap.

## 2026-07-18 — Safety, content, and submission audit

- Subsystems: all 22 Rust modules, installation/configuration, presets, drum
  data, private/public storage, judge path, and Build Week presentation.
- Repository evidence: verified the pre-opening `1dad8087` baseline, 31 later
  commits, 21 public/424 private presets, 72 visible drums, zero WAV input, and
  zero tracked `user/` paths. Audited every source module and important asset
  class rather than using file counts as a quality claim.
- Codex-assisted fixes: preserved repeated-note REC ownership, refused
  ambiguous MIDI outputs, stabilized exact process-identity recording/retry,
  blocked recording-recovery symlinks, bounded WAV-loop and Idea reads, synced
  Idea publication, removed a drum display collision and exact duplicate, and
  added regression tests.
- Content/documentation: reproduced all 21 public presets from the generator,
  validated 145 current synthv1 parameters per file, statically scored every
  sound and groove, reconciled stale help/routing text, and prepared the audit,
  feature matrix, judge path, original-song outline, video script, fallback,
  Devpost draft, and final checklist.
- Validation: Rust format, 251 tests, warning-denied Clippy, release build,
  ShellCheck, JSON/XML/generator, staged install/uninstall, local/external links,
  images, private-path checks, and a release non-audible smoke all passed.
- Target-native workflow: Codex CLI ran on the Raspberry Pi and invoked the
  source work and complete Cargo compile/test/Clippy/release cycle there. This
  was not desktop-PC development followed by cross-compilation or deployment.
  Separately, the creator reports a working session with two active Codex CLI
  instances while SHR-DAW's managed synth was running; this is qualitative user
  evidence, not a benchmark reconstructed from repository data.
- Autonomy: the creator reports that all Codex CLI work in this development
  push used the literal `--yolo` invocation, with little command-by-command
  control or terminal-screen reading. Durable repository rules supplied
  recurring safety constraints; human checkpoints remained mandatory for
  hardware, listening, destructive or system actions, product/music judgment,
  and publishing.
- Human/hardware boundary: no JACK start/restart, MIDI transmission, audible
  test, physical action, or public upload was performed. The creator must make
  the low-gain musical selections, verify the external route, perform/record the
  song, narrate/upload the video, and submit.
- Main challenge: maximizing a truthful submission while retaining the fact
  that a substantial side project existed before the contest opened.
- Milestone: no known P0 code/data/licensing blocker remains; human listening,
  original-song evidence, public video, `/feedback`, and Devpost completion are
  now the critical path.

## 2026-07-18 — Human story and pre-release model provenance

- Documentation: moved the contest-opening timestamp out of the main human
  story while retaining the exact eligibility evidence in the audit.
- Human context: recorded that the first public release was dedicated to the
  creator's uncle, and that SHR-DAW is a personal weekend project rather than
  the creator's primary `bee247.hr` work.
- Local evidence: aggregated private Codex CLI metadata without reading prompts
  into public documentation. All 144 recorded pre-commit turns for this
  checkout across 12 session files name `gpt-5.6-sol`; the final one is about
  29 seconds before the first commit.
- Privacy/claim boundary: no raw log, prompt, response, local path, or Session
  ID was published. The metadata corroborates the model used in recorded work;
  it is not described as platform proof of authorship for every line.
- Milestone: the README, Build Week record, Devpost draft, and video ending now
  tell the personal dedication and target-native Codex story before the
  eligibility detail.

## 2026-07-18 — Two-interface MIDI readiness check

- Proposed hardware: one interface MIDI output to the Casiotone MT-240 and one
  to the Roland D-50; the second MIDI cable is unavailable until Monday.
- Read-only machine evidence: neither hardware interface nor the MiniLab was
  connected during this check. The active private configuration still names
  only the AudioBox/Casiotone default route.
- Proven in code/tests: simultaneous Pattern pages retain independent exact
  MIDI targets/channels; ambiguous duplicate names are refused; D-50
  target-specific Program Change and its bundled profile pass focused tests.
- Important limitation: distinct interface port names are required today. A
  generic D-50 interface name needs an explicit configured-profile association;
  no hardware, MIDI transmission, JACK, or listening result is claimed.
- Decision: treat the D-50 as an optional Monday enhancement and preserve the
  one-Casiotone/software fallback so one missing cable or ambiguous port cannot
  block the video.

## 2026-07-18 — Insert/send effects architecture recovery

- Product direction: restored the creator's idea of routable source/master
  inserts, shared aux sends/returns, external hardware loops, and explicit
  effect-chain order instead of reducing it to a generic deferred effects list.
- Primary-source finding: JACK 2 mixes multiple inbound connections at an input
  port. A stereo effect client can therefore prototype a JACK-summed master
  insert or global wet send without first building a general mixer.
- Remaining design boundary: independent per-source send levels require scaled
  taps/ports; graph transitions need headroom, feedback, doubled-path, click,
  ownership, and client-loss protection.
- Measurement plan: compare release-mode Pi baseline and 1/2/4 instances using
  callback distribution, xruns, CPU/core load, RSS, latency, gain/clipping,
  bypass transitions, sample-rate behavior, and shutdown. No JACK or audible
  measurement was run in this documentation session.
- PC/Pi decision: desktop prototyping can be isolated in a separate
  branch/worktree, but final locked builds and performance evidence must come
  from the Pi and the development split must be described truthfully.
- Added creator requirement: physical capture input should become a first-class
  source that can run with playback and traverse the same inserts, sends,
  master, monitor, and pre/post-effect recording graph. “Free wiring” is defined
  as composable validated routes, with cycles, feedback, doubled monitoring,
  ambiguous ports, and partial graph publication rejected.
