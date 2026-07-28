# Workspace handoff

This file contains only current machine state and decisions that must survive a
new thread in `$HOME/p/shsynth`. Durable repository policy is in
`AGENTS.md`; detailed helper behavior is in `docs/MAINTAINER_HELPERS.md`. Never
record credentials, GitHub device codes, or private file contents here.

## Current priority and shared checkout

The Build Week snapshot is preserved by its tag; the repository itself is
unfrozen and ordinary development continues on `main`. Do not keep or recreate
a standing `dev` branch before the owner opens the planned 0.6 milestone. The
temporary combined build-and-test gate in `AGENTS.md` still applies; an
unfrozen repository is not implicit permission to compile.

The current ordered targets are owned by `docs/RELEASE_ROADMAP.md`: 0.4 checks
all existing menus/workflows plus clean install/setup on Raspberry Pi OS Lite;
0.5 completes the owner-specified FT2 behavior without pulling random future
features into scope; 0.6 implements and physically accepts simultaneous
18-channel playback and 18-channel recording. Package version `0.3.92` is the
corrected starting point; the current checked-progress version is `0.4.2`.

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

Plain `shr` resolves to this checkout's `scripts/local.sh` through both
`/home/patch/.bash_aliases` and `/home/patch/.local/bin/shr`. The launcher uses
`target/debug/shr` unless `SHSYNTH_BIN` is explicitly set; the debug TUI shows
`DEV`. Do not restore the obsolete release-binary alias.

## Active DSP/JACK continuation (2026-07-22)

The current DSP closure pass must be continued, not recreated. It
adds validated FFT/alias analyzers; centered four-point Lagrange interpolation
for delay, chorus, and flanger; first-order ADAA on the filter cubic pre-drive;
short reverb input all-pass diffusion; comprehensive nonlinear/interpolation/
reverb tests; and private level-matched audition renders. Distortion retains
first-order ADAA after multi-bin characterization. The implementation and
focused provenance are in `src/dsp/`, `src/effects/`, `src/effect_schema.rs`,
`src/main.rs`, `docs/AUDIO_GRAPH.md`, and `docs/CONFIGURATION.md`. Do not edit
the roadmap or historical Phase 2/3/4 measurements for this work.

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
`loops/cleared-loops.txt`, and `demos/cleared-demos.json`.

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

- The active development system is a Raspberry Pi 4 with 4 GB RAM and microSD.
  A Pi 5 with 2 GB RAM, active cooler, 27 W supply, bottom NVMe base, and
  128 GB NVMe was ordered but is not installed or measured. The planned
  480×320 display occupies the top GPIO position rather than HDMI, so the design
  cannot use a top-mounted M.2 HAT; its housing will be self-designed and
  printed around the measured stack. Keep Pi 4 evidence labelled accurately;
  migration and Pi 5 claims remain deferred to `docs/PI5_HEADROOM_PLAN.md`
  after a clean checkpoint.
- Local configuration selects the MiniLab 3 controller, JACK
  `system:playback_1`/`system:playback_2`, AudioBox USB 96 stereo capture on
  `system:capture_1`/`system:capture_2`, and the AudioBox MIDI port as external
  output. These are private configuration values, not portable defaults.
- The reviewed controller profile is `arturia-minilab-3`; controller and
  performance MIDI roles are separate. Its configured eight-pad layout uses
  four page pads plus semantic positions 5–8: STOP/PANIC, PLAY/LOAD/PREVIEW,
  REC/capture, and TAP. MiniLab notes 40–43 use the canonical
  `stop`/`play`/`rec`/`tap-tempo` roles; legacy item-role configuration remains
  readable. The master rotary browses content and its press selects/confirms.
  The Routing screen reports live visibility, not merely remembered
  configuration.
- The optional audio profile reserves CPU 3. Boot isolation is active; the
  performance-governor service and JACK affinity drop-in are installed. Inspect
  with `shr-audio-tune status`; removal requires the helper's managed removal,
  clearing `audio.engine_cpu`, and reboot. Never edit around its ownership
  records in `/var/lib/shr-audio-tune/`. This is deliberate real-time isolation
  for demanding simultaneous playback/recording, not a dormant JACK core that
  ordinary builds reclaim when JACK stops. General builds use CPUs 0–2 until
  removal and reboot; final Rust linking is largely serial and remains the
  longest build stage. The unsupported `nohz_full=3` and `rcu_nocbs=3` tokens
  were removed from the persistent boot command line on 2026-07-26 but remain
  in the live command line until a safe reboot; `shr-audio-tune doctor`
  correctly reports that reboot requirement. The supported isolation,
  governor, IRQ-affinity, and JACK-affinity state is ready.
- The per-user `fluidsynth.service` and system `amidiminder.service` are masked
  and stopped. `/usr/bin/fluidsynth` and the TimGM bank remain for SHR-owned
  on-demand use. Setup and tuning do not start or restart JACK.
- All project equipment is borrowed. Preserve its configuration and require
  explicit approval before any JACK, synth, MIDI, recording, audible, or other
  physical-hardware test.

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
Back behavior; ROUTE draft cancellation without Project mutation; the Loop
Library's explicit PLAY preview, stop/rollback, and return behavior; and every entered screen,
including an MTR FX caller return, starting on controller-menu page 1. Record
observed failures before changing behavior.

A later user-authorized musical/hardware pass should exercise the
standalone/FT2 synth ownership split, N00B versus Play/REC/Edit, independent
Edit length/ADD values, routing-default confirmation, and percussion smart
column reuse. Do not start that pass merely because the overlay review is
complete. Detailed UI contracts live in `docs/CONTROLLER_INTERFACE.md`,
`docs/TRACKER.md`, and the focused routing/effects documents linked from
`docs/README.md`.

## Terminal project-note layer

This machine is operated through a TTY. Its terminal-only `zk` notebook is
rooted at `/home/patch/p`, covering `shsynth`, `shr-skills`, and later project
directories below that root without copying or moving their Markdown. The
entry point is `/home/patch/p/Project Hub.md`. Configuration, the rebuildable
SQLite index, and its template are below `/home/patch/p/.zk/`, outside both Git
repositories.

Use:

```sh
pnotes
pnotes search words
```

The first command opens all indexed notes in an `fzf` picker; the second
full-text filters before opening the picker. The wrapper is
`~/.local/bin/pnotes`. Direct `zk` commands must use
`--notebook-dir=/home/patch/p --working-dir=/home/patch/p` when invoked from
elsewhere.

The index excludes `.git`, `target`, `user`, `node_modules`, and `.zk`
subtrees; keep `shsynth/user/` excluded. The notebook uses normal Markdown
links, `nano` for editing, `less` for paging, and `sed` for previews. Installed
components are ARM64 `zk` 0.15.5 at `~/.local/opt/zk-0.15.5/` and Debian `fzf`
0.38.0. There is no resident process. Plan about 25 MiB disk, 31 MiB RAM while
actively searching, and zero RAM while idle, suitable for the planned 2 GB Pi
5.

## Installed tools and current validation boundary

Rust 1.85, `gh`, `xmllint` (`libxml2-utils`), `shellcheck`, `zk`, and `fzf` are
installed.
Use the scoped validation policy in `AGENTS.md`; historical full
suites, release builds, benchmarks, and screenshot batches are evidence in
their dated documents, not instructions to repeat them. No current physical or
audible acceptance should be inferred from synthetic or hardware-independent
checks.
