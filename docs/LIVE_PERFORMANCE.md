# Live performance

SHR-DAW has two complementary performance systems in the FT2 workspace:

- **Live Patterns** launches existing MIDI tracker Patterns.
- **Loop Mix** launches and mixes up to four private WAV loops or stems.

They share transport timing, safety controls, and the configured controller,
but they are separate Project data and separate runtime owners. A Pattern is
MIDI tracker material; a Loop Mix slot is decoded audio. Neither turns
SHR-DAW into a full DJ application.

## Live Patterns

Open **FT2** → **TOOLS** → **LIVE**. The screen shows four Patterns from the
current group. Turning the encoder or using Up/Down browses through all
Patterns in groups of four. Browsing changes only the white selection; it does
not launch anything and does not move the FT2 Arrangement Step, row, page,
lane, column, or edit cursor.

The display distinguishes:

- the selected Pattern with the selection highlight;
- the sounding Pattern with green `PLAY`; and
- the queued Pattern with yellow `Q`.

**LAUNCH** or Enter queues the selected Pattern. The first Live Pattern starts
directly because there is no earlier Live Pattern boundary to wait for.
**NOW** is the deliberate immediate action. **RETRIG** queues the currently
playing Pattern again, including when it is also the selected Pattern.
**CANCEL** removes a pending launch without changing playback. A later launch
before the boundary replaces the earlier queue.

The **TIMING** controller page selects either:

- **PAT Q** — activate at the end of the current Pattern; or
- **BAR Q** — activate at the next complete Project-meter bar.

The keyboard equivalents are `l` for quantized launch, `L` for immediate
launch, `r` for retrigger, `c` for cancel, and `q` to toggle Pattern/bar
quantization. `s` or Space is literal Stop. Panic remains on the SYS page and
through the controller's global safety action.

Live Patterns does not change the saved Arrangement. Outside this screen the
ordinary Arrangement plays exactly as saved.

### Note and instrument ownership

A successful quantized change transfers a lane only when its destination,
channel, held note, and next Pattern event still agree. Otherwise SHR-DAW sends
the exact old note-off before the new event. It does not release all lanes just
because the Pattern number changed.

If the queued Pattern needs a different software instrument, the old owners
are released at the activation boundary, the one managed engine is replaced,
and the new Pattern begins only after the replacement succeeds. SHR-DAW never
layers managed synth engines. A failed replacement restores the previous
managed session where possible and resumes the previous Pattern from a clean
boundary; the failed launch is not captured.

Immediate launch deliberately releases the old scheduled owners first. Stop,
lane mute, target loss, Project replacement, Panic, shutdown, and application
exit release the exact destination/channel/note owners and retain the normal
all-channel panic. Missing or ambiguous external targets remain visible and
are never replaced with another route.

## Capturing a Live Pattern performance

**CAPTURE** arms an empty temporary list. Only successful Pattern activations
at their actual boundaries are added. Browsing, a replaced or cancelled queue,
a failed launch, and ordinary Pattern looping are not captured.

Press **CAPTURE** again to stop and enter the reversible confirmation state.
Then choose:

- **APPEND** to add the captured Pattern references after the existing
  Arrangement; or
- **REPLACE** to replace the Arrangement with the captured references.

Back/CAPTURE cancels confirmation and leaves the original Arrangement
unchanged. An empty capture cannot replace it. Repeated launches remain
repeated Pattern IDs; Pattern rows and pages are not cloned.

## Live lane shaping

The lower half of Live Patterns controls the selected Pattern page's four MIDI
lanes. Left/Right selects a lane. The controller SHAPE page or keyboard offers:

- `m` — transient live mute;
- `v` — velocity/intensity scaling;
- `g` — gate-length scaling; and
- `t` — transpose in semitones.

After choosing velocity, gate, or transpose, turn the main encoder or use
Up/Down; press Enter to return to Pattern browsing. Velocity and gate are
bounded to 10–200%. Transpose is bounded to -48..+48 semitones. Resulting MIDI
velocity and gate values remain in their legal ranges, and transposed notes
are clamped to 0–127 without wrapping.

These controls shape a runtime copy of the Pattern. They never rewrite stored
cells or persisted lane settings. Velocity, gate, and transpose changes take
effect on the next activation boundary, so changing them while a note is held
cannot release another owner. Mute releases only that live lane immediately.
Repeated notes continue to use the sequencer's exact ownership ledger.

The state survives navigation while the same Project is open. Loading or
creating a Project resets every live shape to 100% velocity, 100% gate,
zero transpose, and unmuted. The main encoder is a relative navigation
control from `controller.conf` or the selected controller profile, so these
controls do not introduce hard-coded CCs or an invisible absolute-knob mode.
Any absolute mapped control elsewhere in SHR-DAW retains the established
pickup rule and is re-armed when values load or reset.

Lane shaping is MIDI performance data, not per-lane audio processing. Several
lanes can share one stereo synth output, and external devices do not share a
universal filter CC.

## Loop Mix

The fourth FT2 page is **Loop Mix**. It shows four Project WAV slots at once
and keeps selection separate from activation. Left/Right or **SLOT-**/**SLOT+**
selects a slot. Each row shows only useful state:

- `PLAY` or `STOP`;
- `Q▶` or `Q■` for a queued bar-boundary command;
- `MUTE`;
- `FLT`; or
- `—` for an empty/missing slot.

Each slot stores a private mono or stereo WAV filename, source BPM,
half/normal/double interpretation, non-destructive start and length in beats,
whole-bar placement offset, level, and bipolar filter. Different loop lengths
are allowed when they are whole bars at the same interpreted tempo.

**LAUNCH** and **STOP** queue independent actions for the next Project bar.
Repeated commands replace the earlier action for that slot. **CANCEL** clears
it. Keyboard equivalents are `p`, `P`, and `c`. `m` toggles mute. Level is
bounded to 0–150%; the slot filter is bounded to -100..+100%.

At filter centre a small deadband is neutral. Turning left progressively
low-passes the slot; turning right progressively high-passes it. `0` returns
to neutral. Level and filter changes are smoothed. Filter state and output are
finite and bounded under rapid movement.

**IMPORT**/**LIBRARY** uses the existing private browser for the selected slot.
Inbox WAVs are copied without replacement below private XDG/user storage;
existing private/current/saved entries attach without copying. **REMOVE**
requires confirmation and removes only the selected Project reference while
keeping the private WAV. Preview remains explicit and stops on selection
change, Stop, Back, browser close, or leaving the browser.

### Tempo, rate, and failures

Loop Mix does not time-stretch or preserve pitch while changing tempo. Every
active slot's interpreted BPM must equal the current Project tempo. An
incompatible slot is refused rather than allowed to drift. Each WAV must also
match the current JACK sample rate; SHR-DAW does not resample it in the
callback. Corrupt, oversized, missing, incompatible, or failed WAVs fault only
their own slots, and healthy slots continue.

All four renderers follow the same transport origin, meter, and bar clock.
Their phase is derived from Project beat position and placement offset, so
different whole-bar lengths remain aligned across repetition and restart.
Stop, Project replacement, JACK shutdown, and exit stop all four.

## Audio routing and realtime limits

The four WAV renderers sum inside one owned `shs-loop` JACK client and expose
the existing logical Loop stereo source. The final bus therefore remains
exactly Synth, Loop, and Input. It does not gain four general-purpose strips.

In direct mode the Loop output owns its configured playback connections. A
final-bus transaction moves that one summed source into the graph and removes
the direct path, with rollback on failure. The limiter, `FINAL OUT`, and final
stereo recorder receive the complete four-slot sum once. SHR-DAW does not
create or disturb unrelated JACK connections.

WAV file access, decoding, validation, import, and analysis happen outside the
JACK callback. The callback has four fixed slot positions and fixed meter/DSP
memory. It performs no allocation, locking, file access, formatting, or
unbounded work.

## Deliberate limits

There are no cue/headphone buses, crossfader, scratching or jog-wheel
emulation, beat-grid editor, automatic beat matching, time-stretching,
pitch-preserving tempo change, waveform editor, unrelated track library, or
new general-purpose mixer strips.
