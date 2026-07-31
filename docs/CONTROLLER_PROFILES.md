# Automatic controller setup and MIDI learn

SHR-DAW uses a small reviewed input-controller catalog plus MIDI learn. A
controller profile describes messages produced by physical knobs, encoders,
and buttons. It is different from an external-instrument profile, which
describes messages accepted by a synthesizer.

Run the normal setup wizard after connecting a controller:

```sh
shr-setup
```

The wizard selects an ALSA MIDI input, loads a matching known profile, or
offers non-audible MIDI learn for the missing controls. Learning never forwards
messages to a synth. It identifies `POT 1` through `POT 12`, numbered physical
pads, either direction convention for a relative encoder, and a CC or note
encoder press. It never shows or stores an instrument parameter or screen
action as a controller identity. An optional **ENCODER SHIFT** step learns
the held button plus the rotary's held left/right messages used for secondary
navigation. Each step keeps the first qualifying gesture. The in-app learner
visibly keeps `OK` on that role until the physical gesture is finished: a
button advances on its matching CC-off, Note Off, or velocity-zero Note On,
while a knob/fader or relative encoder advances automatically after its CC
stream has been quiet for the short settle period.
Extra values and encoder neutral/reset packets extend that same gesture instead
of becoming the next role. On entry, release the control that opened MIDI Learn
and wait for the ready indication; its release and already queued traffic are
quarantined.

First turn the master encoder left and let it settle, turn it right and let it
settle, then click and release it. At the optional encoder Shift step, hold the
modifier, turn left once, then release it; Learn stores either the
ordinary rotary CC or the different relative CC emitted only while Shift is
held. Skipping remains valid. The learned encoder then browses the optional
numbered POT and PAD positions.
One rotary gesture moves by exactly one role,
regardless of how many packets it emits. Each learned POT advances
automatically after settling, and each learned PAD advances after release. The
next clean encoder click saves the mappings
learned so far, makes a backup, activates the new file, and exits after release;
Esc cancels and keeps the previous file. Conflicting assignments from a
different already-mapped control are rejected without replacing an accepted
`OK` message with errors from trailing traffic. Relative encoders using either
the center-64 convention or high/low values such as 125–127 left, 1–3 right,
and neutral 0 are supported.

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
shr pads pot 1 74              # explicitly bind POT 1 to incoming CC 74
shr pads pad 1 note 10 36      # bind PAD 1 to channel-10 note 36
```

The bundled catalog lives in `controller-profiles/catalog.json`. Installation
copies it below `share/shsynth/controller-profiles/`. `shr pads update`
downloads the current catalog from the SHR-DAW public repository, validates it
fully, and atomically installs it below
`${XDG_DATA_HOME}/shsynth/controller-profiles/`. Set
`SHSYNTH_CONTROLLER_PROFILE_DIR` for a private override. Machine-specific
learned mappings remain in the private state directory as `controller.conf`.
The setup helper uses `SHSYNTH_STATE_DIR` internally when an explicit
`--state-dir` is supplied.

When no private `controller.conf` exists, startup uses the explicitly
configured controller input to select one unique reviewed profile before the
MIDI router opens. The bundled MiniLab 3 default mirrors the verified learned
mapping: encoder turn CC 114 and press CC 115 on channel 1, plus the eight
Arturia/DAW factory pads on channel 10. The currently learned Shift CC 9 on
channel 1 is the held encoder modifier, and its shifted turn is relative CC
112.
Ordinary turns therefore stay on the directly learned CC114 while held Shift
turns are classified on CC112. The earlier reviewed DAW-mode CC27 modifier and
CC29 shifted turn remain a catalog-declared compatibility variant, so an older
learned MiniLab mapping receives only its missing shifted CC in memory; SHR
does not rewrite the private file. Unknown or ambiguous controllers remain
unmapped rather than inheriting this device-specific default.

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
`match_names`, a 4/5/8-pad layout, and any known mappings. `pots` maps one-based
physical POT positions to incoming CC numbers. `note_pads` and `cc_pads` map
one-based physical PAD positions to incoming notes or CCs; the parallel
`note_pad_channels` and `cc_pad_channels` objects qualify those positions with
1-based MIDI channels. MIDI learn records the observed channel for every PAD,
and save/load retains it. Instrument parameters and screen commands are absent
from this schema.
Encoder turn, optional shifted turn, press, held modifier, and optional lock
messages are separate so they cannot collide with continuous controls. The
held modifier is stored as `encoder.modifier=cc.CHANNEL.NUMBER` or
`note.CHANNEL.NUMBER`. A controller that changes the rotary CC while Shift is
held also stores `encoder.modified_relative_cc`; its direction convention uses
`encoder.modified_relative_reverse`. All physical note and CC numbers must be
valid MIDI data bytes (0–127), and an encoder press cannot reuse a PAD note.
An optional `shifted_encoder_compatibility` array retains previously reviewed
ordinary CC, modifier, shifted CC, direction, and channel tuples. These entries
never change a fresh profile; they only complete an older learned map in memory
when every identifying field matches.
Learned page-cycle chords are stored as `page_cycle.modifier` and
`page_cycle.trigger` values such as `cc.1.27`; the modifier and trigger must be
different messages, while the trigger may deliberately reuse a normal mapping.

Profiles may be partial. After one is loaded, `shr pads learn` asks only for
POT, PAD, and encoder positions that are still empty.

Version-7 private files and older downloaded catalogs remain readable. Their
synthv1 target CCs and semantic button roles are converted in memory to the
equivalent one-based positions. SHR does not inspect or rewrite an existing
private file merely to migrate it; its next explicit save writes positional
version-8 syntax.

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
