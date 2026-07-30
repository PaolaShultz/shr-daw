# Final stereo performance bus

The owned audio graph has one deliberately small final bus. It is not a
free-wiring view or a general-purpose mixer. Input is the only required source;
the managed synth, Loop Mix, and SHR Drums attach when present:

```text
managed software instrument -> source inserts/optional aux sends --\
owned four-slot native-rate Loop Mix sum ---------------------------+-> stereo sum
configured JACK capture L/R ---------------------------------------/
SHR Drums in-process stereo bus ------------------------------------/
    -> optional aux returns, where routed from the managed source
    -> master insert rack
    -> master level
    -> fixed Project MASTER STRIP
       INPUT -> TONE -> GLUE -> COLOR -> IMAGE -> LOUD/true-peak limiter
    -> FINAL meter
    -> final 24-bit stereo WAV tap
    -> configured JACK playback L/R
```

The logical Loop and external-input bus strips do not gain individual insert
racks, aux sends, pan, solo, automation, or waveform editing. Drums has only
its fixed Reverb-then-Delay rack before this bus; it does not become a general
mixer strip. Synth, Loop, and Drums retain a smoothed level and mute. Input
instead has one unambiguous **MON ON**/**MON OFF** control in the same MTR
source-control position; it is never shown beside a duplicate Input mute.
Loop Mix applies its four
slot-local level/filter/mute controls before this one logical source. The
managed source keeps its existing Project-owned
insert/aux routing. Master level follows the complete sum. Source gain is
bounded to -60..+6 dB, master gain to -60..0 dB, and all level/mute transitions
use a 10 ms sample ramp. New runtime buses start each source at -6 dB to leave
basic summing headroom. Input monitoring always starts OFF. These live
performance controls are not
Project data; current Project format 13 stores effect racks/routing and the
fixed MASTER STRIP at Project scope and four Loop Mix settings under each
Pattern, but not these final-bus levels or mutes. JACK assignments remain
machine configuration.

## Exact routing and availability

The configured input is `audio.graph.input=LABEL|LEFT|RIGHT`. When that optional
new key is blank, the first legacy `capture.input` pair is reused so older
runtime configuration remains useful. Both exact names must exist and be
distinct. A similar-looking or adjacent port is never substituted. MTR Input
**MON ON** can activate the bus with only this input and the exact playback
pair. It never starts a synth, WAV loop, or drum host. Missing optional sources
stay `OFFLINE`; when Loop is present, its four active renderers sum to one pair
before the graph, so the bus, limiter, final meter, and recorder receive the
complete Loop sum once.

Before attachment, synth, drums, and Loop may have their ordinary direct stereo
routes. The graph connects a present source to its fixed input and removes only
that source's exact owned direct links as one rollback-capable transaction.
Optional disappearance is silence, not bus failure; periodic owner-thread
reconciliation reconnects only the remembered exact names when they return.
Input loss while MON is on faults a final recording and reports the nearby
recovery state. JACK loss deactivates the callback before restoring available
owned direct routes. No path invents a replacement or alters an unrelated
link.

`audio.graph.input_direct_monitoring` describes whether the interface's own
zero/low-latency direct monitor is also audible. The final bus is software
monitoring. Enabling both without
`audio.graph.confirm_doubled_monitoring=true` refuses **MON ON** because
the delayed software copy and direct copy can comb-filter or sound doubled.
Confirmation is deliberately explicit; it does not change interface hardware.

## MASTER STRIP and limiter

The Project-owned strip follows the live master level in one fixed stereo
order. Its optional INPUT, broad TONE, full-band linked GLUE, declared ADAA
COLOR, and conservative M/S IMAGE stages default bypassed. LOUD is a 0..+6 dB
push into an unbypassable stereo-linked true-peak limiter. The ceiling is
-2.0..-0.5 dBTP and defaults to -1.0 dBTP.

The limiter uses fixed 8×, 24-tap-per-phase interpolation before its detector
and again after limiting for the published true-peak result. The larger
reconstructed channel peak controls one shared gain. Its 2.5 ms look-ahead has
a shaped attack, 1 ms hold, 100 ms release, and a 0.25 dB internal guard. The
supported post-limiter tolerance is +0.30 dBTP. The final numerical clamp is a
counted safety boundary, not the normal limiting mechanism.

Complete fixed strip latency is 1 COLOR alignment sample, 12 interpolation
delay samples, and `round(rate * 0.0025)` look-ahead samples: 133 samples
(2.770833 ms) at 48 kHz and 123 samples (2.789116 ms) at 44.1 kHz. JACK and the
interface add their own periods, safety buffers, and converter delays. SHR
does not hide those in the software figure. For example, one 128-frame period
is 2.667 ms at 48 kHz.

The final snapshot distinguishes input/output sample peak, output dBTP,
GLUE/limiter gain reduction, correlation, and LUFS-M/S/I. Details, exact
parameters, algorithms, provenance, exclusions, and repeatable measurements
are in [Fixed stereo MASTER STRIP](MASTER_STRIP_MEASUREMENT.md).

## Final recording

MTR `REC` arms one final-mix recording. Start and stop are sampled only at
whole callback boundaries. The callback gives the recorder the same final
limited `StereoFrame` slice that is then copied to JACK playback. A bounded
interleaved stereo ring transfers it to a non-real-time writer, which performs
24-bit conversion, file writes, flush, synchronization, and no-replace
publication.

The result is one conventional little-endian PCM RIFF/WAVE file: two
interleaved channels, 24 bits, and the active JACK sample rate. It includes
every present unmuted optional source and the configured external input only
while Input MON is on, plus managed-source aux returns, master rack, master
level, and complete MASTER STRIP. It excludes raw recorder stems, unrelated
JACK clients, interface
direct monitoring, hardware mixer/insert processing after JACK playback, and
any downstream speaker/headphone processing.

Classic RIFF has a 32-bit data size. Stereo 24-bit audio uses six bytes per
frame, so SHR stops before 715,827,876 frames instead of wrapping. That is about
4:08:33 at 48 kHz or 4:30:32 at 44.1 kHz. A zero-frame take is not published.
Overflow, writer failure, JACK shutdown/xrun, oversized callback, invalid
buffer, or required-source loss stops/faults the take visibly. A faulted
`*.wav.part` remains recoverable; it is never presented as a successful final
WAV. Existing raw multitrack sessions and legacy stereo recovery remain
unchanged.

## Generic interface setup and future MR18 acceptance

Use the setup wizard or edit private runtime configuration only after obtaining
the exact JACK names from the current machine. Choose one stereo capture pair
that already contains the desired external-gear mix. Keep interface direct
monitoring off for the normal software-monitored workflow, set conservative
hardware gains. Leave Input MON off until the hardware direct monitor is
disabled, then use MTR **MON ON**; `audio.graph.enabled` is needed only for
automatic final-bus startup, not for that explicit input-only action.

Optional sources reconcile automatically when they appear or return. MTR
`RESET` remains a manual retry for a stopped or failed bus. Neither path
rewrites the mapping or chooses another port.

No MR18 name is compiled into SHR. A future MR18 acceptance must follow
[the hardware plan](MR18_TEST_PLAN.md): discover and record the actual names,
confirm the intended two-channel external mix, decide direct-monitor behavior,
then test full duplex at conservative level. It must verify channel identity,
source unplug/reconnect faults, no doubled direct/software path, final playback
versus WAV equality, xruns/dropouts, and teardown restoration. Synthetic tests
and the current AudioBox-era configuration are not an MR18 pass.

Maintainers can exercise the production hardware-independent path with:

```sh
shr final-mix-stress DEST [SECONDS] [RATE] [CALLBACK]
```

It uses four distinguishable stereo sources, the production faders/strip,
bounded callback handoff, stereo writer, and full PCM equality check without
opening JACK, starting a synth, transmitting MIDI, or producing sound. See
[maintainer helpers](MAINTAINER_HELPERS.md#synthetic-final-mix-stress).
