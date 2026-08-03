# Everyday screens

[Manual home](../MENU_MANUAL.md) · [FT2 and Projects](TRACKER_AND_PROJECTS.md) ·
[Loops and effects](LOOPS_AND_EFFECTS.md)

All values shown below are deterministic presentation data. The screens are
real SHR-DAW renders, but no instrument, MIDI take, recorder, or meter was live
while the images were made.

## Home

Home is the navigation root. Its nine labels are centered inside equal 36-cell
bars spanning zero-based columns 2–37 on the 40-column display. The block is
centered vertically, always leaving the first terminal row empty at the native
40×13 size, and scrolls safely on compact supported terminals. Home's own final
line is shown only for active owned playback/recording, a current fault, or the
reason MIDI Learn was recommended; routine selection and confirmation messages
stay off the menu.

![Home screen with FT2 selected](../images/menu/home.png)

The master rotary browses the current screen content and its press opens or
confirms the selection. On the configured MiniLab, pads do not navigate this
plain list: the first four select controller-menu pages and the other four
invoke that page's items wherever a controller strip is present.

## Presets

**Software Synths** opens Presets. Turn the main encoder, use the arrow keys,
or use the mouse wheel to choose a sound. Hold the configured encoder Shift
while turning to move between the synthv1, Yoshimi, FluidSynth, and Moj Sint
catalogs. Loading replaces the one managed software instrument; it never
layers engines.

Home keeps **MIDI Learn**, **Routing**, and **Effects** separate. Routing is the
rotary browse/edit/confirm/cancel editor; Effects is the existing Project rack.
Routing selections wrap, and merely browsing never writes configuration or
opens/transmits through a MIDI output. Its live state names the discoverable
interface separately from the configured downstream profile: a resolved
AudioBox needs no healthy suffix, while an unresolved interface gets `OFFLINE`
or `AMBIG`; a known D-50 profile does not claim device detection because DIN
supplies no presence feedback. If a
configured controller is offline, unreviewed, or has an incomplete learned
encoder, MIDI Learn is selected first and Home explains why. Keyboard arrows
and Enter remain available. A learned turn-and-click encoder is sufficient;
optional command buttons may remain unmapped.

Presets and Playback share one Software Synth sound, and leaving them keeps it
running for effects and other screens. Global panic, shutdown, replacement, or
an explicit different FT2 software route stops only that SHR-owned engine. A
genuinely new, empty, unsaved default FT2 Project adopts the current selection
on page 1 without restarting it; with no Player instrument, FT2 loads the first
available synthv1 preset. Saved or explicitly changed Projects keep their own
routes.

### OPS — browse and load

![Populated Presets screen with the OPS controller page](../images/menu/presets-ops.png)

`LOAD` starts the highlighted sound. `FIRST` and `LAST` jump to the ends of the
current engine's catalog. Keyboard PageUp/PageDown still move by ten sounds;
physical command pads deliberately do not duplicate that coarse scrolling.
On the controller, LOAD uses semantic position 6; position 5 remains available
for STOP/PANIC where that direct safety action exists.

### Change instrument host

Shift-turn the main rotary to move among synthv1, Yoshimi, FluidSynth, and Moj
Sint in either direction. Keyboard `[`/`]` and clicking the left/right half of
the Presets heading remain equivalent. Catalog changes are silent: they only
change the list and reset its selection according to the existing catalog
contract. `LOAD` is the sole managed preset start or replacement action.

The Moj Sint catalog has seven ordered starts: Full Bass, Full Lead, Full
Filter Articulation, Matched Idealized, Matched Linear Mixer, Matched Linear
Ladder, and Matched No Drift or Feedback. Selecting one only browses; `LOAD`
starts it.

### SYS — safety and help

![Populated Presets screen with the SYS controller page](../images/menu/presets-sys.png)

`PANIC` stops owned playback and notes. `HELP` opens the local help reader.
`EXIT` returns to Home. MIDI never quits the application; quitting remains
computer-keyboard-only from Home.

## Playback

Playback appears after a sound is loaded. At native 40×13 the body shows the
held chord and notes, each note's decimal MIDI strike velocity directly beneath
it, and the selected backend's 12 mapped controls. Taller terminals use spare space for a
continuous two-row keyboard state. The
aligned velocity row helps with gentle/strong control, consistent chord
attacks, and bass-plus-chord balance. It is MIDI input data, not an audio
loudness meter; controller and instrument response determine the audible
result.
On the keyboard, red white-key areas are held natural notes and red upper `└`
marks are held sharps. Parameter colors are relative to the loaded preset:
green below the original value, bright yellow near it, and red above it. The
Moj Sint uses EVOLVE through SPACE in the first two rows and ADSR in the third;
synthv1 retains its own names and indices. The main encoder press resets only
these mapped controls and re-arms pickup; it does
not restart the synth.

### PLAY — capture a MIDI take

![Populated Playback screen with the PLAY controller page](../images/menu/playback-play.png)

`PLAY` plays or stops the captured take. With no take, it still sends a fresh
Start to the configured controller clock so the MiniLab 3 External-Sync
arpeggiator works with live keys. `RECORD` starts the same transport before
free-time MIDI capture. `STOP` sends controller Stop and All Notes Off without
unloading the sound. `TAP` updates the current Pattern/controller tempo; it does
not start transport, so PLAY remains the deliberate start gesture.

### SOUND — reset, save sound, scale filter, and sounds

![Populated Playback screen with the SOUND controller page](../images/menu/playback-sound.png)

`RESET` restores the 12 mapped parameters in place and re-arms hardware pickup.
`SAVE` opens `OVERWRITE`, `SAVE NEW`, and `CANCEL`. Factory and system sounds
are read-only, so their Overwrite row clearly saves a new private `User NNN`
sound instead. Save New numbers sounds independently for synthv1, Moj Sint
Model D, and Moj Sint Six-Op PM. A successful save becomes the current sound
and the new RESET baseline immediately; it does not restart the engine, release
held notes, or change the controls. Presets refreshes immediately, and a Moj
sound remains inside its Model D or Six-Op PM hierarchy in FT2 ROUTE. Cancel
closes only the overlay and preserves cursor/list state, live values, and held
notes. A failed save preserves that state and any prior user file while keeping
the overlay open for retry. Other backends show SAVE UNAVAILABLE. `N00B`
toggles the optional scale filter without leaving Playback or hiding any normal
content.
`SOUNDS` returns directly to Presets, where `LOAD` starts the highlighted
sound. While N00B is on, a single compact `SCALE` rotary appears below the 12
controls; turning the master encoder cycles every chromatic root in major and
natural minor. Pressing N00B again removes only that control and restores
chromatic play.

### SYS — safety, effects, help, and return

![Populated Playback screen with the SYS controller page](../images/menu/playback-sys.png)

`PANIC` performs the global owned stop. `FX` opens the current Project rack
without restarting the sound. `HELP` opens help and returns here afterward.
`EXIT` returns to Presets, then Presets `EXIT` returns Home.

### N00B-on Playback pages

N00B changes only the scale filter and its compact rotary; the three Playback
controller pages keep the same actions and ordering.

![Playback PLAY page with N00B enabled](../images/menu/playback-noob-play.png)

![Playback SOUND page with N00B enabled](../images/menu/playback-noob-sound.png)

![Playback SYS page with N00B enabled](../images/menu/playback-noob-sys.png)

## Ideas

Ideas are timestamped or numbered free-time MIDI takes. A synthv1 Idea carries
a private preset snapshot; external-engine Ideas retain their sound identity
instead. Turn the encoder to select an entry.

### PLAY — inspect, play, record, or delete

![Populated Ideas screen with the PLAY controller page](../images/menu/ideas-play.png)

`INSPECT` shows the Idea's sound and recording metadata. `PLAY` plays or stops
the take. `RECORD` starts or stops capture. `DELETE` requires a repeated
confirmation and only removes the selected Idea.

### FILE — load or save an Idea

![Populated Ideas screen with the FILE controller page](../images/menu/ideas-file.png)

`LOAD` restores the selected Idea, asking for confirmation before replacing an
active sound. The old engine remains usable if validation, start, or activation
fails, and the same Idea stays selected for another LOAD attempt. `SAVE`
publishes a new non-overwriting Idea. `FIRST` and `LAST` select the list
boundaries.

### SYS — safety, help, and return

![Populated Ideas screen with the SYS controller page](../images/menu/ideas-sys.png)

`PANIC` stops owned notes and transports. `HELP` opens contextual help. `EXIT`
returns Home.

## MIDI Learn

![Non-audible MIDI Learn screen waiting for a master-encoder gesture](../images/menu/midi-learn.png)

MIDI Learn isolates controller messages from instruments while it captures the
master rotary's counter-clockwise turn, clockwise turn, and click, followed by
optional absolute controls and command buttons. Release each opening control
as prompted. The review step writes a private controller profile only after
confirmation; Back cancels without saving. A learned master rotary is enough
to browse and confirm even when optional buttons are skipped.

## Help

Help is always available locally with `?` or F1, even if the optional temporary
LAN page cannot bind. Turn the encoder one rendered row at a time. On eight- or
five-button layouts, encoder press follows a selected section link.
Opening Help preserves the exact caller, controller page, FT2 mode/location,
cursor, editor draft, and active transport. EXIT restores that exact context.
If the LAN port cannot be acquired, only the local reader opens and reports the
failure; no URL is advertised.

### OPS — read and follow links

![Populated Help screen with the OPS controller page](../images/menu/help-ops.png)

`OPEN` follows the highlighted internal link and is the required link action on
a four-button layout. `TOP` returns to the beginning. Keyboard
PageUp/PageDown retain page scrolling; physical pads do not.

### SYS — safety and return

![Populated Help screen with the SYS controller page](../images/menu/help-sys.png)

`PANIC` remains available while reading. `EXIT` returns to the exact screen
that opened Help.

## Audio recorder

The recorder captures any deliberately configured set of JACK source ports as
one synchronized take with a 24-bit mono WAV per input and a shared manifest.
The compact list shows named tracks as ready or missing. Its body shows armed
count, elapsed time, sample rate, writer high-water mark, drop/xrun counts,
final path, or the failure reason; the one shared final status row remains
separate below the two controller rows. It never starts or restarts JACK.

### RECORD — arm and record

![Populated Audio recorder screen with the RECORD controller page](../images/menu/audio-recorder-record.png)

`RECORD` starts all armed tracks at one callback boundary. `ARM` toggles the
selected track. `LEVELS` opens the separate 18-channel Levels overview. An
armed missing source prevents a take from starting.

### TRACK — choose the inputs

![Populated Audio recorder screen with the TRACK controller page](../images/menu/audio-recorder-track.png)

`PREV` and `NEXT` select a track. `SOURCE` cycles deliberately through the
currently discovered sources (and blank); `NAME KB` opens computer-keyboard
text entry for the musician-facing
label. Runtime absence never overwrites a remembered source.

### SETUP — prepare tracks

![Populated Audio recorder screen with the SETUP controller page](../images/menu/audio-recorder-setup.png)

`ALL` arms every resolved track, `NONE` disarms everything, and `REFRESH`
discovers current JACK audio sources without changing assignments.

### SYS — finalize safely

![Populated Audio recorder screen with the SYS controller page](../images/menu/audio-recorder-sys.png)

`PANIC` stops owned activity. `HELP` opens help. `EXIT` returns Home without
silently changing recorder state.

## 18-channel input Levels

This native overview keeps exactly 18 recording inputs visible as three groups
of six. It is not a route/setup inspector, the Synth/Loop/Input final-bus MTR,
Live Patterns, Loop Mix, or a full mixer strip.

![Native 18-channel Levels overview](../images/menu/input-monitor-take.png)

Rows 2–10 are nine circular-LED thresholds at −48, −36, −30, −24, −18, −12,
−6, −3, and −1 dBFS. Green covers −48 through −18, yellow −12 through −3,
and red −1. Smoothed RMS fills the column; a brighter LED of the same colour
holds sample peak and then decays. Normal silence stays dark gray. `M` means a
configured input is missing, `F` is a meter/callback fault, and held `C` marks
genuine clipping.

At exact 40×13 the meter columns occupy only columns 1–20 and the visible
command page only columns 21–40. The two ordinary controller rows are omitted;
row 13 remains the unchanged shared status renderer. Below native size a
compact ordinary screen replaces the overview instead of cropping or banking
channels.

### TAKE — setup and take control

![Levels TAKE command page](../images/menu/input-monitor-take.png)

`SETUP` returns to Audio Recorder naming/routing. `RECORD`, literal `STOP`, and
`RESET` start a take, stop it, or clear presentation holds. Keyboard `u`, `r`,
`s`, and `x` are equivalent.

### CHANNEL — select without hiding

![Levels CHANNEL command page](../images/menu/input-monitor-channel.png)

Encoder turn, Left/Right, `j`/`k`, `PREV`/`NEXT`, or a pointer click selects a
channel without changing the 18 visible meters. Encoder click, Enter, Space,
or `ARM` toggles only that channel; `REFRESH` resolves remembered exact sources.

### SYS — literal safety

![Levels SYS command page](../images/menu/input-monitor-sys.png)

`STOP`, `PANIC`, `HELP`, and `EXIT` remain visible; literal `STOP` is also on
TAKE. Panic also closes the recorder-owned meter client and follows global All
Notes Off. Monitoring and recording clients are mutually exclusive, and
shutdown closes either one.

### Populated level and state examples

![Quiet but active input](../images/menu/input-monitor-quiet.png)
![Nominal recording input](../images/shr-daw-input-monitor.png)
![Yellow, red, and clipped peaks](../images/menu/input-monitor-peaks.png)
![Missing and faulted inputs](../images/menu/input-monitor-fault.png)
![First channel selected](../images/menu/input-monitor-selected-first.png)
![Middle channel selected](../images/menu/input-monitor-selected-middle.png)
![Last channel selected](../images/menu/input-monitor-selected-last.png)
![Record transport status](../images/menu/input-monitor-record.png)
![Stop transport status](../images/menu/input-monitor-stop.png)
![Compact fallback](../images/menu/input-monitor-compact.png)

Selection and command page survive ordinary navigation and reset with a new or
loaded Project. The right side is operational command space, not a numeric
RMS/peak, route, or metadata detail panel.

## Performance meter

With the final bus active, MTR selects Synth, Loop, exact Input, or SHR Drums,
controls its bounded level and one context-specific source action, shows
readiness and the post-limiter
true-peak final meter, opens the fixed MASTER STRIP, and controls final stereo
recording. With the graph inactive it
keeps the CPU/legacy meter presentation without pretending that direct output
is being measured. CPU is whole-core `/proc/stat` activity, not callback timing
or xruns.

Every horizontal meter cell is a circular `●` LED. Unlit cells are dark gray;
safe active cells use one green; yellow and red appear only at their active
thresholds; a held peak is a brighter circle in the applicable threshold
colour. No square bar or line-marker glyph represents level or peak.

### OPS — source and level

![Populated performance meter with the OPS controller page](../images/menu/performance-meter-ops.png)

`SOURCE-`/`SOURCE+` choose Synth, Loop, Input, or Drums. `LEVEL-`/`LEVEL+` change only
that source's bounded final-bus level.

### MIX — mute, record, and holds

![Populated performance meter with the MIX controller page](../images/menu/performance-meter-mix.png)

The first position is `MUTE` for Synth, Loop, and Drums. For Input it is
`MON ON` while monitoring is off and `MON OFF` while monitoring is on; no
second Input mute is shown. Controller, pointer, and keyboard `m` all invoke
this same visible action. MON ON starts the owned final bus if needed without
starting an optional source. `RECORD` toggles the callback-boundary final
stereo recorder. `RESET` clears presentation peak/clip holds; it does not reset
effects, CPU state, or transport.

### NAV — FX master overlay

![Populated performance meter with the NAV controller page](../images/menu/performance-meter-nav.png)

`FX` opens the same master-overlay layer used by FT2. Choose SOURCE, AUX 1,
AUX 2, or MASTER, then click/Enter to open that rack.

![Effects-routing overlay over the performance meter](../images/menu/overlay-performance-fx.png)

The MTR remains underneath; pressing the highlighted `FX` again closes the
overlay without changing audio or Project state.

### SYS — safety and return

![Populated performance meter with the SYS controller page](../images/menu/performance-meter-sys.png)

`PANIC` remains available. `HELP` opens the explanation of meter scope. `EXIT`
returns to Home. The screenshot says `Presentation · no live audio` because
its meter values are seeded for documentation rather than measured.

## Routing

Routing is a transactional editor for controller and performance inputs,
controller role, external MIDI/profile, controller clock, and the stereo audio
destination. Turn to browse rows and press to start an isolated draft. Turn to
change that field, then press to validate, save, and activate it; Back cancels
the draft without writing. Audio and clock changes that cannot be activated
live are clearly marked for the next managed-engine start.

### EDIT — browse or change one route

![Routing editor with the EDIT controller page](../images/menu/routing-edit.png)

`PREV` and `NEXT` browse the same wrapping row list as the master rotary.
`EDIT/OK` starts or confirms the selected field. `CANCEL` abandons the active
field, or returns Home when no draft is active.

### SYS — safety and return

![Routing editor with the SYS controller page](../images/menu/routing-sys.png)

`PANIC` stops owned playback and sends All Notes Off. `HELP` opens the local
reference. `EXIT` cancels an active draft first, otherwise returning Home.
