# Automatic controller setup and MIDI learn

SHR-DAW uses a small reviewed input-controller catalog plus model-owned MIDI
learn. A
controller profile describes messages produced by physical knobs, encoders,
and buttons. It is different from an external-instrument profile, which
describes messages accepted by a synthesizer.

Run the normal setup wizard after connecting a controller:

```sh
shr-setup
```

The wizard selects an ALSA MIDI input, loads a matching known profile, or
offers non-audible MIDI learn for the missing controls. Learning never forwards
messages to a synth. The current surface model identifies all sixteen physical
rotaries, the clicks on rotaries 1 and 9, and numbered physical pads. Rotary 1
uses either supported direction convention for relative navigation. Rotaries
2–16 must report direction-only steps and carry SHR's current value. It never
shows or stores an instrument parameter or screen
action as a controller identity. An optional **ENCODER SHIFT** step learns the
rotary's Shift-layer turn used for secondary navigation. MiniLab 3 emits a
standalone modifier plus a shifted turn; MiniLab mkII consumes Shift internally
and emits the alternate encoder CC documented by Arturia. Each step keeps the
first qualifying gesture. The in-app learner
visibly keeps `OK` on that role until the physical gesture is finished: a
button advances on its matching CC-off, Note Off, or velocity-zero Note On,
while a rotary advances automatically after its CC
stream has been quiet for the short settle period.
Extra values and encoder neutral/reset packets extend that same gesture instead
of becoming the next role. On entry, release the control that opened MIDI Learn
and wait for the ready indication; its release and already queued traffic are
quarantined.

Each in-app session replaces the private `midi-learn-last.log` in SHR's
configured state directory. The trace records the requested physical step,
learner state, and every received MIDI message as hexadecimal bytes, including
traffic that is filtered or rejected. It remains after Save or Cancel so the
last failed physical attempt can be inspected without repeating it first.

First turn rotary 1 left and let it settle, turn it right and let it settle,
then click and release it. At the optional encoder Shift step, press Shift and
turn rotary 1 left until three left packets are verified, then release Shift.
Press Shift again, turn right until three right packets are verified on the
same CC, then release Shift again. Learn waits briefly for the shifted turn so a Shift
button packet cannot win before the rotary packet arrives. Learn stores
either an explicit MIDI modifier plus its relative turn CC, or the MK2-style
alternate relative CC when Shift itself produces no MIDI event. The shifted
axis learns its own direction encoding; it may be the reverse of the ordinary
rotary. Skipping Shift remains valid. Learn then proceeds literally through
rotaries 2, 3, 4, and so on to rotary 16 before the PAD positions. Rotary 9's
click is captured immediately after its turn, just as rotary 1's special click
and Shift actions stay with rotary 1.
As soon as rotary 1's
left/right axis is learned, turn it left or right to move one Learn step back
or forward; no click is required. The selected step re-enters the short input
quarantine, so trailing packets from that turn cannot move twice. For each
performance rotary, turn left slowly until Learn verifies three left-direction
packets, let the gesture settle, then turn the same rotary right until three
right-direction packets are verified. The MIDI channel and CC must match in
both directions. Once a candidate rotary has started its proof, traffic from a
different CC is ignored without losing that proof. A wrong-direction or
positional packet from the candidate itself rejects that attempt, waits for
the gesture to be released, and automatically re-arms the same step. Direction
changes, success, and retry remain visible through a 650 ms quiet window, which
also prevents late packets from the previous turn being treated as the next
requested direction. Packet proof counts remain in the private trace rather
than flashing onscreen. Sweeping
a positional knob through the values around 64 therefore cannot become a
mapping and needs no keyboard recovery. One completed left/right proof moves
by exactly one role regardless of how many packets either gesture emits. Each
learned PAD advances after release. The
learner ends at one explicit Review step; only there does a rotary-1 click save
the mappings under the reviewed controller model, make a backup, activate that
model, and exit after release. Earlier clicks cannot save or end the session.
The controller workflow requires no computer keyboard. The in-app Learn screen
shows exactly two rows total and no status footer. The first row starts with
the complete required action; the second contains only its immediate
instruction, success, or retry. It does not repeat the Learn title, mapping
counts, safety commentary, recovery prose, or navigation summaries. Conflicting assignments
from a different already-mapped control are rejected without replacing an
accepted `OK` message with errors from trailing traffic. Relative encoders
using either the center-64 convention (61–63 left and 65–67 right) or high/low
values (125–127 left and 1–3 right, with neutral 0) are supported.

The first twelve learned turns, physical rotaries 2–13, control the existing
twelve-parameter instruments and mixer positions. They add or subtract from
SHR's value immediately and have no physical position to catch. Dual Filter uses rotaries 2–16 for its
fifteen parameters and rotary 9's click for its core toggle. On other current
instruments, turns 14–16 and the second click remain safely consumed without
leaking their messages to the synth.

SHR does not guess how many pads the controller has. The in-app path determines
an eight-pad layout when any of the first four page-row positions is captured,
a five-pad layout when the alternate first-pad page-cycle gesture is captured,
and otherwise a four-pad action-row layout. The displayed identities remain
`PAD 1` onward in every case; page and action meanings belong to the runtime
layout and current screen.

Page-cycle may be one dedicated button or a held modifier plus another control.
For a dedicated button, press and release it once, then press it again to
confirm; this prevents a single exploratory Shift press from becoming the
mapping. For a chord, hold the modifier and move or press the intended trigger,
then release the modifier. The trigger may reuse a normally mapped knob or
button because it cycles the page only while that learned modifier is held;
one held chord triggers once regardless of packet count. Partial layouts are
valid, so spare hardware buttons can remain musical or unassigned.

The generic installed `controller.conf` is deliberately empty. An unknown
device therefore remains a normal musical MIDI input instead of accidentally
inheriting another controller's command notes. Selecting a different unknown
device with `shr pads auto` clears the previous device's mappings before MIDI
learn begins; the old file is backed up first.

## Commands

```sh
shr pads ports                 # list detected MIDI inputs
shr pads profiles              # list installed known profiles
shr pads auto [PORT_MATCH]     # select input and apply a known profile
shr pads learn [PORT_MATCH]    # learn only what remains unassigned
shr pads update                # download the reviewed SHR catalog
shr pads list                  # show the resulting mapping
shr pads rotary 2 74           # bind physical ROTARY 2 to incoming CC 74
shr pads pad 1 note 10 36      # bind PAD 1 to channel-10 note 36
```

The bundled catalog lives in `controller-profiles/catalog.json`. Installation
copies it below `share/shsynth/controller-profiles/`. `shr pads update`
downloads the current catalog from the SHR-DAW public repository, validates it
fully, and atomically installs it below
`${XDG_DATA_HOME}/shsynth/controller-profiles/`. Set
`SHSYNTH_CONTROLLER_PROFILE_DIR` for a private override. `controller.conf` is
the active private mapping. Each explicitly learned known model is also
retained in the private state directory as
`controller-mappings/PROFILE-ID.conf`. On later startup, the sole connected
reviewed controller automatically restores its model-owned mapping; a model
without a learned copy uses its bundled reviewed default. Automatic switching
backs up and replaces only the active selector and never overwrites another
model's learned copy.
The setup helper uses `SHSYNTH_STATE_DIR` internally when an explicit
`--state-dir` is supplied.

When no private `controller.conf` exists, startup uses the explicitly
configured controller input to select one unique reviewed profile before the
MIDI router opens. When a previously selected controller is offline, startup
also adopts one exact connected endpoint if and only if it is the sole endpoint
with a reviewed profile. It rebuilds the mapping from that profile instead of
copying messages from the absent device. Unknown inputs and multiple reviewed
replacement candidates remain unselected rather than guessed.

The bundled MiniLab 3 default retains only its verified direction-only and
button mapping: encoder turn CC 114 and press CC 115 on channel 1, plus the eight
Arturia/DAW factory pads on channel 10. The currently learned Shift CC 9 on
channel 1 is the held encoder modifier, and its shifted turn is relative CC
112.
Ordinary turns therefore stay on the directly learned CC114 while held Shift
turns are classified on CC112. The earlier reviewed DAW-mode CC27 modifier and
CC29 shifted turn remain a catalog-declared compatibility variant, so an older
learned MiniLab mapping receives only its missing shifted CC in memory; SHR
does not rewrite the private file. Its positional parameter knobs are not
mapped. Unknown or ambiguous controllers remain
unmapped rather than inheriting this device-specific default.

The bundled MiniLab mkII entry is deliberately a partial identity profile. The
official hardware manual establishes its sixteen assignable encoders, clickable
encoders 1 and 9, and two banks on eight physical pads, but those controls can
emit user-programmed messages from several hardware memories. Startup can
therefore select one connected MK2 automatically, then recommends MIDI Learn;
it assigns no rotary, PAD, encoder, or command message before direct learning.

## Upstream mapping sources

There is no universal controller-description standard. These projects provide
useful input-controller knowledge:

- [Ardour MIDI maps](https://github.com/Ardour/ardour/tree/master/share/midi_maps)
  cover many keyboard controllers and control surfaces.
- [Mixxx controller mappings](https://github.com/mixxxdj/mixxx/tree/main/res/controllers)
  cover many DJ and grid controllers, including USB identifiers and scripts.
- [Zynthian controller drivers](https://github.com/zynthian/zynthian-ui/tree/master/zyngine/ctrldev)
  demonstrate matched plug-and-play drivers plus MIDI learn.

Their mappings bind hardware to application-specific actions and may execute
device setup or LED scripts. SHR-DAW does not download or run those files.
Reviewed profiles may use their documentation as a source, but raw note/CC
facts must be verified on hardware and recorded with provenance. This keeps a
foreign transport command from silently becoming an SHR panic, record, or
navigation command. Those upstream repositories use copyleft licences; none
of their mapping data is included in the MIT SHR catalog.

The [Pencil Research MIDI dataset](https://github.com/pencilresearch/midi) is
CC BY-SA 4.0 and is valuable for external synth CC/NRPN and drum-note profiles.
It does not describe the physical controls emitted by USB input controllers,
so it is not used for controller autoloading.

## Catalog profile format

Each JSON entry has stable `id`, display `name`, normalized ALSA
`match_names`, a 4/5/8-pad layout, and any known mappings. `rotaries` maps
physical rotary numbers 2–16 to incoming CC numbers; rotary 1 has separate
turn fields. The in-app left/right gestures accept Arturia Relative 1 and
Relative 2 and reject positional 0–127 output with an instruction to change
the hardware mode. `note_pads` and `cc_pads` map
one-based physical PAD positions to incoming notes or CCs; the parallel
`note_pad_channels` and `cc_pad_channels` objects qualify those positions with
1-based MIDI channels. MIDI learn records the observed channel for every PAD,
and save/load retains it. Instrument parameters and screen commands are absent
from this schema.
Rotary 1 turn, optional shifted turn, rotary 1 press, rotary 9 synth press,
held modifier, and optional lock messages are separate so they cannot collide
with continuous controls. The held modifier is stored as
`encoder.modifier=cc.CHANNEL.NUMBER` or
`note.CHANNEL.NUMBER`. A controller that changes the rotary CC while Shift is
held also stores `encoder.modified_relative_cc`; its direction convention uses
`encoder.modified_relative_reverse`. A direct alternate relative CC is valid
without a standalone modifier when hardware consumes Shift internally.
Relative-mode recognition does not
depend on the controller emitting a neutral packet between the learned left and
right turns. All physical note and CC numbers must be
valid MIDI data bytes (0–127), and an encoder press cannot reuse a PAD note.
Rotaries 2–16 always emit signed direction steps; their incoming values are
never treated as parameter positions.
An optional `shifted_encoder_compatibility` array retains previously reviewed
ordinary CC, modifier, shifted CC, direction, and channel tuples. These entries
never change a fresh profile; they only complete an older learned map in memory
when every identifying field matches.
Learned page-cycle chords are stored as `page_cycle.modifier` and
`page_cycle.trigger` values such as `cc.1.27`; the modifier and trigger must be
different messages, while the trigger may deliberately reuse a normal mapping.

Profiles may be partial. After one is loaded, `shr pads learn` asks only for
rotary, PAD, and encoder positions that are still empty.

Version-7 private files and older downloaded catalogs remain parseable.
Semantic button roles are converted in memory to physical positions, while
old positional continuous-control entries are dropped rather than guessed as
relative steps. SHR does not rewrite an existing private file merely to
migrate it; the next explicit save writes only learned relative `rotary.2`
through `rotary.16` syntax.

The reviewed MiniLab 3 profile maps `PAD 1` through `PAD 8` to factory
Arturia/DAW notes 36–43 on channel 10. At runtime the eight-pad layout uses
positions 1–4 for pages and positions 5–8 for the current screen's contextual
actions. The profile itself contains none of those action names.
Direct capture on this unit found User 1 pads on channel 1, the same
channel as its keyboard, so User 1 pads are not safe command pads: their
messages are indistinguishable from keyboard notes. The current learned Shift
emits CC9 and is bound only as the held encoder modifier; its shifted rotary
CC112 is accepted only while that configured modifier is down. Neither is a
persistent pad lock, so normal arpeggiator, program, and bank gestures cannot
toggle SHR lock state. The earlier reviewed DAW-mode CC27/CC29 pair remains
compatible but is not the fresh-profile default.
Selecting the controller's DAW program does not itself require a proprietary
DAW script for these ordinary MIDI note commands. Arturia mode has the same
captured channel-10 pad notes, so use DAW mode only if another ordinary mapping
has been verified to be useful.
