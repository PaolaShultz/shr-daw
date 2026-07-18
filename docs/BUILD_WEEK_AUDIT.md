# OpenAI Build Week audit

Audit date: 2026-07-18. Scope: every tracked Rust module and every important
script, configuration, profile, preset, rhythm, package, document, and image
class in this checkout. No audible test, JACK start/restart, MIDI transmission,
public upload, or private-data mutation was performed as part of the code and
submission audit.

## Contest and eligibility truth

The official [OpenAI Build Week page](https://openai.com/build-week/),
[Devpost overview](https://openai.devpost.com/), and
[rules](https://openai.devpost.com/rules) were checked on 2026-07-18. The
submission closes July 21, 2026 at 5:00 PM Pacific Time. A submission needs a
working project, category/description, inspectable repository and README test
path, a public YouTube demo under three minutes with audio, spoken explanation
of Codex and GPT-5.6 use, and the `/feedback` Codex Session ID from the thread
where most core functionality was built. Judges may evaluate only the submitted
text, images, and video.

SHR-DAW is a pre-existing project. Commit `4e779b55` (`Initial SHSynth release`)
is dated 2026-07-13 16:31:23 BST, about 29 minutes before the official 17:00 BST
opening. Commit `1dad8087` at 16:33:49 BST documented the handoff/private-data
boundary and is the honest eligibility baseline. The 31 commits after that
baseline begin on July 14 and change 81 files (18,627 additions and 2,604
deletions). Change volume is context, not a quality claim; dated features and
diffs are the evidence. See `BUILD_WEEK.md` for the before/after account.

The creator describes SHR-DAW as a weekend/free-time side project, sometimes
developed in parallel with the primary `bee247.hr` portal work. That context
explains its personal scale; it does not change the pre-existing-project
eligibility statement.

The creator says the first public commit marked the initial release and a
dedication to their uncle, who died while the software was being released,
rather than the start of coding. A privacy-preserving review of local Raspberry
Pi Codex CLI metadata found 144 pre-commit turns for this checkout across 12
session files, from 2026-07-12 13:23 BST through 2026-07-13 16:30:53 BST.
Every turn records `gpt-5.6-sol`; none has a missing or different model label.
This corroborates the model used in the recorded work but is not line-by-line
authorship proof; no raw logs, prompts, responses, or private Session IDs are
published.

## Baseline verified

| Item | Evidence at this audit |
|---|---|
| Rust | 24,165 physical lines, 22 modules, 251 `#[test]` functions after this audit |
| Public sounds | 21 manifest-listed MIT-cleared synthv1 presets |
| Private sounds | 424 local presets below ignored `user/`; not read into public content |
| Rhythms | 72 visible bundled entries: 60 catalog plus 12 standalone |
| Engines | synthv1, Yoshimi, FluidSynth as separately installed processes/data |
| Interface / hardware | 40×20 TUI; Raspberry Pi; MiniLab 3; AudioBox USB 96; optional external MIDI |
| WAV input | zero `.wav` files in the repository and private `user/` tree |
| Private tracking | zero tracked paths below `user/` |

Counts are inventory checks, not proof of product quality.

## Ranked work

### P0 — submission blockers

| Item | State |
|---|---|
| Record and publicly upload an original YouTube demo shorter than three minutes, with spoken Codex and GPT-5.6 explanation | Open; requires human hardware, performance, voice, and upload |
| Preserve this project thread and run `/feedback`; keep the returned Session ID private until entered in Devpost | Open; human submission action |
| Perform low-gain preset/drum listening, choose the demo sound/groove, record the original song, and verify the final video audio | Listening required |
| Confirm final Devpost fields, public repository visibility/licence, category, and submission before the deadline | Open; publishing requires human authorization |

No P0 code, ownership, data-loss, or tracked-licensing blocker remains known.

### P1 — completed high-value work

1. Preserve repeated note ownership during real-time tracker recording.
2. Refuse identical/ambiguous MIDI output matches instead of choosing the first.
3. Prevent interrupted-recording recovery from following `.part` symlinks.
4. Bound in-memory WAV loop decoding and remove the temporary double buffer.
5. Bound owned Idea MIDI/metadata reads and fsync complete Idea contents before
   atomic publication.
6. Make all 72 drum entries visible, remove one exact duplicate, and reduce
   overpromising names.
7. Require two stable process-identity observations and retry exact ownership
   verification before treating an engine marker as stale.
8. Give judges a non-audible path and prepare the audit, feature matrix,
   sound/rhythm scorecards, demo plan, video script, and Devpost copy.

### P2 — polish if time remains

- Run the human curation passes and update `keep/revise/drop` decisions.
- Capture the real final Project/video stills after the song is approved.
- Consider bounded pre-read helpers for every low-risk local text/config file;
  Project and drum decoders reject oversized decoded text, but their current
  `read_to_string` call allocates before that logical limit.
- Add JACK server-shutdown and real disconnect integration tests on expendable
  hardware; unit tests cannot reproduce every driver/xrun/disk-full condition.
- Measure Pi CPU/memory with the final song and actual JACK buffer settings.
- Compare source-insert, master-insert, aux-send/return, and external-hardware
  loop topologies plus a simultaneous live-input/output chain; collect a
  release-mode Pi baseline before selecting a DSP implementation. Measure
  full-duplex input-to-output latency and doubled-monitor paths. JACK/audio
  measurement still requires explicit permission.

### Intentionally deferred

- Native synth, sampler architecture, multitrack audio timeline, a complete
  effects suite/mixer, time-stretching, software input monitoring, stable
  aliases for identical USB adapters, and multi-target live-thru. The bounded
  insert/send architecture study remains P2 rather than being discarded.
- Publishing the uncleared preset archive or any loop without documented rights.

These are lower submission value than a stable demo and should not be started
before the deadline.

## Findings ledger

Criteria abbreviations: `T` technological implementation, `D` design/coherent
experience, `I` credible impact, `N` quality/creativity/novelty.

| ID | Severity / criterion | Evidence and consequence | Recommendation | Effort / regression risk | Status |
|---|---|---|---|---|---|
| SUB-01 | Blocker / all | Devpost requires the public sub-three-minute demo and judges may stop there; no final video exists | Record from the prepared script after the song is approved | Human session / medium deadline risk | Open |
| SUB-02 | Blocker / T | Required `/feedback` Session ID is absent; losing this thread could make it hard to retrieve | Preserve thread, run `/feedback` at handoff, paste ID only into Devpost | Minutes / low | Open |
| ELIG-01 | Critical / I,N | `4e779b55` predates opening by ~29 minutes; claiming an all-new project would be false | Use `1dad8087` baseline and dated post-opening feature table | Small / low | Fixed in docs; validate final form |
| LIC-01 | Critical / T,I | `user/` contains 424 private presets and an uncleared archive; redistribution rights are absent | Keep ignored/private; package only 21-file manifest | Existing boundary / high impact if violated | Validated: zero tracked `user/` paths |
| ENG-01 | Important / T,D | Repeated QA exposed an intermittent ownership-test orphan: a one-shot `/proc` identity observation could be transient, causing an audit-owned `sleep` marker to be discarded without signalling its child | Record two consecutive PID/start/executable identities and retry exact verification briefly; never signal on mismatch | Small / low | Fixed; guarded test and two 100-run stress passes, full suite, and release build pass |
| MIDI-01 | Important / T,D | `ui.rs` keyed REC lane ownership by note only; repeated same-note/channel instances overwrote an owner and could free the wrong lane | Key by source channel/note and retain a LIFO lane stack | Small / low | Fixed; repeated-instance regression test passes |
| MIDI-02 | Important / T,D | `sequencer.rs` selected the first configured partial/exact output; two same-name adapters could route the song to the wrong instrument | Prefer one exact match, allow one configured partial, otherwise report ambiguity | Small / low | Fixed; exact/partial/duplicate tests pass |
| WAV-01 | Important / T,D | `audio_recorder.rs` followed a `.wav.part` symlink during recovery and could modify/rename its target | Require regular directory entries and open with `O_NOFOLLOW` | Small / low | Fixed; target-preservation test passes |
| WAV-02 | Important / T | `loop_player.rs` collected an unbounded raw vector then a stereo vector; a large WAV could exhaust Pi memory | Stream directly into one stereo vector and cap at 6,000,000 frames | Medium / medium | Fixed; decode and limit tests pass |
| IDEA-01 | Important / T,D | Idea MIDI/metadata used unbounded `fs::read`; a malicious/private file could consume memory or use a symlink | Require regular owned files and 16 MiB/1 MiB/64 KiB limits | Small / low | Fixed; symlink/oversize test passes |
| IDEA-02 | Important / T,D | Temp Idea files were renamed atomically but file contents/directory were not explicitly synced first | Sync every owned file and temp directory before no-replace rename | Small / low | Fixed; normal round trips pass; power-loss simulation unavailable |
| DRUM-01 | Important / D,N | Two Bossa entries shared a display name, so dedup exposed 71 of 72 | Give the standalone version a distinct descriptive name and require exactly 72 in test | Small / low | Fixed and validated |
| DRUM-02 | Important / D,N | `New Orleans Funk` and `Second Line` were identical; cultural and 6/8/triplet labels implied more authority than static 3/4 data supports | Vary the duplicate and use narrower creative/meter labels; document labels as inspirations | Small / medium musical judgment | Fixed statically; listening required |
| PRESET-01 | Important / D,N | 21 sounds are structurally legal but none can be called “good” from XML | Low-gain human pass; keep 5–8 distinct demo sounds | 60–90 min / low code risk | Listening required |
| LOOP-01 | Important / D,I | No WAV exists in public or private input, so content/provenance/seam/level review cannot run | If supplied, record model/service/account terms and analyze before public use | Input-dependent / licensing risk | Blocked by absent input, not passed |
| JUDGE-01 | Important / I,D | Hardware-heavy README path could discourage a judge without Pi/JACK/MIDI | Document tests, `config init`, `list`, screenshot JSON, and offline TUI behavior | Small / low | Fixed; debug and release non-audible smoke passed |
| HW-01 | Important / D,I | Planned Casiotone + D-50 demo needs two safe, distinct MIDI outputs and a stereo audio mix; the second cable, second known-safe interface, synth receive settings, and audio return remain unverified | Retain the one-device/software fallback; use a second interface only if independently proven, or evaluate a controlled insert/send architecture separately | Human hardware session / medium deadline risk | Intentionally deferred; four focused software routing/profile tests pass |
| UI-01 | Polish / T,D | `geometry.rs` used `u16` additions in hit testing; extreme synthetic rectangles could overflow in debug builds | Use subtraction-based bounded hit testing | Tiny / very low | Fixed; boundary test passes |
| DOC-01 | Polish / D | Help had a stray duplicated sentence; routing docs still said an ambiguous port might select first | Reconcile Help, Configuration, Future Improvements, and loop limits | Small / low | Fixed; local/external reference and diff checks passed |
| RT-01 | Deferred / T | JACK callbacks are unit-inspected but real server-loss, xrun, Pi load, driver disconnect, and disk-full behavior require hardware | Run only with explicit approval on expendable session, then record measurements | Hardware session / medium | Hardware required |

## Rust module audit ledger

| Module | Audit focus and evidence | Result |
|---|---|---|
| `audio_recorder.rs` | FFI lifecycle, fixed SPSC callback, 24-bit conversion, RIFF limit, worker/drop, recovery | Callback has no allocation/lock/I/O; symlink recovery fixed; real JACK/disk-full still hardware-required |
| `chord.rs` | Repeated held notes and chord naming | Exact counts and cleanup covered; no safety finding |
| `config.rs` | Default/installed/user precedence, migration aliases, ranges, save semantics | Hardware names remain data; invalid ranges fail; no P0/P1 finding |
| `control.rs` | Twelve synthv1 0.9.29 indices/ranges and ±0.03 colors | Unique mappings and bipolar range tested; invariant preserved |
| `controller_learn.rs` | Port ambiguity, non-forwarding learn, relative encoder, backups | Conflicts rejected and learn is non-audible; no new finding |
| `controller_profile.rs` | Catalog size/schema, match order, update boundary | Reviewed profile applies as data; downloaded update validates before atomic write |
| `device_profile.rs` | Bank/program numbering, writable/unknown names, override order | D-50 zero-based 0–63/64–127 model matches recorded Roland sources; generic 0–127 remains |
| `drum_pattern.rs` | Format limits, notes/velocities, arrange, save/delete boundary, 72 discovery | Duplicate/display-name issue fixed; static music audit in `DRUM_PATTERN_AUDIT.md` |
| `engine.rs` | Owned process marker, `/proc` identity, start/stop/crash, MIDI route, pickup, All Notes Off | Never signals an unverified process; stable-identity/retry race fixed and stressed; live server loss still hardware-required |
| `fsutil.rs` | Atomic replace/no-replace, unpredictable temporary names, directory sync | Storage primitive is durable and collision-safe on supported Linux filesystem |
| `geometry.rs` | 40×20 hit boxes, scrolling, overflow | Extreme-coordinate overflow fixed; render tests pass |
| `help.rs` | Markdown rendering, escaping, port binding, request/time bounds | Local help independent of LAN; one bounded `/help` page; no sensitive data served |
| `loop_player.rs` | Decode/format, pulse estimate, signed offsets, region/wrap/fade, sample-rate check, RT callback, library delete | Memory cap/streaming fixed; callback immutable/bounded; content audit blocked by zero WAVs |
| `main.rs` | CLI dispatch, daemon cleanup, doctor, config paths, destructive confirmations | Judge-safe commands identified; `start`/Idea play remain deliberately audible and excluded |
| `midi.rs` | Pickup crossing, scaling, re-arm, command/musical split | Loaded/reset values block mapped CC until catch; tests cover crossing directions |
| `navigation.rs` | Every screen/context action, modal reachability, hidden slots | Controller reachability is exhaustively checked; quit stays keyboard-only by design |
| `pads.rs` | Pad on/off consumption, layouts, relative encoder, config validation | Command releases consumed; collisions and ranges rejected |
| `preset.rs` | Three-engine discovery, XML-by-name, schema, SoundFont bounds, resolve ambiguity | 21 public files exact; legacy XML index issue avoided; listening remains separate |
| `recording.rs` | Musical MIDI filter, SMF timing/parser, Idea paths/save/load/replay/cleanup | Bounds/durability fixed; stop always emits channel cleanup |
| `scale.rs` | Boundary notes, nearest-note tie, chromatic roots | Mapping stays in 0–127 and downward tie is deterministic |
| `sequencer.rs` | Model/validation/storage/migration, event planning, route collision, ownership, tempo/mute/thru | Ambiguous port fixed; schedule/runtime ownership tests strong; pre-read allocation is P2 |
| `ui.rs` | Lifecycle, every screen/modal, route changes, REC, load/reset pickup, 40×20 render | Stop-all and screen transitions cancel owned work; repeated REC ownership fixed; hardware UX pending |

## Non-Rust and asset ledger

| Class | Files reviewed | Result |
|---|---|---|
| Cargo/package | `Cargo.toml`, `Cargo.lock`, `Makefile` | Rust 1.85 locked release passed; staged install contained 82 expected files/links, 21 presets, no private path, and staged uninstall removed product data |
| Install/setup/tuning | all five shell scripts | Setup is non-audible and backs up; tuner owns/reverses only its files; ShellCheck passes |
| Screenshot helper | `render-readme-screenshots.py`, `preview.html` | Python compilation passes; debug/release JSON each renders nine 40×20 screens with 800 cells |
| Runtime/controller config | both `.conf` templates | Generic controller is unmapped; all routes/paths stay configurable |
| Controller profile | `controller-profiles/catalog.json` | One MiniLab 3 profile with source note; parser tests pass |
| MIDI device profile | `midi-devices/roland-d-50.json` | Writable card names remain unknown; three Roland sources recorded; parser tests pass |
| Presets/generator | 21 XML, manifest, generator | XML/schema/range/type/generator checks pass; see `PRESET_AUDIT.md`; listening required |
| Drum data | 12 `.shdrum`, 60-entry catalog | 72 visible after curation; see `DRUM_PATTERN_AUDIT.md`; listening required |
| Documentation | README, `THIRD_PARTY.md`, all `docs/*.md` | Core contradictions reconciled; 67 local targets exist and external sources were checked; whitespace check passes |
| Images | 12 README PNG/JPEG assets and preview HTML | Formats match extensions; ten TUI PNGs are 960×640 or legacy 480×320, header is 1600×640, diagram is 1200×675 |
| Private/local | `.gitignore`, `user/`, archive boundary | Zero tracked private paths; zero WAVs; uncleared bank excluded |

## Judge testability and supported paths

- Supported build family: Debian/Raspberry Pi OS/Patchbox Linux, with native
  ALSA/JACK libraries. Real hardware was developed on Raspberry Pi/Debian 12;
  other OS families are not claimed.
- Fastest non-audible path: locked tests, private `/tmp` config initialization,
  preset listing, screenshot JSON, existing README screenshots, and interactive
  TUI at 40×20. These exercise real code and seeded presentation state; they do
  not simulate sound.
- Browser, Help, Project/Pattern editing, parsers, and screenshot rendering do
  not require JACK. A missing controller produces a status error rather than
  preventing the interface.
- Loading software instruments, playing WAV loops, and stereo capture require
  an already-running JACK server. External-MIDI audition/playback requires the
  named output. Musical validation requires actual monitoring and a human.
- Generated judge state is confined to `/tmp/shr-daw-judge-state` and optional
  `/tmp/shr-daw-screens.json`; delete those paths after evaluation.

## Validation record

All Rust commands used the required Rust 1.85 toolchain. On 2026-07-18:

- `cargo fmt -- --check`: passed;
- `cargo test --locked`: **251 passed, 0 failed**;
- `cargo clippy --locked -- -D warnings`: passed;
- `cargo build --release --locked`: passed;
- release non-audible smoke: initialized isolated `/tmp` configuration, listed
  all 21 public synthv1 sounds, and rendered nine 40×20 screens;
- ShellCheck: all tracked shell scripts passed;
- JSON: every tracked profile/catalog parsed with `jq`;
- synthv1: all 21 XML files passed `xmllint`; clean generator output reproduced
  all 21 byte for byte; 145 unique current synthv1 0.9.29 parameters per file
  passed name, index, type, and range checks;
- drums: the Rust parser/arranger tests passed and require exactly 72 visible
  bundled entries;
- package: an isolated `DESTDIR` install/uninstall passed, shipped 21 allowlisted
  presets, and contained no `user/` path;
- documentation/assets: 67 local Markdown targets exist, cited external pages
  were reachable or verified through their official site, Python screenshot
  helper compilation passed, and all 12 images passed format/dimension checks;
  and
- privacy/diff: no tracked `user/` path, no staged path, no WAV, and
  `git diff --check` passed.

No audible runtime/JACK smoke was substituted for the release smoke because it
was not authorized. Driver disconnect, xrun, real MIDI, listening, and Pi-load
claims remain explicitly outside this validation record.

## QA conclusion

The strongest proven qualities are explicit engine ownership, exact MIDI note
ownership, pickup-safe controls, conservative atomic/private storage, a real
40×20 workflow, and hardware adaptation through data. The largest remaining
submission risk is not another missing subsystem: it is absence of human
listening, final song evidence, the public video, and `/feedback`/Devpost
completion. Presentation and musical curation now have higher value than new
architecture.
