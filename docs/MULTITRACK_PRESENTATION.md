# Three-minute multitrack presentation

Created: 2026-07-19

Status: presentation aid with a still-gated physical MR18 result

This script is truthful before and after the MR18 test. Replace the bracketed
hardware result only when the signed test sheet supports it.

## Script and shot list

**0:00–0:25 — old limitation.** Show the previous stereo-recorder image or a
two-channel legacy configuration. Say: “SHR-DAW used to capture one configured
stereo pair into one 24-bit WAV. That was useful for a mix, but it could not
preserve a whole band's independent inputs.” On screen: `Before: 1 stereo WAV`.

**0:25–0:55 — generic architecture.** Show the compact AUDIO track list. Say:
“The recorder now accepts an arbitrary deliberately mapped set of JACK audio
sources. Every callback transfers all armed channels together into one bounded,
preallocated ring. Disk writing happens off the audio thread.” On screen:
`Generic JACK capture · 1–64 configured tracks · no software monitoring` and
`One shared start/stop callback boundary`.

**0:55–1:25 — musician workflow.** Select several tracks named `Input 1 ·
Vocal`, `Input 5 · Bass mic`, and `Line L/R`. Show arm dots, source assignment,
NAME, ALL/NONE, then open Levels. All 18 nine-segment columns remain visible;
show one quiet group, nominal green, yellow/red peaks, and a missing `M`. Say:
“I name and route inputs in setup, then compare every recording level at once.
Selection never hides a channel. A missing source stays missing and blocks
start; SHR never substitutes a nearby port.”

**1:25–1:55 — result.** Stop. Show one `.take` directory, its separate mono WAV
files, and `session.json`. Highlight `sample_rate`, `total_frames`, each track's
stable ID/label/preferred and actual source, filename, equal frame count,
`completeness: complete`, and zero error counters. Say: “Each physical input is
directly importable as a mono stem. The shared manifest proves the common
timeline and records exactly what was connected.”

**1:55–2:20 — failure honesty.** Temporarily show a configured preferred source
with the interface absent. Do not remap it. Say: “Portable labels may be blank,
but a remembered machine route remains exact. Offline hardware is visible and
cannot silently become another input. Overflow, xrun, disconnect, JACK loss,
write failure, or unequal finalization marks the take incomplete.” On screen:
`missing ≠ fallback` and `damaged takes never appear complete`.

**2:20–3:00 — MR18 target and evidence.** Say: “This is not an MR18-specific
recorder. The Midas M AIR MR18 is tomorrow's first full acceptance target:
18×18 USB audio, 16 mic/line inputs plus stereo line inputs 17–18, 24-bit
conversion, and 44.1/48 kHz support. We will test all 18 at 48 kHz progressively
and record xruns, drops, high-water, CPU, temperature, storage, frame agreement,
and identity.”

Before hardware acceptance, finish with: “Today the production buffering and
file path have passed synthetic 18-channel 48 kHz validation. That is not an
MR18 hardware result.” On screen: `SYNTHETIC · 18 ch · 48,000 Hz · [period] ·
[duration] · 0 dropped · identity verified`.

After a real pass only, replace that line with the exact signed result:
`MR18 HARDWARE PASS · 18 ch · 48,000 Hz · [frames × periods] · [duration] · 0
xruns · 0 dropped · identity verified`, plus the date, storage target, maximum
CPU/temperature, and test-sheet link. Never use the hardware-pass card for a
partial, failed, or synthetic run.

## Required close-ups

1. Old stereo limitation.
2. 40×13 Levels overview with all 18 meters in three groups of six.
3. Quiet, nominal, yellow/red, clipped, missing, and faulted meter examples.
4. Several arm dots, elapsed time, active count, drop/xrun/high-water summary.
5. Separate mono 24-bit WAV filenames.
6. Manifest with equal frames, complete state, and zero counters.
7. Generic JACK statement before naming MR18.
8. MR18 acceptance card clearly labelled `PLANNED` or a dated measured `PASS`.
9. Synthetic fallback card containing the word `SYNTHETIC` throughout.

Keep the Audio setup list and Levels overview visibly separate. Levels uses
−48, −36, −30, −24, −18, −12, −6, −3, and −1 dBFS, with RMS fill and a
brighter same-colour held peak. Do not add selected-channel numeric/route
detail or imply that visual metering is audible software monitoring.
