# Controller pad LED feedback

Status: researched integration proposal, 2026-09-28. SHR-DAW currently reads
controller commands but does not drive controller LEDs. No controller firmware,
memories, mappings, backlight settings, or LEDs were changed for this research.

## MiniLab mkII capability

The MiniLab mkII can receive pad colour commands through its MIDI output
endpoint. These are manufacturer-specific SysEx messages; sending ordinary
note-on messages back to the controller does not provide LED feedback.
Arturia staff confirm that distinction in their
[LED control discussion](https://legacy-forum.arturia.com/index.php?topic=93116.0).

The independently documented
[MiniLab mkII protocol](https://github.com/mhugo/sysex#arturia-minilab-mk-ii)
uses this message, with hexadecimal bytes:

```text
F0 00 20 6B 7F 42 02 00 10 PP CC F7
```

`PP` is `70` through `7F`, addressing logical pads 1–16. Eight physical pads
provide two banks. `CC` selects one of the following documented values:

| Colour | Hex byte |
| --- | --- |
| Off | `00` |
| Red | `01` |
| Green | `04` |
| Yellow | `05` |
| Blue | `10` |
| Magenta | `11` |
| Cyan | `14` |
| White | `7F` |

For example, pad 1 green is `F0 00 20 6B 7F 42 02 00 10 70 04 F7`;
replace `04` with `00` to turn it off. This is a discrete colour palette, not
an established arbitrary RGB or brightness interface. Do not reuse the
MiniLab 3 RGB protocol for the mkII.

The [Arturia manual, sections 2.1.7 and 4.7.6](https://dl.arturia.net/products/minilab-mkII/manual/minilab-mkii_Manual_1_1_EN.pdf)
explains the separate **Pad off Backlight** setting: with it enabled, an
inactive pad uses its preset colour and an activated pad becomes white. With
it disabled, an inactive pad is dark. White therefore cannot distinguish
activation when that setting is enabled. Pad-bank and Shift gestures also
have hardware-owned meanings. The documented colour command is sufficient to
plan a driver, but interaction with these modes still needs validation on the
connected unit and firmware.

## Recommended first use in SHR

Start with the existing eight-pad menu layout: reflect the same page and
action table that draws the two controller rows. This makes feedback useful
across working screens without a separate set of screen-specific mappings.
The following colours are proposed, not implemented:

| Meaning | Proposed indication |
| --- | --- |
| No action at this pad | Off |
| Available navigation or command | Blue |
| Selected menu page | Cyan |
| Playing / enabled toggle | Green |
| Stopped transport | White |
| Recording | Red |
| Pending or queued action | Yellow |

Next candidates are LIVE's playing/queued Patterns, recording arms, mute/solo,
and effect bypass. A light must always describe what pressing that physical
pad will do in the current screen and bank. A global transport colour must
not hide a different current pad action. Keep the on-screen labels and shared
status row: colour supplements them and cannot be the only indication.

Use steady states first, including solid red for recording. Brightness or
pulsing needs a verified device capability; beat animation also needs its own
MIDI traffic budget and listening evidence.

## Implementation boundary

The existing profile/learner supplies input identity and physical-pad actions.
It does not currently describe an LED output protocol or establish that a
learned note/CC number equals a hardware LED address. A first implementation
needs:

1. An explicit output/protocol capability in `controller.conf` or the reviewed
   profile, with an unambiguous matching output endpoint. Never choose the
   first MIDI output or send LED data to the synth/external instrument route.
2. A pure conversion from the canonical menu/action state to eight desired
   colours. Map logical banks explicitly and preserve Shift/memory selection.
3. One non-audio worker with a bounded latest-state mailbox. Coalesce changes,
   send only changed pads, and enforce a small update-rate limit. Do not send
   from JACK or hold the performance-MIDI path while writing feedback.
4. Repaint after reconnect and after device gestures that overwrite LEDs;
   verify press/release behaviour on the target firmware. An output failure
   must leave keyboard, command input, and audio usable.
5. A tested exit policy for the claimed pads. Do not save to hardware memory,
   change gate/toggle mode, or rewrite global backlight preferences merely to
   display application state. Restore an established prior display when the
   protocol permits; otherwise document the transient exit state explicitly.

The factory palette and protocol bytes belong with the device profile/driver;
hardware endpoint names remain configuration. Unknown controllers retain the
current input-only behaviour. Reuse the existing ownership and reconnect
rules rather than adding automatic MIDI routing.

## Acceptance before enabling feedback

Offline tests should cover every palette/address byte, invalid values, both
banks, menu/context transitions, bounded coalescing, output failures, and
shutdown/reconnect. Hardware acceptance must separately check pad release,
Shift, bank/memory changes, persistence, disconnection, and continued musical
input. The first hardware trial should change one pad and restore it before
trying a whole-screen update. That trial requires an explicit controller-write
session; this research did not perform one.
