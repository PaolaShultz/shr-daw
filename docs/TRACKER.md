# Tracker guide

The FT2 screen is a vertical MIDI pattern sequencer. Its quick, top-to-bottom
editing style is inspired by FastTracker II, but SHR-DAW is not an FT2 clone.
It does not use FT2 code or read XM files.

This guide owns FT2 behavior and Project editing. Use the [screen and menu
manual](MENU_MANUAL.md) for button-by-button screenshots and [Configuration
and routing](CONFIGURATION.md) for stored fields and machine defaults.

On the native 40×13 display, the FT2 body and its own compact page/lane footer
end above the two controller rows. Row 13 is the shared working-screen status
row: steady green `>` for play, steady white `■` for stop, steady white `‖` for
pause, or a `●` that pulses only between red and bright red for record. The
tracker header does not repeat `PLY` or `REC` state beside the Project title.

## Modes

The normal FT2 screen has **PLAY**, real-time **REC**, and detailed
**EDIT** modes. **N00B is a separate on/off filter** that can remain enabled in
all three. It keeps the selected melodic page as the instrument and filters
input through the Project-wide song key: a chromatic tonic plus major or
natural-minor scale. The key remains saved and available to SHR Drums when
N00B is off. An
in-scale key keeps its original pitch; an out-of-scale key is consumed and
stays silent. N00B never quantizes a rejected key to a different note.
Each entry to the main FT2 screen opens controller-menu page 1, **PLAY**, where
the **PLAY** and **RECORD** buttons are immediately available.
PLAY, REC, and EDIT are mutually exclusive. Selecting one ends the other active
mode first; pressing the active PLAY, REC, or EDIT control stops or leaves it.

In Play, N00B changes only what is heard. In REC and EDIT, allowed notes can be
written normally while rejected notes remain silent and unwritten. Turning the
filter on or off never changes Play/REC/EDIT. N00B is refused on a percussion
page; moving onto Drums turns only the filter off and preserves the current
mode.

The N00B button stays in the same FT2 **SYS** position in Play, REC, and EDIT.
Each press toggles the Player-selected scale directly without opening another
screen or changing existing cells. Command pads and their releases remain
consumed.

On the main tracker grid, the physical main rotary always moves rows. Holding
the configured encoder Shift button while turning selects the previous or next
column, continuing through page boundaries from Software Synth to MIDI, Drums,
and later pages. The selected column has a subtle dark full-column shade; the
yellow cell cursor and row/warning emphasis remain stronger. Keyboard arrows
retain row navigation in every mode. Shift-rotary column selection does not
move the row, playhead, Arrangement Step, or transport, and is ignored while a
recorded note is still held.

## Projects, patterns, and arrangement

An SHR-DAW Project contains FT2 Patterns and an FT2 Arrangement. An FT2 Pattern
is a self-contained tracker pattern. The FT2 Arrangement is the ordered chain
of Arrangement Steps; each step references a pattern ID. Repeating a step reuses
the same pattern until you explicitly clone or paste a new pattern.

Each FT2 Pattern owns its own rows, meter, hundredths-BPM master tempo, pages, page targets,
per-column MIDI channels/banks/programs, velocity defaults, mutes, percussion
settings, note-entry mode/anchor, drum classification overrides, lane settings,
and cell data. A new Project starts with one pattern
whose FT2 workspace exposes four musician-facing pages:

1. `Software Synth`, a four-track page using the first available synthv1 preset;
2. `MIDI`, a four-track page using the configured external output, MIDI channel
   1, and program 1;
3. `Drums`, a four-track page using the installed SHR Drums starter kit when
   available, otherwise the discovered FluidSynth General MIDI compatibility
   kit, with the existing GM percussion-note mapping;
4. `Loop Mix`, that Pattern's four-slot decoded-WAV source.

Loop Mix is a page in the musician-facing FT2 workflow, not four empty
MIDI lanes. **SELECT** → **PAGE** opens it directly, so a new
Project does not require adding or naming a page before importing a WAV.
The blank Pattern, unloaded loop state, loop inbox, and startup MIDI-output
snapshot are initialized when SHR-DAW starts. Entering a genuinely new, empty,
unsaved FT2 Project loads its page 1 software instrument immediately. If Player
already owns a loaded instrument, page 1 adopts that exact instrument and the
same managed engine session becomes FT2-owned without a restart. Otherwise
page 1 loads the first available synthv1 preset.

Loop Mix settings are saved with their Pattern. Launch, stop, mute, queued
commands, playback position, faults, and Live Pattern shaping remain runtime
performance state; none of them creates MIDI lanes or Arrangement Steps.

Channels and programs are zero-based in MIDI bytes and in the in-memory model.
Every musician-facing screen shows channels 1–16 and programs 1–128.

Each page keeps one MIDI target plus four independent column channel, bank, and
master-program setups. It also keeps velocity, mute, percussion, optional
device-profile metadata, and lane settings. A software target stores its engine
and that engine's stable instrument identity in the Pattern. Moj Sint stable
identities are model-qualified. When page 1 is
part of a genuinely new, empty, unsaved default Project, entering FT2 may
replace its factory route with the currently loaded Player engine/instrument.
A loaded/saved Project or an unsaved Project with any explicit change is never
retargeted, even when its Pattern has no notes.
For external MIDI, columns may share a destination/channel only when their
master bank and program match, because MIDI program selection is channel-wide.
A software route owns its preset instead of using those external master-program
fields.

Pages play together, and page count is not an instrument or polyphony limit.
Any number of pages and columns may share the exact same software
route/channel. Their four lanes remain independent: two shared pages provide
eight simultaneous tracker lanes, and further pages extend that pool within
the Project and synth voice limits. The same route may also be used on several
channels.

SHR still owns one synth host at a time. synthv1, Yoshimi, Moj Sint, and SHR
Sampler expose one current instrument, while one owned FluidSynth process is multitimbral: each distinct
SoundFont preset/channel pair is selected once without changing other
channels. For example, bass on channel 1, keys on 2, pad on 3, and a drum kit
on 10 play together through the existing stereo synth output. Channel 10 is
the normal percussion-page default, not a reservation; an explicit Project may
route it differently. SHR Sampler packages are preloaded before its JACK/ALSA
host starts; it does not auto-connect or share a process with another backend.

Drums pages can explicitly store an SHR Drums kit, a configured/exact external
MIDI output, or a FluidSynth General MIDI compatibility route. An unavailable
saved target remains visibly offline and silent; it never falls back to another
route. SHR Drums is an in-process stereo source and does not consume the one
managed melodic-synth slot, so it can play beside synthv1, Yoshimi, FluidSynth,
Moj Sint, or SHR Sampler. Switching targets sends All Notes Off and immediate
drum chokes.
Loaded Projects keep their saved routes and channels, and loading a reusable
drum pattern copies cells only.

The complete musician-facing comparison, including Moj model controls and
saves, Sampler package validation, installed drum kits, ownership, and failure
recovery, is in
[SHR-DAW instruments and drums](INSTRUMENTS_AND_DRUMS.md).

Two different FluidSynth presets cannot share one channel in the current
playback loop. SHR selects stable channel parts before scheduling, and note
tails plus Pattern/Arrangement loop boundaries are not treated as safe dynamic
preset-change points. Consequently even apparently non-overlapping uses on one
channel are refused for now with a channel-conflict error. Identical
route/channel sharing is safe and is never that conflict. FluidSynth plus
another managed backend is also refused because that would require a second managed
backend; external MIDI pages and the WAV loop remain independent.

Pressing Play on such a mixed Project opens an explicit recovery choice instead
of leaving only a status-line refusal. **NO** keeps every route and cell
unchanged. **YES** opens a list of matching FluidSynth sounds for each
incompatible page. Moving through the list is silent; **PREVIEW** performs one
short deliberate audition, and selecting a sound applies that page before Play
is retried. Back during the sequence restores the complete pre-remap Project.
This conversion is never performed merely because Play was pressed.

Computer-keyboard notes and ordinary incoming musical MIDI audition the
selected page's target, channel, program, and drum mapping throughout the FT2
workspace. Shift-rotary column navigation preserves already sounding notes on
their original routes while later notes start from the newly selected column.
Explicit page/track route, preset, channel, program, or destination changes
still end notes on the old route. The FX rack/editor is an FT2 child: live input
and the owned synth stay active, and Back returns to its FT2 caller. Leaving
top-level FT2 for an unrelated workspace ends notes and unloads its owned synth.

`AUTO · machine default` is a real portable target. Its saved channel, bank,
program, and setup fields are blank; at playback the machine's configured
melody/percussion channels and available default destination are used. `AUTO`
does not mean channel 1, channel zero, muted, or disabled. Choose an explicit
target only when a song intentionally belongs to particular hardware.

Use FT2 **SELECT** → **PAGE** to browse every page without leaving the Pattern
or changing its selected column. Its final row opens the full **TRACKS** screen. There you can add or
select a page, choose a column, set its target, channel, bank, and program, and
open **SYS** → **ENTRY** to choose that page's note-entry layout.
**DONE** validates shared-channel compatibility and keeps the changes. Internal
routes use `TARGET → ENGINE → INSTR`; Moj Sint inserts its explicit `MODEL →
PATCH` hierarchy after `ENGINE`; external routes use
`TARGET → MIDI OUT → CH → INSTR/PROG`. **SYS**
→ **EXIT** restores the Project as it was before TRACKS opened. A disconnected
saved target is marked `OFFLINE` (or `AMBIG` for duplicate stable identities);
its exact route, notes, raw channels 1–16, and programs 0–127 are not changed.
EXIT from a nested Tracks field restores that field's complete original route
while retaining unrelated draft edits.

For a quick routing change, **SELECT** → **ROUTE** opens a centered overlay over
FT2. It shows target type, software engine/instrument or MIDI output, optional
device profile, plus all four columns' channel, bank, program/instrument name,
and interface availability. With an SHR Drums target, the **KIT** field cycles
the installed drum sets; there is no redundant engine field. Big Rock is the
fresh-Project default when installed, while other kits remain explicit
choices. Applying a different kit must start it before the
route change completes and resets the old kit's tuning overrides; the Project
key and drum effects remain unchanged. A failed load restores the previous kit
and keeps the editor open with the failure visible.

For Moj Sint, **ENGINE** stays `Moj Sint`, **MODEL** cycles Model D, Six-Op PM,
and Strange Oscillator, and **PATCH** stays inside the selected model.
Changing the model selects its first available patch; changing patches never
crosses the model boundary. These live field changes remain inside the same
Route Apply/Cancel transaction.

Turn and click/Enter to activate a field. Turning the active field validates
and applies each choice to the Project and live route at once, so an available
instrument or kit can be auditioned without leaving the overlay. A failed
choice restores the previous field value and route. Click/Enter keeps the
current field value; Back/Esc restores the value from before that field was
opened.

**APPLY ROUTING** keeps the live result. **CANCEL** or Back from the main list
restores the complete route snapshot from when ROUTE opened. The contextual
**ROUTE** page puts **APPLY** at physical position 5 and **CANCEL** at position
8; those controller buttons, mouse targets, and keyboard `A`/`C` share the same
actions even while the field list scrolls. At 40×13 the bordered outer window
is 38×9 at `(1,1)`, its usable inner area is 36×7 at `(2,2)`, rows 11 and 12 are
the controller rows, and row 13 remains the shared status row.

## Step editing

Step entry accepts notes and chords from any configured musical input.
**TRACKS** → **SYS** → **ENTRY** selects one persisted layout for each page:

- **Manual** is the backward-compatible default. A note starts in the selected
  column and a chord continues through later columns.
- **One column** stores every note in the chosen C1–C4 anchor. It is deliberately
  monophonic: a new note interrupts the earlier note in that lane, and a chord
  collapses in deterministic pitch order to its final note. The selected cursor
  does not move to the anchor.
- **Drum auto** allocates each simultaneous percussion group across the current
  page's four ordinary lanes. Kick and snare share a compact primary lane when
  they alternate, while simultaneous hits spill into distinct safe lanes.
  Toms, fills, hats, cymbals, and other percussion reuse established lanes when
  safe and never overwrite an existing target-row cell.

Changing layout affects future entry and recording only; it never rearranges
existing Pattern data or changes column routing. The compact layout label and
One-column anchor appear in the FT2 footer and Tracks screen. Legacy ordinary
pages load as Manual; legacy pages with the persisted percussion flag retain
their prior automatic drum layout.

The same Tracks **ENTRY** list stores **NOTE OFF ON/OFF** per page. It controls
whether future Edit and Record input writes automatic release cells. Melodic
pages default to ON; one-shot percussion pages default to OFF. Percussion
playback never generates a release from the Project gate, a retrigger, a later
same-lane hit, or the Arrangement boundary. A drum voice rings until an
explicit OFF/CUT, a kit choke, or Stop, Panic, mute, route-change, or shutdown
cleanup catches it. The setting never removes an existing explicit OFF cell
and does not disable those deliberate releases.

**ADD** opens an overlay for every persistent advance from 0 through 32 rows
for note/chord entry, blank, erase, and note-off; 0 keeps the current row. The
FT2 title shows `EDIT +n`.
A computer keyboard can enter notes with `Z S X D C V G B H N J M`.
Those lowercase letter keys remain musical in REC as well as Play/Edit; in REC,
use uppercase `S` or Space for Stop and Esc or uppercase `B` for Back so the
`S` and `B` note keys are never shadowed.

**LENGTH** is a separate Edit overlay. It chooses `1/1`, `1/2`, `1/4`,
`1/8`, `1/16`, `1/32`, `1/64`, or `1/128` for melodic entries and defaults to `1/16`. The
selected duration writes the existing gate/explicit note-off representation;
it does not change the independent **ADD** cursor advance or create a second
timing system.

Edit has four controller pages: **EDIT**, **SET**, **SIZE**, and **SYS**.
Normal FT2 **SELECT** remains one Exit away and still owns PAGE, PATTERN, SONG,
and ROUTE. **SIZE** changes the current Pattern across every page and lane:

- **HALF** accepts an even length of at least two rows. A populated Pattern asks
  **KEEP TOP**, **KEEP BOTTOM**, or **CANCEL** and reports the cells each half
  would discard. An empty Pattern keeps the top half directly.
- **ROW-** removes the cursor row and shifts later rows up. A populated row
  requires one confirmation with its exact discarded-cell count; a one-row
  Pattern is unchanged.
- **ROW+** inserts one empty row after the cursor and shifts later rows down,
  up to the 256-row limit.
- **DOUBLE** works through 128 rows. A populated Pattern asks **COPY NOTES**,
  **EMPTY HALF**, or **CANCEL**; an empty Pattern appends empty rows directly.

Successful SIZE changes stop FT2 Play/REC, keep the selected page, lane, and
column, preserve Pattern setup and Arrangement references, and then mark the
Project dirty. Refusal, failure, and Cancel leave the Pattern unchanged.

Drum auto also checks sounding lane state. An unrelated long-tail cymbal makes
its lane unavailable, so later kick, snare, tom, clap, hat, or ornament hits
spill elsewhere. Another cymbal does the same when capacity permits. A
same-note retrigger or matching explicit choke group may reuse that lane; the
new same-lane note then performs the tracker’s ordinary interruption. General
MIDI cymbals and hi-hat group 1 are the defaults. Unknown notes predictably
fall back to short `other percussion`, never cymbal. A page can persist non-GM
role/choke overrides. If a whole group cannot fit, `DRUM LANES FULL` leaves
the Pattern unchanged; Drum auto does not create a page or drop a hit silently.

This protection is placement only. Playback never reallocates a note or gives
cymbals special ownership: any note already stored in the same lane interrupts
the previous lane note. Manual entry into a cymbal lane and every One-column
entry therefore keep normal monophonic interruption.

The editor can add a note, note-off, or blank step. It can also change the page
program and pattern master tempo, mute a lane, and move through rows, lanes,
pages, and arrangement steps.

Pressing **PLAY** on the main FT2 screen starts the first pass at the selected
row. When playback reaches the end, subsequent passes restart at row 1 of that
Pattern rather than at the original play cursor.

Tempo commands inside cells still work inside the current pattern. When
playback enters the next arrangement step, tempo starts again from that
referenced pattern's master tempo. The arrangement boundary itself does not
send note-off for active lanes. Melodic lanes are released by their own
gate/cut/note-off, by a later same-lane note, or by stop/panic/mute cleanup.
Percussion lanes release only from an explicit OFF/CUT, a kit choke, or
deliberate cleanup.

## Cell editing

**CELL EDIT** changes one cell as a draft. **CONFIRM** saves the draft. **EXIT**
or cancel restores the original cell.

A cell contains:

- a blank, MIDI note 0–127, or note-off;
- an inherited gate or a gate from 1–100% of one row;
- inherited velocity or MIDI velocity 0–127;
- inherited program or a MIDI program override stored as 0–127 and shown as
  instrument/program 1–128;
- one optional command: cut or delay tick 0–15, retrigger count 1–8, or decimal
  tempo 20.00–300.00 BPM.

The grid shows `C` for cut, `D` for delay, `R` for retrigger, and `T` for tempo.
One cell cannot contain more than one command. Velocity, program, gate, and
retrigger need a note-on in a newly confirmed edit. Invalid combinations stay
in the draft and show an error.

Choosing **PROGRAM** opens a full-height sound browser. A matching MIDI device
profile adds the instrument's slot labels and sound names. Without a profile,
all MIDI programs 1–128 remain available. Performance notes audition the
draft sound on that page's exact target and selected-column channel. Confirm
keeps the cell override without changing the column master; cancel restores
the previous value and selection.

In the ROUTE overlay, confirming an external column's PROGRAM sends that
column's bank/program selection immediately, so a connected hardware
instrument changes sound for stopped FT2 free play as the field choice is
applied. APPLY ROUTING keeps the route in the Pattern and sends the selected
column's program again. The Tracks screen's DONE action follows the same
selected-column rule. These actions do not wait for Play or for the next note.

## Real-time recording

From stopped transport, **REC** starts the selected Pattern from row 1 and
loops it. Pressing **REC** during Play punches into the current Arrangement
position without replacing that schedule; punch-out returns to Play. Between
notes, Shift plus the main rotary may select another column or page without
leaving REC, and later notes use that selected page. While one or more recorded
notes are held, Shift-rotary turns are ignored rather than queued; movement
resumes only after every matching Note Off.
Played notes use the active page's Manual, One-column, or Drum-auto allocator
and are quantized to Pattern rows. Events quantized to one row occupy distinct
Drum-auto lanes. REC ignores the Edit note-length setting: each note-on records
its exact Pattern/page/lane owner. With automatic Note Off enabled, its matching
release writes the quantized note-off in that lane even after cursor movement,
a Pattern loop, or an Arrangement boundary. With it disabled, release still
clears the live ownership without writing an OFF cell. Repeated identical input
notes keep independent owners and cannot release one another early. Newly captured notes and releases
are published to the next stopped-record loop without restarting its current
cycle. Each assigned lane
auditions through that column's channel and the selected page's exact
Pattern-owned software or hardware instrument. It does not leak into an
unrelated standalone Player instrument.
The source port, not a special MIDI channel, separates a performance keyboard
from a control-only surface. A combined device retains channel-qualified
controller mappings.

Real-time recording accepts the selected page when its exact target is online,
including the factory Software Synth page. An offline or missing target refuses
**REC** instead of substituting another destination. Stop, mute, panic, target
failure, route interruption, Project replacement, and exit clear every recorded
input owner and release auditioned notes. A Drum-auto capacity fault keeps
recording and transport responsive, reports `DRUM LANES FULL`, and leaves
existing cells unchanged.

## Live audio-level mixer

The FT2 mixer is available without leaving Play, REC, or Edit. In normal Play,
open `SOUND` → `MIX`; REC keeps the same `MIX` action in its MODE page. In any
of the three modes, Shift plus main-encoder click opens the same panel, which
is the controller path from Edit without displacing its four contextual command
pages. The panel snapshots the current Arrangement step/Pattern, cursor row,
tracker page, lane/column, mode, and controller-menu page. Main-encoder click,
Back, or SYS `EXIT` returns to that exact editing location.

The panel shows at most twelve current-Pattern strips in one 4×3 grid. A strip
is not a MIDI velocity or CC-volume control: it points directly to one existing
final-bus audio owner. synthv1, Yoshimi, FluidSynth, Moj Sint, and SHR Sampler
pages use `SYN`; SHR Drums pages use `DRM`; attached Pattern Loop Mix appears as `LOP`
when the twelve-strip cap has room; and an external-MIDI page uses `INP` only
when an exact two-port SHR input return is configured. Otherwise it says `NO
RETURN` and has no gain or VU. MTR owns whether that pair is stereo or dual
mono and the two dual-mono pans; FT2's linked `INP` strips retain the one
shared owner gain. The configured Input remains marked `M` while
software monitoring is off; opening the mixer does not silently defeat the
direct/software doubled-monitoring guard.

The heading identifies the source Pattern and current POT bank. Configured
physical POT positions are ranked in order, so one through twelve pots are
usable. Twelve map directly to strips 1–12. Fewer pots map to the current bank;
turn the main encoder or use `BANK-`/`BANK+` to change banks. Each strip shows
its Pattern page/name, owner, signed dB gain, pickup `↑`/`↓`/`↕`/`✓`, five-LED
VU, and `L2` or higher when multiple strips share that owner. The VU uses the
same circular LED language as the rest of SHR-DAW.

Linked strips read and write one canonical owner gain and one post-gain owner
meter. Moving either is audible immediately through the existing 10 ms final-
bus ramp. A linked change re-arms the other assigned non-motorized pots, so a
physical position can never jump the shared gain. Changing banks also re-arms
pickup.

In Play and REC, the mixer follows the Pattern that the sequencer is actually
sounding, including Arrangement and Live Pattern changes, even while another
Pattern remains the saved editor location. In Edit, it follows the Pattern
being edited. Opening activates the owned final bus and keeps live Edit
audition on the same route: owner gain, Project processing/master strip,
master volume, limiter/final meter, recorder tap, and output.

## Live Patterns

Open **TOOLS** → **LIVE** to browse four launchable Patterns at a time without
changing playback. Selection, current playback, and the replaceable queue have
distinct screen states. Launches can use the next Pattern boundary or the next
complete bar; immediate launch is a separate deliberate action. Queue cancel,
current-Pattern retrigger, literal Stop, and Panic remain directly reachable.

Successful activations can be captured into a temporary list, then explicitly
appended to or used to replace the Arrangement. Cancelling leaves the original
Arrangement unchanged. The four lanes on the selected Pattern page also have
transient live mute, velocity, gate, and transpose shaping which resets only
when another Project loads or is created.

The full keyboard/controller workflow, exact held-note transfer, failure
behavior, and capture confirmation contract are in [Live
performance](LIVE_PERFORMANCE.md#live-patterns).

## WAV Loop Mix

Open **TOOLS** → **LOOP** for four independent private mono/stereo WAV slots.
Each stores its filename, source BPM, half/normal/double interpretation,
non-destructive start and length, whole-bar offset, level, and bipolar filter.
The selected slot is not launched by browsing it.

WAV has no dependable standard BPM metadata, so import and **AUTO** estimate
pulse spacing when useful and otherwise use duration plus the current tempo to
choose a whole-bar length. **BPM-**/**BPM+** and **BPM x** correct source
interpretation; **UNIT** changes cut adjustment between beats and bars.
**ALIGN** re-runs bounded offline analysis or moves placement by a whole bar.

Each slot queues launch/stop for the next Pattern-local bar. A later command
replaces the earlier one, and Cancel removes it. All active slots must match
their Pattern's interpreted tempo and JACK's sample rate; there is no
time-stretching or callback resampling. Different whole-bar lengths stay
phase-aligned under that Pattern's tempo and meter. A missing, corrupt,
incompatible, late, or failed slot is isolated while healthy slots and MIDI
continue.

The screen always edits the Pattern under the FT2 cursor, but browsing another
Pattern does not change the sounding Pattern. At an Arrangement or Live Pattern
boundary, the outgoing slots stop and the incoming Pattern's prepared slots
start with MIDI. Every Arrangement step is a fresh instance: a repeated
reference restarts phase at Pattern-local beat zero, while playback begun at a
middle row seeks from that local row without adding earlier Pattern durations.

**LIBRARY** opens the private browser for the selected slot. Browsing is
silent; preview is explicit and stops on selection change, Stop, Back, browser
close, or leave. `INBOX` imports; `PRIVATE`, `CURRENT`, and `SAVED` attach an
existing private file. **REMOVE** requires confirmation, clears only the
selected Pattern slot, and keeps the private WAV.

See [Live performance](LIVE_PERFORMANCE.md#loop-mix) for level/filter controls,
bar scheduling, routing, realtime limits, and the deliberately unsupported DJ
features.

## Copy and Paste

Pattern copy stores the complete current FT2 Pattern, including rows, pages,
routes, channels, programs, mutes, meter, tempo, and all four Loop Mix
references/settings. Paste-new and Clone make independent Pattern copies;
paste-over replaces the destination Pattern's loops only after its existing
confirmation. Repeated Arrangement references do not clone: editing the shared
Pattern changes every step that references it.

The FT2 tools clipboard can copy and paste one lane/column or one full page
block. Lane and page paste keep note, velocity, program, gate, and command
cells. When source and destination row counts differ, only overlapping rows are
pasted and the status line reports truncation. Page paste targets the selected
destination page; missing destinations are not created implicitly.

This is a cell-block clipboard, not a complete Page operation. It does not
copy the Page name, destination, column setup, entry behavior, mute state, or
Page-targeted automation. Rename, complete duplicate, reorder, remove, clear,
and cross-Pattern Page operations are recorded under
[Future Page operations](FUTURE_IMPROVEMENTS.md#future-page-operations).

## Drum pattern library and transpose

Open **FILES** → **PATTERN** → **DRUMS** for reusable rhythms stored separately
from Projects. The bundled library has 72 authored grooves across
Rock, Pop, House, Techno, Hip-Hop, Funk, Reggae, Breaks, Latin, and Jazz. The
**FILTER** page selects genre, 3/4 or 4/4 meter, and phrase length. 4/4 offers
32/64/128 rows (2/4/8 bars at the default four steps per beat); 3/4 offers the
matching 24/48/96 rows. Longer choices add alternating-bar changes and
genre-aware phrase-end fills rather than merely duplicating a filename. Genre
names are compact creative labels for editable starting points, not claims of
an authoritative historical transcription.

**LOAD** replaces only the current Pattern's first percussion page. Its
destination, channels, bank/program setup, lane state, tempo, and arrangement
remain unchanged. An empty melodic Pattern is resized to the selected meter and
length for the quick load-drums-then-enter-bass workflow. If melodic cells
already contain data, any load that would resize or change meter is refused.

**SAVE** writes the current percussion page as a non-overwriting `.shdrum` file
below `${XDG_DATA_HOME:-~/.local/share}/shsynth/drum-patterns/`. **DELETE**
requires confirmation and applies only to user-saved files; bundled grooves
are read-only.

The Pattern **TRANS** page moves all note-ons on non-percussion pages by a
semitone or octave up/down. Percussion pages and note-offs are never changed.
If any melodic note would leave MIDI range 0–127, the whole transpose is
refused without changing the Pattern.

## FT2 Arrangement

Use **SELECT** → **SONG** for quick Arrangement-step navigation. Choose **EDIT
ARRANGEMENT** there to edit the FT2 Arrangement separately from
pattern editing and Project files. The ARRANGE screen can select a step, append
or insert the current pattern, duplicate or remove a step, move a step earlier
or later, jump to the referenced pattern for editing, and play from the selected
step.

## Automation, metronome, and count-in

Each Pattern owns independent sparse automation lanes; automation never uses
the cell command character. Open **AUTO** from FT2's SOUND page. Up/Down picks
a lane without moving the tracker cursor. The controller pages arm capture,
add/delete and browse points, adjust a point, choose a stable target, inspect
the target-owned curve type, and confirm **CLEAR**. Continuous instrument,
external CC, and effect parameters show `RAMP`: the value reaches the next
automation point exactly. Integer, choice, toggle, bypass, mode, and division
targets show `STEP` and change at their point. One point holds.

Arm is explicit and applies only to the selected lane. Touching its control
records at the real transport position and monitors the value while tracker
notes may be recorded at the same time. Unarmed controls do not replace an
automated value. Start, stop, Project replacement, reset, arm changes, and
leaving AUTO re-arm physical pickup against the effective automated value.
Playback chases the current point and active ramp when started with Play Here;
Pattern/Arrangement loops continue without restoring a preset value. Recorded
knob streams are thinned into a bounded curve.

**CLICK** toggles SHR-DAW's final-bus metronome. It accents beat one and never
sends click notes to a page. REC from stop shows one Pattern-meter bar as
`4 3 2 1 → REC` (or the matching meter), then transport and capture begin at
row zero. REC while already playing punches in without restarting transport.
Count-in clicks are neither Pattern events nor borrowed instrument/drum voices.

## Pattern and Project files

Pattern setup starts with the convenient 4/4 sizes 8, 16, 32, 64, and 128 or
the matching 3/4 sizes 6, 12, 24, 48, and 96. Its **LNGTH** overlay also makes
every row count from 1 through 32 plus 48, 64, 96, 128, 192, and 256 available
for either meter.

The Files screen saves, loads, previews, and deletes the whole Project. Its
**PATTERN** child keeps create, clone, copy, paste, resize, clear, transpose,
and drum-library operations together. NEW starts with empty Loop Mix slots.
Clone copies all MIDI and Loop Mix settings into an independent Pattern.
Resize retains Loop Mix settings. Confirmed CLEAR keeps established page/routing
setup but clears cells and explicitly detaches attached loops. Arrangement
repeat/duplicate adds another step that references the same Pattern. **CLEAN**
offers only Pattern records with zero Arrangement references, confirms
deletion, preserves at least one Pattern, and never deletes private WAV files
or rewrites an Arrangement step.

**NEW PRJ** requires a second press before replacing the in-memory Project and
chooses the next free `project-001` style name. **SAVE AS** immediately writes
the next free `<current-name>-copy-001` style copy and switches to it. These
automatic names keep both actions usable from a four-button controller.
**NAME** starts with the current display name. Main-rotary click accepts it,
while computer-keyboard editing is optional; collisions are refused and a
saved rename keeps the loaded Project state.

FT2 workspace Exit and computer-keyboard quit open the dirty-Project guard only
after at least one note event exists anywhere in the Project. With zero note
events, Exit discards unsaved setup experiments back to the clean baseline and
returns without a save question; quit likewise never asks to save. Empty
routing/template work is retained only through an explicit **SAVE**. New
Project, LOAD, and MIDI replacement still protect any dirty Project before
replacement. The rotary guard opens on `SAVE (AUTO)`, followed by
`SAVE (NAME)`, `DON'T SAVE`, and `BACK`.
`SAVE (AUTO)` reuses a saved identity or chooses the next free automatic name.
`SAVE (NAME)` starts with a collision-free automatic suggestion that rotary
click accepts without typing. `DON'T SAVE` explicitly restores the clean
Project baseline before FT2 Exit. `BACK`, Esc, and any failed or pending save
keep the Project and exact mode/order/Pattern/page/lane/row context.

**MIDI** uses the private configured MIDI inbox and follows an analyse-then-
confirm workflow. Analysis changes nothing and reports parts/pages,
Patterns/rows, tempo and meter, exact and quantized timing, maximum
displacement, stripped events, and important mappings. Confirmation creates a
new unsaved FT2 Project; a dirty current Project uses the same four-choice
rotary guard. Parsing, conversion, allocation, preparation, or
cancellation keeps the current Project, cursor, routing, Loop Mix, effects,
and clean baseline unchanged.

The importer accepts bounded regular SMF format 0/1 files with PPQN timing,
running status, conductor tracks, track names, tempo maps, fixed 3/4 or 4/4,
and fixed 6/8 mapped visibly to the compound 3/4 grid. It groups each MIDI
track/channel part, preserves channel, velocity, initial CC0/CC32/program, and
bakes sustain into note lengths. Four monophonic lanes are allocated
deterministically per page, with overflow pages and bar-boundary Pattern
splits as needed. SMPTE, format 2, changing/unsupported meters, malformed
files, and bounded-limit violations are refused.

Lyrics, copyright, markers, cues, key metadata after reporting, SysEx,
aftertouch, machine-control/timecode/realtime messages, sequencer metadata,
unsupported CC automation, pitch bend, and later unrepresentable bank/program
changes are stripped and counted. Imported system or SysEx data is never
transmitted. Timing stays in musical ticks; non-representable positions are
quantized and reported rather than flattened to elapsed microseconds.

**EXPORT** on FILES' PREVIEW page writes the whole Arrangement as genuine
tick-domain SMF format 1. The first press analyses and reports track count plus
omitted Loop Mix slots and SHR effect lanes; a second press saves below
`${XDG_DATA_HOME:-~/.local/share}/shsynth/exports/`. Existing files are never
overwritten. The conductor carries exact tempo and meter changes. Named
page/channel tracks carry bank, program, CC, velocity, notes, and exact gates
in deterministic setup/CC/note order. Instrument and external-CC automation
are exported. Audio loops and internal effect automation are omitted and
counted rather than disguised as portable MIDI.

The FX rack, editor, and fixed MASTER STRIP always show the owning Project plus
`NEW`, `SAVED`, or `DIRTY`; source, AUX, master racks, and strip are all
Project data. The strip remains global when Arrangement or Live Patterns
changes Pattern.

Projects are readable `.shsong` text files stored below
`${XDG_DATA_HOME:-~/.local/share}/shsynth/songs/`. Current Project format 14
stores each Pattern's tempo, meter, pages, four column setups, lanes, setup
messages, per-page entry mode/anchor, automatic Note Off choice, and drum-role
overrides, cells, persistent Project tonic/mode, selected drum kit and tuning,
the fixed internal-drum Reverb-then-Delay rack, source insert rack, two aux
routes, and master rack. Portable
pages use explicit `default` markers rather than numeric routing. Pattern-owned
software pages store explicit engine and stable instrument identities; optional
external-device profiles are stored separately from raw output/channel/bank/
program data. Each Pattern also stores exactly four optional Loop Mix slot
records and bounded sparse automation lanes. Format 13 and older Projects gain
empty automation in memory. Format 7's four Project-global slots migrate in memory into every
distinct Pattern. Format 6's single `loop=` record similarly migrates to slot 1
of every Pattern with its filename, BPM interpretation, cut, and placement
unchanged; level becomes unity and the filter neutral. Only references and
settings are copied, never WAV files. Loading, previewing, or inspecting does
not rewrite an old file; explicit save writes format 14. Formats 0–9 migrate
their whole-BPM Pattern and tempo-command values to integer hundredths in
memory. Format 10 persists those fields as integer hundredths, so `10050`
means 100.50 BPM. Formats 0–8 gain a neutral fixed strip only in memory.
Format 10 pages infer automatic Note Off as ON for melodic pages and OFF for
percussion pages; format 11 persists the explicit per-page choice. Formats
0–11 migrate with C major, the starter-kit identity, tuning OFF, and their
original page routes unchanged. Format 12 preserves those values and routing
while adding restrained family-specific drum-effect defaults in memory.
Format 5 and older ordinary pages load as Manual with anchor C1 and no
overrides. Pages carrying the old explicit percussion flag retain their prior
automatic drum entry.
Versions 0 and 1 gain empty effects routing; version 2 retains its source rack and gains
empty aux/master routing. Format 3 routes stay explicit. Version 0 page-wide setups copy the old
channel/bank/program into all four columns. Unknown newer versions, fields, or
invalid effect shapes are not loaded or overwritten. Older
`ActiveInstrument` and old `synthv1:<preset name>` routes are upgraded in
memory to explicit synthv1 engine/instrument routes and are not rewritten until
the musician explicitly saves the Project.

If an empty Pattern's routing differs from the current new-Pattern template,
**SAVE** asks whether to make it the new routing default. The compact prompt
states that new Patterns will use the route, that confirming saves the default,
and that cancelling keeps the old default. Confirming
queues the private template and writes it only after the Project save succeeds;
cancelling saves the Project but keeps the old template. A pending, refused, or
failed Project save leaves the template unchanged. A Pattern with notes never changes that template implicitly, and no
prompt appears when routing is unchanged. The template is stored outside the
repository at
`${XDG_DATA_HOME:-~/.local/share}/shsynth/ft2-routing-defaults.shsong` and is
used by every subsequently created Project or Pattern.

## Cleared demo songs

Setup seeds ten public-domain demo Projects into the same song directory, so
they appear on **FILES** without an import step. Matching format-1 MIDI files
and the clearance manifest live below
`${XDG_DATA_HOME:-~/.local/share}/shsynth/demos/`. Seed copies never replace a
same-named user Project. Each arrangement has separate drums, bass, pad, lead,
and counterline pages on `AUTO`, making it easy to choose new sounds or bind a
page to hardware. See [Public-domain demo songs](DEMO_SONGS.md).

## Effects saved with the Project

The Project also owns the managed instrument's ordered source insert rack, two
aux send/rack/return routes, and master rack. Those settings are independent of
the Pattern/Arrangement structure: repeating a Pattern does not duplicate an
effect, and changing Arrangement steps does not change rack order. The two aux
sends take their pre/post source-insert taps from the one managed software
instrument, not from individual MIDI lanes.

With the opt-in graph active, the managed source and wet returns, SHR Drums,
private WAV loop, and exact configured two-port external-input return meet once
before the master rack and final meter. SHR Drums has its own final-bus level
and mute but no multi-mic live mixer. The loop, drums, and external input do
not acquire the melodic source inserts or aux sends; the raw multitrack
recorder remains a separate capture path. See
[How SHR-DAW works](HOW_IT_WORKS.md#the-managed-audio-graph) for the musical
workflow and [Audio graph and DSP contract](AUDIO_GRAPH.md) for exact effect
schemas and limits.

## Detailed controls and routing

See the [Controller interface](CONTROLLER_INTERFACE.md) for the full FT2 menu
map. See [Configuration and routing](CONFIGURATION.md) for page routing, exact
targets, note ownership, and Project behavior.

FastTracker II was created by Fredrik “Mr.H” Huss and Magnus “Vogue” Högdahl of
the demo group Triton. Learn more at
[Demozoo](https://demozoo.org/productions/99958/).
