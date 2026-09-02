# Controller action inventory and paging design

This document owns the controller action inventory and paging contract. Use the
[screen and menu manual](MENU_MANUAL.md) for screenshots and the musician
guides for task order. The inventory follows the current keyboard, mouse,
encoder, command-pad, screen, and contextual dispatch paths.

## Startup splash

Startup first shows a 40×13 old-school stereo LED animation using all thirteen
top-origin terminal rows. Row 0 contains the indicator strip; rows 1–3 are
empty; row 4 contains a lowercase `shr - daw` wordmark, where one bright
light-blue glyph moves quickly across otherwise bright-white text; rows 5–7 are
empty; rows 8–9 contain the `R` meter; row 10 is empty; and rows 11–12 contain
the `L` meter. Each meter uses two identical horizontal `●` rows. Unlit meter
LEDs are dark gray; lit LEDs use one green below −12 dBFS, yellow from −12
through −3 dBFS, and red above −3 dBFS. The animation is decorative and does
not start audio, playback, or MIDI transmission.

The indicator strip contains six identical five-cell indicators: one dynamic
build-mode cell followed by ` CFG `, ` SND `, ` TTY `, ` CTL `, and ` INP `.
The build cell displays blue ` DEV ` in a debug build or green ` REL ` in a
release build. Each three-letter label is centred in its coloured cell. At 40
columns their 30 coloured cells and five two-cell black separators fill the row
exactly, with no outside margin. They sweep from red over 2.5 seconds, then hold
complete before the three-second minimum ends. At full load the build cell has
its mode colour and every completed startup phase is green, so an inactive
build mode does not leave a false red warning. These are loader phases, not
claims that JACK or all synth engines are running.

A terminal computer keyboard remains an optional fallback for controller
navigation and text editing; the main rotary and numbered pads complete the
core workflows without it. Only when none of those inputs is available does the splash
remain open, show `CONNECT KEYBOARD OR MIDI INPUT` and the expected input in
the normally empty recovery rows, and rescan the configured MIDI inputs. `Esc`
or `q` can still exit from the splash.

## Action inventory

| Screen or mode | Existing user-facing operations and input paths |
|---|---|
| Home | Centered startup navigation root with equal-width bars for Software Synths, FT2, Recorder, Performance, MIDI Learn, Routing, Effects, Ideas, and Help. Encoder/Up/Down selects a workspace and encoder click/Enter opens it. Its existing bottom line overrides ordinary guidance with the exact owning workspace whenever recording or transport remains active. Home has no MIDI quit command; Esc or `q` quits from the computer keyboard. A note-bearing dirty Project uses the rotary `SAVE (AUTO)` / `SAVE (NAME)` / `DON'T SAVE` / `BACK` protection; a zero-note Project never asks to save on exit. |
| Presets | Select previous/next, keyboard page up/down, first/last, Shift-rotary previous/next engine across synthv1, Yoshimi, FluidSynth, Moj Sint, and SHR Sampler, and load the selected sound. Moj Sint sounds show their synthesis model separately from the Moj Sint engine; SHR Sampler packages remain read-only. Its physical pages contain only sound browsing, load, panic, contextual help, and Exit to Home; engine browsing remains available from Shift-rotary, `[`/`]`, and the two header halves. |
| MTR | With the final bus active: choose Synth/Loop/Input/Drums, adjust its bounded smoothed level, use MUTE for optional sources or the one MON ON/MON OFF Input action, switch Input between stereo and dual mono, and pan configured Input 1 and Input 2 independently in dual mono. `IN CTRL` cycles LEVEL/PAN 1/PAN 2, and the ordinary minus/plus pair follows that focus. Both new Input choices live on the visible NAV controller page and add no dedicated computer-keyboard shortcuts. Dual mono starts at `1L100 2R100`, so its initial output matches stereo. Inspect final sample/true peaks and linked reduction, and start/stop the callback-boundary final stereo recording. Input monitoring defaults off and MON ON can activate the bus without launching an optional source. At native 40×13 the body reserves its final rows for recording integrity and a doubled-monitoring refusal; healthy sources omit `READY`/`ON`. With the bus inactive, the passive CPU/VU view puts an unavailable reason in the VU heading instead of clipping it below the body. NAV opens either the selected source/AUX/master rack overlay or the fixed Project MASTER STRIP and owns the two Input controls. |
| Playback | Inspect held notes/chords and aligned decimal MIDI strike velocities, with keyboard state added only when the terminal is taller than native 40×13; toggle the N00B filter in place and, while enabled, turn the main rotary through all root plus major/natural-minor choices shown by a compact `SCALE` control; reset the active synth parameters in place; open and return from the FX rack without stopping the sound; use STOP/PLAY/RECORD/TAP for a free-time take or the dedicated external-sync controller arpeggiator; save the current synthv1 or Moj Sint sound through the explicit Overwrite/Save New/Cancel overlay; use `SOUNDS` to return directly to the Presets catalog and its visible `LOAD`; panic; contextual help; return to Presets. PLAY sends a fresh controller Start even with no take, RECORD starts the same clock before capture, STOP sends Stop without unloading the sound, and TAP changes tempo without starting transport. Factory/system sounds are read-only and redirect Overwrite to a private numbered sound. Saving changes neither the running engine nor held notes and makes the values the new RESET baseline. N00B never replaces the Player body. The native parameter surface is always three by five: Synthv1 and the five older Moj models show twelve synth controls plus AUX 1/2/3 sends, while Moj Sint Dual Filter shows its fifteen synthesis controls. Aux sends use a live 10 ms gain ramp and refuse changes during recording. Direction-only mappings carry current values. |
| Ideas | Previous/next/first/last idea; inspect, load, play, delete, record, and save; panic; contextual help; Exit to Home. |
| FT2 normal | The main rotary always moves rows; holding the configured encoder Shift while turning selects the previous/next column across page boundaries, while Shift-click opens the FT2 live mixer. Keyboard Up/Down also moves rows. PLAY holds cell edit and transport, SELECT opens PAGE/PATTERN/SONG/ROUTE rotary overlays, SOUND contains PARAM, MIX, AUTO, and next-boundary FILL, and SYS holds panic/N00B/help/Exit. PARAM and AUTO return to the exact cursor, mode, transport, route, and launching page. CLICK toggles the internally owned final-bus metronome from FT2 Tools SYS item 3. With external `SYNC` selected, local PLAY enters visible clock waiting or performs literal Stop; only an acquired incoming Start launches Arrangement step 1/row 1 or selected Pattern/row 1. In ROUTE, turning an active field validates and applies the choice to the Project and live route. Moj Sint exposes `ENGINE → MODEL → PATCH`; patch browsing stays within its selected model. `APPLY` keeps the result; `CANCEL` restores the route snapshot from when the overlay opened. Back restores the active field first. The physical item buttons, mouse, and keyboard `A`/`C` share those actions. Its `KIT` field selects an installed SHR Drums kit when the target is SHR Drums; a successful kit change resets old kit-specific tuning while preserving the Project key and drum effects. Exit never asks to save when the entire Project has zero note events; explicit SAVE remains available for an empty routing template. |
| FT2 PARAM | Reuses Playback's active backend/model three-by-five renderer, values, relative-to-preset colours, held-note presentation, and configured rotary 2–16 mapping while keeping tracker engine ownership. Older 12-control instruments expose AUX 1/2/3 in the final row; Dual Filter retains all fifteen synth controls. SOUND provides RESET, SAVE, N00B, and one empty position; SYS provides Panic, one empty position, Help, and Exit. RESET is in place. SAVE reuses Overwrite/Save New/Cancel and retargets only the active matching FT2 route after success. Unsupported backends visibly provide neither editable parameters nor save support. The ordinary and Shift rotary do not edit synth parameters. Back/Esc/Exit restores the exact FT2 state and launching menu page. |
| FT2 mixer | A mode-preserving live audio child for up to twelve current-Pattern strips. Configured active rotary positions map directly with twelve controls or to a rotary-selected bank with fewer. Each strip shows page/name, canonical Synth/Drums/Loop/Input owner, signed dB gain, five-LED post-gain VU, link count, and honest `NO RETURN`, monitor-off, or offline state. Linked pages share gain/VU. Play/REC follow the sounding Pattern; Edit follows the edited Pattern. Opening activates the final bus without silently enabling Input monitoring. Main-encoder click, Back, or SYS Exit restores the exact tracker context. |
| FT2 automation | Pattern-owned sparse-lane child. Opening is read-only; NEW explicitly creates an unused target lane. Up/Down changes lanes; REC arms only the selected target and adds/deletes points; POINT browses and adjusts points; LANE changes an empty lane's target, explains fixed RAMP/STEP meaning, and confirms clear. Populated lanes refuse target browsing until CLEAR is confirmed. Touch capture uses transport time, can run beside note REC, monitors the armed value, and becomes safe rather than writing through a Pattern boundary. Effect removal or confirmed type replacement reports and clears only that effect's now-unresolvable lanes; Cancel retains them. Back restores exact tracker context. |
| FT2 HARMONY | Read-only FT2 Tools overlay derived from the current Project tonic and major/natural-minor mode. It shows both circle-of-fifths neighbours, relative and parallel keys, and seven diatonic triads using the configured English/German note-name policy and SHR's canonical sharp spelling. Opening, repeated opening, encoder turns, and closing cannot change or dirty the Project, generate notes, or alter transport. The launcher/Exit, keyboard `H`/Back/Esc/Enter, controller, and mouse paths restore the exact caller page, page-select mode, Pattern/order/page/lane/column/cursor, FT2 mode, and transport. |
| FT2 HISTORY | Bounded runtime history for committed mutations to one existing Pattern. Page 1 is exactly Undo, Redo, Snapshot, Recall; page 2 opens FEEL, GROOVE, CYCLE, and GEN. FEEL drafts Pattern swing division/amount. GROOVE drafts one deterministic preset/scope/strength and writes exact cell timing/velocity only on Apply. CYCLE drafts the selected lane's independent length, rate, and direction; stopped Apply is one history entry and never moves the FT2 cursor or changes Arrangement duration. GEN opens the one offline inspectable Euclidean/accumulator/seeded-mutation/controlled-FILL/arpeggio/chord/harmonizer workflow described below. Unavailable restore controls are visibly disabled. Snapshot capture is non-dirty and Recall is undoable. Mouse and controller dispatch the same actions; keyboard adds `Ctrl+Z`, `Ctrl+Y`, and `Ctrl+Shift+Z`. Restore preserves Pattern/order/row/page/lane/column/mode/controller context and currently requires stopped FT2 transport; a Play-time attempt leaves the stack unchanged, while REC Undo finishes the take and held-note cleanup first. |
| FT2 GENERATOR | Offline draft for the selected Pattern/page/lane/cursor context, with explicit harmonic source/target, complete tool settings, affected/replacement/collision/protected counts, affected rows, out-of-scale skips, range/scope refusals, collision policy, and retained seed where used. ROLL is percussion-only: amount selects 1–8 total pulses, length selects the bounded span, DETAIL changes velocity depth and EVEN/ACCENT/CRESCENDO shape, and its visible NEW CLONE default makes Apply use the existing independent Clone transaction. ARPEGGIO reads the cursor-row chord and exposes order/octaves/rate/gate/repetitions; CHORD uses Project key/degree/inversion/close-or-open voicing/three lanes/rate/repetitions; HARMONIZER copies a bounded source-lane span to an explicit target as a diatonic third/fifth above/below under REFUSE/SKIP policy. SHAPE selects tool/length/amount; DETAIL exposes tool-specific secondary/control values and policy; VALUE changes gate, chord rate, harmony target, or existing seed as named in the body, then offers Repeat and Inspect; APPLY owns Stop, current-Pattern Apply, Apply to Clone, and Cancel. Opening and every draft action preserve Song, History, dirty state, Arrangement, automation, and FT2 cursor. Current Apply is one stopped History transaction. Clone uses the existing independent Pattern plus explicit appended Arrangement-step owner. The result is ordinary Cells; no playback-time generation exists. HARMONY remains a separate read-only browser. |
| FT2 record | Record row-quantized note-ons by default and, when enabled for the page, release-based note-offs through the selected page's target and persisted Manual, One-column, or Drum-auto layout. CAPTURE offers runtime-only REC FEEL, which keeps the nearest row plus a bounded callback-time offset. MODE keeps MIX directly reachable. Stopped internal REC runs one visible meter-aware count-in bar and starts capture at row zero; REC during Play punches into the Arrangement without restart. External sync refuses stopped REC until acquired Start, then retains the same running punch-in owner. Exact page/lane owners survive cursor moves and boundaries. Shift-rotary column turns are ignored while recorded notes are held; Edit note length does not affect REC. |
| FT2 edit | A contextual four-page command set only: EDIT has cell edit, blank/skip, erase, and note off; SET has independent 1/1–1/128 LENGTH and 0–32 ADD selectors plus column movement; SIZE has HALF, ROW-, ROW+, and DOUBLE across the complete Pattern; SYS has panic, N00B, help, and a one-level Exit back to normal FT2. Shift-click opens the live mixer without displacing these pages. Populated destructive/copy choices use a modal confirmation with exact cell counts. Normal FT2 SELECT remains available after Exit. It contains no Play/Record/Edit mode duplicates. Manual writes from the selected column, One column uses its C1–C4 anchor, and Drum auto allocates simultaneous hits without moving the cursor. |
| FT2 N00B | Independent on/off scale filter layered over Play, Record, and Edit on a melodic page, using the scale selected on Player. Accepted notes keep their pitch; rejected notes stay silent. Play remains non-writing, while Record/Edit write only accepted notes. Toggling N00B is immediate, opens no screen, preserves the current mode, and moving to Drums turns only the filter off. |
| Live Patterns | Browse four existing Patterns at a time without launch; use Shift-rotary or Left/Right for the tracker lane; distinguish selection/current/queue; replace or cancel Pattern/bar-boundary queues; deliberate immediate launch and current retrigger; capture only successful activations with Append/Replace confirmation; transient four-lane mute, velocity, gate, and transpose; literal Stop, Panic, keyboard equivalents, and preserved FT2 cursor. In external Pattern sync, incoming Start launches the selected shaped Pattern at row 1 and later launch/retrigger boundaries retain the external owner. |
| Loop Mix | Pattern-owned fourth musician-facing FT2 page; browse inbox WAVs with the ordinary rotary and select one of four private WAV slots with Shift-rotary, Left/Right, or the existing slot buttons, all without launch; queue independent launch/stop at the next Pattern-local bar; replace/cancel commands; show play/stop/queue/mute/missing/fault states; smoothed level and bipolar filter; import/attach/remove only the FT2 cursor Pattern's selected slot; isolate faults; shared library overlay and align child. |
| FT2 cell edit | Transactional route/channel/instrument, banks, note, gate, velocity, per-note program, independent 1/96-row timing, single command type/parameter, clear-field, save/cancel, and panic actions. Four-button encoder page selection remains available. |
| Tracker files | Select saved Project; load; preview/stop; save with overwrite confirmation; create a confirmed blank Project; save a numbered non-overwriting copy; delete with repeat confirmation; rename; open the Pattern child; back/cancel and panic. |
| Pattern tools | New, clone, clear, copy, paste-new, paste-over, or clean unused Patterns; transpose melodic pages by semitone or octave; open reusable drum patterns. |
| Drum patterns | Filter 72 bundled plus user rhythms by genre, meter, and 2/4/8-bar size; browse the filtered list with the ordinary rotary and change genre with Shift-rotary or the existing Genre−/Genre+ actions; load into the percussion page; save that page separately; confirmed deletion of user saves only. Empty Patterns may adopt the selected shape, while existing melody blocks resizing. |
| FT2 arrange | Select an Arrangement step; append/insert the current Pattern; duplicate/remove a step; move it earlier/later; jump to its Pattern; or play from it. FORM opens the non-writing A A B A assistant with the selected Pattern as A and no implicit B. B-/B+ explicitly choose an existing Pattern ID. The assistant keeps four pages: FORM, APPLY with Stop/Append/Replace/Cancel, one empty page, and canonical SYS. Append and Replace validate before changing only the order; dirty Replace uses the existing unsaved-Project guard. Cancel and every refusal preserve Pattern data, History, dirty state, transport, and FT2 cursor/context. |
| Pattern setup | Choose 3/4 or 4/4 and pattern length; CONFIRM performs NEW/CLEAR with that shape, KEEP performs the same operation with the current Pattern's shape, and Exit cancels. |
| Tracks page manager | Select pages with the encoder and change the selected column with Shift-rotary or the existing Column−/Column+ actions; add a four-lane page; edit target, channel, bank, program, and the per-page Manual/One-column/Drum-auto entry layout; confirm all changes; or exit and restore the original Project. |
| Target/channel field mode | Previous/next choice, confirm field, cancel field. Encoder turn/press and menu items share these operations. |
| Audio recorder | Select and name a track (`NAME` accepts the current value by rotary click and allows optional keyboard editing); assign an exact discovered JACK source; arm/disarm one, every resolved track, or all; refresh source discovery without rewriting preferences; start/stop one synchronized take; inspect elapsed time, active count, selected-track activity, drop/xrun/high-water status, final basename or failure; Exit to Home and panic. The native body uses a selection-following five-track window and reserves its final two rows for integrity/recovery and the result. Healthy tracks omit `ready`; only `MISSING` is called out. |
| 18-channel Levels | Show all 18 recording inputs simultaneously as three groups of six fixed nine-LED dBFS meters. Encoder, Left/Right, `j`/`k`, or pointer selects without scrolling; encoder click/Enter/Space arms the selected channel. Visible TAKE, CHANNEL, and SYS pages provide setup, record, literal Stop, reset, previous/next, arm, refresh, Panic, Help, and Exit. At native 40×13 it omits controller rows but keeps shared row 13; compact geometry falls back rather than cropping. |
| FX rack/editor | Show the owning Project and its `NEW`/`SAVED`/`DIRTY` state; choose source, AUX 1, AUX 2, AUX 3, drum, or master with Shift-rotary or the existing forward TARGET action; select the typed `+ INSERT EFFECT` row with the ordinary rotary; add/select/remove/bypass/reorder bounded effects; and edit every parameter using explicit compact labels and type-aware values. Shift-rotary is inert in the type and parameter editors. The native 40×13 EQ is a dedicated fullscreen logarithmic graph with four one-cell band markers and all bypass, band, low-cut, and output fields; other processors retain the 2×4 physical-control grid. The rack and parameter fields keep the current selection visible. Aux time effects are forced wet. An active graph publishes FX changes only with stopped transport and recording; a disabled graph accepts Project-only edits without touching audio. |
| MASTER STRIP | Compact fixed-order INPUT, TONE, GLUE, COLOR, IMAGE, and LOUD/CEIL front page with one selected-section value and bounded mastering meters; DETAIL opens only that section's advanced values. In DETAIL, the ordinary rotary browses that section's parameters and Shift-rotary changes section through the existing previous/next order. Optional sections have smoothed bypass; A/B retains fixed latency and true-peak protection; RESET I clears integrated loudness. Playback allows numerical audition without a topology rebuild, final recording rejects edits, and a disabled graph changes only Project state. Back preserves caller, page, FX/tracker selection, and cursor. |
| Routing | Transactional rotary editor for controller input/role, every repeated performance input plus an explicit add row, external MIDI enable/output/profile, controller clock enable/output, exclusive `SYNC` internal/external owner, exact `SYNC IN`, `SYNC POS` Arrangement/Pattern Start, and audio output. Browsing never writes or transmits. Field confirmation validates the whole candidate, rejects duplicate or ambiguous inputs, backs up and atomically saves it, safely activates live MIDI input changes while transport is stopped, refreshes discovery, and rolls back on failure. All controller layouts reach the same scrolling rows through the canonical menu dispatcher. Interface availability and unverified downstream DIN profile are separate states. |
| Help | Compact Markdown user help, temporary LAN web help when port 80 is available, section links selected by the master encoder, keyboard page scrolling, top, and return to the previous screen. |
| Global/safety | Stop MIDI playback, tracker transport, recorder, managed engine, and owned notes; All Notes Off; cancel or leave the current controller level. Application exit remains computer-keyboard-only. Help is also reachable from `?` or F1. Process termination remains limited to the engine owned by SHR-DAW. |

The complete final screen × page × item mapping is maintained below. The table
uses expanded action names where that is clearer; the compact visible label is
shown in parentheses when it differs materially. `src/navigation.rs` is the
executable canonical copy: labels and dispatch actions are one definition. A unit test builds the
union of every normal and contextual menu and checks every action in this
  screen-specific inventory for controller reachability. Top-level Home entries
  are reached by the master rotary rather than duplicated on child command pages.

## Shared status row and master overlays

Every working screen except Home and the native 40×13 fullscreen EQ reserves
the final row. Native 18-channel Levels is a separate exception only to the
controller rows: it still reserves that shared row while using rows 1–12 for
meters, labels, and visible right-side commands. Its first status cell is the
transport state: steady green `>` for play, steady white `■` for stop, steady
white `‖` for pause, or red `●` for record. Record alone pulses between normal
and bright red; the circle never disappears. One space leaves exactly 38 cells
for current useful status. A configured CPU temperature remains right-aligned
as `CPU 52°C` whenever useful state or a fault fits beside it. Longer actionable
text temporarily owns all 38 cells so its consequence and recovery are not
lost; the temperature returns when that message clears. Text is fitted after
reserving ownership, consequence, and recovery. Routine success lasts at most
1.5 seconds; a retained-work, rollback, or All Notes Off consequence
lasts at most three seconds. Confirmations and faults remain until resolved,
while active recording always names its owner and merges any recording fault.
With no configured temperature and no message, an idle transport cell alone is
the normal healthy state. Screen bodies do not add generic gray status lines,
and the two controller rows sit immediately above the shared status row.
Working-screen frame cleanup stops above that row; only the shared renderer
clears and replaces it. The fullscreen EQ deliberately owns all thirteen rows: its final
row is the 50 Hz–20 kHz logarithmic axis, temporarily replaced only by a useful
range or fault message. It has no visible controller rows. A compact
terminal falls back to the ordinary FX editor and shared status layout.
Levels likewise falls back to its compact ordinary layout below 40×13.

Horizontal meters use the same circular LED language everywhere. Every cell is
`●`: dark gray when unlit, one consistent green at a safe active level, then
yellow and red only when their documented thresholds are active. A held peak
uses a brighter version of its threshold colour rather than a line or square.

An overlay is transient state above its caller, not another `Screen` and not a
second Project/engine owner. Its central state records identity, caller, title,
canonical launcher, selection/scroll, active field snapshot, typed draft, and
the caller's controller-page state. At 40×13 an ordinary overlay's outer
rectangle is exactly `x=1`, `y=1`, `width=38`, `height=11`; its bordered inner
content is `x=2`, `y=2`, `width=36`, `height=9`. ROUTE reserves the two
controller rows and therefore uses outer `38×9` and inner `36×7` rectangles at
the same origin. Every form leaves the final shared status row untouched.

While an ordinary navigation overlay is open, the launcher remains on its
bottom border near the original physical item position and with an active
highlight. Loop Browser keeps STOP at position 5 and PLAY preview at position
6, and the Song overlay keeps TAP at position 8. ROUTE is the deliberate
exception: its window ends above the canonical controller page/action rows,
where the ROUTE page maps APPLY to position 5 and CANCEL to position 8 through
the same table used for rendering and physical dispatch. All unrelated caller
commands are hidden and silent. No overlay occupies or clears the shared status
row. There is no controller-strip Back button. The rotary and
Up/Down browse; rotary click and Enter select or confirm. Back/Esc cancels an
active field first, then cancels the overlay draft and closes, before a later
Back can leave the caller. Four-button page-selection state and every layout's
previous page are restored deterministically.

FT2 demonstrates eight caller-specific adapters: PAGE lists only the Pattern's
musician-facing pages, preserves the selected column, and links to Tracks;
PATTERN navigates the Project's existing Pattern
owners and links to Pattern/Project tools; SONG navigates Arrangement steps and
links to its detailed editors; ROUTE applies valid active-field choices to the
Project and live route, while Cancel restores the opening route snapshot;
Edit LENGTH chooses 1/1 through 1/128 and ADD chooses 0 through 32 rows;
Tracks ENTRY chooses Manual, One column C1–C4, Drum auto, and the per-page
automatic Note Off setting; Pattern Setup
LNGTH chooses every value from 1 through 32 plus 48, 64, 96, 128, 192, and
256.
Edit SIZE is not a navigation overlay: its canonical action row performs HALF,
ROW-, ROW+, and DOUBLE on the complete Pattern. Populated HALF, ROW-, and
DOUBLE operations switch to one modal choice page; controller items, rotary
selection, keyboard, and mouse all share its confirmation dispatcher. The
prompt and both controller rows remain above the shared status row.
Loop Mix preserves slot and command-page context while the FT2 cursor remains
on one Pattern, then resets that context safely on Pattern or Project change.
Its LIBRARY launcher uses the overlay for one combined inbox/private
browser. Selection is silent until PLAY explicitly previews it; changing
selection, STOP, Back, closing the browser, or leaving the browser stops
that preview. Activating an inbox selection imports and loads it, while
activating a private/current/saved selection attaches and loads it. A failed
preview or import keeps the selection and caller context for retry. MTR's FX
launcher reuses the same rendering,
input, toggle, and return layer.

## Input model

Shift state comes either from a configured MIDI modifier or from a dedicated
alternate encoder CC produced only by the controller's hardware Shift layer.
The latter is the MiniLab mkII path: Arturia documents that Shift changes the
CC of encoders 1 and 9, so SHR does not wait for an independent Shift packet
that the controller does not expose. Releasing Shift restores the ordinary
rotary CC in hardware. The following table is the current
secondary-navigation contract:

| Screen/context | Ordinary rotary | Shift+rotary | Existing action reused |
|---|---|---|---|
| Presets | Preset in current catalog | Previous/next engine catalog, wrapping in engine order | PreviousEngine/NextEngine; same behavior as `[`/`]` and the header halves |
| FT2 Play/REC/Edit grid | Row | Previous/next column across page boundaries | Existing page-spanning tracker-column move; ignored in REC while a recorded note is held |
| Tracks browse | Page | Previous/next column, bounded within the page | Column−/Column+ |
| Live Patterns | Pattern browse, or the active shape value | Previous/next tracker lane | Left/Right and PreviousTrack/NextTrack |
| Loop Mix | Inbox WAV browse | Previous/next loop slot, wrapping | Slot−/Slot+ and Left/Right |
| Drum patterns | Filtered rhythm list | Previous/next genre, using existing wrap | Genre−/Genre+ |
| FX rack, including an empty rack | Effect row | Previous/next source/AUX/drum/master target | TARGET's existing forward order plus its exact reverse |
| MASTER STRIP detail | Parameter in the current section | Previous/next section, wrapping | The front-page section order; its existing section change resets parameter selection to the first value |

Shift+rotary is intentionally inert on Home, Playback, FT2 PARAM, Ideas, Help, Project
files, Pattern tools, Arrangement, Pattern setup, FT2 Tools, Loop Align, Audio
Recorder, Levels, the FX type and parameter editors, the MASTER STRIP front
page, MTR, and Routing because each has no separate reversible adjacent
navigation axis beyond the ordinary rotary. It is also inert in FT2 cell edit,
Tracks field edit, overlays, confirmations, naming, and other transient editors:
their one browse/edit axis and caller boundary stay exclusive. Four-button
page-select mode remains an explicit exception in which the rotary continues
to select controller pages while Shift is held.

- Eight buttons: four direct page selectors plus four item buttons.
- Five buttons: one page-cycle button plus four item buttons.
- Four buttons: four item buttons; encoder press enters/leaves page-selection
  mode and encoder turn changes pages while that mode is visible.
- Outside four-button page-selection mode, encoder turns retain list, row, and
  field adjustment. On the normal FT2 grid, an ordinary turn always moves rows;
  holding the configured encoder Shift while turning moves columns across page
  boundaries. Encoder press
  retains the existing select/confirm action on eight- and five-button layouts.
  Menu slots do not duplicate those master rotary selection actions.
- An open overlay always gives the encoder to overlay browsing/editing, so a
  four-button controller cannot become stranded in page-selection mode.
- Entering any screen or contextual mode selects its page 1, preventing a page
  choice from a previous visit from becoming the new screen's hidden meaning.
- Page 1 holds the primary screen workflow; for FT2 normal mode it is PLAY.
  On every workspace, child screen,
  and contextual editor, `EXIT` is page 4/item 4 and returns exactly one level.
  Home is the root and has no MIDI Exit; quitting remains keyboard-only.
- Physical positions 5–8 are semantic anchors: STOP/PANIC, PLAY/LOAD/PREVIEW,
  REC/capture, and TAP respectively when those actions are present. Empty
  anchors remain contextual rather than becoming unconditional global
  commands. The page-4/item-4 Exit flow remains at the right edge when TAP is
  absent.
- When a configured controller is offline, lacks a matching reviewed profile,
  or has an incomplete learned encoder, Home initially selects MIDI Learn and
  gives the reason. A learned master encoder with turn and click is usable even
  without optional pads. Home itself neither learns nor transmits. During
  Learn, master turn/click packets cannot browse or skip capture roles; only
  explicit keyboard skip/back can move past an unlearned optional control.
  Learn renders exactly two action-first rows total and no shared status row.
- Help is a child screen. It tries to show the same help at
  `http://<LAN-IP>/help` while open. The master encoder moves one help row at a
  time. Encoder press follows a highlighted internal section link on eight-
  and five-button layouts; four-button layouts use OPS `OPEN` because encoder
  press is reserved for page selection. The compact help text uses a stable
  38-column width so link targets and rendered rows remain identical.
- Target/channel fields use encoder press to confirm on eight- and five-button
  layouts. Four-button layouts use the visible OPS `CONFIRM` item; SYS `EXIT`
  cancels the field on every layout and restores that field's complete
  target/engine/instrument/output/channel snapshot without reverting unrelated
  Tracks edits.
- Empty items and pages are not drawn, are silent when pressed, and are skipped
  by page cycling. The interface exposes working actions only.
- Physical command pages never contain PageUp/PageDown. Keyboard
  PageUp/PageDown change order while preserving page/lane/column and retaining
  the row unless a shorter destination Pattern requires clamping. Pattern and
  Song overlays follow the same rule; the rotary continues ordinary one-step
  list and row movement.
- Every genuine rotary/Up/Down browse list wraps first-to-last and last-to-first,
  including Home, file/library lists, Arrangement, tracker browse cursors,
  recorder/meter/FX lists, overlays, Routing rows, and enumerated field choices.
  Empty lists are inert, one-item lists remain stable, stale selections clamp
  before wrapping, and scroll offsets follow the selected row. Bounded numeric
  editing does not inherit list wrapping.
- Functional sentinels are typed logical entries, not inferred from their
  visual text. Blank/Skip, Off, Clear, Default/AUTO, and FX `+ INSERT EFFECT`
  therefore remain distinct and reachable exactly once; decorative blank lines
  remain non-selectable.
- The rendered controller strip is centered and capped at 40 columns. Labels
  and brackets use their natural width instead of expanding with the terminal.
- Command notes and CCs may be qualified by MIDI channel. The MiniLab factory
  Arturia/DAW pads are notes 36–43 on channel 10: 36–39 select pages 1–4 and
  40–43 activate items 1–4. Matching pressure and releases are consumed, while
  the same notes on channel 1 remain keyboard input. User 1's captured
  channel-1 pads cannot safely be commands because they collide with the keys.

## Complete controller map

Blank physical positions and wholly empty pages are omitted.

| Screen/context | Page | Item 1 | Item 2 | Item 3 | Item 4 |
|---|---|---|---|---|---|
| Presets | Ops | First | Load | Last | — |
| Presets | Sys | Panic | Help | — | Exit |
| MTR | Ops | Source− | Source+ | Level− | Level+ |
| MTR | Mix | Mute or Input MON ON/OFF | — | Final rec/stop | Reset holds |
| MTR | Nav | FX overlay | MASTER STRIP | Input stereo/dual mono | Input LEVEL/PAN 1/PAN 2 |
| MTR | Sys | Panic | — | Help | Exit |
| Playback | Play | Stop take/arp | Play take/arp | Record MIDI | Tap tempo |
| Playback | Sound | Reset controls | Save sound | N00B on/off | Sounds |
| Playback SAVE | Save | Overwrite | Save new | — | Cancel |
| Playback | Sys | Panic | FX | Help | Exit |
| FX rack | Ops | Add | Delete | Edit type | Parameters |
| FX rack | Order | Up | Down | Bypass | MASTER STRIP |
| FX rack | Route | Target | Send− | Send+ | Point |
| FX rack | Sys | Panic | Return | Help | Exit |
| FX rack empty | Ops | Add | — | — | — |
| FX rack empty | Route | Target | Send− | Send+ | Point |
| FX rack empty | Sys | Panic | Return | Help | Exit |
| FX type | Type | Type− | Type+ | OK | Cancel |
| FX editor | State | Bypass | — | — | — |
| FX editor | Sys | Panic | — | Help | Exit |
| MASTER STRIP | Section | Previous | Next | Detail | Bypass |
| MASTER STRIP | Compare | A/B | Reset LUFS-I | — | — |
| MASTER STRIP | Sys | Panic | — | Help | Exit |
| Strip detail | Param | Previous | Next | Value− | Value+ |
| Strip detail | State | Bypass | A/B | Reset LUFS-I | — |
| Strip detail | Sys | Panic | — | Help | Exit |
| Ideas | Play | Inspect | Play | Record | Delete |
| Ideas | File | Save | Load | First | Last |
| Ideas | Sys | Panic | — | Help | Exit |
| Help | Ops | Open link | Top | — | — |
| Help | Sys | Panic | — | — | Exit |
| FT2 | Play | Cell edit | Play | Record | Edit |
| FT2 | Select | Page overlay | Pattern overlay | Song overlay | Route overlay |
| FT2 | Sound | Parameter view | Mixer | Automation | Metronome |
| FT2 AUTO | Rec | Arm/safe | Add point | Delete point | New lane |
| FT2 AUTO | Point | Previous | Next | Value− | Value+ |
| FT2 AUTO | Lane | Target− | Target+ | Curve meaning | Clear/confirm |
| FT2 AUTO | Sys | Panic | — | Help | Exit |
| FT2 | Sys | Panic | N00B | Help | Exit |
| FT2 PARAM | Sound | Reset controls | Save sound | N00B on/off | — |
| FT2 PARAM SAVE | Save | Overwrite | Save new | — | Cancel |
| FT2 PARAM | Sys | Panic | — | Help | Exit |
| FT2 tools | Ops | Arrange | Live Patterns | FX | Loop Mix |
| FT2 tools | Clip | Copy lane (`COPY L`) | Paste lane (`PASTE L`) | Copy page (`COPY PG`) | Paste page (`PSTE PG`) |
| FT2 tools | Page | Mute page (`MUTE PG`) | Mute lane (`MUTE`) | Harmony browser | History |
| FT2 HARMONY | Overlay | — | — | Harmony/close | Exit |
| FT2 HISTORY | History | Undo | Redo | Snapshot | Recall |
| FT2 HISTORY | Rhythm | Feel | Groove | Cycle | Generator (`GEN`) |
| FT2 HISTORY | Sys | Panic | Help | — | Exit |
| FT2 FEEL | Feel | Division | Amount− | Amount+ | Apply |
| FT2 FEEL | Sys | Panic | Help | — | Exit/cancel |
| FT2 GROOVE | Shape | Preset | Scope | Strength− | Strength+ |
| FT2 GROOVE | Apply | Apply | — | — | — |
| FT2 GROOVE | Sys | Panic | Help | — | Exit/cancel |
| FT2 CYCLE | Cycle | Length− | Length+ | Rate | Direction |
| FT2 CYCLE | Apply | Stop | Apply | Reset | Cancel |
| FT2 CYCLE | Sys | Panic | Help | — | Exit/cancel |
| FT2 GENERATOR | Shape | Tool | Length− | Length+ | Amount |
| FT2 GENERATOR | Detail | Offset/phase/depth− | Offset/phase/depth+ | Context control or Roll shape | Collision/clone policy |
| FT2 GENERATOR | Value | Value− | Value+ | Repeat | Inspect |
| FT2 GENERATOR | Apply | Stop | Apply | Apply to Clone | Cancel |
| FT2 tools | Sys | Panic | Help | — | Exit |
| Live Patterns | Launch | Launch | Cancel queue | Retrigger current | Immediate |
| Live Patterns | Timing | Stop | Pattern boundary | Bar boundary | Capture |
| Live Patterns | Shape | Mute | Velocity | Gate | Transpose |
| Live Patterns | Sys | Panic | Append capture | Replace Arrangement | Exit |
| Loop Mix | Play | Stop slot | Launch slot | Previous slot | Next slot |
| Loop Mix | Mix | Level− | Level+ | Mute | Remove |
| Loop Mix | Filter | Filter− | Filter+ | Cancel queue | Align |
| Loop Mix | Sys | Panic | Import | Library | Exit |
| FT2 loop align | Ops | Auto | Bar− | Bar+ | Done |
| FT2 loop align | Sys | Panic | Help | — | Exit |
| FT2 record | Mode | — | Play | Record/stop | Edit |
| FT2 record | Capture | FEEL | — | — | — |
| FT2 record | Sys | Panic | N00B | Help | Exit |
| FT2 edit | Edit | Cell edit | Blank/skip | Erase | N-off |
| FT2 edit | Set | Note-length overlay | ADD 0–32 overlay | Column− | Column+ |
| FT2 edit | Size | Half | Row- | Row+ | Double |
| FT2 SIZE choice | Choose | Top/Copy/Confirm | Bottom/Empty | Cancel | — |
| FT2 edit | Sys | Panic | N00B | Help | Exit edit |
| FT2 cell edit | Route | Destination | Channel | Instrument | Timing |
| FT2 cell edit | Sound | Bank MSB | Bank LSB | Cell program | Clear field |
| FT2 cell edit | Cell | Note | Gate | Velocity | Effect |
| FT2 cell edit | Done | Panic | Save | Effect parameter | Exit/cancel |
| Files | Ops | Save | Load | Delete | MIDI import |
| Files | Project | New Project | Save As | Name/rename | Pattern tools |
| Files | Preview | — | Preview/stop | — | — |
| Files | Sys | Panic | — | Help | Exit |
| Routing-default prompt | Default | Confirm | Cancel | — | — |
| Routing-default prompt | Sys | Panic | — | — | Exit/cancel |
| Pattern tools | Ops | Blank new (`BLANK`) | Clone | Clear | Drum patterns |
| Pattern tools | Clip | Copy | Paste as new (`PASTE+`) | Paste over (`OVER`) | Clean unused (`CLEAN`) |
| Pattern tools | Trans | Octave− (`OCT-`) | Semitone− (`NOTE-`) | Semitone+ (`NOTE+`) | Octave+ (`OCT+`) |
| Pattern tools | Sys | Panic | — | Help | Exit |
| Drum patterns | Ops | Save | Load | Delete user | — |
| Drum patterns | Filter | Genre− | Genre+ | Meter | Size |
| Drum patterns | Move | First | Last | — | — |
| Drum patterns | Sys | Panic | — | Help | Exit |
| Arrange | Ops | Jump | Play | Append | Insert |
| Arrange | Step | Up | Down | Repeat | Remove |
| Arrange | Form | A A B A assistant (`AABA`) | — | — | — |
| Arrange | Sys | Panic | Help | — | Exit |
| A A B A draft | Form | B Pattern− | B Pattern+ | — | — |
| A A B A draft | Apply | Stop | Append | Replace | Cancel |
| A A B A draft | Sys | Panic | Help | — | Exit/cancel |
| Pattern setup | Ops | 3/4 | 4/4 | Length (`LNGTH`) | — |
| Pattern setup | Apply | Confirm | Keep | — | — |
| Pattern setup | Sys | Panic | — | Help | Exit/cancel |
| Tracks | Ops | Add four lanes | Target | Channel | Done |
| Tracks | Column | Column− | Column+ | Program− | Program+ |
| Tracks | Bank | MSB− | MSB+ | LSB− | LSB+ |
| Tracks | Sys | Panic | Entry layout | Help | Exit/cancel |
| Target/channel editor | Ops | Confirm | — | — | — |
| Target/channel editor | Sys | Panic | — | Help | Exit/cancel |
| Audio recorder | Record | Open 18-channel Levels | — | Record/toggle | Arm selected |
| Audio recorder | Track | Previous track | Next track | Assign source | Name track |
| Audio recorder | Setup | Arm all resolved | Disarm all | Refresh sources | — |
| Audio recorder | Sys | Panic | — | Help | Exit |
| 18-channel Levels | Take | Stop | Recorder setup | Record/toggle | Reset holds |
| 18-channel Levels | Channel | Previous | Next | Arm selected | Refresh sources |
| 18-channel Levels | Sys | Stop | Panic | Help | Exit |
| Routing | Edit | Previous row/value | Next row/value | Edit/OK | Cancel |
| Routing | Sys | Panic | Help | — | Exit |

## Routing editor contract

Routing opens in browse mode with a highlighted row. Rotary/Up/Down moves one
row and wraps; click/Enter opens a detached field draft; rotary/Up/Down changes
only that draft; click/Enter validates and confirms; Back/Esc restores the
original field. Back/Esc from browse returns Home. Re-entry always starts with
clean browse state.

Confirmation validates the complete runtime and controller candidate, creates
non-overwriting backups, atomically replaces both files, releases source-owned
notes/controller state, replaces SHR-owned MIDI inputs without layering, and
refreshes live discovery. Failure restores the old files and runtime route.
An audio-output change is saved for the next managed engine start and reported
as `AUDIO NEXT START` instead of being described as hot/live. Controller-clock
enable/output changes likewise report `CLOCK NEXT START`; live MIDI input role/
source changes activate immediately. Selecting
or confirming a MIDI output uses discovery only; it never opens an output as a
probe and never transmits.

The MIDI row shows the selected stable ALSA interface identity with an
`OFFLINE` or `AMBIG` suffix only when discovery cannot resolve it. A known
Device profile shows its configured model without claiming live detection;
raw external MIDI remains `UNVERIFIED`. `AudioBox` plus `D-50` is therefore
the normal concise presentation, while SHR never claims that the D-50 itself
was detected.

## FX editor and 40×13 text contract

Non-EQ processors use a spatial 2×4 grid matching the eight physical rotary
positions. Every control has its title above its value; the selected pair is
highlighted yellow while browsing and green while editing. Titles use clear
words such as `RATE`, `RATIO`, `ATTACK`, and `FEEDBACK`, while values retain
type-aware units.

At 40×13, EQ instead fills the display. Its 20-column plot maps 50 Hz through
20 kHz logarithmically and draws one `─` for each low, low-mid, high-mid, and
high band. Gain markers snap visually to the nearest labelled row from −18 to
+18 dB; displayed and editable gain values retain 0.5 dB precision. The master
rotary browses bypass, each band frequency and gain, low-cut state and
frequency, and output trim. Yellow means selected, green means editing, and a
bypassed EQ is dim. Knobs 1–4 remain logarithmic band frequencies and knobs
5–8 their half-decibel gains. Low cut is never misrepresented as knob 1.

All working-screen single-line regions have explicit terminal-cell budgets.
Static operational labels are written to fit; unpredictable device/file/user
names pass through cell-aware fitting; fixed label/value rows reserve the
selection marker and right-side state. Help remains the intentional wrapped,
scrollable prose surface. The controller footer, `DEV`/`REL` badge, Help, and
Exit areas retain their assigned cells at 40×13.

## FT2 cell editor inventory and mapping

A cell contains `note`, optional `velocity`, optional per-note `program`,
optional `gate`, independent signed `timing`, deterministic `probability`, one
loop-aware `condition`, and one `command`: none, cut, delay, retrigger, or
tempo. Project format stores all of these fields directly inside each FT2
Pattern.

| Page | Item 1 | Item 2 | Item 3 | Item 4 |
|---|---|---|---|---|
| Route | Destination | Channel | Instrument | Timing |
| Sound | Bank MSB | Bank LSB | Cell program | Clear selected field |
| Cell | Note | Gate | Velocity | Effect type |
| Done | Panic | Save | Effect parameter | Exit/cancel |

CHANCE, CONDITION, COND A, and COND B follow Timing in the cell editor's
rotary field sequence. They deliberately do not add a fifth direct-action page:
turn while inactive to select one, press to edit, and turn to change it.
Normal FT2 SOUND uses item 4 for the next-boundary FILL latch; the displaced
CLICK action remains on FT2 Tools SYS item 3. Keyboard `f` shares FILL.

The timing spacer uses `<` for early, `>` for late, and blank for on-grid. The
command marker still uses `C`, `D`, `R`, or `T`. The data model supports one
command plus timing per cell. Gate is 1–100% of a row or inherited; delayed
notes and retrigger pulses remain bounded. Program is routed before the shifted
note on the same exact target/channel.

Physical MIDI notes and CCs remain configuration. Profiles name only `PAD 1`
through `PAD 8`; the active screen gives pads 1–4 their page positions and
pads 5–8 their contextual STOP/PANIC, PLAY/LOAD/PREVIEW, capture, and TAP
meanings. Older semantic mappings still decode to those physical positions
without changing note numbers.

## Parameters, relative turns, and extension points

Menu navigation is discrete. Rotary 1 supplies relative navigation and its
click selects. Rotaries 2–16 supply up to fifteen continuous parameter slots;
each backend or editor supplies its own parameter table for those slots. The
MiniLab mkII discards incoming rotary position and carries SHR's
current parameter by signed steps across preset load, reset, mixer-bank changes,
and linked controls. Rotary 9's click is learned and persisted;
the verified synthv1 0.9.29 indices/ranges and green/yellow/red ±0.03 indicators
are unchanged. Existing models retain their 12-position synth tables and use
positions 13–15 for Project AUX 1, AUX 2, and AUX 3 sends on Player and FT2
PARAM. These Project controls are consumed before backend forwarding. Moj Sint
Dual Filter instead uses all 15 parameter rotaries for two filter blocks,
STRUCTURE, filter ADSR, and amp ADSR.

The master rotary always remains SHR navigation. Rotary 9's learned click is
also stored as the optional `synth.press_cc` or `synth.press_note` binding. On
Dual Filter it toggles only `CORE: INDUSTRIAL` and
`CORE: COUNTER`; release is consumed, current rotary values are preserved, and
the visible core state is saved with the preset.

All Shift-rotary secondary navigation uses either the profile/configured MIDI
modifier or the learned hardware Shift-layer CC plus the same semantic actions
as visible or keyboard navigation. The MiniLab mkII alternate CC is also
relative. Shift Learn names and proves both directions as separate gestures:
Shift plus repeated left packets, release, then Shift plus repeated right
packets on the same CC, followed by release. Its direction encoding is learned
independently and may oppose the ordinary axis.
For each performance rotary, Learn requires repeated left-direction
packets followed by repeated right-direction packets on the same channel and
CC. Positional or wrong-direction input is rejected and cannot resume on that
role until explicit retry or skip.

`Action` and the empty menu slots remain extension points. Future features are
not shown on the hardware menu until they actually dispatch a working action.
