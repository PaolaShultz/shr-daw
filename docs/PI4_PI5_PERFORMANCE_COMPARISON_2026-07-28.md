# Raspberry Pi 4 and Raspberry Pi 5 performance comparison

This 2026-07-28 measurement used SHR-DAW `0.4.2` at exact commit
`628e88cca39e6bf01201fd3eea2a2f6a2ba3a239`. It measured the Raspberry Pi 5
before any optimization work. No Rust source, profile, DSP algorithm, package
version, audio configuration, or tuning setting was changed.

The unchanged hardware-independent MASTER STRIP workload is the only
compatible timing comparison. The Raspberry Pi 5 completed its representative
mean callback work about 1.82 to 1.84 times as fast as the Raspberry Pi 4, a
45.2 to 45.9% time reduction. The 4× and 8× interpolation means improved by
1.67× and 1.71×. Historical build results took longer on the Pi 4, but their
older source revision and different cache state prevent a hardware speedup
claim.

## Evidence and comparison classes

The Raspberry Pi 5 results below are directly observed. Raw transcripts,
one-second thermal/clock/memory samples, temporary Cargo targets, PMU output,
and synthetic WAV/take output remain in one ignored private measurement
directory. No private configuration, hostname, network address, storage
identifier, preset name, or route is reproduced here.

Pi 4 evidence is classified as follows:

| Evidence | Class | Why |
| --- | --- | --- |
| MASTER STRIP benchmark and two-second final-mix stresses | Compatible historical baseline | The public result was recorded on the Pi 4 at the MASTER STRIP introduction. Git inspection shows no change to `src/master_strip.rs`, `src/audio_recorder.rs`, or the benchmark/stress command between that implementation and `628e88c`; only unrelated commands and package metadata changed. |
| Phase 1 dry graph | Contextual only | Older commit `573c6ad`, one-source graph before later buses and the fixed final strip. |
| Phase 2 insert profiles | Contextual only | Older graph revision without the later final strip. |
| Phase 3/4 `dry` and `phase4-full` profiles | Contextual only | The current callback includes the later fixed neutral strip, so the workload is materially different even when the profile name matches. |
| Private 2026-07-22 Pi 4 build transcripts | Contextual only | Version `0.3.93`; normal warm target with only the application crate rebuilding, not a fresh target at `628e88c`. |
| Pi 4 storage description | Contextual only | The historical system used microSD, but no matched throughput command was retained. |

No exact matched Pi 4 baseline exists for commit `628e88c`. The compatible
MASTER STRIP classification is narrower: the measured processor and command
are unchanged, even though unrelated application code and the package version
advanced.

For lower-is-better timings, `speedup = Pi 4 / Pi 5` and time reduction is
`(Pi 4 - Pi 5) / Pi 4 × 100%`. Maximum latency is kept separate from
representative speedup.

## Hardware and software

| Item | Historical Raspberry Pi 4 | Observed Raspberry Pi 5 |
| --- | --- | --- |
| Board | Raspberry Pi 4 Model B Rev 1.4 | Raspberry Pi 5 Model B Rev 1.1, revision `b04171` |
| CPU | Four Cortex-A72 cores | Four Cortex-A76 r4p1 cores; CPUs 0 to 3 online |
| Cache | Not retained in the measurement pages | Per core: 64 KiB L1 data, 64 KiB L1 instruction, 512 KiB L2; shared 2 MiB L3; 64-byte lines |
| RAM | 4 GB model, about 3.7 GiB usable | 2 GB model; 2,058,432 KiB / 1.963 GiB usable |
| OS/kernel | Raspberry Pi OS generation represented by Linux `6.12.93+rpt-rpi-v8`, aarch64 | Debian 13.6, Linux `6.18.34+rpt-rpi-2712`, aarch64, PREEMPT |
| Firmware | Not retained | Firmware and bootloader dated 2026-05-26 |
| Root storage | microSD; model and mount options not retained | 128 GB-class Kingston DRAM-less NVMe, 119.2 GiB device, ext4 `rw,noatime`, PCIe 3.0 8 GT/s ×1 |
| Rust/Cargo | Rust 1.85 | `rustc 1.85.0`, `cargo 1.85.0`, LLVM 19.1.7 |
| Source | Revisions named in the historical pages | Exact `628e88c`, package `0.4.2` |
| CPU policy | Performance governor; CPU 3 isolated | Performance governor at 2.4 GHz; CPU 3 scheduler-domain isolated; default IRQ affinity CPUs 0 to 2 |
| JACK | 1.9.21; 48 kHz; 3 periods; 128 and 64 frames; RT 95; JACK and managed synth on CPU 3 | 1.9.22; 48 kHz; 128 frames; 3 periods; RT 95; server confined to CPU 3 |
| Cooling/power | Cooling and supply model not retained | Active PWM fan observed; reported 27 W supply was not electrically instrumented |

The Pi 5 was already tuned when this task began. `shr-audio-tune` owns the
CPU-3 isolation, performance-governor service, IRQ policy, and JACK affinity.
It therefore has only a current/tuned result, not an untuned result.
`doctor` found one configuration issue: host tuning owns CPU 3 while
`audio.engine_cpu` is unset in runtime configuration. The live JACK server is
still pinned correctly, but that mismatch should be resolved before a future
managed-engine acceptance pass. No tuning installation, removal, hand edit, or
reboot was performed.

## Matched commands

"Cold" below means a new empty `CARGO_TARGET_DIR`; downloaded crate sources and
the operating-system page cache were not discarded.

| Command/workload | Pi 5 execution | Pi 4 comparison |
| --- | --- | --- |
| `cargo fmt --all -- --check` | Timed once | No retained matched timing |
| `cargo test --locked` | Fresh target, then immediate warm repeat | Older warm 0.3.93 run; contextual |
| `cargo clippy --locked --all-targets -- -D warnings` | Fresh target, then warm repeat | Earlier revisions passed; current source fails four lints |
| `cargo build --release --locked` | Fresh target, then immediate warm repeat | Older application-only 0.3.93 rebuilds; contextual |
| `shr master-strip-bench 20000 48000` | Three uninstrumented timing runs | Compatible historical result with unchanged workload |
| `shr final-mix-stress … 2 48000 64/128` | Both callback sizes | Compatible two-second Pi 4 pass/fault result |
| `shr recorder-stress … 2 18 48000 128` | One matched two-second run | Compatible historical 18-channel synthetic result |
| 60-second final-mix and 18-channel recorder stress at 48 kHz/128 | Additional sustained window | Compatible older 60-second acceptance, but different application revision |
| 1 GiB direct write/read | Pi 5 only | No Pi 4 matched command; contextual |
| PMU MASTER STRIP runs | Pi 5 only | No Pi 4 PMU result |
| Connected `dry` / `phase4-full`, 128/64 | Not run; safety/ownership gate failed | Historical results remain contextual |

All compilation and stress commands used `/usr/bin/time -v`. Long runs also
captured temperature, clock, throttle flags, available memory, and swap once
per second, plus before/after process and kernel-warning snapshots.

## Build, memory, and footprint

| Pi 5 command | Wall | CPU | Peak RSS | I/O in/out | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| Format check | 1.71 s | 99% | 117,616 KiB | 0 / 0 | Pass |
| Tests, fresh target | 110.65 s | 196% | 1,501,136 KiB | 1,665,512 / 1,336,064 | 840 passed, 4 ignored |
| Tests, warm | 17.97 s | 258% | 110,192 KiB | 64,560 / 192 | 840 passed, 4 ignored |
| Clippy, fresh target | 50.42 s | 202% | 830,064 KiB | 300,712 / 384,608 | Failed four lints |
| Clippy, warm | 32.85 s | 171% | 826,272 KiB | 55,560 / 184,288 | Same four lints |
| Release, fresh target | 223.14 s | 120% | 1,407,344 KiB | 923,144 / 220,704 | Pass |
| Release, warm no-op | 0.18 s | 41% | 36,624 KiB | 64,600 / 64 | Pass |

I/O figures are `/usr/bin/time -v` filesystem blocks, not bytes.

The current source fails warning-denied Rust 1.85 Clippy for one `map_entry`,
two `obfuscated_if_else`, and one `unnecessary_lazy_evaluations` finding.
Those are pre-existing source findings. They were not repaired because this
pass must measure exact commit `628e88c` without changing code.

The available Pi 4 transcript ran 652 tests from a warm target in 20.32 s.
The Pi 5 ran 844 tests in 17.97 s. Three Pi 4 `0.3.93` release rebuilds that
compiled only SHR itself took 453.40 to 464.28 s; the Pi 5 fresh-target `0.4.2`
release built SHR and all dependencies in 223.14 s. These raw observations show
better practical turnaround on the Pi 5, but changed source, test count, and
cache coverage prevent an honest build speedup calculation.

The Pi 5 has about 1.737 GiB less usable RAM than the historical 3.7 GiB Pi 4
figure, about 46.9% less. By sold capacity class it is 2 GB versus 4 GB,
exactly 2 GB or 50% less. This is a difference, not a speedup.

The historical Pi 4 application-only release rebuilds had median peak RSS
1,686,932 KiB. The Pi 5 full fresh-target release peaked at 1,407,344 KiB,
279,588 KiB or 16.6% lower, but the differing source and cache workload make
that memory comparison contextual.

The 2 GB Pi 5 did complete every build without OOM. It was not memory-idle:
the OS had a 2 GiB zstd zram swap device with about 96 to 100 MiB already in use.
During the fresh test, sampled available memory fell to 179,584 KiB and
`SwapFree` transiently fell by about 894 MiB. During the fresh release build,
available memory fell to 111,744 KiB and `SwapFree` transiently fell by about
675 MiB. Most of that transient allocation disappeared when compiler processes
exited; post-run swap use remained about 57 MiB higher after tests and 67 MiB
higher after release than at their respective starts. `/usr/bin/time` reported
zero process swaps, but the system sampler proves zram activity, so these
results must not be described as swap-free. No OOM or memory-pressure kernel
warning was recorded; memory PSI was unavailable on this kernel.

The stripped release ELF was 4,262,352 bytes. Its principal sections were:

| Section | Bytes |
| --- | ---: |
| `.text` | 3,271,080 |
| `.rodata` | 259,672 |
| `.eh_frame` | 229,152 |
| `.gcc_except_table` | 120,804 |
| `.data.rel.ro` | 130,824 |
| All reported ELF sections | 4,254,473 |

The complete fresh release Cargo target occupied 78,662,498 apparent bytes,
about 75.0 MiB.

## Storage

The Pi 5 booted from its NVMe root, not microSD. A 1 GiB direct-I/O file on the
same ext4 filesystem measured 801 MB/s write with final data sync and 903 MB/s
read. The NVMe SMART log showed 43 °C, zero media errors, zero warning or
critical-temperature time, and zero percentage used. These are short local
throughput and health checks, not an endurance test.

The historical Pi 4 pages establish only that their root was microSD. Because
no matching Pi 4 command, media model, filesystem, or mount options were
retained, no storage speedup is calculated.

## MASTER STRIP DSP

Each Pi 5 value below lists all three runs. The historical Pi 4 value is the
compatible unchanged workload.

### Mean timing

| Frames/profile | Pi 4 mean | Pi 5 means, runs 1/2/3 | Pi 5 median (range) | Speedup | Time reduction |
| --- | ---: | ---: | ---: | ---: | ---: |
| 64 neutral | 92.204 µs | 53.829 / 49.817 / 50.552 µs | 50.552 µs (49.817 to 53.829) | 1.824× | 45.2% |
| 64 active | 92.154 µs | 54.060 / 49.885 / 49.853 µs | 49.885 µs (49.853 to 54.060) | 1.847× | 45.9% |
| 128 neutral | 181.988 µs | 102.623 / 99.489 / 99.418 µs | 99.489 µs (99.418 to 102.623) | 1.829× | 45.3% |
| 128 active | 183.467 µs | 100.188 / 99.687 / 99.637 µs | 99.687 µs (99.637 to 100.188) | 1.840× | 45.7% |

Median mean deadline use was 3.791%, 3.741%, 3.731%, and 3.738% in the same
row order, down from 6.915%, 6.912%, 6.825%, and 6.880% on the Pi 4.

### Percentiles

| Frames/profile | Pi 4 p95 | Pi 5 p95 median (range) | p95 speedup / reduction | Pi 4 p99 | Pi 5 p99 median (range) | p99 speedup / reduction |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 64 neutral | 91.963 µs | 50.185 µs (49.926 to 76.888) | 1.832× / 45.4% | 111.444 µs | 77.185 µs (51.833 to 94.222) | 1.444× / 30.7% |
| 64 active | 92.000 µs | 49.999 µs (49.962 to 76.777) | 1.840× / 45.7% | 101.092 µs | 51.889 µs (51.870 to 79.221) | 1.948× / 48.7% |
| 128 neutral | 182.925 µs | 99.795 µs (99.666 to 126.073) | 1.833× / 45.4% | 192.036 µs | 102.165 µs (101.963 to 140.870) | 1.880× / 46.8% |
| 128 active | 184.480 µs | 100.073 µs (100.073 to 100.222) | 1.843× / 45.8% | 193.758 µs | 102.388 µs (102.258 to 103.573) | 1.892× / 47.2% |

The first Pi 5 run was consistently noisier at 64 frames. The range is retained
rather than selecting only the two tighter runs.

### Maximum timing and uncertainty

| Frames/profile | Pi 4 maximum | Pi 5 maxima, runs 1/2/3 | Pi 5 median | Interpretation |
| --- | ---: | ---: | ---: | --- |
| 64 neutral | 2,885.372 µs | 122.277 / 80.018 / 105.518 µs | 105.518 µs | Pi 4 had one known non-real-time descheduling outlier. |
| 64 active | 200.277 µs | 121.518 / 73.092 / 84.925 µs | 84.925 µs | All Pi 5 maxima below 9.2% of deadline. |
| 128 neutral | 275.406 µs | 373.775 / 123.147 / 119.388 µs | 123.147 µs | First Pi 5 run was noisier; worst was 14.0% of deadline. |
| 128 active | 520.923 µs | 7,446.423 / 128.110 / 116.869 µs | 128.110 µs | One Pi 5 process-descheduling event reached 279.2% of the nominal deadline while p99 stayed 103.573 µs. |

The command is an ordinary non-real-time process. Its isolated 7.446 ms maximum
is not a JACK xrun, and it is not averaged into a "maximum speedup." It does
show why scheduler outliers and representative DSP throughput are separate
questions.

### Interpolation and state

| Workload | Pi 4 mean | Pi 5 mean runs | Median speedup / reduction | Pi 4 p99 | Pi 5 p99 runs | Median p99 speedup / reduction |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 4×, 128 frames | 32.096 µs | 19.209 / 19.276 / 19.271 µs | 1.666× / 40.0% | 32.056 µs | 19.260 / 19.352 / 19.297 µs | 1.661× / 39.8% |
| 8×, 128 frames | 50.291 µs | 29.404 / 29.369 / 29.365 µs | 1.712× / 41.6% | 54.333 µs | 29.555 / 29.519 / 29.500 µs | 1.841× / 45.7% |

Pi 5 maximum ranges were 23.741 to 42.555 µs for 4× and 42.278 to 58.518 µs
for 8×. Processor state remained 21,632 bytes, including 1,056 bytes of
limiter delay storage. This is expected: the code and workload are unchanged.

## PMU counters

The matching `linux-perf` package was installed only after the kernel exposed
the Cortex-A76 PMU and confirmed `CONFIG_PERF_EVENTS`, `CONFIG_HW_PERF_EVENTS`,
`CONFIG_ARM_PMU`, and `CONFIG_ARM_PMUV3`. Separate two-event runs scheduled
each counter for 100% of the measurement instead of relying on multiplexed
estimates:

| Counter | Result |
| --- | ---: |
| Cycles | 17,529,346,748 |
| Instructions | 33,007,189,777 |
| Instructions per cycle | 1.88 |
| L1 data-cache loads / misses | 9,353,593,453 / 70,257 |
| L1 instruction-cache loads / misses | 8,387,138,591 / 70,489 |
| Last-level-cache loads / misses | 102,827 / 62,371 |

These counters characterize the Pi 5 workload only. There is no Pi 4 PMU run,
so they do not establish cache speedup. The last-level miss percentage is high
for a very small architecture-event subset and should not be generalized to
all memory traffic.

## Synthetic recording and final mix

| Pi 5 run | Frames | Callback result | Writer result |
| --- | ---: | --- | --- |
| Final mix, 2 s, 64 frames | 96,000 | Mean 51.381 µs; p95 53.351; p99 65.759; max 112.666 | High-water 128 frames; zero drops/overflows; playback/WAV equal |
| Final mix, 2 s, 128 frames | 96,000 | Mean 104.051 µs; p95 126.499; p99 129.444; max 172.887 | High-water 128 frames; zero drops/overflows; playback/WAV equal |
| 18-channel recorder, 2 s, 128 frames | 96,000 per stem | Real-time pacing completed in 2.155 s | High-water 640 frames; zero drops/overflows; channel identity verified |
| Final mix, 60 s, 128 frames | 2,880,000 | Mean 105.187 µs; p95 127.906; p99 137.554; max 465.107 | High-water 768 frames; zero drops/overflows; playback/WAV equal |
| 18-channel recorder, 60 s, 128 frames | 2,880,000 per stem | Real-time pacing completed in 60.206 s | High-water 1,408 frames; zero drops/overflows; channel identity verified |

The compatible Pi 4 evidence recorded the same pass/fault outcome for the
two-second final-mix runs and the 18-channel two-second recorder, but did not
retain callback timing for a speedup calculation. The 60-second Pi 4 acceptance
also passed, though it used an older application revision.

These are synthetic writer and DSP results. They are not physical-input,
interface, listening, JACK scheduling, or xrun evidence.

## Thermal, clock, and power state

The Pi 5 stayed at 2.4 GHz in every one-second build sample under the
performance governor. The complete measured SoC range was 54.0 to 63.35 °C;
the fresh release build reached 61.7 °C, and both 60-second recorder stresses
stayed at or below 57.3 °C. The active cooler was visible as a four-state PWM
fan and was already running at state 1 during the identity capture.

`vcgencmd get_throttled` remained `0x0` before, during, and after the long
workloads. Kernel logs contained no undervoltage, throttling, thermal-limit, or
OOM warning. The CPU clock did not vary in the samples. The NVMe remained at
43 °C with no recorded warning-temperature time.

This proves no firmware-recorded throttle or undervoltage event occurred during
the pass. It does not measure wall power, transient supply voltage at the
connector, acoustic noise, or cooling performance in an enclosure.

## Connected JACK gate and restoration

Connected measurement was not run. At the start, an interactive SHR process
was attached to the physical TTY. It later exited, but a new interactive SHR
session then started an owned synth and direct playback route. Physical output
safety was also not confirmed. Starting another checkpoint would have layered
owners or required stopping a session that this measurement task did not own.

No checkpoint note or MIDI message was sent. No JACK service, sample rate,
period size, process, ALSA subscription, or audio connection was changed by
this task. There was therefore no connected teardown and no teardown-only xrun
to classify. The interactive session later exited without intervention from
this task. The final capture had one active JACK server at the original
48 kHz, 128 frames, three periods, RT 95, and CPU 3 placement; no SHR or synth
process; no JACK audio connection; and no task-owned client or note. The task
did not try to relaunch the earlier TTY session.

A matched rerun needs a deliberately idle session, confirmed-safe output, one
cleared preset class, exact `dry` and `phase4-full` durations, and both 128- and
64-frame JACK states restored through the managed helper. It must capture
callback count, mean, p95, p99, maximum, misses, oversized callbacks, sustained
versus teardown xruns, owner/synth CPU and RSS, migrations, graph storage,
temperature, clocks, throttling, and byte-identical route snapshots. Because
the current fixed strip did not exist in the historical Phase 3/4 runs, an
exact hardware speedup still requires rerunning the Pi 4 at `628e88c`.

## Conclusions

The compatible MASTER STRIP result is clear:

- representative complete-strip mean time fell by 45.2 to 45.9%, a
  1.82 to 1.84× speedup;
- most p95 and p99 rows improved by about 45 to 49%, with the noisier 64-frame
  neutral p99 improving by 30.7%;
- 4× and 8× interpolation means fell by 40.0% and 41.6%; and
- the same fixed processor state and limiter storage produced those gains.

The Pi 5 also completed a fresh full release in less wall time than the older
Pi 4 took to relink only SHR, and its NVMe delivered strong short direct-I/O
throughput. Those observations are operationally useful but are not exact
hardware speedups without a matched Pi 4 rerun.

The 2 GB configuration passed compilation, tests, DSP, and 60-second synthetic
writer stresses without OOM, thermal throttling, undervoltage, drops, or
overflows. It did use zram heavily during fresh compilation, leaving much less
memory margin than the 4 GB Pi 4. This evidence supports the 2 GB Pi 5 for the
measured CLI/TUI and synthetic workloads; it does not yet establish 2 GB as a
universal recommendation for concurrent compilation, Codex, JACK, synth,
recording, and other services.

Still unproven are an exact build speedup, exact current audio-callback speedup,
microSD-to-NVMe speedup, untuned-versus-tuned Pi 5 behavior, connected 64-frame
stability, physical-interface xruns, MR18 or recording hardware, listening
quality, wall power, and long enclosure/ambient thermal behavior.
