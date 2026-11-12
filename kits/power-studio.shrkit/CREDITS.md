# Power Studio — source credits and licences

Power Studio is a processed derivative sample pack assembled by the SHR Drums
project. The original recordists and performers below are not affiliated with
and do not endorse this derivative pack.

## MuldjordKit — CC BY 4.0

Original recordings: **Lars Muldjord**. Stereo SFZ/WAV assembly for FreePats:
**roberto@zenvoid.org**. Source: **MuldjordKit SFZ+WAV 2020-10-18**.

- Release: https://github.com/freepats/muldjordkit/releases/tag/2020-10-18
- Source page: https://freepats.zenvoid.org/Percussion/acoustic-drum-kit.html
- Creator: https://www.muldjord.com/
- Licence: https://creativecommons.org/licenses/by/4.0/
- Complete upstream notices: `SOURCE-MULDJORD.txt`, `LICENSE-CC-BY-4.0.txt`.

These recordings supply both kicks, the main snare, toms, closed/open hats,
left/right crashes, left/right rides, both ride bells, and china. Their processed
samples remain under CC BY 4.0. Preserve this attribution when redistributing.

## Virtuosity Drums — CC0 1.0

Created by **Versilian Studios and Karoryfer Samples** (Karoryfer Lecolds),
performed by **Austin McMahon**, recorded at **Virtuosity Musical Instruments,
Boston, Massachusetts**. Source commit:
`9f04cf9a734527edfbb0a4eee1f674e45bbf71bc`.

- Product and recording details: https://versilian-studios.com/virtuosity-drums/
- Source: https://github.com/sfzinstruments/virtuosity_drums
- Dedication: https://creativecommons.org/publicdomain/zero/1.0/
- Complete source notices: `SOURCE-VIRTUOSITY.md`, `LICENSE-CC0-1.0.txt`.

Selected overhead recordings supply side-stick, rimshot, pedal/half-open hats,
studio crash, sizzle crash, and flat-ride crash. The choked crash is a shortened
studio-crash derivative, not a separately recorded hand choke. Attribution is
retained voluntarily for these CC0 recordings.

## Original material and changes

SHR Drums dedicates its original synthesized clap, tambourine, splash, cowbell,
and generated kick reinforcement to CC0 1.0. Splash is synthesized, not a
recording of an additional acoustic cymbal. Deep-floor and high-tight toms are
pitch-shifted variants of Muldjord Tom4 and Tom1; the other four toms are distinct
recorded drums.

Processing: selected velocity layers and alternating takes, 48 kHz stereo
resampling, onset/tail trimming with fades, high-pass filtering, drum parallel
compression and saturation, mild cymbal peak shaping, kick reinforcement, and
voice/kit balancing. `PROVENANCE.json` maps every delivered WAV to its source,
source SHA-256 and licence. `manifest.json` hashes every delivered sample.

The mixed pack is **not wholly CC0**. Muldjord-derived files require CC BY 4.0
attribution; Virtuosity and original synthesized files use CC0. Build software
is MIT-licensed in the SHR Drums repository. No third-party MIDI or commercial
sample pack was used.
