# Maintainer helper scripts

This document is the source-of-truth guide to the repository helpers in
`scripts/`: their arguments, environment variables, files changed, safety
boundaries, and the reasons they work the way they do. End-user commands remain
in the normal setup guides; this page explains the maintenance machinery.
Repository-wide priority, validation, data, and publishing policy remains in
`AGENTS.md`; current installed state remains in `docs/WORKSPACE_HANDOFF.md`.

All shell helpers use `set -euo pipefail`. An unhandled failure, unset variable,
or failed pipeline stops the operation instead of continuing with partial
assumptions.

## At a glance

| Helper | Intended use | Main side effects |
|---|---|---|
| `setup-local.sh` | Configure this checkout inside ignored private storage | Writes below `user/` by default; may run the interactive hardware wizard |
| `local.sh` | Run the checkout without using normal home-directory state | Writes runtime data below `user/` by default |
| `setup.sh` / installed `shr-setup` | Seed loops/demos and configure display/MIDI/JACK choices | Backs up and rewrites owned configuration; optionally masks two conflicting auto-services, downloads private loops, installs one marked JACK boot service or writes `~/.jackdrc`, and installs CPU tuning after confirmation |
| `install.sh` | Install dependencies and SHR-DAW on Debian/Raspberry Pi OS | With grouped consent, may use `sudo apt-get --no-install-recommends`, mask one user service, install owned RT policy, run rustup, `sudo make install-files`, and open setup |
| `audio-performance.sh` / installed `shr-audio-tune` | Diagnose audio policy and reversibly manage RT permissions or one audio CPU | Read-only plan/status/doctor plus owned limits/group, boot, systemd, governor, and JACK-affinity settings; CPU isolation requires reboot |
| `generate-docs-site.py` | Regenerate or drift-check the public GitHub Pages documentation | `--write` atomically replaces only `docs/index.html`; `--check` is read-only |
| `render-readme-screenshots.py` | Regenerate or validate real TUI documentation images | Writes tracked PNGs below `docs/images/` only |
| `generate_cleared_presets.sh` | Reproduce the authored public synthv1 bank | Creates named preset files only when they do not already exist |
| `generate_demo_songs.py` | Reproduce or validate cleared public-domain demos | `--write` replaces only tracked demo outputs; normal mode is read-only and rejects changes/extras |
| `capture-minilab-midi.sh` | Passively capture and label MiniLab 3 MIDI evidence | Temporarily stops and restores `amidiminder`; writes one unique log below `/tmp` by default |
| `shr recorder-stress` | Non-audibly exercise the production multistem buffer/writer without JACK | Creates one unique synthetic take below an explicit destination |
| `shr final-mix-stress` | Non-audibly exercise the three-source final DSP and stereo writer without JACK | Creates one unique 24-bit stereo stress WAV below an explicit destination |
| `shr master-strip-bench` | Compare neutral/active production strip callback cost and isolated 4×/8× interpolation without JACK | Read-only deterministic CPU work; creates no files |

None of the setup, tuning, preset, or screenshot helpers starts JACK, a synth
engine, MIDI playback, or an audible test. `local.sh` is the exception only in
the ordinary sense that it launches the application the user explicitly asked
to run; what the application subsequently starts depends on that user action.

## Passive MiniLab evidence capture: `capture-minilab-midi.sh`

### Invocation

```sh
./scripts/capture-minilab-midi.sh --check
./scripts/capture-minilab-midi.sh
./scripts/capture-minilab-midi.sh --output /tmp/named-minilab-capture.log
```

`--check` discovers the one connected MiniLab 3, enumerates all of its readable
ALSA sequencer ports, checks routing safety, and compiles the monitor without
opening a MIDI port or changing a service. A real capture writes a unique
timestamped log below `/tmp` unless `--output` names a new file. Existing files
are never replaced. While it runs, any non-empty line typed at the terminal is
recorded as a timestamped operator marker; `Ctrl-C` ends the capture.

The log records receive time and total order, ALSA client and port,
reconstructed raw MIDI bytes and status, decoded message type and channel, and
the relevant note, velocity, pressure, controller, program, or pitch-bend
value. It preserves SysEx bytes and MIDI realtime events as well. The monitor
subscribes to every readable port on the selected MiniLab client but never
creates a MIDI output or forwards a captured event.

### Safety boundary

The helper refuses to run when more than one MiniLab matches, an SHR or synthv1
process is active, a MiniLab ALSA route leads somewhere other than JACK's own
sequencer backend, or a MiniLab JACK MIDI port has an active graph connection.
It reports the exact client and ports before opening them. If the system
`amidiminder` service is active, the helper temporarily stops it so the newly
created monitor cannot be auto-connected to unrelated MIDI hardware, then
restores it on every normal, error, signal, or `Ctrl-C` exit. Existing MIDI and
JACK connections are not removed. The helper does not start or stop JACK, open
an audio device, launch SHR-DAW or a synth, transmit MIDI or SysEx, or read or
write anything below `user/`.

The small C monitor is compiled into a unique temporary directory for each run
and removed on exit. This avoids adding a persistent binary while using ALSA's
sequencer API directly enough to retain source-port identity, message ordering,
realtime events, and raw status bytes that a formatted `aseqdump` transcript
would otherwise obscure.

## Repository-local setup: `setup-local.sh`

### Invocation

```sh
./scripts/setup-local.sh
SHSYNTH_USER_DIR=/absolute/private/path ./scripts/setup-local.sh
```

Environment:

- `SHSYNTH_USER_DIR` selects the private root. It defaults to the repository's
  ignored `user/` directory.
- `SHSYNTH_BIN` may explicitly select an already-built `shr` executable. It
  defaults to `target/debug/shr` in this checkout.

The wrapper exports:

- `XDG_STATE_HOME=$SHSYNTH_USER_DIR/state`;
- `XDG_DATA_HOME=$SHSYNTH_USER_DIR/data`;
- `SHSYNTH_PRESET_DIR=$SHSYNTH_USER_DIR/presets/synthv1`;
- `SHSYNTH_LOOP_INBOX=$SHSYNTH_USER_DIR/data/shsynth/loop-inbox`.

It requires an executable SHR-DAW binary, creates the private preset directory,
copies only missing public presets into it, and then replaces itself with
`setup.sh --state-dir "$XDG_STATE_HOME/shsynth"`. The shared wizard seeds only
the missing WAV names in `loops/cleared-loops.txt` and missing cleared demo
Projects. Matching demo MIDI/manifest files live in the private XDG demo tree.

### Why it exists

The regular setup command belongs to an installed application and therefore
uses normal XDG user directories. A checkout needs a hard, visible boundary
between public repository files and local Projects, Ideas, recordings,
downloads, routes, and uncleared sounds. This thin wrapper establishes that
boundary while reusing the exact same setup wizard. It never overwrites a
same-named private preset because a private edited sound takes precedence over
the public seed copy.

It deliberately refuses to compile automatically: configuration should use the
binary that was explicitly built and tested, not silently change code or wait
through an unexpected build.

## Repository-local launcher: `local.sh`

### Invocation

```sh
./scripts/local.sh
./scripts/local.sh doctor
./scripts/local.sh screenshots
SHSYNTH_USER_DIR=/absolute/private/path ./scripts/local.sh
```

All arguments are passed unchanged to `shr`. The environment and private-preset
copy rules match `setup-local.sh`. An explicit executable in `SHSYNTH_BIN`
wins. Otherwise, the launcher always uses `target/debug/shr`; it never chooses
an installed or release binary by timestamp. It resolves its own symlink before
finding the repository, so a user-local `shr` symlink or shell alias may safely
target this launcher. The launcher refuses to run until both the debug binary
and local `shsynth.conf` exist.

### Why it uses `exec`

`exec` makes SHR-DAW replace the wrapper process. Signals, exit status, terminal
ownership, and clean shutdown therefore reach the application directly instead
of passing through a redundant shell parent. This matters for All Notes Off and
owned engine shutdown.

The launcher does not recopy or reset the whole private tree. It validates the
demo corpus and creates only required directories and missing public preset,
loop, and demo seeds, preserving all user work.

## Hardware setup wizard: `setup.sh` / `shr-setup`

### Invocation and inputs

```sh
./scripts/setup.sh
./scripts/setup.sh --state-dir /absolute/state/shsynth
shr-setup
```

Options:

- `--state-dir DIR` overrides the runtime/controller configuration directory.
- `-h`, `--help` prints usage.

Environment:

- `XDG_STATE_HOME` changes the normal state root.
- `XDG_DATA_HOME` changes the recording/data root written into configuration.
- `SHSYNTH_BIN` selects the binary used for config initialization and controller
  profile commands.
- `SHSYNTH_PRESET_DIR`, when present, becomes the configured synthv1 preset
  directory.
- `SHSYNTH_LOOP_INBOX`, when present, becomes the configured and seeded loop
  import inbox.

The source-tree form reads templates from `config/`, MIDI-device profiles from
`midi-devices/`, allowlisted starter WAVs from `loops/`, and the cleared demo
manifest/files from `demos/`. The installed form resolves all four beneath
`share/shsynth/`. If configuration is missing in the
normal state directory it uses `shr config init`; for an explicit state
directory it copies only missing template files.

Setup always creates or preserves configuration, selects the active XDG/private
loop inbox for new configuration, copies missing allowlisted starter loops,
copies missing demo Projects to `songs/`, and mirrors the cleared demo corpus
under `demos/`. The manifest itself may be refreshed; user Projects are never
replaced.
If standard input is not a terminal it then stops; it never guesses display,
download, or hardware choices in automation.

Interactive setup tracks each externally meaningful phase. Normal completion
prints the full phase summary. An error, `Ctrl-C`, or termination prints which
phases completed, which phase may be partial, which later phases did not start,
and the exact rerun command. It prints restore commands only when this run
created named configuration or `.jackdrc` backups, and names service/tuning/
private-loop recovery only when that side effect was recorded. It does not
attempt a blanket rollback across those ownership domains.

### Interactive sequence

Before changing configuration, the wizard creates unique timestamped backups of
both `shsynth.conf` and `controller.conf`. It then:

1. detects live RT limits and offers owned `audio`-group/PAM policy repair,
   defaulting to no and requiring logout/login when accepted;
2. when present and not already masked, offers the recommended exclusive-MIDI
   cleanup for exactly the per-user `fluidsynth.service` and system
   `amidiminder.service`, also defaulting to no;
3. asks whether note names use English `B` or German `H`/`B` spelling;
4. retains an existing Patchbox/administrator JACK service owner or a live
   ownerless `jackd` process; only when neither exists can it select a stable
   ALSA card name and JACK timing. On stock systems it previews and, by default,
   offers one marked SHR-managed boot service. Declining that service writes a
   backed-up one-line `~/.jackdrc` fallback instead;
5. selects the controller input, chooses combined or control-only behavior,
   then selects zero or more independent performance inputs; controller-only,
   keyboard-only, combined, and separate-device setups are all explicit;
6. writes the controller exact match to runtime/controller configuration and
   repeated performance matches only to runtime configuration, then runs
   non-audible `shr pads auto`, optionally followed by `shr pads learn` if
   no reviewed profile matches;
7. discovers physical JACK playback ports, writes the same preferred stereo
   pair for synth and loop playback, then optionally records a named internal
   fallback and a distinct final analogue-headphone fallback;
8. optionally downloads four MusicRadar 80s drum beats, converts them to the
   chosen WAV rate with SoX, and records their source/redistribution terms;
9. optionally configures a distinct stereo capture pair and label;
10. optionally configures an external MIDI destination and data-driven device
   profile;
11. on systems with at least four CPUs, prints `shr-audio-tune plan`, then
    optionally invokes it and records the selected engine CPU. The prompt
    defaults to no.

### Design decisions

- Hardware/client names are written to configuration, never Rust constants.
- ALSA and JACK discovery is advisory. Manual exact values remain possible so
  setup can be completed while hardware or JACK is offline.
- System, Midi Through, and SHR-owned MIDI ports are filtered from controller
  candidates to avoid feedback and self-connection.
- JACK choices require distinct left/right ports.
- Configuration keys are replaced through a temporary same-directory file and
  `mv`, preserving file permissions when possible. This avoids leaving a
  half-written configuration.
- Values containing newline or carriage-return characters are rejected, and
  capture labels also reject the field separator `|`.
- ALSA card numbers are rejected for the managed service because USB discovery
  order can change. The service uses `hw:NAME`, runs as the musician account,
  retries after a temporarily absent interface, and sets
  `JACK_NO_AUDIO_RESERVATION=1` because a headless system unit has no desktop
  session bus. It is enabled but never started by setup.
- The wizard may write `~/.jackdrc` only after the managed service is declined,
  and only when no system JACK service already owns lifecycle. Patchbox's
  shared service and `/etc/jackdrc` remain Patchbox-owned.
  It never starts or restarts JACK because doing that during a live session can
  interrupt or produce audible output.
- Destructive or unrelated system-changing prompts default to no. On stock
  systems where the user has explicitly chosen JACK hardware and timing, the
  single managed boot-service prompt defaults to yes so a fresh keyboardless
  installation works after reboot. The exclusive-routing prompt
  remains explicit because
  `amidiminder` is a system-wide service that another application might use.
  It stops and persistently masks only `fluidsynth.service` and
  `amidiminder.service`, verifies both masks, and leaves packages, SoundFonts,
  JACK, unrelated synths, and arbitrary ALSA subscriptions untouched. The user
  FluidSynth mask does not prevent SHR from executing the binary directly.
  Restore with `systemctl --user unmask fluidsynth.service` and
  `sudo systemctl unmask amidiminder.service`; setup does not start either unit.
- Controller learning is non-audible: learned MIDI is not forwarded to a synth.
- Controller learning listens only to the selected controller source;
  performance-only inputs bypass command interpretation.
- Existing configuration is backed up rather than silently discarded.
- Hardware discovery never overwrites a remembered route merely because that
  hardware is absent. The user must explicitly choose a changed/disabled route.
- Public and downloaded-private loop seeds never replace a same-named inbox
  file. Public packaging is constrained by `loops/cleared-loops.txt`.
- Cleared demo Projects never replace same-named user songs. Demo source
  packaging is constrained by `cleared-demos.json` and deterministic validation.
- The optional 78 MB archive is fetched directly from MusicRadar into a
  temporary directory and deleted after extracting four tempo-labelled beats.
  Those raw WAVs remain private because MusicRadar forbids redistribution.

## Installer: `install.sh`

### Invocation

```sh
./scripts/install.sh
./scripts/install.sh --no-deps
./scripts/install.sh --no-config
./scripts/install.sh --plan
./scripts/install.sh --yes
./scripts/install.sh --no-deps --no-config
```

Options:

- `--no-deps` skips `apt-get update` and dependency installation.
- `--no-config` skips the final interactive `shr-setup` run.
- `--plan` performs prerequisite checks and exits before builds or changes.
- `--yes` is explicit non-interactive consent for the package/service and
  real-time-policy groups; without it a non-terminal changing run stops.
- `-h`, `--help` prints usage.

The installer rejects root invocation: it must run as the musician account and
uses `sudo` only for named system changes. It verifies `apt-get` and `sudo`
before the first dependency mutation.

With dependencies enabled, it requires a Debian-style `apt-get` system and uses
`sudo` with `--no-install-recommends` to install the build toolchain, ALSA/JACK
runtime and headers, SoX and unzip for optional loop installation, Python 3 for
demo validation/seeding, ripgrep for helper policy/config inspection, the three
supported software instruments, and their explicitly named packaged data. It
then resolves Debian 13's `jack-example-tools` or the earlier `jack-tools` name
and installs that one small tools package with its runtime recommendations so
`jack_lsp` and the packaged JACK bridge clients are complete. Avoiding
recommendations for the main package group is deliberate:
the FluidSynth CLI recommends Qsynth, which in turn recommends the roughly
142 MiB FluidR3 GM bank, while SHR explicitly installs and configures the much
smaller TimGM bank. It requires
Rust 1.85 or newer; when necessary it installs the official minimal rustup
toolchain for the current user and runs Cargo as `cargo +1.85.0`.

It then runs locked tests, creates a locked release build, installs the files
with `sudo make install-files`, and normally opens `shr-setup`.

Before its first package or service mutation, the installer prints the enabled
package, per-user FluidSynth mask, Rust, test/build, install, and setup phases,
then gathers grouped consent. After packages, it separately offers missing
real-time audio policy, defaulting to no; accepting records owned group/limits
state and explains the required logout/login. It explains the exact FluidSynth
service consequence before masking rather than after the action.

If a later command fails after system mutation began, the exit report names
`sudo dpkg --configure -a` for an interrupted package transaction, the
idempotent installer rerun, the exact optional FluidSynth unmask, and
`shr-audio-tune recover` for a pending permissions transaction. It does not
pretend `apt` can be atomically rolled back with SHR-owned files.

That is the install helper's production behavior, not the normal development
validation policy. While the combined build-and-test gate in `AGENTS.md` is
active, do not invoke the installer merely to obtain a full suite or release
build.

### Why install is explicit and relatively heavy

SHR-DAW is a live-audio program. Installing an untested binary or silently using
an old distro compiler is a worse failure mode than spending time on a locked
test/build. Dependencies are installed rather than quietly skipping parts of
the application. `--no-deps` and `--no-config` exist for maintainers and package
builders who have already satisfied those responsibilities.

After explicit package/service consent, the installer reloads the current user
manager, stops and masks the exact package-enabled `fluidsynth.service`, and
verifies the persistent mask. An unowned FluidSynth can load a large bank, open
audio and MIDI devices, and layer with SHR. The mask does not prevent direct
execution of the FluidSynth binary.

The installer does not start JACK or a synth. The normal interactive setup that
follows detects and offers to mask `amidiminder` before hardware routing. A
non-interactive setup or `--no-config` makes no additional system-wide service
change; package builders and automated installers must establish their intended
MIDI auto-patching policy separately.

## Audio CPU tuning: `audio-performance.sh` / `shr-audio-tune`

### Invocation

```sh
sudo shr-audio-tune install
sudo shr-audio-tune install 3
shr-audio-tune plan 3
shr-audio-tune status
shr-audio-tune doctor 3
sudo shr-audio-tune recover
sudo shr-audio-tune remove
sudo shr-audio-tune permissions-install USER
sudo shr-audio-tune permissions-remove
shr-audio-tune jack-plan USER CARD RATE PERIOD_SIZE PERIODS
sudo shr-audio-tune jack-install USER CARD RATE PERIOD_SIZE PERIODS
sudo shr-audio-tune jack-remove
```

Commands:

- `plan [CPU]` previews detected platform, topology, kernel support, exact
  boot tokens, lifecycle and tradeoff without changing state.
- `install [CPU]` reserves the zero-based CPU; the default is the highest
  online CPU.
- `status` reports configured intent, kernel feature support, live isolation,
  governors, RT policy, JACK ownership/lifecycle, and rollback availability.
- `doctor [CPU|none]` emits actionable configured-versus-live states and
  returns failure only for partial, stale, conflicting, duplicate, unsupported,
  interrupted, reboot-required, or live-mismatch conditions.
- `recover` restores hash-matching pre-transaction group/limits state after an
  interrupted permission change and command-line/files after an interrupted
  CPU install.
- `remove` reverses only the settings installed by this helper and keeps the
  original boot-command-line backup.
- `permissions-install USER` and `permissions-remove` manage only missing
  `audio`-group/limits state, guarded by pre/post hashes so later administrator
  edits remain untouched.
- `jack-plan` validates the musician account, connected stable ALSA card name,
  and bounded timing values without mutation.
- `jack-install` refuses a live or external JACK owner, creates marked
  `/etc/jackdrc` and `jack.service`, records their hashes, and enables but does
  not start the service. `jack-remove` refuses to stop live audio and removes
  only unchanged marked files.
- `runtime-start` and `runtime-stop` are internal systemd-service entry points,
  not normal maintainer commands.

Environment:

- `SHR_TUNE_ROOT=/fixture/root` prefixes all managed absolute paths and disables
  real `systemctl` calls. It exists for isolated tests and inspection; the
  fixture still needs representative `/sys`, `/proc`, `/boot`, and `/etc`
  paths.

### Managed state

Installation requires at least four online CPUs and refuses non-contiguous or
unusual online-CPU layouts instead of inventing a mask. It records a versioned
manifest and recoverable transaction beneath `/var/lib/shr-audio-tune/`, backs
up the one detected Raspberry Pi boot command line, and manages only:

- `isolcpus=domain,managed_irq,<CPU>`;
- `nohz_full=<CPU>` only with `CONFIG_NO_HZ_FULL=y`;
- `rcu_nocbs=<CPU>` only with `CONFIG_RCU_NOCB_CPU=y`;
- `irqaffinity=<housekeeping CPUs>`;
- `/etc/systemd/system/jack.service.d/90-shr-audio-cpu.conf`;
- optional `/etc/systemd/system/jack.service` and `/etc/jackdrc`, with separate
  ownership state below `/var/lib/shr-audio-tune/jack-service/`;
- `/etc/systemd/system/shr-audio-performance.service`;
- `/usr/local/libexec/shr-audio-tune-runtime`.

The runtime service records each existing CPU governor before selecting
`performance` where supported, then restores the recorded values when stopped.
Installation enables the service for the next boot but does not start it live.
The JACK drop-in applies the audio CPU affinity, real-time priority limit, and
unlimited memory lock on JACK's next start.

### Safety rationale

- Pre-existing kernel keys or managed-path collisions are refused unless this
  helper already owns the installation.
- A different already-installed CPU must be removed before changing CPUs.
- Installation stages ownership and pre-images before mutation. An ordinary
  failure rolls back immediately; a killed/interrupted operation leaves the
  exact `recover` path. Real-time permissions use the same pre-image and
  later-admin-edit protection. Repeated same-CPU install converges.
- `remove` deletes only exact tokens and unchanged hashes owned by this helper;
  it does not restore an entire possibly-stale command line over later
  administrator work or remove later edits.
- The untouched original command line remains as a recovery artifact.
- Installation and removal never start or restart JACK. Managed service removal
  refuses while JACK is live. Kernel isolation and an enabled JACK boot service
  wait for reboot; the affinity drop-in also applies to an explicit safe start.
- `audio.engine_cpu` belongs to `shsynth.conf`; removal tells the user to clear
  it rather than modifying an unknown runtime configuration path as root.

## Public documentation site: `generate-docs-site.py`

### Invocation and dependencies

```sh
make docs-site
make check-docs-site

python3 scripts/generate-docs-site.py --write
python3 scripts/generate-docs-site.py --check
```

Exactly one argument is required. `--write` atomically replaces
`docs/index.html`. `--check` regenerates the complete page into a temporary
directory, compares its bytes with the tracked file, and fails on drift without
changing the checkout.

The renderer requires Python 3.11, Debian's pinned
`python3-markdown-it` 2.1.0-5 and `python3-mdit-py-plugins` 0.3.3-1 packages.
The helper verifies the corresponding upstream versions, enables CommonMark
tables and strikethrough plus GFM task lists, and fails rather than producing
different output with an unreviewed renderer version. Generation needs no
network access, JavaScript runtime, Rust build, audio dependency, or package
manager invocation. HTML Tidy, Chromium, ChromeDriver, and Selenium are
validation tools, not generator dependencies.

### Sources, output, and grouping

The complete input is `README.md`, every `*.md` file recursively below
`docs/`, `THIRD_PARTY.md`, `LICENSE`, the package version in `Cargo.toml`, and
every local image those Markdown sources reference below `docs/images/`.
`docs/README.md` remains the category authority: its six named groups control
the generated navigation and document order. The repository landing page,
nested menu chapters, unlisted public supporting records, and licence receive
explicit overview/current/archive/legal placement around those owned groups.

The single tracked output is `docs/index.html`; `docs/.nojekyll` is a static
GitHub Pages control file and is not generated. CSS and JavaScript are inline.
Referenced images remain relative files below `docs/images/`. The social-card
metadata uses the absolute production URL for the dedicated lossless 1200×630
PNG connection diagram. It preserves the diagram at native scale and omits its
title strip so link-preview crawlers do not receive a reduced or lossy copy.

The visible page is a product presentation, not an expanded repository dump.
It uses one introduction and one screenshot tour. Detailed guides are closed
by default, and measurements, maintainer records, handoffs, and proposals sit
inside a separately labelled technical archive. When the same image bytes are
referenced more than once, the first occurrence owns the image and later
references link back to it.

### Social-card image QA

The connection diagram is functional documentation. Never use an image
generator to recreate its hardware or routing: a plausible-looking cable,
self-loop, invented port, or label attached to the wrong device is a functional
error. Begin with the approved diagram, keep the route geometry intact, and
inspect every connection and label at full resolution before publication.

Dense diagrams degrade quickly in link cards. The current social card removes
the nonessential title strip from the approved 1200×675 PNG, retains the
remaining 1200×625 pixels without resampling, and adds five neutral rows at the
bottom to reach 1200×630. Keep the result as PNG. Do not shrink the entire
diagram into a framed composition or convert it through JPEG; Facebook will
scale and compress the asset again. In addition to full-size inspection, view
it near a typical feed width of 500 pixels and confirm the routes remain
distinguishable and the important labels remain readable.

Validate a regenerated crop by comparing its unchanged 1200×625 region against
the approved source and requiring zero differing pixels. A material social-card
revision should use a new public filename, update `SOCIAL_IMAGE` and both image
metadata URLs in the generator, regenerate `docs/index.html`, deploy, and then
use Meta's Sharing Debugger to request a fresh scrape. Existing Facebook posts
may retain their original attachment even after the page metadata changes.

Regenerate `docs/index.html` when intentionally refreshing the public
presentation. An ordinary change to a Markdown guide does not require a site
build, and neither `make test` nor `make install-files` runs the drift check.
`make check-docs-site` remains available for a maintainer who is preparing a
site update.

### Determinism and safety boundary

The generator adds no timestamp, temporary path, machine name, branch tip,
commit hash, remote script, runtime fetch, analytics, cookie, or embedded image
payload; dated facts already present in public source documents remain intact.
Referenced-image dimensions and SHA-256 values are derived from the public
files, so a material image change participates in drift detection. Markdown
raw HTML is disabled; source text is escaped, while the task-list plugin emits
only its fixed disabled-checkbox markup.
Machine-specific `/home/patch` names are shown as neutral checkout, workspace,
or `$HOME` labels, and the ignored `user/` root is shown as
`$SHSYNTH_USER_DIR/`. This keeps the public copy useful without publishing a
machine-local or private path.

Generation fails for a missing source, broken local file or heading fragment,
duplicate generated anchor, unsupported image format or URL scheme, remote
image, query-bearing or repository-external local path, link into `user/`,
credential-like content, or unrecognised renderer version. Included Markdown
documents link to their same-page anchors; public repository files outside the
page link to their GitHub source. No file below `user/` is read, copied,
linked, or written. The helper does not build or launch SHR-DAW, JACK, a synth,
MIDI, playback, recording, or hardware.

## TUI screenshot renderer: `render-readme-screenshots.py`

### Invocation

```sh
# Render every README and menu-manual image.
python3 scripts/render-readme-screenshots.py

# Render one exact manifest name for visual inspection.
python3 scripts/render-readme-screenshots.py \
  --only menu/ft2-step-edit-set.png

# Validate the pinned font and independent glyph/row fixtures without Rust.
python3 scripts/render-readme-screenshots.py --self-test

# Exhaustively validate the complete manifest without rewriting images.
python3 scripts/render-readme-screenshots.py --check
```

Options:

- no option renders every frame returned by the Rust manifest;
- `--only NAME` renders only an exact output name from that manifest;
- `--self-test` checks the approved decompressed font hash, known independent
  glyph rasters, Unicode coverage, byte stride, bit order, all 24 source rows,
  and cell-boundary placement without producing a manifest;
- `--check` rejects missing, stale, extra, duplicate, unsafe, or
  non-fixed-palette outputs and reconstructs every expected source pixel from
  the manifest plus approved PSF before checking 960×624 dimensions and exact
  2×2 replication.

Environment:

- `CARGO` overrides the Cargo executable used for `cargo run --locked`.
- `SHR_SCREENSHOT_COMMAND` replaces the complete manifest-producing command;
  it is parsed with shell-style quoting but run directly, not through a shell.

The default command uses the installed Rust 1.85 toolchain when present and
runs `shr screenshots`. Rust renders the real application `draw` function into
40×13 ratatui test buffers seeded by the deterministic `ScreenshotScenario`
and `ScreenshotSpecialScenario` fixtures in `src/ui.rs`. The renderer derives
the complete overview/menu/context/overlay count from that manifest rather
than embedding an expected count. The compact Levels, Loop Mix, and MASTER
STRIP fallbacks are rendered at 38×12 and padded with black to the manifest's
40×13 canvas so the same renderer can prove the non-native path without
changing image dimensions. JSON supplies each cell's symbol, foreground,
background, and bold state. A complete render removes only stale TUI PNGs in
the two owned output namespaces after writing the current manifest. No JACK
server, engine, MIDI port, or private user file is involved.

### Image parameters

- terminal geometry: 40 columns × 13 rows;
- cell geometry: 12×24 pixels;
- native content raster: 480×312 pixels;
- final scale: exactly 2;
- final PNG: 960×624 pixels;
- primary font: `/usr/share/consolefonts/Uni2-TerminusBold24x12.psf.gz`;
- accepted fallback: a byte-identical decompressed copy at
  `target/Uni2-TerminusBold24x12.psf`;
- output roots: `docs/images/shr-daw-*.png` and `docs/images/menu/*.png`.

This is the exact PSF2 font loaded on tty1 by `/etc/default/console-setup`
(`FONTFACE=TerminusBold`, `FONTSIZE=24x12`). Each glyph is natively 12×24, so
the renderer copies its bits directly without horizontal stretching, font
substitution, smoothing, or host font metrics. The 40×13 application content
occupies 480×312 of the 480×320 framebuffer; the remaining eight framebuffer
pixels are outside ratatui's terminal-cell surface and are not invented in the
documentation image. Ratatui's ANSI colors and bold modifier are converted
through a fixed palette. The renderer refuses unsupported manifest symbols
instead of silently substituting another glyph. This approved font contains a
dedicated U+2016 double-vertical `‖`; the renderer uses that bitmap directly
and independently proves it is not the U+2551 box-border glyph.

The generated documentation site displays TUI PNG content at exactly
480×312 CSS pixels (one CSS pixel per native terminal pixel) and uses a local
horizontal scroller on narrower viewports. It does not fractionally shrink a
960×624 source into a two-column card, which can drop or blend glyph rows even
when the stored PNG itself is exact.

### Why the renderer is intentionally slow

The final enlargement uses explicit nested loops that copy each native pixel
into an exact 2×2 square. A library resize could be faster, but the explicit
operation makes the contract obvious in code and cannot silently acquire
interpolation, antialiasing, color blending, or a version-dependent sampling
rule. This preserves the pixel font and makes mobile/browser display crisp
without pretending the application has more than 40×13 cells.

`--check` is also deliberately exhaustive. It opens every expected image,
checks every 2×2 block, and compares every pixel with a fresh native raster
decoded from the approved font instead of trusting file metadata or the name
of a resize filter. On the Raspberry Pi, rendering or validating the complete
manifest takes noticeable time. That time is an accepted
documentation-integrity cost, not an optimization bug. Do not replace the
scaler or weaken the check merely to make the command faster. First render one
representative image and inspect it; then run the complete batch.

Pillow is used as a bitmap container and PNG writer, not for font rendering.
Using a TTF, browser screenshot, GUI terminal, or Pillow text API would make
glyph metrics dependent on a desktop font and could introduce smoothing.

## Cleared-preset generator: `generate_cleared_presets.sh`

### Invocation

```sh
./scripts/generate_cleared_presets.sh
```

There are no command-line options or environment controls. The script uses
`presets/synthv1/Velvet Tines.synthv1` as the complete current-schema template.
Its internal helper has the conceptual form:

```text
make_preset NAME PARAMETER=VALUE ...
```

For each authored sound, it copies the template, changes the XML preset name,
and replaces selected `<param>` values by exact parameter name with Perl. Values
not listed in the recipe remain inherited from the known template.

### Why it refuses to overwrite

Every destination is checked before copying, and the whole run stops if it
already exists. This prevents a reproducibility tool from overwriting a later
hand-edited or reviewed public sound. Consequently, run it in a clean temporary
checkout or against an intentionally absent generated bank when auditing
reproducibility; do not run it casually over the normal populated checkout.

The generator is an authorship recipe, not a licence grant. Adding or changing
a public preset still requires schema/XML validation, listening review when
authorized, an entry in `cleared-presets.txt`, and provenance in
`THIRD_PARTY.md` as described by `docs/NEW_PATCHES.md`.

## Cleared demo generator: `generate_demo_songs.py`

### Invocation

```sh
./scripts/generate_demo_songs.py
./scripts/generate_demo_songs.py --files
./scripts/generate_demo_songs.py --write
```

Normal mode regenerates all expected bytes in memory and validates the exact
`demos/` directory. It fails for a missing, changed, or extra regular file.
`--files` performs the same validation and then prints the manifest-cleared
repository paths used by `make install-files`. `--write` is the only mutating
mode: it creates `demos/` if needed and replaces the 10 MIDI files, 10 current
`.shsong` Projects, and `cleared-demos.json` with deterministic output. It does
not touch user/XDG song data.

The script uses only Python's standard library. Each format-1 MIDI contains a
conductor track and five named musical parts; each Project contains the same
parts with canonical compatibility-format-5 `default` routes. The JSON
manifest owns title,
tempo, meter, key, parts, description, style ideas, original-arrangement
licence, public-domain reasoning, institutional source URLs, filenames, and
SHA-256 hashes. `src/demo.rs` validates that manifest, MIDI chunk structure,
native Project loading/routing, metadata, and exact directory membership.

The generator's melody/harmony/event data are the original SHR-DAW
arrangements. Do not replace them with downloaded MIDI or a transcription of a
modern recording. Any new title needs its own public-domain analysis and source
record before `--write`; changing the source requires rerunning validation and
reviewing the regenerated hashes. No JACK client or MIDI output is opened.

## Related Make targets

The Makefile is not a script, but the installer delegates its final file layout
to it:

```sh
make build
make test
make check-demos
make docs-site
make check-docs-site
sudo make install
sudo make install-files
sudo make uninstall
```

Variables:

- `CARGO` selects Cargo;
- `PREFIX` defaults to `/usr/local`;
- `DESTDIR` prefixes the install tree for packaging or a non-root fixture.

`install-files` first runs `check-demos`, then installs only presets and demos
named by their cleared manifests, the
configuration and device/profile data, drum patterns, documentation, nested
menu chapters, and nested menu images. The public `shr` binary receives the
compatibility aliases `shs` and `synth-player`; no separate process binary is
installed for those names.

Use `DESTDIR` to inspect installation without touching the host:

```sh
fixture=$(mktemp -d)
make install-files DESTDIR="$fixture" PREFIX=/usr/local
find "$fixture/usr/local" -type f -o -type l
```

Choose a dedicated temporary directory and remove it only after confirming the
expanded path. `uninstall` is intentionally broad within the exact selected
`PREFIX`/`DESTDIR` application paths; never point those variables at an
unresolved or unintended root.

## Helper-specific validation

Match validation to the helper's effects:

- Shell helper: run `shellcheck` on each changed shell file.
- Python renderer: run `python3 -m py_compile`, inspect one image, render the
  full batch, and run `--check`.
- Documentation-site generator: run `python3 -m py_compile`, regenerate twice,
  run `make check-docs-site`, validate HTML syntax and local references, and
  inspect JavaScript-enabled and disabled layouts at representative phone,
  tablet, and desktop widths.
- This guide: check its local references and run `git diff --check`.
- Preset generator or output: validate every affected `.synthv1` with
  `xmllint`, confirm parameter names, manifest membership, and provenance.
- Demo generator or output: compile the Python helper, run its normal check,
  run the Rust structural test, inspect manifest provenance, and verify the
  staged package contains only `--files` output.
- Installer, setup, runtime, Makefile, Rust fixture, Cargo, or application
  behavior: follow the fast debug validation policy in `AGENTS.md`; run full
  tests, warning-denied Clippy, and release validation only on explicit request.
- Audio host policy: run `scripts/test-audio-performance.sh`. Its isolated roots
  mock boot, proc, sysfs and systemd state; it must never call a real service or
  hardware operation.
- Install layout: use a validated explicit `DESTDIR` fixture and confirm the
  nested manual chapters/images and cleared-only preset bank.

Apply the private-data and publishing checks in `AGENTS.md` before any commit.

## Synthetic multitrack recorder stress

### Invocation

```sh
shr recorder-stress DEST [SECONDS] [CHANNELS] [RATE] [CALLBACK]
```

`DEST` is required and must be an explicit non-root directory. Defaults are 10
seconds, 18 channels, 48000 Hz, and 128 frames/callback. Bounds are 1–86400
seconds, 1–64 channels, 8000–384000 Hz, and 16–65536 callback frames.

The command does not load runtime configuration, open/start JACK, register a
port, transmit MIDI, start a synth, or produce sound. It paces deterministic
distinguishable samples at the requested real-time rate through the same
interleaved SPSC ring, mono-WAV writer, manifest, fsync, and no-replace take
publication used by live capture. It reports total frames, wall time, aggregate
write throughput, writer high-water frames, drops, overflows, channel-identity
verification, and the exact published session.

The only persistent side effect is one uniquely named
`synthetic-multitrack*.take` below `DEST`. Existing names are never replaced.
Temporary work is one matching `*.take.part` owned by this invocation; the
recorder never cleans the destination, follows a temporary symlink, or removes
unrelated content. A successful take remains for inspection. Use a dedicated
temporary destination when the caller intends to remove it later, and validate
that exact expanded path before doing so.

This helper exists because a fast in-memory mock would not exercise Raspberry
Pi storage scheduling, the real bounded transfer, per-stem conversion, flush,
manifest, or atomic publication. It is evidence for hardware-independent
capacity and file correctness, never evidence that an MR18 or any other
physical interface passed.

## Synthetic final-mix stress

### Invocation

```sh
shr final-mix-stress DEST [SECONDS] [RATE] [CALLBACK]
```

`DEST` is required and must be an explicit non-root directory. Defaults are 10
seconds, 48000 Hz, and 128 frames/callback. Bounds are 1–86400 seconds, a
supported rate of 44100 or 48000 Hz, and 16–4096 callback frames.

The command does not load runtime configuration, open/start JACK, register a
port, transmit MIDI, start a synth, or produce sound. It feeds three
deterministic, distinguishable stereo sources through the production source and
master smoothing, fixed strip and linked true-peak limiter, final meter,
callback-boundary capture,
bounded SPSC ring, non-real-time 24-bit stereo WAV writer, fsync, and
no-replace publication. It paces callbacks in real time and reports callback
mean/p95/p99/maximum, limiter maximum gain reduction, writer high-water, drops,
overflows, frame count, full playback/file sample equality, and final path.

The only persistent side effect is one uniquely named
`final-mix-*.stress.wav` below `DEST`; existing paths are never replaced. An
in-progress matching `.wav.part` is owned by that invocation and remains for
honest recovery after failure. Use an explicit dedicated temporary directory
when results are disposable, and validate that exact expanded path before
removal.

This helper is intentionally separate from `recorder-stress`: raw multitrack
evidence concerns many synchronized mono stems, while final-mix evidence must
exercise the exact post-strip stereo playback/tap equivalence. Neither helper
is physical-interface, JACK scheduling, listening, or MR18 acceptance evidence.

## MASTER STRIP callback benchmark

### Invocation

```sh
shr master-strip-bench [CALLBACKS] [RATE]
```

Defaults are 20000 callbacks per profile and 48000 Hz; at least 1000 callbacks
are required and the rate must satisfy the graph's 8000–384000 Hz contract.
The optimized binary should be used for recorded evidence.

The command does not load runtime configuration, open JACK, start a synth,
transmit MIDI, pace to wall-clock audio, or write a file. It runs the same
deterministic stereo buffer through the production processor at 64 and 128
frames, first neutral and then with every optional section maximally active.
It separately times the same 128-frame interpolation work at 4× and 8×.
Results include mean, p95, p99, maximum, mean percentage of the callback
deadline, fixed processor-state bytes, and limiter-delay bytes.

This is a hardware-independent release-mode DSP comparison. It is not an xrun,
JACK scheduling, full-duplex, temperature, listening, or physical-interface
result. The owning evidence and algorithm choices are in
[Fixed stereo MASTER STRIP](MASTER_STRIP_MEASUREMENT.md).
