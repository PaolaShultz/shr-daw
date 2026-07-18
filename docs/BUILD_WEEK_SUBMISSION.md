# OpenAI Build Week submission package

Working date: 2026-07-18. Target deadline: **July 21, 2026 at 5:00 PM
Pacific Time**. The official [Build Week page](https://openai.com/build-week/),
[Devpost overview](https://openai.devpost.com/), and
[rules](https://openai.devpost.com/rules) are the authority if a form field or
requirement changes.

This file is a working public package, not proof that the final human actions
have happened. Boxes that require listening, recording, publishing, or a
private Session ID remain unchecked on purpose.

## Submission identity

**Title:** SHR-DAW: a 40×20 Raspberry Pi music workstation

**One-sentence pitch:** SHR-DAW turns a Raspberry Pi, a tiny terminal, and the
MIDI gear a musician already owns into a focused, mouse-free workstation for
sounds, Patterns, external instruments, loops, musical Ideas, and recording.

**Provisional category:** Apps for Your Life

**Category rationale:** This is a personal creative tool for a concrete daily
life: bedroom musicians and Linux-audio makers who want to make music with
inexpensive or reused hardware, but do not want their session dominated by
fragmented ALSA, JACK, MIDI, controller, and storage setup. It is an app rather
than a developer library, and its value is the coherent musical workflow.

## Problem and audience

A small Linux music rig can be physically affordable yet operationally
difficult. The musician must reconcile MIDI ports, JACK audio routes, program
numbers, controller mappings, files, and several independent instruments before
they can play. Large desktop DAWs can also be a poor fit for a 480×320 display
and a controller-led session.

SHR-DAW is specifically for:

- bedroom musicians assembling a focused workstation from a Raspberry Pi and
  inexpensive or reused MIDI hardware;
- Linux-audio makers who value visible routes and recoverable local data;
- musicians who prefer a compact physical workflow to a large mouse-driven
  desktop; and
- people who can state a musical goal but do not already speak Rust, ALSA,
  JACK, MIDI, or synthesizer terminology.

No adoption, accessibility, commercial, latency, education, or performance
benchmark is claimed without evidence.

## Creator note

I publish this repository as `PaolaShultz`, my gaming name and a nickname I
sometimes use online. I chose it after the empty tombstone used in the
buried-alive sequence in *Kill Bill: Volume 2*—the marker where nobody was
actually buried before that scene. `PaolaShultz` is not a company or another
developer; it is me, the person making the product and musical decisions.

That personal context matters to SHR-DAW. This is my weekend/free-time
instrument project, built around the Raspberry Pi and music hardware I own,
sometimes alongside my main work on the `bee247.hr` portal. Codex accelerated
and guided the work, but it did not invent why I wanted this instrument or make
the final musical choices for me.

The first public commit was the point when I released the initial version and
dedicated it to my uncle, who died while I was releasing it. It was not the
start of the code. I developed the code leading to that release with GPT-5.6
Sol through Codex CLI on the Raspberry Pi.

## Solution and product experience

The product presents an actual DAW workflow in a 40×20 terminal:

1. browse one of three separately installed software instruments;
2. load a sound and move physical controls through pickup, so a knob cannot
   make the sound jump when it has not caught the stored value;
3. record free-timed MIDI Ideas or build four-lane FT2-style Patterns;
4. assign Pattern pages to the active instrument or named external MIDI ports,
   retaining routes while devices are offline;
5. load and edit reusable drum grooves, chain Patterns in an Arrangement, and
   optionally align tempo to a private WAV loop without pretending to
   time-stretch it; and
6. capture a configured JACK stereo input as a 24-bit WAV.

Keyboard operation is always available. A MiniLab 3 can make the same focused
workflow mouse-free, and unknown controllers begin safely unmapped before the
non-audible learn flow observes their controls. Engine ownership, All Notes
Off, exact live/playback note ownership, ambiguous-port refusal, pickup, atomic
private storage, and the public/private data boundary are product behavior—not
presentation-only claims.

## Why the idea is distinctive

- It is a real DAW-shaped workflow designed to remain legible at **40×20**,
  rather than a desktop interface reduced until it happens to fit.
- Physical control is spatial and page-based; the same small controller reaches
  sounds, recording, Patterns, routing, files, drums, loops, and Arrangement.
- Controller and external-instrument knowledge is data, not Rust constants, so
  the workstation can adapt to a musician's existing equipment.
- One compact interface spans software synths, external MIDI, free-timed Ideas,
  grid Patterns, reusable rhythms, private loops, and stereo recording.
- Codex served as a hardware/software/music navigator: it connected musical
  intent to inspectable routes, configuration, safety constraints, sound and
  rhythm data, QA, and documentation—not only code generation.

## Technical architecture

SHR-DAW is a Rust terminal application for Debian-family Linux and Raspberry
Pi. `ratatui` renders the fixed small-screen workflow. ALSA handles MIDI;
separately installed synthv1, Yoshimi, or FluidSynth processes provide one
managed software instrument at a time; JACK carries their audio, WAV-loop
playback, and stereo recording. The FT2-style sequencer schedules MIDI pages
to the current software instrument or exact ALSA output ports.

The development loop also runs on the target: Codex CLI executes directly on
the Raspberry Pi, where source inspection/editing, Cargo compilation, tests,
Clippy, and optimized release builds all happen. This is not a PC development
path followed by cross-compilation or deployment to the Pi; the Pi is both the
instrument and the development/build machine. The creator reports a working
session with two active Codex CLI instances while SHR-DAW's managed synth was
running. That observation shows the actual on-device workflow, but it is not
presented as a formal CPU, latency, or maximum-concurrency benchmark.

The creator reports that all Codex CLI work in this development push used the
literal `--yolo` invocation. The creator supplied the goal and durable
repository guardrails but did very little command-by-command control or
terminal-screen reading, instead reviewing checkpoints and real outcomes.
Codex paused for physical hardware, audible judgment, destructive/system
actions, publishing, and product decisions that required the human. This is
evidence of a deliberately high-autonomy workflow on the creator's own Pi and
checkout, not a recommendation to bypass safeguards in general.

Local Codex CLI metadata also corroborates the model used before the first
public release. A privacy-preserving review found 144 recorded pre-commit turns
for this checkout across 12 session files, from July 12 through roughly 29
seconds before the initial commit. Every recorded turn names `gpt-5.6-sol`,
with no missing or different model label. This supports the creator's account
that GPT-5.6 Sol was used throughout that recorded development work; it is not
presented as platform proof of line-by-line authorship. Raw logs, prompts,
responses, and Session IDs remain private.

The code is deliberately conservative around live use:

- an engine process is stopped only after its recorded identity is verified;
- route, page, target, mute, stop, panic, and shutdown transitions release
  exactly the notes SHR-DAW owns;
- mapped synthv1 CC messages are blocked until a physical control catches the
  loaded value;
- command-pad note-on and note-off are consumed while musical MIDI passes;
- realtime audio callbacks use fixed/bounded structures and do not perform file
  I/O, take mutexes, or allocate; and
- Project, Idea, recording, and imported-loop paths use private directories,
  bounded formats, and atomic publication rules.

The detailed subsystem/test map is in
[`BUILD_WEEK_FEATURE_MATRIX.md`](BUILD_WEEK_FEATURE_MATRIX.md); the complete
audit and open risks are in [`BUILD_WEEK_AUDIT.md`](BUILD_WEEK_AUDIT.md).

## Eligibility: what existed before the challenge

The first public commit is a personal release and dedication marker, not a
claimed start date for the code. For eligibility, SHR-DAW is **not** presented
as a project created wholly during Build Week. The initial `4e779b55` commit is
dated 2026-07-13 16:31:23 BST, and the last pre-opening handoff commit,
`1dad8087` at 16:33:49 BST, is the comparison baseline.

That baseline was already a substantial project named SHSynth. It had a 40×20
terminal, one-owned-engine hosting for synthv1/Yoshimi/FluidSynth, preset
browsing and pickup-safe synthv1 controls, MIDI Ideas, an initial external
Casio tracker, stereo JACK capture, setup scripts, 21 public cleared presets,
and the MiniLab controller workflow.

The creator works on SHR-DAW mainly as a weekend/free-time side project,
sometimes in parallel with the primary `bee247.hr` portal project. This is
context about the human scale of the work, not a claim of contest novelty.

## Dated Build Week extensions

The 31 commits after `1dad8087` currently change 81 files: 18,627 insertions and
2,604 deletions. Those counts are an audit aid, not evidence of quality. The
feature diffs and working demonstration are the evidence.

| Date | Commit evidence | Meaningful extension during Build Week |
|---|---|---|
| July 14 | `21e4fcc`, `baf4842` | Reframed the appliance as SHR-DAW; expanded FT2 editing and introduced configurable Pattern pages |
| July 14 | `2924535`, `d5cf25d` | Added real-time FT2 recording, external-device profiles, controller auto-detection, and non-audible MIDI learn |
| July 14 | `4f66ded`, `0d99f8e` | Added Play/Rec/Edit/N00B modes, private WAV-loop playback, and local web help |
| July 16 | `f53731d`, `ac8ce70` | Widened Pattern/Arrangement architecture and matched tracker tempo to analyzed loop beats without time-stretching |
| July 16 | `f1148f2` | Added per-column program/channel routing and hardened Project storage/route ownership |
| July 16 | `8df0391` and related image commits | Built deterministic 40×20 presentation screenshots and mobile-sized documentation assets |
| July 18 | `8b64d16` | Added reusable drum-pattern workflow and 72 authored grooves |
| July 18 audit work | current working tree | Fixed stable engine-identity recording, repeated-note recording ownership, ambiguous MIDI selection, symlink recovery, bounded loop/Idea reads, drum visibility/duplication, and prepared judge/submission documentation |

The exact inventory, changed-file ledger, and unresolved human checks are in
[`BUILD_WEEK_AUDIT.md`](BUILD_WEEK_AUDIT.md).

## How GPT-5.6 through Codex contributed

Codex was used as a persistent collaborator across engineering and music
workflow. It:

- inspected repository evidence and machine-visible ALSA/JACK/MIDI state before
  proposing routes;
- translated one-at-a-time physical controller observations into configuration
  and a reusable, non-audible learning flow;
- implemented and reviewed Rust behavior for engine ownership, MIDI cleanup,
  pickup, Pattern recording/routing, loops, recording, storage, and the compact
  interface;
- proposed original synthv1 parameter sets and editable rhythm data, then
  statically validated their schema, ranges, distinctness, mapping, and safety
  hypotheses;
- found edge cases through adversarial audit, including repeated-note ownership,
  ambiguous identical output names, symlink recovery, and unbounded WAV/Idea
  reads;
- separated private or uncleared data from public assets and retained source
  notes for external device information; and
- organized the setup path, feature/quirk matrix, judge path, truthful timeline,
  demo plan, script, and submission copy.

This collaboration did not depend on a remote build machine: Codex CLI and the
complete Rust validation/release cycle ran on the same Raspberry Pi that hosts
SHR-DAW.

It was also unusually hands-off. With Codex CLI invoked as `--yolo`, the creator
did not read or approve most individual terminal actions. Durable repository
rules carried the recurring safety constraints, while the creator reviewed
checkpoints and remained responsible for scope, physical actions, listening,
musical taste, and public release.

The video must say **GPT-5.6** and **Codex** aloud; an on-screen text card alone
does not satisfy the spoken-explanation requirement.

## Human decisions and validation

The human creator supplied the musical goal, product scope, available hardware,
physical connections, and final taste. The human decides what sounds and
rhythms are musically good, performs and records the original song, chooses the
final mix, verifies the actual hardware routes, narrates the demo, and controls
all public publishing and submission actions.

Codex did not physically connect a cable and has not heard the presets, grooves,
or final song. Static XML/MIDI analysis is not described as listening. The
public preset and drum scorecards therefore leave final keep/revise decisions
to a documented low-gain human pass:

- [`PRESET_AUDIT.md`](PRESET_AUDIT.md)
- [`DRUM_PATTERN_AUDIT.md`](DRUM_PATTERN_AUDIT.md)

## Setup and testing for judges

Supported build family: Debian/Raspberry Pi OS/Patchbox Linux. Real audio and
MIDI were designed around Raspberry Pi hardware; other operating systems are
not claimed.

Install on a compatible system:

```sh
./scripts/install.sh
shr-setup
shr doctor
shr
```

A judge without JACK, a controller, or the original hardware can still inspect
real parsing, routing, storage, and rendering code:

```sh
cargo test --locked
SHSYNTH_STATE_DIR=/tmp/shr-daw-judge-state cargo run --locked -- config init
SHSYNTH_STATE_DIR=/tmp/shr-daw-judge-state cargo run --locked -- list
SHSYNTH_STATE_DIR=/tmp/shr-daw-judge-state cargo run --locked -- screenshots > /tmp/shr-daw-screens.json
```

`cargo run --locked -- menu` opens the real browser/tracker in a terminal at
least 40×20 cells. The browser, Help, Project/Pattern editing, drum library, and
external-MIDI tracker remain inspectable without JACK. Loading a software
instrument, playing a WAV loop, and stereo recording require an already-running
JACK server; an external instrument requires its configured MIDI output.
Missing hardware appears as a status/error condition rather than being faked.

Delete `/tmp/shr-daw-judge-state` and `/tmp/shr-daw-screens.json` after the
non-audible walkthrough. Full instructions are in the
[`README`](../README.md), [`INSTALLATION.md`](INSTALLATION.md), and
[`FIRST_RUN.md`](FIRST_RUN.md).

## Known limitations

- Only one SHR-DAW-managed software engine is active at a time.
- Pattern pages have four monophonic event lanes; chords occupy several lanes.
- WAV loops are tempo references and players, not time-stretched audio tracks.
- The loop decoder accepts PCM WAV and caps decoded material at 6,000,000
  frames; its sample rate must match JACK.
- Stereo capture has no software input monitor; use an interface's direct
  monitoring or an external mixer.
- MIDI output names can be ambiguous. SHR-DAW refuses multiple matches rather
  than guessing; a uniquely identifiable port or future stable alias is needed.
- The bundled drum labels describe original grooves inspired by broad rhythmic
  conventions, not authoritative transcriptions of a culture or recording.
- Real driver disconnect, xrun, disk-full, Pi performance, and final musical
  quality still require hardware/human validation.
- No WAV loop content is bundled or currently present; absent input is not a
  passed loop-content audit.
- Sampling, effects, native synthesis, time-stretching, multitrack audio, and
  software input monitoring are intentionally deferred.

## Licensing and content statement

Project code, newly authored cleared synthv1 presets, and the bundled original
rhythm data are published under MIT. The public preset allowlist contains 21
files. The ignored `user/` tree contains private local data, including 424
presets whose redistribution provenance is not established; none is tracked or
submission content. No WAV loop is bundled. Separately installed engines,
SoundFonts, hardware patch names, and optional user content retain their own
licences. See [`THIRD_PARTY.md`](../THIRD_PARTY.md).

## Original demo Project plan

This plan avoids WAVs and uncleared presets. It uses one public synthv1 sound
and the configured external Casiotone percussion route already represented by
the repository's proof-of-concept configuration. Because the sounds have not
yet had their final listening pass, the route sheet is **provisional until the
human checks it at low gain**.

### Recording sheet

| Part | Pattern page / route | Sound or program | Notes |
|---|---|---|---|
| Bass, harmony, lead | `ActiveInstrument`, MIDI channel 1 | public synthv1 `Velvet Tines` | One managed sound across the song; separate lane ranges make the parts readable |
| Drums | `ConfiguredExternal` → exact AudioBox USB 96 MIDI output, channel 2, bank select off | configured program 9; repository sparse drum map | Must be verified on the connected Casiotone before recording; do not assume General MIDI |
| Fallback drums | `ActiveInstrument` FluidSynth route, channel 10 | installed SoundFont drum program | Use only after verifying the installed SoundFont and program; do not change routes during a take |

### Optional Monday hardware enhancement

The creator would like independent MIDI outputs for the Casiotone MT-240 and a
Roland D-50. One required MIDI cable will not be available before Monday. This
is an optional demonstration improvement, not a dependency for the song or
submission.

The software path is ready non-audibly: independent Pattern pages schedule to
independent exact MIDI outputs/channels, and the D-50 profile and ambiguity
tests pass. The hardware route is **not yet validated**. At the latest read-only
inspection, no safe second interface route existed, and the private
configuration still contained only the AudioBox/Casiotone default.

A second interface may enter the demo only if it is already known-safe on this
Pi and exposes a distinct MIDI port without risky system or JACK changes near
the deadline.

After both safe interfaces are connected, record their exact ALSA output names
before attaching the synth cables. If the names are distinct, keep the
Casiotone on one exact page target and make the D-50 interface the configured
route with `external_midi.profile=roland-d-50` when its port name is generic.
If both interfaces expose an identical name, SHR-DAW will correctly refuse the
ambiguous route; do not weaken that rule or risk the demo on an unproven alias
change. Verify one low-gain instrument and its note-off/panic path at a time,
then verify simultaneous playback and the audio return/mix. Internal D-50
memory is writable, so the bundled factory names are audition suggestions, not
proof of the patches currently stored in the unit.

Use the D-50 only if the exact MIDI route, receive channel, patch selection,
single-path note behavior, audio return, and original Project take all pass on
Monday. Otherwise retain the one-Casiotone plan above or the software fallback.

If `Velvet Tines` cannot cover the bass cleanly, audition `Compact Bass` and
`Deep Sub`; do not call either selected until the creator hears it. If the
Casiotone program or note map differs, write the actual channel/program/map in
the Project and replace this row before recording. This is a required route
check, not permission to guess.

### Musical outline

- **Tempo/meter:** 112 BPM, 4/4.
- **Length:** 32 bars, approximately 69 seconds before a short final decay.
- **Original motif:** a compact two-bar melody, answered by a simpler two-bar
  phrase. Write it in the Project rather than borrowing a recognizable song.
- **Harmony:** a four-chord eight-bar cycle chosen at the controller after the
  preset pass. For a simple safe starting point, use only white keys beginning
  on A: A minor, F, C, G. Treat this as a suggestion, not a originality claim.
- **Bass:** roots plus one connecting note near the end of every second bar;
  leave space around the main kick.
- **Drums:** begin from `Syncopated Verse`; compare `Dry Funk` in the listening
  pass and retain only the groove that works with the bass.

| Bars | Pattern purpose | Musical change |
|---:|---|---|
| 1–4 | `INTRO` | hats/side-stick, two-bar motif, no full bass |
| 5–12 | `GROOVE-A` | selected drums, root bass, sparse chord answers |
| 13–20 | `GROOVE-B` | bass connection notes, fuller hats, lead answer |
| 21–28 | `LIFT` | highest register, one restrained fill at bar 28, no new subsystem |
| 29–32 | `OUTRO` | remove kick, return motif, stop cleanly and leave a short decay |

The aim is a clear original demonstration, not stylistic complexity. Keep
channels and programs stable, avoid a dense bass/kick collision, and use the
Arrangement to make the development visible.

### Required listening lock before recording

1. Set interface/headphone gain low and confirm the direct-monitor path.
2. Audition `Velvet Tines` at low/middle/high notes and several velocities.
3. Compare `Syncopated Verse` with `Dry Funk`; choose one and record why.
4. Verify the Casiotone's real channel, program 9 behavior, and every mapped
   percussion note. Replace the route sheet if the device differs.
5. Check that bass and kick remain distinct and that the final release is not
   too long.
6. Mark the selected preset/groove `keep` in their audit scorecards.

## Recording checklist

- [ ] Use only the public allowlisted preset, original Project MIDI, selected
  bundled original groove, and verified external factory sound.
- [ ] Reconnect the AudioBox and verify `shr doctor`; start/restart JACK only as
  a deliberate human action, never as an audit side effect.
- [ ] Optional dual route: after the missing cable arrives, record both exact
  ALSA MIDI names and confirm they are distinct before assigning Casiotone and
  D-50 pages; fall back immediately if names or audio returns are unreliable.
- [ ] Confirm there is one MIDI path to the external instrument and no doubled
  direct-thru route.
- [ ] Begin with low output/headphone gain; use direct monitoring because
  SHR-DAW does not provide software input monitoring.
- [ ] Verify 112 BPM, 4/4, all 32 bars, exact page destinations, channels,
  programs, mutes, and Arrangement order.
- [ ] Send Stop/Panic before and after route changes; confirm no held note.
- [ ] Record one clean stereo take and retain the original Project separately.
- [ ] Check the WAV header/duration/peak non-audibly, then listen for clipping,
  balance, timing, artifacts, and the final decay.
- [ ] Keep private file paths, usernames, terminal history, notifications, and
  Session IDs out of screen capture.
- [ ] Record the interface at exactly 40×20 and capture the physical controls in
  enough light to see the action.

## Hardware-failure fallback

- **External instrument unavailable:** use a verified FluidSynth SoundFont for
  the final Project or play a previously recorded original SHR-DAW master while
  explicitly saying the external route is offline. Do not imply the audio is
  live.
- **Controller unavailable:** operate the same real screens from the documented
  keyboard controls; retain a short existing shot of pickup from the successful
  controller session.
- **JACK unavailable during filming:** show the non-audible live Project edit
  and use the previously verified original stereo take under it, labelled
  “recorded earlier from SHR-DAW.”
- **Camera/display problem:** use the deterministic real TUI capture plus one
  still of the physical setup. Do not replace product behavior with a mockup.

## Public YouTube demo: target 2:50

**Video title:** SHR-DAW — a 40×20 Raspberry Pi music workstation | OpenAI
Build Week

**Description draft:**

> SHR-DAW turns a Raspberry Pi, a 40×20 terminal, and existing MIDI hardware
> into a focused music workstation for sounds, Patterns, Arrangement, external
> instruments, musical Ideas, loops, and stereo recording. This was a
> pre-existing weekend side project substantially extended during OpenAI Build
> Week. GPT-5.6 through Codex helped connect musical goals to MIDI/JACK setup,
> Rust implementation, safety audits, original sound/rhythm design, and the
> submission workflow; the human creator made the product, hardware, musical,
> listening, performance, and publishing decisions. `PaolaShultz` is my gaming
> nickname, not a company or separate contributor. The first public release was
> dedicated to my uncle, who died while I was releasing it. Repository:
> https://github.com/PaolaShultz/shr-daw

Mention that the music is original. Do not include a private Codex Session ID
in the description.

### Shot list and spoken script

| Time | Picture / action | Spoken line |
|---|---|---|
| 0:00–0:12 | Open with the strongest 8–10 seconds of the completed original song; quick physical and TUI cuts | “This whole music workstation is a Raspberry Pi, a 40-by-20 terminal, and MIDI gear I already owned.” |
| 0:12–0:28 | One readable shot of Pi, MiniLab, AudioBox, display, and optional Casiotone; overlay the simple signal path | “SHR-DAW is my weekend side project for making music without a large desktop DAW or a maze of hidden routes.” |
| 0:28–0:48 | Browse the public sound, load it, move a knob before/through pickup; show indicator/status | “It owns one software instrument at a time. Pickup blocks a mapped knob until it catches the saved value, so loading a sound cannot make it jump.” |
| 0:48–1:09 | Open Drum Patterns, load the chosen groove, change one visible hit/velocity | “I can start from one of 72 original editable grooves, then make it mine instead of playing back a sealed loop.” |
| 1:09–1:31 | Record a short melodic phrase into FT2 REC, stop, correct one cell | “Live notes become real Pattern data. I can correct the note, velocity, gate, or program with the keyboard or the same controller.” |
| 1:31–1:50 | Show Pages: active software sound plus exact external route; unplugged/offline state only if safe and already captured | “Each four-lane page remembers its destination, channels, and programs. Offline devices stay in the Project, and ambiguous port names are refused rather than guessed.” |
| 1:50–2:08 | Open Arrangement and place/show the five song sections | “Arrangement turns those Patterns into one compact original song, still readable on this tiny screen.” |
| 2:08–2:30 | Play the best 18–20 seconds; show performance and then recorder result | “This audio is the original Project recorded from SHR-DAW.” |
| 2:30–2:46 | Fast cuts: controller learn, on-Pi code/build result, audit/scorecard | “I ran GPT-5.6 through Codex CLI in yolo mode directly on this Pi—not a PC—with very little screen-watching. It handled coding, builds, MIDI and JACK, sounds, rhythms, QA, and documentation—even two active sessions alongside the synth.” |
| 2:46–2:58 | Before/after card: first release/dedication → dated SHR-DAW extensions; end on title/repository | “My first public commit released this existing project and dedicated it to my uncle. During Build Week I turned it into SHR-DAW, making the final product, musical, listening, and performance decisions.” |

Keep the exported file safely below 3:00; 2:58 leaves only two seconds of
margin, so aim for 2:50 after the first edit. The upload must be **Public on
YouTube**, contain audio, and use the spoken Codex/GPT-5.6 explanation.

## Emergency 60-second edit

| Time | Keep |
|---|---|
| 0:00–0:08 | Original-song hook plus physical setup |
| 0:08–0:18 | 40×20 sound load and pickup catch |
| 0:18–0:29 | Load/edit drum groove and record one melodic row |
| 0:29–0:39 | Pages/routes and Arrangement |
| 0:39–0:50 | Strongest song section |
| 0:50–1:00 | Spoken: “I used GPT-5.6 through Codex to connect MIDI/JACK hardware setup, Rust safety, sound and rhythm design, QA, and documentation; this pre-existing weekend project was substantially extended during Build Week, while I made the final product, musical, and performance decisions.” |

## Devpost description draft

SHR-DAW turns a Raspberry Pi, a 40×20 terminal, and whatever MIDI gear a
musician already owns into a focused music workstation. It plays one managed
synthv1, Yoshimi, or FluidSynth instrument; maps physical controls with safe
pickup; records free-timed MIDI Ideas; builds four-lane FT2-style Patterns;
routes Pattern pages to software or external instruments; arranges a complete
Project; loads editable original drum grooves; plays private WAV loops; and
captures a JACK stereo mix.

The audience is bedroom musicians and Linux-audio makers who want a physical,
distraction-limited workstation but are intimidated by the gap between a
musical goal and ALSA/JACK/MIDI configuration. The interface stays usable at
40×20 and from a keyboard, while a small controller can reach the whole
workflow without a mouse. Missing devices remain visible and recoverable rather
than corrupting a Project.

Live-audio safety shaped the implementation. SHR-DAW never layers managed
engines or terminates a synth process it cannot prove it owns. It sends cleanup
for its exact live and sequenced note owners, consumes command-pad releases,
blocks mapped CC until pickup catches the loaded sound, refuses ambiguous MIDI
ports, keeps realtime callbacks bounded, and publishes private Project/Idea
data atomically. Public sounds and rhythms are allowlisted; the uncleared local
preset bank and all user data remain outside the repository.

This was a pre-existing weekend/free-time side project, not a from-scratch
Build Week claim. The first public commit marked the initial release and a
dedication to my uncle, who died while I was releasing it; it was not the start
of the code. Dated July 14–18 commits substantially expanded it into SHR-DAW
with configurable Pattern pages, controller auto-detection and non-audible
learn, real-time FT2 recording and modes, external-device profiles, WAV-loop
playback and alignment, wider Pattern/Arrangement architecture, per-column
routing, hardened Project storage, controller navigation, web help,
presentation assets, and 72 authored grooves.

One human detail behind the repository name: `PaolaShultz` is my gaming and
online nickname, inspired by the empty tombstone in the buried-alive sequence
of *Kill Bill: Volume 2*. It is not a company or an additional developer. This
is the same personal weekend project I built around my own Raspberry Pi and
music hardware while also working primarily on the `bee247.hr` portal.

GPT-5.6 through Codex worked as an engineering and music-workflow collaborator.
It inspected MIDI/JACK state, translated physical observations into portable
configuration, implemented and adversarially audited Rust, proposed original
synth parameters and rhythm data, enforced licensing/private-data boundaries,
and organized judge and submission paths. The human creator supplied the
product goals and hardware, performed physical actions, chose the scope, and is
the final authority for musical listening, performance, recording, and public
release. Codex CLI, source work, and the complete Cargo build/test/release cycle
ran directly on the Raspberry Pi, not on a desktop PC followed by cross-compile
or deployment. The Pi was both the instrument and its development/build
machine. The creator also observed two active Codex CLI instances working
alongside SHR-DAW's managed synth; this is reported as a real session, not a
performance benchmark.

For the initial release specifically, local Codex CLI metadata records 144
pre-commit turns for this checkout across 12 private session files, and every
one names `gpt-5.6-sol`. That corroborates my account that I used GPT-5.6 Sol
throughout the recorded work leading to the first commit. Raw prompts,
responses, logs, and Session IDs are not public submission content.

All Codex CLI work in this development push used my literal `--yolo`
invocation. I provided the goals and persistent repository guardrails but did
very little command-by-command supervision or terminal-screen reading,
reviewing checkpoints and working outcomes instead. I still controlled
physical actions, listening, musical/product decisions, destructive system
changes, and publishing. This is a description of my deliberate high-autonomy
workflow, not a recommendation to bypass safeguards.

Judges without the same Raspberry Pi hardware can open the real TUI without
JACK and run locked tests, preset listing, private `/tmp` setup, and deterministic
screenshot rendering. Real software-instrument audio, loops, and stereo capture
require JACK; external audition requires the configured MIDI device. These
requirements and limitations are explicit in the repository.

## Final submission checklist

### Musical and product lock

- [ ] Complete the low-gain 21-preset listening pass and mark decisions.
- [ ] Compare shortlisted drum grooves in the actual song and record the human
  selection.
- [ ] Verify/fix the exact Casiotone route sheet; no assumed program or drum map.
- [ ] Save the 32-bar original Project and one verified stereo master.
- [ ] Confirm no uncleared preset, unlicensed loop, copied melody, private path,
  or unsupported performance claim appears.

### Repository and QA

- [x] Required Rust format/test/clippy/release build all pass with Rust 1.85.
- [x] ShellCheck, XML, JSON, drum, Markdown-link, image, package, private-path,
  and `git diff --check` validation pass.
- [ ] Public repository contains the exact reviewed commit and MIT licence.
- [ ] README clone/setup/sample/evaluation/testing path works from a clean
  checkout; expected no-hardware behavior is visible.
- [x] Current deterministic screenshots render at 40×20 and contain no private
  data; replace only if the final human song needs a new hero shot.

### Video and Devpost

- [ ] Record the real physical setup, pickup, drum edit, Pattern record/edit,
  Pages, Arrangement, and resulting original song.
- [ ] Say **Codex** and **GPT-5.6** aloud and explain contributions beyond code.
- [ ] State the pre-existing-project truth; do not imply a from-scratch entry.
- [ ] Export below 3:00, watch the entire final file, confirm intelligible voice
  and music, then upload **Public** to YouTube.
- [ ] Confirm the repository URL and paste the public YouTube video URL.
- [ ] Select Apps for Your Life, paste/review the description, complete every
  required field, and submit before July 21 at 5:00 PM Pacific.
- [ ] Preserve the core project thread, run `/feedback`, and store the returned
  Session ID in the private Devpost field or private submission notes. **Never
  commit the private ID unless the form explicitly requires public disclosure.**
- [ ] Save private confirmation screenshots/receipts after submission.
