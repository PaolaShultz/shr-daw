# Instrument channel inserts and MASTER

Home **EFFECTS**, Player **FX**, FT2 **SOUND → FX**, and Performance **FX**
open the same **MASTER** workspace. Its six rows are CHANNEL INSERTS, AUX 1,
AUX 2, AUX 3, MASTER INSERTS, and MASTER STRIP. Choose a row and press the
encoder/Enter. Back returns through MASTER to the original caller, retaining
its controller page, selection, FT2 cursor, and editing context. Navigation
neither loads an engine nor starts or stops transport. STOP ends the relevant
musical action; PANIC performs the existing bounded note recovery.

CHANNEL INSERTS opens **INSERTS**. Up/Down selects an instrument output;
Enter or FIELD selects BASS, TREBLE, or COMP. VALUE−/VALUE+ (keyboard Left/Right)
changes that field. BYPASS (keyboard X) explicitly enables/disables the strip.
Changing a value while bypassed keeps it bypassed. New instruments have flat EQ
and compression OFF. The screen shows backend, portable instrument identity,
Project strip number when assigned, bypass state, and output level/reduction
when that instrument has an active return. Unavailable returns stay explicit.
These screens use the shared controller rows and status renderer at 40×13.

## Actual backend coverage

The following boundaries were inspected in the host code and the component
renderer sources. This is software evidence, not a listening or hardware test.

| Backend | Independently processable output in this implementation | Boundary and limitation |
| --- | --- | --- |
| SHR Sampler | The one loaded package, identified by its package instrument ID | The host loads one package and `Engine::render_block` sums its voices into one stereo pair. Regions, velocity layers, and voices are parts of that instrument, not separate instrument outputs. Multiple simultaneous packages require a sampler host/renderer interface extension. |
| SHR Drums | The selected kit, independent of the managed melodic instrument | The pinned `shr-drums` 0.2.0 `DrumEngine::process` sums voices before its kit bus. Individual kick/snare/etc. outputs are unavailable at that API; piece strips require a change in the `shr-drums` repository. No duplicate engines are created. |
| Moj Sint | The loaded patch | Model D, Six-Op PM, Strange Oscillator, Swarm Machine, Bass Matrix, Dual Filter, and Pressure Chain are selectable algorithms within one owned renderer. The loaded patch provides one stereo output; the seven models and Pressure Chain’s three preset topologies are not simultaneous independent outputs. |
| synthv1 | The one owned preset | Its exact stereo return belongs to that instrument. Lanes referencing it share the strip. This does not create multitimbral synthv1 outputs. |
| Yoshimi | The one instrument loaded by the existing host | SHR-DAW loads one `.xiz` instrument and accepts one exact stereo pair. Yoshimi's synthesis engines within that instrument are not separate outputs. Multipart/dedicated part output management is not implemented in SHR-DAW. |
| FluidSynth (optional) | Unavailable for channel strips | SHR-DAW already manages multiple MIDI parts within one process, but receives their shared stereo mix. The current resolver requires exactly two outputs. Supporting part strips needs host configuration and explicit per-part JACK port/routing support in this repository; no shared mix strip is substituted, even for a currently single-part plan. |
| External MIDI | Unavailable for channel strips | MIDI port/channel identity does not establish audio isolation. The configured Input return may contain a mix. Explicit instrument-to-independent-return bindings and additional input routing are required. |

Owning source references: `src/engine.rs` (`backend_command`,
`resolve_managed_audio_outputs`), `src/drums_host.rs`, `src/ui.rs`
(`SoftwarePlaybackPlan`, `channel_bindings`), and `src/audio_graph_client.rs`
(`process_block`). Component boundaries are described in
[How SHR-DAW works](HOW_IT_WORKS.md#maintained-component-repositories).
No other repository was modified. Existing engine replacement, All Notes Off,
pickup, and process ownership rules remain in force.

## Signal and identity

The channel strip runs once at the isolated return, before the source fader,
legacy SOURCE inserts, and their PRE/POST AUX taps. Both AUX tap positions
therefore hear the processed managed instrument. AUX remains wet-only; channel
strips do not add new sends or change existing send levels. The drum kit strip
runs after the owning drum renderer's existing kit bus and fixed Reverb/Delay,
before its final-bus fader. Drums retain their existing routing; they do not
acquire melodic AUX sends. MASTER INSERTS still processes the complete dry/wet
sum, followed by the live master fader and separate fixed MASTER STRIP.

Processing requires the existing final bus. With that bus disabled or absent,
settings remain Project-owned and INSERTS reports no active return. Browsing
MASTER/INSERTS does not activate JACK or change direct audio routing.

Project format **19** adds a required strict `channels` record. Formats 0–18
migrate to an empty channel collection in memory. Loading never rewrites a
file. SOURCE racks and their automation retain their own identifiers and data.
Unknown fields/backends, duplicate bindings/IDs, zero IDs, malformed values,
and more than 128 strips are rejected before replacement.

Each configured strip has a Project-local numeric ID and a backend-qualified
portable catalog/package binding. Lane number, lane name, MIDI channel, and
catalog order are not strip keys. Repeated routes across Patterns share the
same binding. Changing a lane's routing selects the destination instrument's
settings; it does not transfer the old instrument's strip. Unreferenced saved
strips remain stored. Saving the owned preset under a new name updates its
binding while retaining the numeric ID when the destination binding is free
and no other Project route retains the previous preset. Otherwise the old
strip stays attached to those routes and the new instrument starts flat/OFF.
External filesystem renames are not guessed or matched by a similar name.

## DSP and verification

BASS and TREBLE are 120 Hz and 8 kHz shelves, bounded to ±6 dB in 0.5 dB steps.
They reuse the existing stable-filter crossfade implementation. COMP is a
stereo-linked, soft-knee compressor: amount moves threshold from roughly −6 to
−30 dBFS and ratio from 2:1 to 4:1, with 10 ms attack, 150 ms release, and
6 dB knee. Compensation rises only to +1.5 dB at maximum amount. It is not
loudness normalization or automatic mastering. Zero amount bypasses compression.
Strip and compression enable changes use 10 ms fades; shelf changes crossfade
stable filters over the existing 64-sample transition.

Two small processors are preallocated for the managed instrument and drum
returns, not one arbitrary rack per lane or saved instrument. Stable flat/OFF
skips filters and compression and preserves samples exactly; peak metering
still scans the block. Control settings and an instrument-reset generation
cross the callback boundary in one atomic word. The callback uses no blocking
synchronization or allocation, performs no catalog lookup, and stays within the
existing graph frame bounds. Changing instrument identity clears previous DSP
history. Tests instrument allocation on both parameter changes and processing.

Run focused checks with `cargo test --locked channel_` and
`cargo test --locked workspace_effects`. The complete normal suite also covers
legacy automation, recovery, recording, ownership, STOP/PANIC, and shutdown.
The synthetic cost renderer is explicitly opt-in:

```sh
cargo test --locked effects::channel::tests::channel_strip_offline_cost -- --ignored --exact --nocapture
```

Observed on 2026-09-05 with Rust 1.97.1 (`8bab26f4f68e0e26f0bb7960be334d5b520ea452`),
AArch64, LLVM 22.1.6: formatting and locked check passed; the complete normal
suite passed 1,127 tests with 14 opt-in tests ignored. The focused channel
filter passed 54 tests. Only the new relevant cost test was additionally run
with `--ignored`; unrelated historical/exhaustive tests were not run.

The offline test measured 10,000 warmed 128-frame calls at 48 kHz: **4.053 µs
OFF** and **57.115 µs ON** (both shelves and COMP 60), including input-block
refill and loop overhead. The measured increment was **53.062 µs per block**.
These are isolated processor-call costs in Cargo's **unoptimized test profile**,
not complete JACK callback timings or DEV/REL performance. Dividing that
increment by a 2,666.7 µs block period estimates **1.99% of one core per active
strip** for this synthetic/test-profile workload. It does not measure process
CPU utilization, scheduling interference, worst-case callback cost, or real-time
headroom. No release binary was rebuilt, and no audio services were started.

Listening to bass/treble extremes, compressor behavior on real drums, rapid
controller gestures, physical 40×13 readability, actual backend returns, and
real-time headroom remain for the later coordinated human session. Offline
sample/transition tests do not prove perceptual transparency or live headroom.
