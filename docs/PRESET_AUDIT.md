# Cleared synthv1 preset audit

This audit covers only the 21 public, MIT-cleared files named in
`presets/synthv1/cleared-presets.txt`. The 424 private presets and the uncleared
392-file archive were not used as sound or submission content.

## What static validation proved

- `xmllint --noout` accepts all 21 files.
- The manifest exactly matches the public directory: 21 files, no extras.
- Every file has the same complete 145-name/145-index schema as `Velvet Tines`.
- Every parameter is unique and legal for its float, integer, or Boolean type
  against the [synthv1 0.9.29 parameter table](https://sources.debian.org/src/synthv1/0.9.29-1/src/synthv1_param.cpp).
- A clean temporary run of `scripts/generate_cleared_presets.sh` reproduced all
  20 derived files byte-for-byte; the template is the 21st file.
- No files are identical. The closest parameter pair still differs in 13 of
  145 values; no two main amplifier ADSR shapes are identical.
- Values that deserve listening attention include resonance 0.82 in `Liquid
  Acid`, releases 0.88–0.92 in the drones, and combined first/second-engine
  volumes above 0.7 in several sounds. The limiter remains enabled, but that is
  not proof against unpleasant tone or a clipped downstream mix.

Static inspection cannot establish loudness, absence of clicks, useful
velocity response, or musical quality. Every description below is a hypothesis.
The preset files contain a full keyboard range rather than authored note-range
metadata, so the suggested ranges are audition prompts, not enforced limits.

## Scorecard

| Preset | Role | Oscillator idea, in plain language | Filter/envelope idea | Likely strength | Likely risk | First audition | Listening / decision |
|---|---|---|---|---|---|---|---|
| Compact Bass | Bass | Octave-lowered blend with a quiet second layer | Moderate filter, short attack/release | Controlled, compact low end | Fast attack may click; chord gain | C1–C3, single notes then octaves | Not heard / undecided |
| Copper Pluck | Pluck | Bright blended pair with restrained ring modulation | Short decay, zero sustain | Clear rhythmic transient | Could become thin at high velocity | C3–C6, soft/hard repeated notes | Not heard / undecided |
| Dark Canopy | Pad | Dark two-shape blend | Low cutoff, inverted filter movement, slow amp | Distinct shadow pad | 0.70 release may blur changes | C3–C5 minor and major triads | Not heard / undecided |
| Deep Sub | Sub bass | Octave-down, first engine only | Low cutoff, little resonance, short release | Safest static bass gain | May disappear on small speakers | C1–C3, sustained single notes | Not heard / undecided |
| Drawbar Glow | Organ | Octave-separated steady layers | Open low-resonance filter, high sustain | Stable held chords | Layer gain may cloud low chords | C2–C6 triads, then sixths | Not heard / undecided |
| Dust Delay | Effect / sequence | Narrow pulse-like layer against a brighter shape | Medium filter, low sustain, audible delay | Characterful rhythmic motion | Delay feedback and transients may clutter | C3–C5 short notes at 90–120 BPM | Not heard / undecided |
| Frozen Drone | Drone | Detuned high/low layers with stereo motion | Slow attack, 0.92 release, reverb | Long evolving texture | Tail may be too long or dense | C2–C4, one note for 8–12 seconds | Not heard / undecided |
| Glass Saw Lead | Lead | Detuned matching bright shapes plus second layer | Open resonant filter, fast attack, mono | Clear solo presence | Brightness may turn harsh | C3–C6, soft then hard mono lines | Not heard / undecided |
| Hollow Organ | Organ | Narrow pulse-like and octave layer | Mid cutoff, high sustain, light phaser | Different color from Drawbar Glow | Low octave could muddy chords | C2–C6 triads and held fifths | Not heard / undecided |
| Liquid Acid | Acid bass / lead | Mono blend with glide | Resonance 0.82 and strong short filter sweep | Obvious animated role | Primary pain/loudness candidate | C2–C4 at very low gain, one note at a time | Not heard / undecided |
| Low Orbit Drone | Drone | Octave-low detuned blend | Dark filter, slow attack, 0.88 release | Low, slowly moving bed | Long bass energy may build up | C1–C3, single held notes | Not heard / undecided |
| Mono Pulse Lead | Lead | Narrow pulse-like shape blended with a bright layer | Open filter, subtle pitch motion, mono glide | Expressive single-note line | Fast attack or upper range may bite | C3–C6 legato and detached | Not heard / undecided |
| PWM Horizon | Lead | Two different-width pulse-like shapes | Moderate filter, slow width motion, mono | Clear modulation identity | Modulation may sound seasick or phasey | C3–C5 held notes, then melody | Not heard / undecided |
| Reed Pluck | Pluck | Narrow pulse-like/reed blend | Medium resonance, short decay, near-zero sustain | Organic short-note contrast | 0.001 attack may click | C3–C6, velocity ladder | Not heard / undecided |
| Restrained Sweep | Effect / pad | Balanced two-shape blend | Slow filter attack/decay, moderate resonance | Demonstrates movement without extreme FX | Sweep may be too subtle or too long | C3–C5 held fifths and triads | Not heard / undecided |
| Rubber Circuit | Bass / sequence | Mono contrasting shapes with glide | Resonant positive filter envelope | Bouncy sequenced role | Resonance plus layer gain may thump | C2–C4 eighth-note pattern | Not heard / undecided |
| Shimmer Veil | Pad | Detuned, octave-separated layers | Open filter, slow attack/release, chorus/reverb | Bright stereo pad | Dense high end and long tail | C3–C5 sparse triads | Not heard / undecided |
| Silver Bell | Bell | Octave layer with strong ring modulation | Bright filter, low sustain, longish release | Metallic identity | High notes may become piercing | C4–C7 single notes, low gain | Not heard / undecided |
| Soft Chime | Bell / keys | Softer ring-modulated octave blend | Bright filter, slower decay/release and reverb | Gentler contrast to Silver Bell | Reverb may smear repeated notes | C4–C7 soft and hard notes | Not heard / undecided |
| Velvet Tines | Electric keys | Balanced two-engine blend | Medium-open filter, quick attack, modest tail | Broad, playable starting point | Highest first-engine volume; chords may clip | C2–C6 triads at three velocities | Not heard in this audit / undecided |
| Warm Cloud | Pad | Slightly detuned layered blend | Gentle filter, slow attack, medium release | Simple warm chord bed | Combined layers may feel thick | C3–C5 major/minor triads | Not heard / undecided |

## First listening shortlist

Start with eight roles rather than all 21: `Deep Sub`, `Compact Bass`, `Mono
Pulse Lead`, `Copper Pluck`, `Warm Cloud`, `Drawbar Glow`, `Silver Bell`, and
`Dust Delay`. This is a static-coverage shortlist, not a claim that they are the
best sounds. Keep five to eight only after human listening.

## Safe human listening protocol

1. Set the interface/headphone level low before loading the first sound. Use one
   preset at a time; do not change JACK or start a second engine.
2. Play the suggested range softly, then at normal velocity. Raise monitoring
   only after the first notes are comfortable.
3. Ask: Is the attack clear or clicky? Is the low end stable? Does resonance
   become painful? Is release too long? Does high velocity disappear or turn
   harsh? Is the role distinct from the previous sound?
4. For basses and leads, test single notes and legato. For pads/organs, test a
   three-note chord before larger voicings. For delays/drones, stop and wait for
   the entire tail.
5. Record `keep`, `revise`, or `drop`, plus one sentence of evidence. Revise only
   failed sounds, re-run XML/schema tests, and listen again at low level.

Codex proposed and statically checked the parameter designs. The human owns the
monitor level, audible judgment, final curation, and any statement that a preset
sounds good.
