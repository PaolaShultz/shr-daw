# Drum-pattern musical data audit

The library contains 108 catalog seeds and 12 standalone patterns. All 120 were
revoiced for Power Studio's expanded note map. These are original editable
starting points, not transcriptions or authoritative examples of musical traditions.
No third-party MIDI or protected recording was transcribed.

## Structure and compatibility

Catalog version 2 stores explicit note and velocity per hit; version 1 remains
readable. Four lanes bound simultaneous events. Catalog bars contain 16 rows in
4/4 or 12 in 3/4. Arrangement expands to 2, 4 or 8 bars with alternating pickups
and genre-aware endings. FILL selects the final 4, 8 or 16 rows, or OFF. On a
32-row phrase those starts are 28, 24 and 16; preceding rows are unchanged.

The palette includes three distinct crash recordings, two rides and ride bells,
six tom choices, side-stick, rimshot, clap, pedal/half/open hats, china, splash,
sizzle, flat-ride crash, tambourine and cowbell. Some sounds are synthesized or
pitched derivatives: the [pack credits](../kits/power-studio.shrkit/CREDITS.md)
identify each source. The [note map](../kits/power-studio.shrkit/README.md) is
canonical. Notes 60–65 are extensions, not General MIDI percussion assignments.

Factory loads adapt unsupported notes to the selected internal kit without
silently dropping hits; unsupported mappings refuse the whole edit. User files
remain exact. Kit-derived extended tail/choke classes preserve user overrides.
Routes, melody and undo remain covered by integration tests.

## Software validation

Exact Rust 1.97.1 verifies the complete package through the production loader,
including sample hashes, 30 voices, 171 samples and the decoded-memory bound.
All 120 patterns use supported Power Studio notes. Tests cover catalog versions,
invalid data, compatibility mapping, unchanged fill prefixes, serialization and
undo. Offline render checks cover all voices at three rising velocity levels,
repeatability, finite output and the output ceiling. No audible or physical
hardware test is implied; balance and musical preference still need listening.

The 2026-11-12 x86_64 pass completed locked checking, all-target/all-feature
DEV and REL builds, and all 1,268 normal tests in both profiles. All 14 opt-in
offline tests passed in DEV, along with 21 Python helper tests, 31 isolated
audio-policy cases and ten cleared demo arrangements. Formatting and whitespace
checks passed; upstream notices retain their original bytes.

## Current per-pattern ledger

Generated from the shipped musical data. Hit counts describe seed patterns,
before phrase expansion or added fills. Note numbers refer to the pack map.

| Pattern | Genre | Meter | Hits | Notes | Velocity range |
| --- | --- | --- | ---: | --- | --- |
| Arena Backbeat | Rock | 4/4 | 18 | 36,38,44,48,49,51 | 78–118 |
| Garage Push | Rock | 4/4 | 21 | 36,38,40,42,45,47,55,57 | 54–118 |
| Half Time Heavy | Rock | 4/4 | 17 | 36,38,41,53,59,60 | 54–118 |
| Motorik | Rock | 4/4 | 17 | 36,38,49,51,57,64 | 78–118 |
| Power Waltz | Rock | 3/4 | 13 | 36,38,44,49,51 | 78–118 |
| Slow Three | Rock | 3/4 | 13 | 36,40,42,55,57 | 78–118 |
| Radio Straight | Pop | 4/4 | 18 | 36,38,42,54 | 78–118 |
| Dance Pop | Pop | 4/4 | 17 | 36,39,42,55,57,61,63 | 78–118 |
| Syncopated Verse | Pop | 4/4 | 18 | 36,38,44,48,59,60 | 78–118 |
| Anthem Chorus | Pop | 4/4 | 20 | 36,38,42,46,49,54,55 | 78–118 |
| Pop Waltz | Pop | 3/4 | 14 | 36,38,42,54 | 78–118 |
| Ballad in Three | Pop | 3/4 | 11 | 35,38,42,55 | 68–118 |
| Deep House | House | 4/4 | 18 | 36,39,42,46,54,56 | 78–118 |
| Classic Piano House | House | 4/4 | 23 | 36,39,42,54,60,63 | 54–118 |
| Minimal House | House | 4/4 | 13 | 36,37,38,44,46,53 | 54–118 |
| Jackin House | House | 4/4 | 22 | 36,39,42,46,55,56,57 | 78–118 |
| Three Floor | House | 3/4 | 14 | 36,39,42,54,56,63 | 78–118 |
| Rolling Three | House | 3/4 | 18 | 36,39,42,46,54,60 | 78–118 |
| Warehouse Drive | Techno | 4/4 | 17 | 36,38,42,46,52,56 | 78–118 |
| Industrial Pulse | Techno | 4/4 | 20 | 36,38,42,53,56 | 54–118 |
| Broken Techno | Techno | 4/4 | 19 | 36,38,52,55,57,59 | 78–118 |
| Acid Machine | Techno | 4/4 | 20 | 36,38,41,42,45,46 | 78–118 |
| Techno in Three | Techno | 3/4 | 16 | 36,38,42,52,56,63 | 54–118 |
| Broken Three | Techno | 3/4 | 16 | 36,38,42,53,56 | 78–118 |
| Boom Bap Dust | Hip-Hop | 4/4 | 19 | 36,37,38,42,44,46,60 | 54–118 |
| West Coast Bounce | Hip-Hop | 4/4 | 19 | 36,37,39,42,54 | 78–118 |
| Lo Fi Pocket | Hip-Hop | 4/4 | 18 | 35,38,42,44,46,55,61 | 54–118 |
| Trap Half Time | Hip-Hop | 4/4 | 17 | 36,38,42,53,56 | 54–118 |
| Boom Bap Three | Hip-Hop | 3/4 | 14 | 36,38,42,44,60,63 | 54–118 |
| Trap in Three | Hip-Hop | 3/4 | 16 | 36,37,38,42,54 | 54–118 |
| Dry Funk | Funk | 4/4 | 22 | 36,38,42,46,54,56 | 54–118 |
| Crossbeat Funk | Funk | 4/4 | 20 | 36,38,49,51,53 | 54–118 |
| P Funk Stomp | Funk | 4/4 | 19 | 36,38,42,44,46,55,57 | 78–118 |
| Linear Funk | Funk | 4/4 | 14 | 36,37,38,56,59 | 78–118 |
| Funk in Three | Funk | 3/4 | 16 | 36,38,42,54,56,63 | 54–118 |
| Three Beat Pocket | Funk | 3/4 | 14 | 36,38,49,51,53 | 78–118 |
| One Drop | Reggae | 4/4 | 14 | 36,37,38,42,44,46 | 78–118 |
| Steppers | Reggae | 4/4 | 16 | 36,38,42,53,55,60,63 | 78–118 |
| Rockers | Reggae | 4/4 | 17 | 36,37,38,48,59 | 54–118 |
| Dancehall | Reggae | 4/4 | 19 | 36,38,42,46,54,56 | 78–118 |
| Three Drop | Reggae | 3/4 | 11 | 36,37,38,42,44,63 | 78–118 |
| Dub in Three | Reggae | 3/4 | 12 | 36,38,42,46,53,60 | 54–118 |
| Jungle Rim Break | Breaks | 4/4 | 17 | 36,38,40,49,51,55 | 78–118 |
| Two Step DnB | Breaks | 4/4 | 18 | 36,40,42,44,48,53,57 | 78–118 |
| Jungle Chopper | Breaks | 4/4 | 20 | 36,38,40,52,55,59 | 54–118 |
| Big Beat | Breaks | 4/4 | 19 | 36,38,42,54,60,61 | 78–118 |
| Three Step Break | Breaks | 3/4 | 14 | 36,38,49,51,55 | 78–118 |
| Jungle in Three | Breaks | 3/4 | 16 | 36,38,40,42,44,53,57 | 54–118 |
| Bossa Nova | Latin | 4/4 | 19 | 36,37,38,42,56 | 54–118 |
| Samba | Latin | 4/4 | 26 | 36,38,51,54,56 | 54–118 |
| Songo | Latin | 4/4 | 20 | 36,38,42,45,46,48,56 | 78–118 |
| Cumbia | Latin | 4/4 | 18 | 36,38,42,54,56 | 78–118 |
| Latin Waltz | Latin | 3/4 | 15 | 36,37,38,42,56 | 54–118 |
| Clave in Three | Latin | 3/4 | 18 | 36,38,51,54,56 | 78–118 |
| Swing Ride | Jazz | 4/4 | 15 | 36,38,44,51,53 | 54–118 |
| Jazz Shuffle | Jazz | 4/4 | 17 | 36,38,44,59,62 | 54–118 |
| Brush Ballad | Jazz | 4/4 | 15 | 35,37,38,44,51 | 54–118 |
| Syncopated Street | Jazz | 4/4 | 22 | 36,38,44,59,64,65 | 54–118 |
| Jazz Waltz | Jazz | 3/4 | 12 | 36,38,44,51,53 | 54–118 |
| Brushes in Three | Jazz | 3/4 | 11 | 35,38,44,59 | 54–118 |
| Tight Verse | Rock | 4/4 | 16 | 36,38,41,53,59,60 | 54–118 |
| Punk Eighths | Rock | 4/4 | 19 | 36,38,49,51,57,64 | 78–118 |
| Stadium Stomp | Rock | 4/4 | 9 | 36,38,44,48,49 | 78–118 |
| Offbeat Push | Rock | 4/4 | 20 | 36,40,42,45,46,47,55,57 | 78–118 |
| Quiet Verse | Pop | 4/4 | 14 | 35,37,44,48,59 | 54–118 |
| Clap Chorus | Pop | 4/4 | 19 | 36,39,42,49,54,55,63 | 78–118 |
| Half Time Ballad | Pop | 4/4 | 16 | 35,38,42,46,54 | 54–118 |
| Waltz Lift | Pop | 3/4 | 14 | 36,38,42,46,55,57 | 54–118 |
| Organ Pocket | House | 4/4 | 14 | 36,37,38,39,44,46,53 | 54–118 |
| Disco Sixteenths | House | 4/4 | 26 | 36,39,42,55,56,57,63 | 54–118 |
| Dub House Space | House | 4/4 | 13 | 35,39,42,46,54,56 | 54–118 |
| Percussive Floor | House | 4/4 | 15 | 36,39,46,54,60 | 78–118 |
| Minimal Tick | Techno | 4/4 | 11 | 36,52,55,57,59 | 78–118 |
| Offbeat Machinery | Techno | 4/4 | 18 | 36,38,41,42,45,63 | 54–118 |
| Broken Warehouse | Techno | 4/4 | 19 | 36,38,42,46,52,56 | 78–118 |
| Tomless Rumble | Techno | 4/4 | 18 | 36,38,42,46,53,56 | 54–118 |
| Sparse Head Nod | Hip-Hop | 4/4 | 14 | 35,38,42,44,55,61 | 54–118 |
| Ghost Pocket | Hip-Hop | 4/4 | 19 | 36,38,42,53,56,63 | 54–118 |
| Half Time Skitter | Hip-Hop | 4/4 | 20 | 36,37,38,39,42,44,60 | 54–118 |
| Rim And Space | Hip-Hop | 4/4 | 14 | 35,37,42,46,54 | 54–118 |
| Ghost Sixteenths | Funk | 4/4 | 28 | 36,38,42,44,46,55,57 | 54–118 |
| Linear Conversation | Funk | 4/4 | 14 | 36,37,38,56,59 | 78–118 |
| Deep Pocket | Funk | 4/4 | 20 | 36,38,42,46,54,56 | 54–118 |
| Three Beat Ghosts | Funk | 3/4 | 16 | 36,38,49,51,53 | 54–118 |
| Dub Space | Reggae | 4/4 | 9 | 35,37,48,59 | 68–118 |
| Steppers Rim | Reggae | 4/4 | 17 | 36,37,42,54,56,63 | 54–118 |
| Rockers Pickup | Reggae | 4/4 | 19 | 36,37,38,42,44,46 | 54–118 |
| Dancehall Bounce | Reggae | 4/4 | 19 | 36,39,42,46,53,55,60 | 54–118 |
| Rolling Ghost Break | Breaks | 4/4 | 19 | 36,38,40,52,55,59 | 54–118 |
| Half Time Break | Breaks | 4/4 | 19 | 36,38,42,54,60,61,63 | 54–118 |
| Snare Displacement | Breaks | 4/4 | 18 | 36,38,40,49,51,55 | 54–118 |
| Syncopated Breakbeat | Breaks | 4/4 | 19 | 36,40,42,44,46,48,53,57 | 78–118 |
| Soft Bossa Pocket | Latin | 4/4 | 19 | 35,38,42,45,48,56 | 54–118 |
| Rim Dance | Latin | 4/4 | 16 | 36,37,42,54,56 | 78–118 |
| Cumbia Space | Latin | 4/4 | 13 | 35,37,38,46,56 | 68–118 |
| Three Beat Percussion | Latin | 3/4 | 17 | 36,37,51,54,56 | 78–118 |
| Light Comping | Jazz | 4/4 | 16 | 35,37,38,44,51 | 54–92 |
| Broken Brush Pocket | Jazz | 4/4 | 17 | 35,38,44,59,64 | 54–92 |
| Sparse Jazz Waltz | Jazz | 3/4 | 11 | 35,38,44,51,53 | 54–92 |
| Three Beat Comping | Jazz | 3/4 | 13 | 36,38,44,59,62 | 54–100 |
| Two Step Pocket | Garage | 4/4 | 18 | 36,37,39,42,44,46,55 | 54–118 |
| Skipping Hats | Garage | 4/4 | 19 | 36,39,42,54,60,61,63 | 54–118 |
| Four Floor Shuffle | Garage | 4/4 | 18 | 36,39,44,53,56,59 | 54–118 |
| Rim Step | Garage | 4/4 | 17 | 36,37,42,46,55,57 | 54–118 |
| Robot Break | Electro | 4/4 | 18 | 36,39,42,46,52,55,56 | 78–118 |
| Sparse Circuit | Electro | 4/4 | 12 | 35,38,53,56,59,64 | 68–118 |
| Machine Funk | Electro | 4/4 | 20 | 36,39,42,46,54,60,61 | 54–118 |
| Half Time Voltage | Electro | 4/4 | 18 | 36,39,41,42,46,48,55,56 | 54–118 |
| Rock Backbeat | Rock | 4/4 | 17 | 36,38,41,53,59,60 | 70–116 |
| House Four on Floor | House | 4/4 | 17 | 36,37,39,42,44,53,63 | 72–118 |
| Techno Drive | Techno | 4/4 | 17 | 36,38,52,55,57,59 | 68–122 |
| Disco Open Hat | Pop | 4/4 | 18 | 36,38,44,48,59,60 | 48–116 |
| Boom Bap | Hip-Hop | 4/4 | 18 | 36,38,42,44,55,61,63 | 58–120 |
| Hip Hop Sparse | Hip-Hop | 4/4 | 16 | 35,38,42,46,53,56 | 54–118 |
| Trap Halftime | Hip-Hop | 4/4 | 18 | 36,37,38,42,44,46,60 | 48–120 |
| Funk Syncopated | Funk | 4/4 | 20 | 36,38,42,44,55,57,63 | 48–120 |
| Reggae One Drop | Reggae | 4/4 | 13 | 36,37,38,48,59 | 60–112 |
| Drum and Bass Break | Breaks | 4/4 | 17 | 36,38,40,52,55,59 | 58–122 |
| Bossa Side-Stick | Latin | 4/4 | 17 | 36,42,45,48,56 | 54–104 |
| Waltz 3/4 | Jazz | 3/4 | 8 | 36,37,38,44,51 | 68–112 |
