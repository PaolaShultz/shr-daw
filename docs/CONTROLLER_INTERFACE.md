# Controller action inventory and paging design

This document is the implementation checklist used for the paged controller
interface. The authoritative inventory was taken from `ui.rs` keyboard, mouse,
encoder, command-pad, screen, and contextual dispatch paths before paging was
implemented.

## Pre-implementation action inventory

| Screen or mode | Existing user-facing operations and input paths |
|---|---|
| Presets | Select previous/next, page up/down, first/last (keyboard, wheel, encoder); previous/next engine (keyboard/pads); load selected sound (keyboard, mouse, encoder/pad); tracker, ideas, and audio screens (keyboard/pads); stop synth/panic and exit (keyboard/mouse/pad/back). |
| Playback | Reset the 12 mapped parameters in place (encoder press); record/stop/finish-and-save MIDI, play/stop take, save idea (keyboard/pads/mouse); presets/back, ideas, tracker, audio (keyboard/pads/mouse); tap tempo; stop/panic. The 12 configured synthv1 CC controls continuously adjust parameters with pickup. |
| Ideas | Previous/next/first/last idea (keyboard, wheel, encoder); inspect (keyboard/mouse/pad); load with replace confirmation (encoder); play take; delete with repeat confirmation; record/stop MIDI; save timestamped or numbered idea; back/cancel, tracker, audio, presets, panic. |
| FT2 normal | Previous/next row (keyboard/encoder); previous/next lane and cross-page lane movement (keyboard/pads); visible page switch (Tab); previous/next order position (keyboard/pads); play from cursor or start, stop/back (keyboard/pads); enter edit; lane and page mute; page manager and file manager; program and tempo decrement/increment; tap tempo; save/load/new/clone/clear pattern and repeat/remove order shortcuts. |
| FT2 edit | All cursor and transport operations; musical keyboard or incoming MIDI note/chord gesture entry; blank/skip; erase; note off; leave edit; lane mute; program and tempo adjustment. Command notes are consumed for editing and never doubled through the synth. |
| FT2 cell edit | Transactional note, gate, velocity, per-note program, single command type/parameter, clear-field, confirm/cancel, step-entry handoff, stop, and panic actions. Four-button encoder page selection remains available. |
| Tracker files | Select saved song; load; preview/stop; save with overwrite confirmation; delete with repeat confirmation; new and clone pattern; clear immediately or choose confirmed 3/4 (24 rows) / 4/4 (32 rows); previous/next/repeat/remove order entry; back/cancel and panic. |
| Pattern-clear dialog | Choose 3/4 or 4/4, confirm destructive clear, cancel, or use the existing keep-current-size clear operation. |
| Page/track manager | Select previous/next page; add four-lane page; edit target; edit channel; confirm all changes; cancel and restore the original song; open files; mute current page. |
| Target/channel field mode | Previous/next choice, confirm field, cancel field. Encoder turn/press and menu items share these operations. |
| Audio recorder | Start/toggle recording, stop/finalize, inspect status, back, open presets/ideas/FT2, and panic. |
| Global/safety | Stop MIDI playback, tracker transport, recorder, managed engine, and owned notes; All Notes Off; cancel pending edit gestures; back; exit. Process termination remains limited to the engine owned by SHR-DAW. |

The complete final screen × page × item mapping is maintained in the README.
`src/navigation.rs` is the executable canonical copy: labels, enabled/disabled/
planned state, and dispatch action are one definition. A unit test builds the
union of every normal and contextual menu and checks every action in this
inventory for controller reachability.

## Input model

- Eight buttons: four direct page selectors plus four item buttons.
- Five buttons: one page-cycle button plus four item buttons.
- Four buttons: four item buttons; encoder press enters/leaves page-selection
  mode and encoder turn changes pages while that mode is visible.
- Outside four-button page-selection mode, encoder turns retain list, row, and
  field adjustment. Encoder press retains the existing select/confirm action on
  eight- and five-button layouts. Menu slots do not duplicate those master
  rotary selection actions.
- Each screen remembers its last selected page. Entering/leaving a contextual
  mode resets that context to page 1, preventing stale hidden meanings.

## FT2 cell editor inventory and mapping

A cell contains `note`, optional `velocity`, optional per-note `program`,
optional `gate`, and one `command`: none, cut, delay, retrigger, or tempo. Song
format v1 stores all of these fields directly. There are no earlier song
formats or migration paths in the release candidate.

| Page | Item 1 | Item 2 | Item 3 | Item 4 |
|---|---|---|---|---|
| Fields | Note | Gate | Velocity | Program |
| Effect | Effect | Parameter | Clear selected field | Step entry |
| Adjust | Previous field | Next field | Value− | Value+ |
| Finish | Confirm | Cancel/back | Stop | Panic |

The first display spacer uses `C` for cut, `D` for delay, `R` for retrigger,
`T` for tempo, and blank for no command. The data model supports one command
per cell. Gate is 1–100% of a row or inherited; delayed notes and retrigger
pulses are bounded by the row. Program is a per-note override of the page
program, routed before the note on the same exact target/channel.

Physical MIDI notes and CCs remain configuration. Old v1 `arp`, `pad`, `prog`,
`loop`, `stop`, `play`, `rec`, and `tap-tempo` pad role names load as the same
physical first-through-eighth positions, so existing local profiles migrate to
page 1–4 and item 1–4 without changing note numbers.

## Parameters, pickup, and extension points

Menu navigation is discrete. The 12 synthv1 controls are continuous and remain
on configured CCs. Preset load, idea load, and in-place reset re-arm pickup;
the verified synthv1 0.9.29 indices/ranges and green/yellow/red ±0.03 indicators
are unchanged. `MAPPED_CONTROL_CAPACITY` reserves 16 entries while only the 12
schema-verified controls are populated.

`Action` and `SlotState` are extension points. Playback's planned `ARP` slot can
later dispatch arpeggiator state/actions. Files' planned `WAV LOOP` slot can
later open a context for one WAV, BPM detection/entry, sequencer sync, loop
start/stop/replacement, and synchronized transport. Both slots currently have
no dispatch action and cannot accidentally invoke a neighboring operation.
