# Fixed stereo MASTER STRIP

This document owns the implemented mastering path, its algorithm provenance,
and the hardware-independent evidence added with it. It is not a claim of
listening approval or hardware/JACK acceptance.

## Signal order and ownership

The fixed Project-global order is:

```text
source plus aux sum
-> Project MASTER effects rack
-> live master fader
-> INPUT -> TONE -> GLUE -> COLOR -> IMAGE -> LOUD/true-peak limiter
-> final meter
-> identical final WAV tap and JACK playback buffers
```

The MASTER rack remains the reorderable creative/corrective rack. The MASTER
STRIP is one fixed stereo processor, not another rack. Its settings do not
follow Pattern changes. Project formats 9 and 10 store one strict strip record;
formats 0–8 acquire the neutral record in memory without rewriting the source
Project. Unknown fields, missing fields, invalid/non-finite values, an unknown
strip version, and newer Project formats are rejected before replacement.

INPUT, TONE, GLUE, COLOR, and IMAGE have independently smoothed bypasses.
Whole-strip `A/B` fades those optional sections and LOUD push to neutral while
retaining the same delay and protected limiter. The saved edited values are
not reset. The true-peak boundary has no bypass. Numerical changes can be
auditioned while playback runs; a final recording rejects edits. With no active
owned graph, edits change only Project state.

## Parameters

| Section | Parameter | Values; default |
|---|---|---|
| INPUT | trim | -12..+12 dB in 0.5 dB steps; 0 dB |
| INPUT | second-order minimum-phase HPF | off, 20, 30, 40 Hz; off |
| TONE | low shelf frequency/gain | 30, 50, 70, 90 Hz; 50 Hz; -6..+6 dB in 0.5 dB steps; 0 dB |
| TONE | high shelf frequency/gain | 8, 12, 16, 20 kHz; 12 kHz; -6..+6 dB in 0.5 dB steps; 0 dB |
| GLUE | threshold/ratio | -30..0 dB; -18 dB; 1.5:1, 2:1, 4:1; 2:1 |
| GLUE | attack/release | 10, 30, 100 ms; 30 ms; 100, 300, 600 ms; 300 ms |
| GLUE | sidechain HPF | off, 60, 90, 120 Hz; off |
| GLUE | knee/mix/makeup | fixed 6 dB; 0..100%, 100%; 0..+6 dB, 0 dB |
| COLOR | drive/character/mix/trim | 0..12 dB, 0 dB; -100..+100%, 0%; 0..100%, 100%; -6..0 dB, 0 dB |
| IMAGE | width/added-side HPF | 50..150%, 100%; 120, 180, 250 Hz; 180 Hz |
| LOUD | pre-limiter push | 0..+6 dB; 0 dB |
| LOUD | output ceiling | -2.0..-0.5 dBTP in 0.1 dB steps; -1.0 dBTP |

Optional sections default bypassed. The neutral strip is therefore an exact
delayed reconstruction while true-peak protection remains active.

## Algorithms and stereo behaviour

INPUT and TONE use identical biquad coefficients for both channels, prepared
outside the callback. Coefficient and bypass changes
crossfade over 5 ms; gains move over 10 ms. The HPF and broad shelves are
minimum-phase serial filters, with no crossover/recombination path.

GLUE is a feed-forward, full-band compressor. A 10 ms quasi-RMS energy detector
uses the sum of both sidechain channel powers; the one resulting gain is
applied to both output channels. The optional second-order sidechain HPF also
uses matched coefficients. The static curve is a 6 dB quadratic soft knee.
Attack and release are one-pole gain-envelope time constants. There is no
look-ahead and no automatic makeup.

COLOR uses SHR's declared bounded transfer:

```text
odd(x)  = x - x³/3 for |x| < 1, otherwise sign(x) * 2/3
even(x) = clamp(x, -1, 1)²
f(x,c)  = (odd(x) + 0.25*c*even(x)) / (1 + 0.25*abs(c))
```

`c` is character from -1 to +1: magnitude increases the bounded even component
relative to the odd component, while sign reverses that component's polarity.
First-order antiderivative antialiasing (ADAA) evaluates this transfer, drive
is divided back out, the explicit trim follows,
and a 5 Hz DC blocker precedes IMAGE and the limiter. The one-sample dry
alignment stays in the path when bypassed, avoiding a latency change during
comparison. This is harmonic saturation/peak rounding with a specified
transfer, not a transformer or other circuit emulation.

The ADAA candidate was retained instead of placing COLOR in its own oversampled
island: deterministic harmonic, two-tone IMD, foldback, DC, level, and callback
tests cover it, while a second interpolation/decimation pair would add response
error, memory, and callback work. The production 8× island is reserved for
pre-limiter detection and independent post-limiter verification.

IMAGE converts with `M=(L+R)/2` and `S=(L-R)/2`. At 100% it returns the input
numerically unchanged. Narrowing scales the original side. Widening adds only
a high-passed copy of the side, so it does not increase low-frequency width.
It never changes width automatically. The meter publishes correlation and the
UI warns below -0.20; the sum `L+R` remains unchanged.

## True-peak limiter, meters, and latency

The limiter and its post-limiter verifier each use one fixed 8×, 24-tap-per-
phase Blackman-windowed sinc interpolator. ITU-R BS.1770-5 reports a theoretical
maximum under-read of 0.169 dB at 8× versus 0.688 dB at 4× for a Nyquist-limit
signal. The isolated 4× and 8× candidates are benchmarked by the command below;
there is no user-visible quality mode or runtime quality switch.

The detector takes the larger reconstructed left/right magnitude and derives
one shared gain. A 2.5 ms integer-sample look-ahead applies a linear shaped
attack rather than an early gain step, followed by 1 ms hold and a 100 ms
one-pole release. Detection uses a 0.25 dB internal guard below the selected
ceiling. The declared post-limiter tolerance is +0.30 dBTP. A final sample clamp
at the selected ceiling is counted as a safety fault boundary; supported finite
test signals must not rely on it.

Complete fixed software latency is:

```text
1 COLOR alignment sample + 12 interpolation-delay samples
+ round(sample_rate * 0.0025) look-ahead samples
```

That is 133 samples / 2.770833 ms at 48 kHz and 123 samples / 2.789116 ms at
44.1 kHz. JACK periods, driver safety buffers, converters, and hardware paths
are additional and are not included.

The fixed lock-free snapshot contains input and output sample peak, output
true peak (dBTP), GLUE and limiter gain reduction, correlation, and
BS.1770-compatible LUFS-M, LUFS-S, and LUFS-I. K weighting is prepared for the
active sample rate. Momentary and short-term windows are 400 ms and 3 s.
Integrated loudness uses 400 ms blocks at 100 ms steps with the -70 LUFS
absolute gate and -10 LU relative gate. `RESET I` explicitly clears its bounded
histogram. Loudness never changes gain.

Sample peak is the largest stored digital sample. True peak estimates the
largest reconstructed inter-sample waveform peak. LUFS measures K-weighted
programme loudness over a named time window; it is not a peak ceiling.

## Real-time bounds and repeatable evidence

All plans, coefficients, interpolators, the gain lookup table, meters, and
delay storage are created before callback use. The callback has no allocation
or free, locks, file access, logging, formatting, sleep, or unbounded loop.
Non-finite input becomes silence and damaged bounded state resets
deterministically. The graph retains one client, one final slice, and one
recorder/playback path.

Run the hardware-independent production workload in an optimized build:

```sh
target/release/shr master-strip-bench 20000 48000
```

It compares identical deterministic input at 64 and 128 frames with the strip
neutral and maximally active, then isolates the 4× and 8× interpolators. It
reports mean, p95, p99, maximum, mean callback-deadline percentage, fixed
processor state, and limiter-delay bytes. It opens no JACK client or hardware.

The 2026-07-26 run on the repository Raspberry Pi 4 Model B Rev 1.4 used
Rust 1.85, the locked optimized profile, 48 kHz, and 20,000 callbacks per
profile:

| Frames | Profile | Mean | p95 | p99 | Maximum | Mean / maximum deadline |
|---:|---|---:|---:|---:|---:|---:|
| 64 | neutral | 92.204 µs | 91.963 µs | 111.444 µs | 2,885.372 µs | 6.915% / 216.403% |
| 64 | maximally active | 92.154 µs | 92.000 µs | 101.092 µs | 200.277 µs | 6.912% / 15.021% |
| 128 | neutral | 181.988 µs | 182.925 µs | 192.036 µs | 275.406 µs | 6.825% / 10.328% |
| 128 | maximally active | 183.467 µs | 184.480 µs | 193.758 µs | 520.923 µs | 6.880% / 19.535% |

The neutral 64-frame run contains one 2.885 ms descheduling outlier in this
ordinary non-real-time process; its p99 was 111.444 µs, and the active
64-frame maximum was 200.277 µs. The isolated 128-frame interpolators measured
32.096 µs mean, 32.056 µs p99, and 55.593 µs maximum for 4×; 8× measured
50.291 µs mean, 54.333 µs p99, and 88.981 µs maximum. Their mean / maximum
deadline shares were 1.204% / 2.085% and 1.886% / 3.337%. The complete
processor state was 21,632 bytes, including 1,056 bytes of limiter delay
storage. The 8× candidate was retained: the maximally active complete
processor stayed below 7% mean, 8% p99, and 20% maximum of either callback
deadline while providing the lower BS.1770 theoretical under-read.

Two additional two-second, three-source release-mode recorder stresses at
64 and 128 frames processed 96,000 frames each with zero drops or overflows
and byte-identical post-strip playback and WAV PCM. These are synthetic,
hardware-independent timing and identity results, not JACK/xrun, listening, or
physical-device acceptance.

The focused tests additionally cover neutral reconstruction, HPF/shelf
response, GLUE curve/timing/link/sidechain, COLOR harmonics/IMD/alias/DC,
IMAGE unity/mono/correlation, adversarial true peaks, shaped attack and
release, chunk and accepted-rate invariance, parameter/bypass movement,
non-finite recovery, loudness reset, callback allocation, Project migration,
UI callers and 40×13/compact rendering, and final playback/WAV PCM identity.

## Sources and exclusions

Algorithm choices were checked against:

- [ITU-R BS.1770-5](https://www.itu.int/rec/R-REC-BS.1770-5-202311-I/en)
- [EBU Tech 3341](https://tech.ebu.ch/docs/tech/tech3341.pdf) and
  [EBU Tech 3343](https://tech.ebu.ch/docs/tech/tech3343.pdf)
- Giannoulis, Massberg and Reiss,
  [“Digital Dynamic Range Compressor Design—A Tutorial and Analysis”](https://aes2.org/publications/elibrary-page/?id=16354)
- Bilbao, Esqueda, Parker and Välimäki,
  [“Antiderivative Antialiasing for Memoryless Nonlinearities”](https://www.research.ed.ac.uk/files/34115216/bilbao_pdf.pdf)
- the dated [Phase 1](PHASE1_AUDIO_GRAPH_MEASUREMENT.md),
  [Phase 2](PHASE2_AUDIO_GRAPH_MEASUREMENT.md), and
  [Phase 3/4](PHASE3_4_AUDIO_GRAPH_MEASUREMENT.md) Pi 4 evidence
- the [SSL Fusion guide](https://www.solidstatelogic.com/assets/uploads/downloads/Fusion_User_Guide_V1.3.0.pdf)
  only as compact stereo-bus workflow precedent.

The implementation is SHR's own and does not copy third-party DSP or claim to
emulate SSL or another commercial circuit. This scope explicitly excludes
multiband compression, a separate exciter, psychoacoustic/virtual bass,
subharmonic generation, and branded transformer modes.
