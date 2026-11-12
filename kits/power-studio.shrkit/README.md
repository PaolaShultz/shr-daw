# Power Studio 1.0

A 30-voice drum pack for SHR Drums / SHR-DAW: dense, transient-preserving drums,
several distinct cymbals, and a compact set of percussion. Includes 48 kHz
stereo PCM24 one-shots, velocity layers and alternating takes, a format-1
`.shrkit` manifest, per-file provenance and original licence notices.

Extract `power-studio.shrkit` into your kit directory. Select **Power Studio**
in SHR-DAW. Other samplers can use the WAV files; the manifest gives exact
velocity bands and round-robin assignments. Requires SHR Drums >=0.2.1,<1.0.0.
No plugin, external synth, or new audio service is needed.

| MIDI note | Voice |
|---|---|
| 35 / 36 | Tight kick / Power kick |
| 37 / 38 / 39 / 40 | Side-stick / Power snare / Clap / Rimshot |
| 41 / 43 / 45 / 47 / 48 / 50 | Floor / Deep floor / Low / Mid / High / High tight tom |
| 42 / 44 / 46 / 63 | Closed / Pedal / Open / Half-open hat |
| 49 / 57 / 60 | Left / Right / Studio crash |
| 51 / 59 | Left / Right ride bow |
| 53 / 64 | Left / Right ride bell |
| 52 / 55 | China / Synthesized splash |
| 61 / 62 / 65 | Short choked crash / Sizzle crash / Flat-ride crash |
| 54 / 56 | Synthesized tambourine / Cowbell |

All hats share one choke group. Separate crashes can ring together. The short
choked-crash sample has its own note and does not choke other crash voices.
Notes 60–65 are pack-specific extensions, not General MIDI percussion names.

Drums have baked parallel compression and saturation; cymbals keep their
longer dynamics. The modest kit-bus parallel compressor adds interaction
between hits. This is not tempo-synchronized sidechain ducking. Keep output
headroom when adding more Project effects. Source dynamics remain, and playback
velocity still controls gain; ghost notes need not sound like hard hits.

Full mix and physical-controller listening remain the musician's decision.
See `CREDITS.md` before redistribution: this combines CC BY 4.0 and CC0 assets.
Reproduction recipe and build instructions live in the SHR Drums repository:
https://github.com/PaolaShultz/shr-drums/blob/main/packs/POWER_STUDIO.md
