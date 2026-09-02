# SHR-DAW Help

[Controller basics](#controller-basics)
[Presets and playback](#presets-and-playback)
[Effects graph](#effects-graph)
[18-channel input levels](#18-channel-input-levels)
[Performance meters](#performance-meters)
[MIDI ideas](#midi-ideas)
[FT2-style tracker](#ft2-style-tracker)
[Pages and hardware MIDI](#pages-and-hardware-midi)
[Live performance](#live-performance)
[Loops and audio](#loops-and-audio)
[Trouble spots](#trouble-spots)

## Controller basics

The main encoder moves one visible row or value at a time except on the FT2
grid, where Play/REC turns select columns and Edit turns move rows. Press it to
select the highlighted row, confirm a field, or follow a help link.

Home centers every label in one equal-width bar. MIDI Learn, Routing, and
Effects are separate destinations. Routing reports current controller,
performance-input, MIDI-output,
clock, and audio connections and edits them transactionally. Browsing is
read-only; press to edit a detached field, press again to validate/save, or
Back to cancel. Use `shr-setup` for initial machine setup.

If a configured controller is offline, has no reviewed profile, or has not
learned encoder turn and click, Home highlights MIDI Learn and explains why.
Keyboard Up/Down/Enter still work. Optional command buttons may be skipped once
the learned encoder can turn and click, but only through explicit `S` or
keyboard-arrow input; master-rotary traffic is ignored during Learn. The explanation uses Home's single
bottom line so the native menu keeps its empty first row. Home does not learn
or send MIDI by itself; Learn keeps selected-controller messages isolated until
an explicit save or cancel. Its optional encoder-Shift step learns two ordinary
gestures: Shift plus a left turn, release Shift, Shift plus a right turn on the
same CC, then release Shift again. This captures either the
ordinary rotary CC or a different CC emitted only while Shift is held. Separate
performance inputs continue to bypass controller interpretation.
The Shifted CC may encode left/right opposite to the ordinary rotary; Learn
discovers that from the two physical gestures instead of assuming they match.
Learn itself uses exactly two rows total. The first is the complete required
gesture, such as `SHIFT + TURN ROTARY 1 LEFT`; the second is its immediate
state or retry. There is no repeated `MIDI LEARN` title or status footer.

The controller menu has four pages. Page 1 is OPS. On child screens, page
4 item 4 is EXIT and returns one level. Empty buttons are hidden and silent.

Home and MIDI Learn are the screens without the shared working-screen status
row. Learn owns only its two centered action rows. The native fullscreen EQ
owns all thirteen rows. The native 18-channel Levels
screen keeps the shared final row but omits the two controller rows; every
other working screen places those rows immediately above status. The first status
cell is steady green `>` for play, steady white `■` for stop, steady white `‖`
for pause, or red `●` for record; record alone pulses between red and bright
red without hiding the circle.

Four-button controllers use encoder press to enter page-select mode. Turn to
choose a menu page, then press again to return the encoder to screen control.
In Help, use OPS OPEN to follow the highlighted link. In target/channel
editors, use OPS CONFIRM; SYS EXIT cancels the field.

Some navigation actions open a master overlay instead of replacing the current
workspace. The workspace remains visible around a 38×11 border; its usable
inside is 36×9 on a 40×13 display. While the overlay is open, its bottom
border shows only the highlighted launcher near the same physical position;
the final row remains the shared status row. Turn the master rotary or use
Up/Down, then click or press Enter. Press that same menu
item again, or use Back/Esc, to close. The controller strip has no separate
Back item while an overlay is open. Back first cancels an active field, then
cancels any unconfirmed draft and closes the overlay.

FT2 Tools PAGE item 4 opens HISTORY. Page 1 is UNDO, REDO, SNAP, RECALL;
unavailable actions are dim. Ctrl+Z is Undo, while Ctrl+Y and Ctrl+Shift+Z are
Redo. SNAP is one non-dirty runtime Pattern capture and RECALL is undoable.
History covers committed edits inside one existing Pattern, not Project or
Arrangement structure, global mix/effects, runtime launch state, private files,
or an editor draft before Apply. Restore currently requires stopped FT2
transport; Play-time attempts keep history unchanged and ask you to stop.
Undo during REC finishes the take and note cleanup first.

HISTORY page 2 opens FEEL and GROOVE. FEEL sets Pattern EIGHTH/SIXTEENTH swing
from straight 50% through 75%. GROOVE applies one deterministic preset to the
selected cell, lane, page, or Pattern. Both remain drafts until Apply; Cancel
and unchanged Apply do not enter history.

CELL EDIT TIME is independent of the one-command field. It moves a cell up to
half a row early or late in 1/96-row steps; Reset returns ON GRID. The tracker
grid marks early with `<`, late with `>`, and on-grid with a blank. Pattern
swing and cell timing move musical events only; cursor rows, Loop timing, and
MIDI clock stay steady. REC CAPTURE offers runtime-only REC FEEL; quantized REC
remains the default.

CELL EDIT's rotary list continues after TIME with CHANCE, CONDITION, COND A,
and COND B. Chance is deterministic 1–100%. Conditions are FIRST, LAST/N,
A:B, PRE, and FILL; ALWAYS is the default. LAST/N is the final pass in an
N-pass cycle, PRE follows the preceding trigger in the same lane/pass, and
FILL uses the runtime latch. Normal FT2 SOUND `FILL` or keyboard `f` changes
that latch at the next cycle boundary. Stop/new Play clears it. CLICK remains
under FT2 Tools SYS.

## Presets and playback

Presets chooses the instrument engine and sound. Loading a sound starts or
reuses only the engine owned by SHR-DAW; unrelated synth processes are left
alone. Presets and Playback share that owned sound, and leaving those screens
keeps it running. Global panic, shutdown, replacement, or an explicit different
FT2 software route ends it safely. A genuinely new, empty, unsaved default FT2
Project adopts the current engine/instrument on page 1 without restarting it;
without a Player instrument, FT2 loads the first available synthv1 preset.
Saved or explicitly changed Projects keep their routes.

Turn the main rotary to browse sounds. Hold the configured encoder Shift while
turning to change engine catalog in either direction; `[`/`]` and the two
heading halves remain available. Catalog changes are silent. Only LOAD starts
or replaces the managed preset.

Synthv1 controls add or subtract from SHR's current value immediately. The
direction-only rotaries have no physical position to catch and cannot jump to
a stale knob value.

Moj Sint uses the same direction-only behavior with seven model-specific timbre
controls, Volume at physical position 5, and ADSR. RESET restores the loaded
`.mojsint` values in place without restarting the host. Those controls never
use synthv1 XML names or parameter indices.
The current catalog has 21 numbered starts: seven Model D, six Six-Op PM, one
each for Strange Oscillator, Swarm Machine, and Bass Matrix, and five Dual
Filter starts. The public SHR-DAW installer currently pins the 16-start
pre-Dual-Filter catalog. Player and FT2 PARAM always use a 3×5 surface. Synthv1 and the five
older Moj models put their twelve synth controls first and Project AUX 1/2/3
sends last; `NO FX` means that aux needs an effect before the send can move.
With the owned graph active, these send levels ramp live without rebuilding the
graph; recording refuses the change. With it disabled they update Project data.
Dual Filter uses all fifteen positions for synthesis. Moj routes remain inside their selected
model in FT2 ROUTE.

SHR Sampler packages are read-only instruments. LOAD validates the host version
and complete `.shrinst` package before replacing the current sound. A failure
keeps or restores the previous owned session; SAVE stays unavailable. Ideas and
FT2 routes retain the package identity without copying its samples.

SHR Drums is selected from an FT2 Drums-page route, not Presets. It runs in
process beside the one managed melodic engine. KIT changes use the live
Apply/Cancel route transaction and preserve the prior kit on failure.

The dots beside synthv1 values compare the current sound to the loaded preset:
green is lower, yellow is near original, red is higher.

Playback names the held chord and notes, with each note's decimal MIDI Note On
velocity (1–127) directly beneath it. Use the rows to practise gentle/strong
strikes, even chord attacks, or bass-versus-chord balance. Velocity comes from
MIDI and is not an audio volume measurement; the controller and instrument
response matter. On terminals taller than the native 40×13 layout, the spare
space adds a continuous two-row keyboard from C2 through G7 at 40 columns. A
red white-key area means its natural note is held; a red upper `└` means the
following sharp is held. Major triads show `maj` explicitly, such as `C maj`.
`display.note_names=german` uses B/H spelling; `english` uses A#/B.

Playback PLAY starts a saved MIDI take or, with no take, the configured
external-sync controller arpeggiator. RECORD starts the same controller clock
before free-time capture. STOP ends the take/arp and sends All Notes Off without
unloading the sound. TAP changes the current Pattern/controller tempo but never
starts transport by itself.

Playback N00B toggles the filter on the existing Player screen. While on, its
compact SCALE rotary appears below the normal controls; turn the master encoder
to cycle every root plus MAJOR or natural MINOR choice. Notes in the chosen
scale keep their pitch and sound normally; notes outside it stay silent.
Pressing N00B again restores all chromatic notes. Changing or leaving the
filter releases held notes first.

Playback and FT2 PARAM `SAVE` offer OVERWRITE, SAVE NEW, and CANCEL for
synthv1 and Moj Sint. Factory or system sounds are read-only, so Overwrite
clearly redirects to a new private `User NNN` sound. A saved sound becomes the
current RESET baseline without restarting the engine or changing its values. Use the overlay
with the controller, encoder/Enter, or mouse; keyboard `O`, `N`, and `C` select
its three actions while `S` remains Panic. Unsupported backends show SAVE
UNAVAILABLE. A storage or format failure leaves the overlay open and preserves
the current sound, controls, held notes, and existing file. Save MIDI takes from
Ideas. `SOUNDS` returns directly to Presets, where `LOAD` starts the highlighted
instrument.

## Effects graph

Playback SYS FX or FT2 Tools OPS FX opens the current Project's FX rack. In
FT2, uppercase F opens it directly. Back returns to the calling Player or FT2
screen while its instrument remains active. TARGET cycles SOURCE,
AUX 1, AUX 2, AUX 3, DRUMS, and MASTER. Shift-rotary selects that target in either
direction while the ordinary rotary browses rack rows. Source effects change
the instrument in series.
Each aux makes a parallel wet copy: SEND sets how much enters it, POINT chooses
before or after source effects, and RETURN sets how much comes back. Master
effects change the final dry-plus-aux mix.

ADD inserts a provisional processor and opens TYPE. EDIT changes the selected
processor's type; PARAM opens its named values; DEL removes it. ORDER moves the
same stable instance and BYPASS fades a source or master effect toward dry. A
fully bypassed aux returns silence, so it never doubles the dry source; a delay
tail can be allowed to fade with new input muted. Aux effects are forced wet.

The editor selects named parameters and adjusts values in physical units. At
40×13 an EQ fills all thirteen rows: four one-cell markers move on a
50 Hz–20 kHz logarithmic axis, the side panel exposes bypass, every band,
low cut, and output trim, and gains edit in 0.5 dB steps. Turn/click browses and
edits; Back restores an active edit. Knobs 1–4 are logarithmic band
frequencies and knobs 5–8 are their gains. One compact meter row appears for
other effects when the owned graph has data. The compressor uses a dark-red
0.5–24 dB LED row whose bright-red lights show live gain reduction; bypass
leaves every LED dim. Other effects show compact input/output values.
Rack size and total effect count are bounded. With the graph active, stop
transport and all recording before an FX change can publish a replacement
plan. With the graph disabled, the same editor can design and save the Project
silently, but direct playback will not process or meter it.

On the MASTER rack, ORDER STRIP opens the fixed mastering processor; MTR NAV
STRIP reaches the same Project state. Its front page selects INPUT, broad TONE,
linked full-band GLUE, declared harmonic COLOR, conservative M/S IMAGE, or
LOUD/CEIL. DETAIL exposes only that section's values. Optional sections have
smoothed BYPASS. A/B keeps the same delay and protected true-peak limiter.
RESET I clears LUFS-I. Playback allows smoothed value changes, but a final
recording rejects them; with no owned graph they change only the Project.
In DETAIL, the ordinary rotary browses parameters and Shift-rotary changes
section through the existing front-page order.

## Performance meters

Home PERFORMANCE, or keyboard m, opens the meter/mix surface. With the
owned graph inactive it retains the passive CPU and legacy output view. With
the graph active it shows Synth, Loop, Input, and Drums readiness and level;
Synth, Loop, and Drums use MUTE while Input uses one MON ON/MON OFF action;
Input stereo/dual-mono mode and the two independent dual-mono pans;
master level; final sample peak and dBTP; GLUE/limiter gain reduction,
correlation, LUFS-M/S/I; and final-record
elapsed time, size, drop/error state, and path.

Stereo bars use circular `●` LEDs for live smoothed RMS and a brighter,
decaying held peak on a −60 to 0 dBFS scale. Unlit circles are dark gray; safe
active circles use one green, while yellow and red appear only at their active
thresholds. Each channel's `MAX` number separately holds its highest peak
without decay. CLIP is held in red. RESET clears `MAX`, the bright peaks, and
CLIP. Any downward movement of the mapped synthv1 Volume control clears both
`MAX` values; increases,
equal values, and other controls leave them alone. Stopped, unavailable, and
new meter sessions cannot carry an old `MAX` forward.

FINAL OUT is available only for the active owned graph. It measures after all
present optional sources and the deliberately monitored Input, master inserts,
live master level, fixed strip, and
linked 8× true-peak limiter. The same final buffer feeds the stereo recorder and playback. Direct
playback reports this final-bus meter unavailable and stays direct.

The FT2 WAV Loop screen's `LOOP OUT` still measures only the rendered loop. When
the final bus is active, that loop is one of the four sources in `FINAL OUT`.

On MTR, SOURCE-/SOURCE+ choose a source and LEVEL-/LEVEL+ change it in 1 dB
steps. The same source-control position shows MUTE for Synth/Loop/Drums, MON ON
for an unmonitored Input, or MON OFF for a monitored Input. Keyboard `m`, its
controller item, and the visible pointer target invoke that same action. MON ON
activates the input-only final bus when needed; it never starts an optional
source. REC starts/stops the final stereo WAV at callback boundaries. RESET
clears presentation holds and, when the bus is unavailable, retries the same
exact remembered source mapping. Source and master changes are smoothed; there
are no solo, aux, or per-input effect controls.

With Input selected, MTR NAV **DUAL/STEREO** changes the input interpretation.
**IN CTRL** cycles `LEVEL`, `PAN 1`, and `PAN 2`; on either pan, the ordinary
LEVEL-/LEVEL+ positions become PAN1-/PAN1+ or PAN2-/PAN2+. Dual mono starts at
`1L100 2R100`, matching the original stereo image, then pans each configured
capture port independently with an equal-power law. Mode and focus use the
visible MTR controller actions; they add no dedicated computer-keyboard
shortcuts. These are live session controls, not Project data, and a fresh
launch starts in stereo.

## 18-channel input levels

Audio Recorder **LEVELS** opens Levels. At native 40×13 all 18 inputs stay
visible as three groups of six; selection never scrolls or banks them. Each
nine-LED column is smoothed RMS at −48, −36, −30, −24, −18, −12, −6, −3, and
−1 dBFS. Green covers −48 through −18, yellow −12 through −3, and red −1. A
brighter LED in the same colour is the held sample peak.

Turn the encoder or use Left/Right or `j`/`k` to select a channel. Click/Enter
or Space toggles its arm. PageUp/PageDown shows TAKE, CHANNEL, and SYS commands
in the right half; `r` records, `s` stops, `x` resets holds, `u` returns to
setup, and uppercase `S` panics. The shared final row remains the only status
row. `M`, `F`, and held `C` distinguish missing, faulted, and clipped channels
from ordinary silence. This is a recording overview, not the MTR final-bus
mixer, Audio Recorder setup, route detail, audible monitor, or mixer strip.

## MIDI ideas

Ideas record musical MIDI while a sound is loaded. Repeated RECORD stops the
capture; PLAY plays it back through the loaded engine; SAVE stores it for later.

Recording timestamps come from the MIDI callback, and PLAY playback runs
independently of screen redraws. Stopping a take cancels it promptly and sends
all-notes-off cleanup.

Loading an idea can replace the current sound. If a sound is already active,
choose LOAD twice to confirm. Saved synthv1 control values are restored after
the sound loads, and relative turns continue from those restored values.

Ideas are MIDI, not audio. Use the audio recorder when you need a WAV of the
actual JACK input.

## FT2-style tracker

SHR-DAW uses FT2-style Pattern screens for MIDI sequencing; it is not a
sample-based FastTracker II implementation. PLAY starts at the current
Pattern/Arrangement location, REWIND returns to the beginning, and STOP stops
only the tracker transport.
Turn the physical main encoder to move rows. Hold the configured encoder Shift
while turning to select columns across page boundaries. The shaded selected
column does not move the row, playhead, Arrangement Step, or transport. During
REC, Shift-turns made while recorded notes are held are ignored until all of
those notes receive Note Off.

FT2 `SELECT` contains `PAGE`, `PATTERN`, `SONG`, and `ROUTE`. PAGE lists only
the current Pattern's pages, preserves the selected column, and can open the
full Tracks manager. PATTERN selects an existing Pattern or opens Pattern/Project tools.
SONG selects an Arrangement step and can open detailed Arrangement or Loop/
page tools. ROUTE shows the active page destination and all four columns'
channel, bank, program, profile name, and availability. Turning an active field
validates and applies the choice to the Project and live route. `APPLY ROUTING`
keeps the result. `CANCEL` or Back from the main list restores the route from
when ROUTE opened; Back during field editing restores that field first.

Normal FT2 page 3 is `SOUND`, with `PARAM` and `MIX`. PARAM opens a
tracker-owned view of the current software instrument without entering Player
or replacing the tracker engine. It uses the same 3×5 labels, values,
relative-to-preset colours, held-note display, and rotary carry behavior as
Playback. Synthv1 and the five older Moj models use 12 synthesis positions and
three aux sends; Dual Filter uses all 15 for synthesis. Instrument choice stays
in ROUTE; there is no second sound browser.

PARAM SOUND provides RESET, SAVE, N00B, and one empty position. RESET restores
the existing baseline in place without restarting the engine
or releasing notes. SAVE uses the normal preset-save overlay; successful save
becomes the new RESET baseline and changes only the matching active FT2 route.
PARAM SYS provides PANIC, an empty position, HELP, and EXIT. Unsupported
backends visibly have no editable or saveable parameters. Back/Esc or EXIT
returns to the exact Pattern/order/page/lane/column/row, FT2 mode, transport,
route, N00B state, live values, and launching SOUND page.

MIX opens the live audio-level mixer in Play, REC, or Edit; Shift-clicking the
main encoder is the direct shortcut in every mode. It controls canonical final-
bus Synth, Drums, Loop, or configured Input owner gain, never MIDI velocity or
CC volume. Linked pages share gain/VU after either rotary moves.
With fewer than twelve configured active rotaries, turn the main encoder to choose the
active page bank. External MIDI without a configured stereo SHR return says
`NO RETURN`; Input monitoring remains an explicit safety choice. Back, click,
or SYS EXIT restores the exact tracker location and mode. Play/REC follow the
sounding Pattern; Edit follows the Pattern being edited.

With controller clock enabled, SHR sends the current/default tempo at 24 PPQN
to one exact controller MIDI port while the app is open; tracker transport adds
Start/Stop. An empty Pattern may run for a live external-sync arpeggiator;
tracker pages never send notes or programs back through the clock-only route.

EDIT turns incoming notes into pattern data. Encoder press inserts a blank row.
Edit `ADD` opens a rotary overlay choosing any advance from 0 through 32
rows; 0 keeps the cursor on the current row. The FT2 heading shows the active
value. N-OFF writes a note-off.

TRACKS SYS ENTRY persists one entry layout per page. Manual starts at the
selected column and spreads chords across later columns. One column redirects
notes to its C1–C4 monophonic anchor without moving the cursor. Drum auto
places each simultaneous group atomically across four safe lanes without
overwriting cells. Active unrelated cymbal tails reserve their lanes; matching
retrigger/choke groups may reuse them. Unknown drum notes are short
percussion, and a full four-lane group is refused unchanged as
`DRUM LANES FULL`.

CELL edit is transactional. DONE `SAVE` commits the draft cell; `EXIT` cancels
and restores the original value. `PANIC` remains available without introducing
a second partial-commit path.

FT2 N00B toggles the Player-selected scale directly over Play, Record, and Step
Edit on the selected melodic page; it does not open another screen or change
the current mode. Out-of-scale keys stay silent and are never moved to another
pitch. Play can use it without writing; Record and Edit write only the
allowed notes. It turns off automatically on Drums, where the current mode
remains active.

Edit **LENGTH** opens a rotary overlay choosing 1/1, 1/2, 1/4, 1/8, 1/16,
1/32, 1/64, or 1/128 for melodic notes. The independent 0–32-row **ADD**
overlay controls where the next entry goes.

## Pages and hardware MIDI

Each tracker page has four lanes and one destination. New Patterns start with
Software Synth (first synthv1 preset), MIDI (channel 1/program 1), and Drums
(channel 10). Explicit columns show MIDI channel 1–16 and program 1–128. Pages can target
a named synthv1 preset, configured external output, or named MIDI port. Live
keyboard and musical MIDI audition whichever page is selected.
Sharing a destination/channel requires the same master instrument.

Real-time REC uses the selected page's exact target. A named software-instrument
page records through that one owned engine; a hardware page uses its exact MIDI
output. REC refuses an offline, missing, or ambiguous target instead of silently
substituting another instrument.

Exact saved targets keep their data and show OFFLINE or AMBIG when they cannot
resolve; they never substitute another output or the Pattern's software synth.
Portable AUTO pages resolve the current machine default instead. Reconnect an
exact target and play again without rewriting the Project.

The quick FT2 ROUTE overlay keeps `APPLY` and `CANCEL` visible in its bottom
border. Every encoder change updates the live Project route immediately, so
the selected instrument or SHR Drums kit can be heard without leaving ROUTE.
APPLY keeps the live result; CANCEL restores the complete route from when the
overlay opened. Back during a field edit restores that field first, while Back
from the main list restores the whole route. Keyboard `A` and `C` match the
visible actions.

FILES NEW PRJ requires a second press, clears the current unsaved Project, and
starts the next `project-001` style name. SAVE AS writes and switches to the
next non-overwriting `<name>-copy-001` file. Pattern Repeat/Remove operations
remain on the Arrange screen. FILES NAME accepts a display name and safely
publishes a rename; its custom text requires the computer keyboard. LOAD and
computer-keyboard quit protect dirty Project data with Save/Discard/Cancel.
Cancel or a failed/pending Save retains the exact tracker position. FILES
PATTERN groups pattern create/clone/clear, clipboard,
and melody-only semitone/octave transpose actions. PATTERN DRUMS loads bundled
grooves into the percussion page without changing its MIDI route. FILTER picks
genre, meter, and 32/64/128-row length (24/48/96 in 3/4). Empty Patterns resize;
existing melody is protected. Saved drum patterns are separate `.shdrum` files;
only user saves can be deleted. The ordinary rotary browses the filtered list;
Shift-rotary changes genre through the same wrapping action as GENRE-/GENRE+.
FILES CLEAN deletes only a zero-reference Pattern and never edits Arrangement
steps.

## Live performance

FT2 TOOLS `LIVE` opens Live Patterns. Browse without launching. LAUNCH queues
the selected Pattern for the chosen Pattern or bar boundary; NOW is immediate,
RETRIG repeats the current Pattern, and CANCEL removes the queue. CAPTURE keeps
only successful boundary activations, then APPEND or REPLACE explicitly
confirms an Arrangement change.

SHAPE controls transient mute, velocity, gate, and transpose for the selected
page's four MIDI lanes. The values survive navigation but reset on Project
load/new. Keyboard: `l`/`L` launch/now, `c` cancel, `r` retrigger, `q`
quantization, and `m`/`v`/`g`/`t` lane shaping. Shift-rotary selects the lane
through the same previous/next path as Left/Right.

## Loops and audio

Loop Mix has four slots owned by the FT2 cursor's Pattern. Browsing changes the
editor, not the sounding Pattern. SLOT-/SLOT+ changes selection without launch.
Shift-rotary uses that same slot action while the ordinary rotary keeps
browsing WAV choices.
LAUNCH and STOP queue the selected slot for the next Pattern-local bar; a new
command replaces the queue and CANCEL removes it. MIX controls smoothed level
and mute. FILTER turns left for low-pass, right for high-pass, with neutral at
centre. Keyboard: `p`/`P` launch/stop, `c` cancel, `m` mute, `,`/`.` filter,
and `0` neutral.

Every active WAV must match its Pattern's interpreted tempo and JACK's sample
rate. Playback stays native speed/pitch; there is no time-stretching. Pattern
changes switch MIDI and loops together and restart local phase. A failed slot
is isolated while healthy loops and MIDI continue.

`LIBRARY` opens an overlay for the selected slot. Browsing is silent.
Controller PLAY explicitly previews the WAV; repeated PLAY, selection change,
STOP, Back, browser close, or leaving the browser stops preview. Press the
rotary/Enter to import or attach. `INBOX` imports; `PRIVATE`, `CURRENT`, and
`SAVED` attach the existing file. Failure keeps selection and FT2 context.
The browser does not delete WAVs.

`LOOP OUT` is the summed four-slot source after each slot's cut, phase, filter,
level, transport gate, and edge fades. It does not include the loaded synth,
effects, recorder input, hardware gain, or other JACK clients.

The audio recorder arms independently named JACK inputs and writes one 24-bit
mono WAV per input in a synchronized take directory. Select a track, assign an
exact discovered source, name it, then arm it; a missing remembered source stays
missing and blocks recording instead of being replaced. ARM ALL includes only
resolved tracks, and NONE disarms all. Every armed stem starts and stops on the
same JACK callback boundary.

Each take has a `session.json` manifest recording the sample rate, shared frame
count, source identities, grouping, errors, and finalization state. Recognized
interrupted take directories recover conservatively on the next start. Existing
two-port `capture.input` configuration still appears as a linked stereo pair.

## Trouble spots

If nothing sounds, check JACK first, then the page or preset target. Setup does
not start or restart JACK for you.

If controls do not move a synthv1 parameter, verify that every mapped rotary is
configured for Relative 1 or Relative 2 and run MIDI Learn again. Each
performance rotary must prove a slow left turn and then a slow right turn on
the same CC. A `POSITIONAL` or `DIRECTION` message means that role was not
saved; change the hardware mode if necessary, then press `R` to retry.

PANIC sends all-notes-off, stops owned playback/recording, and shuts down the
managed engine. It does not kill synth processes SHR-DAW did not start.

Pad lock lets command pads play as musical notes. Turn pad lock off when menu
buttons appear to do nothing.
