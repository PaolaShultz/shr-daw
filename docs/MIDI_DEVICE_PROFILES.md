# MIDI device profiles

External instrument knowledge is JSON data, not Rust device logic. Profiles
live in `midi-devices/`; installation copies them to
`share/shsynth/midi-devices/`. Private or locally corrected profiles can
override a bundled id from `${XDG_DATA_HOME}/shsynth/midi-devices/` or a path
listed in `SHSYNTH_DEVICE_PROFILE_DIR`.

Schema version 1 records:

- a stable `id`, manufacturer, model, and optional MIDI-port name fragments;
- whether program selection uses no bank select, CC0, or CC0 plus CC32;
- one or more banks with their optional MSB/LSB, Program Change offset,
  native slot labels, optional names, and whether the memory is writable;
- research sources and a note explaining device-specific selection behavior.

`slots` and `names` are parallel arrays. A missing or JSON `null` name means
the slot exists but its current name is unknown. This is important for user
memories and removable cards: the UI must not invent a name for mutable data.
Private overrides may supply the names actually loaded on one owner's device.

The program browser always retains numeric MIDI 0–127 access. For banks with
distinct Program Change ranges, such as the D-50 internal and card groups, a
single cell program can select the whole range. For devices that reuse the
same Program Change values across CC0/CC32 banks, the page's MSB/LSB selects
which bank's names are shown; the existing cell program remains a program-only
override.

The bundled `roland-d-50` profile is the first example, not a special mode. Its
factory names come from Roland's D-05 Parameter Guide “Preset 1: Original
D-50” list. Its MIDI range behavior comes from Roland's D-50 MIDI
Implementation: Program Change 0–63 selects internal memory and 64–127 selects
the card memory group.

Before adding a profile:

1. Use manufacturer documentation for MIDI behavior and names when available.
2. Represent writable/unknown names as `null`; do not guess what a user's
   hardware contains.
3. Keep all MIDI values zero-based in JSON, even if a manual prints 1–128.
4. Add parser/lookup tests and confirm the generic numeric fallback still
   works.
