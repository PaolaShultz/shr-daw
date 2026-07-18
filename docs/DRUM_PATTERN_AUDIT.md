# Drum-pattern musical data audit

This audit treats the bundled rhythms as editable musical data, not just parser
fixtures. It covers 60 one-bar catalog entries and 12 standalone `.shdrum`
files: 72 patterns across ten label groups, 51 in 4/4 and 21 in 3/4.

## Proven structure

- The Rust decoder and arranger load every entry, and the discovery test now
  requires exactly 72 unique visible bundled names.
- Rows are 16 for a catalog 4/4 bar and 12 for 3/4. The arranger expands them
  evenly to 2, 4, or 8 bars (32/64/128 or 24/48/96 rows).
- Bundled notes use only 36, 37, 38, 39, 42, and 46: the project's kick,
  side-stick/rim, snare, clap, closed-hat, and open-hat choices. These follow
  long-established General MIDI-style assignments, but external devices may
  use another map; the configured percussion input map remains the authority.
  The MIDI Association describes the default map as an industry convention
  later standardized by General MIDI: [Default Drum Note Map](https://midi.org/midi-ci-profile-for-default-drum-note-map).
- Velocities span 48–122. Accents, normal hits, ghosts, rims, claps, and open
  hats have distinct catalog tiers; standalone patterns use 5–14 distinct
  velocities each.
- No bundled cell has an explicit long gate or overlapping note. It inherits
  the Project gate (80% by default), so the next row is normally clear.
- Alternating expanded bars receive a restrained pickup when that position is
  empty. Multi-bar phrases receive a final four-row fill selected by broad
  label group.

## Findings and curation

The first audit found that two different Bossa entries shared the same visible
name, so discovery exposed 71 instead of the intended 72. It also found one
exact data duplicate (`New Orleans Funk` and `Second Line`) and an `Amen
Inspired` label that invoked a famous recording without helping the workflow.

The safe curation pass:

- renamed the standalone entry to `Bossa Side-Stick`, making all 72 visible;
- renamed `New Orleans Funk` to the broader `Crossbeat Funk`;
- changed the duplicate Jazz entry to `Syncopated Street` and varied one kick
  placement plus a quiet end ghost;
- renamed `Amen Inspired` to `Jungle Rim Break`;
- renamed `Afro Cuban Six` to the narrower `Clave in Three`.

The per-pattern ledger then exposed eight names that implied 6/8 or triplets
although the current format renders them as 3/4. `Slow Six`, `Ballad Six`,
`Techno Triplet`, `Triplet Trap`, `Six Eight Pocket`, `Dub Six`, `Jungle Six`,
and `Clave Six` now use `Three`/`Three Beat` wording. Their notes and velocities
did not change. This avoids claiming a compound-meter feel that still needs
human listening.

After those edits there are no byte-for-byte musical duplicates. A few patterns
still intentionally share a hit grid while differing in accents and velocity:
`Dance Pop`/`Deep House`/`House Four on Floor`, `Radio Straight`/`Rock
Backbeat`, `Warehouse Drive`/`Techno Drive`, and `Hip Hop Sparse`/`Drum and
Bass Break`. Those are useful dynamic variants, but the demo should not show
two from the same group.

Genre labels are creative navigation hints, not musicology claims or
transcriptions. The files were authored as original patterns; no copyrighted
MIDI or protected recording was transcribed. Culturally specific labels such as
Samba, Cumbia, Songo, Bossa Nova, and Reggae still require human listening and
careful presentation. Describe them as inspired editable starting points, not
authoritative examples of a tradition.

## Musical risks

- Four-on-the-floor patterns place a kick on every beat; a sustained bass on
  those rows can mask the attack. Leave bass space on beats 1/3, shorten its
  gate, or choose a sparser kick pattern. SHR-DAW has no side-chain compressor.
- One-bar seeds are intentionally compact. The arranger adds deterministic
  variation, but it is not human improvisation; inspect the final fill and edit
  repetitions before calling an eight-bar phrase finished.
- Several pairs are near variants. That is appropriate for starting points but
  weak evidence of breadth if shown back-to-back in the video.
- Note maps are device-dependent. Confirm that the selected percussion page
  produces the intended kick/snare/hat sounds before recording.
- Static timing cannot establish swing, pocket, cultural authenticity, or
  whether a groove works with the chosen bass line.
- Dense grids such as `Samba`, `Industrial Pulse`, and `Techno in Three` are
  credible programmed parts but should not be described as one drummer's
  literal limb performance without a human playability review.

## Every-pattern evidence ledger

This ledger accounts for all 72 visible entries. `K` is kick-row placement;
`B` is snare/clap backbeat placement; `H/O/R` counts hat/open-hat/rim events.
For compact catalog patterns, `A/G` counts explicitly marked accent/ghost
events. Standalone files show their actual velocity range. Row 0 is the
downbeat, rows 4/8/12 are quarter-note boundaries in 4/4, and rows 0/4/8 are
the three beats in 3/4. Every row passed the static meter, range, note-map, and
gate checks; every row still needs listening.

| Pattern | Label / meter | K rows | B rows | Texture / dynamics |
|---|---|---|---|---|
| Arena Backbeat | Rock 4/4 | 0,4,8,10,14 | 4,12 | H8/O0/R1; A4/G0 |
| Garage Push | Rock 4/4 | 0,3,7,8,11,14 | 4,12,15 | H8/O0/R1; A4/G1 |
| Half Time Heavy | Rock 4/4 | 0,6,10,14 | 8,15 | H8/O0/R0; A2/G1 |
| Motorik | Rock 4/4 | 0,4,8,12 | 4,12 | H8/O0/R2; A2/G0 |
| Power Waltz | Rock 3/4 | 0,6,9 | 4,8 | H6/O0/R1; A2/G0 |
| Slow Three | Rock 3/4 | 0,5,8 | 4,10 | H6/O0/R1; A2/G0 |
| Radio Straight | Pop 4/4 | 0,4,8,10 | 4,12 | H8/O0/R0; A3/G0 |
| Dance Pop | Pop 4/4 | 0,4,8,12 | 4,12 | H8/O4/R0; A2/G0 |
| Syncopated Verse | Pop 4/4 | 0,3,8,11,14 | 4,12 | H8/O0/R1; A3/G0 |
| Anthem Chorus | Pop 4/4 | 0,4,7,8,12,14 | 4,12 | H8/O4/R0; A4/G0 |
| Pop Waltz | Pop 3/4 | 0,4,8 | 4,8 | H6/O0/R0; A3/G0 |
| Ballad in Three | Pop 3/4 | 0,6 | 4,10 | H6/O0/R1; A3/G0 |
| Deep House | House 4/4 | 0,4,8,12 | 4,12 | H8/O4/R0; A4/G0 |
| Classic Piano House | House 4/4 | 0,4,8,12 | 4,12 | H12/O4/R0; A4/G4 |
| Minimal House | House 4/4 | 0,4,8,12 | 4,12 | H4/O4/R2; A4/G2 |
| Jackin House | House 4/4 | 0,3,4,7,8,11,12,15 | 4,12 | H8/O4/R0; A4/G0 |
| Three Floor | House 3/4 | 0,4,8 | 4,8 | H6/O3/R0; A3/G0 |
| Rolling Three | House 3/4 | 0,3,4,7,8,11 | 4,8 | H6/O3/R0; A3/G0 |
| Warehouse Drive | Techno 4/4 | 0,4,8,12 | 4,12 | H8/O4/R2; A4/G0 |
| Industrial Pulse | Techno 4/4 | 0,4,8,12 | 2,4,10,12 | H8/O0/R4; A6/G2 |
| Broken Techno | Techno 4/4 | 0,3,6,8,11,14 | 4,12 | H8/O0/R2; A4/G0 |
| Acid Machine | Techno 4/4 | 0,4,7,8,12,15 | 4,12 | H8/O4/R0; A4/G0 |
| Techno in Three | Techno 3/4 | 0,4,8 | 2,4,6,8,10 | H6/O3/R0; A5/G3 |
| Broken Three | Techno 3/4 | 0,3,6,8,11 | 4,8 | H6/O0/R2; A3/G0 |
| Boom Bap Dust | Hip-Hop 4/4 | 0,3,7,8,11,14 | 4,12 | H8/O1/R0; A4/G3 |
| West Coast Bounce | Hip-Hop 4/4 | 0,6,8,10,15 | 4,12 | H8/O0/R2; A1/G0 |
| Lo Fi Pocket | Hip-Hop 4/4 | 0,5,8,11,14 | 4,12 | H8/O1/R0; A3/G4 |
| Trap Half Time | Hip-Hop 4/4 | 0,6,12,13 | 8 | H10/O0/R0; A2/G2 |
| Boom Bap Three | Hip-Hop 3/4 | 0,3,6,10 | 4,8 | H6/O1/R0; A3/G2 |
| Trap in Three | Hip-Hop 3/4 | 0,5,9,11 | 6 | H8/O0/R0; A2/G2 |
| Dry Funk | Funk 4/4 | 0,3,8,9,11,14 | 4,7,12,15 | H8/O2/R0; A3/G4 |
| Crossbeat Funk | Funk 4/4 | 0,3,6,8,11,14 | 4,10,12 | H8/O0/R2; A3/G1 |
| P Funk Stomp | Funk 4/4 | 0,2,7,8,10,15 | 4,12 | H8/O2/R2; A4/G0 |
| Linear Funk | Funk 4/4 | 0,6,8,14 | 4,12 | H4/O0/R4; A3/G0 |
| Funk in Three | Funk 3/4 | 0,3,6,9 | 4,8,11 | H6/O1/R1; A3/G1 |
| Three Beat Pocket | Funk 3/4 | 0,5,8,11 | 4,10 | H6/O0/R1; A3/G0 |
| One Drop | Reggae 4/4 | 8 | 8 | H8/O4/R2; A2/G0 |
| Steppers | Reggae 4/4 | 0,4,8,12 | 8 | H8/O4/R2; A5/G0 |
| Rockers | Reggae 4/4 | 0,6,8,14 | 4,12 | H8/O4/R2; A3/G1 |
| Dancehall | Reggae 4/4 | 0,3,7,10,14 | 4,12 | H8/O1/R2; A3/G0 |
| Three Drop | Reggae 3/4 | 6 | 6 | H6/O2/R2; A2/G0 |
| Dub in Three | Reggae 3/4 | 0,8 | 4,10 | H6/O2/R2; A2/G1 |
| Jungle Rim Break | Breaks 4/4 | 0,6,10,15 | 4,12 | H8/O1/R1; A4/G0 |
| Two Step DnB | Breaks 4/4 | 0,6,10,15 | 4,12 | H8/O0/R0; A4/G0 |
| Jungle Chopper | Breaks 4/4 | 0,3,6,10,14 | 4,8,12,15 | H8/O1/R1; A4/G2 |
| Big Beat | Breaks 4/4 | 0,3,8,10,14 | 4,12 | H8/O0/R2; A4/G0 |
| Three Step Break | Breaks 3/4 | 0,5,8,11 | 4,10 | H6/O1/R1; A4/G0 |
| Jungle in Three | Breaks 3/4 | 0,3,7,10 | 4,8,11 | H6/O0/R1; A3/G1 |
| Bossa Nova | Latin 4/4 | 0,4,8,12 | 4,12 | H8/O0/R5; A3/G2 |
| Samba | Latin 4/4 | 0,3,4,7,8,11,12,15 | 4,12 | H8/O0/R8; A6/G2 |
| Songo | Latin 4/4 | 0,6,8,11,14 | 4,12 | H8/O1/R5; A4/G0 |
| Cumbia | Latin 4/4 | 0,4,8,12 | 4,12 | H8/O0/R4; A4/G0 |
| Latin Waltz | Latin 3/4 | 0,4,8 | 4,8 | H6/O0/R4; A2/G2 |
| Clave in Three | Latin 3/4 | 0,5,8,11 | 4,10 | H6/O0/R5; A5/G0 |
| Swing Ride | Jazz 4/4 | 0,8 | 4,12 | H8/O0/R2; A3/G6 |
| Jazz Shuffle | Jazz 4/4 | 0,6,8,14 | 4,12 | H8/O0/R0; A4/G5 |
| Brush Ballad | Jazz 4/4 | 0,8 | 4,12 | H8/O0/R2; A1/G10 |
| Syncopated Street | Jazz 4/4 | 0,3,7,8,11,14 | 4,10,12,15 | H8/O0/R2; A3/G2 |
| Jazz Waltz | Jazz 3/4 | 0,8 | 4,10 | H6/O0/R2; A3/G4 |
| Brushes in Three | Jazz 3/4 | 0,6 | 4,10 | H6/O0/R2; A1/G8 |
| Rock Backbeat | Rock 4/4 | 0,4,8,10 | 4,12 | H8/O0/R0; velocity 70–116 |
| House Four on Floor | House 4/4 | 0,4,8,12 | 4,12 | H8/O4/R0; velocity 72–118 |
| Techno Drive | Techno 4/4 | 0,4,8,12 | 4,12 | H8/O4/R2; velocity 62–122 |
| Disco Open Hat | Pop 4/4 | 0,4,8,12 | 4,12,15 | H8/O4/R0; velocity 48–116 |
| Boom Bap | Hip-Hop 4/4 | 0,3,7,8,11 | 4,12 | H8/O1/R0; velocity 58–120 |
| Hip Hop Sparse | Hip-Hop 4/4 | 0,6,10,15 | 4,12 | H8/O1/R0; velocity 54–118 |
| Trap Halftime | Hip-Hop 4/4 | 0,6,12,13 | 8 | H10/O1/R0; velocity 48–120 |
| Funk Syncopated | Funk 4/4 | 0,3,8,9,11 | 4,7,12,15 | H8/O2/R0; velocity 48–120 |
| Reggae One Drop | Reggae 4/4 | 8 | 8 | H8/O1/R0; velocity 60–112 |
| Drum and Bass Break | Breaks 4/4 | 0,6,10,15 | 4,12 | H8/O1/R0; velocity 58–122 |
| Bossa Side-Stick | Latin 4/4 | 0,4,8,12 | — | H8/O0/R5; velocity 54–104 |
| Waltz 3/4 | Jazz 3/4 | 0 | 4,8 | H3/O1/R0; velocity 68–112 |

## Demo shortlist

These are structurally differentiated candidates, not listening results:

| Pattern | Why it is a useful demo candidate | What to check by ear |
|---|---|---|
| Syncopated Verse | Clear backbeat, syncopated kick, modest rim pickup | Does it feel active without fighting the bass? |
| Deep House | Immediately legible four-on-floor and open hats | Are kick/hat levels balanced on the chosen device? |
| Dry Funk | Accents, ghosts, open hats, and a syncopated kick show editing depth | Does the deterministic fill feel natural? |
| Trap Half Time | Obvious half-time space and fine hat detail | Is the label and feel convincing rather than generic? |
| Jungle Rim Break | Broken kick, backbeat, open hat, and rim show contrast | Does it become rushed at the demo tempo? |
| Bossa Side-Stick | Distinct side-stick lane and quieter dynamics | Present only as an inspired starting point; check feel carefully |
| Jazz Waltz | Demonstrates 3/4 and a sparse three-beat shape | Does the open hat on beat 3 support the phrase? |

For the safest first demo, audition `Syncopated Verse` and `Dry Funk` at
100–116 BPM, then select one. The human should approve the groove, fill, drum
mapping, and bass relationship. Codex assisted with pattern design, structural
analysis, duplicate detection, and shortlist formation; it did not hear the
result.
