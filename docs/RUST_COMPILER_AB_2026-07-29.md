# Rust compiler A/B on Raspberry Pi 5

This is a short native compiler comparison, not a release soak or physical
audio acceptance. It answers whether Rust 1.97.1/LLVM 22 improves the same
current SHR-DAW workload relative to Rust 1.85.0/LLVM 19, then records the exact
repository toolchain adopted after the comparison.

## Conclusion

The result is **mixed by workload, with a clear regression in the final audio
path**:

- the ordered clean Rust 1.97.1 build finished 40.7% sooner, used 6.5% less peak
  RSS, and produced a 7.2% smaller executable;
- standalone SHR Drums dry/Reverb/Delay/combined callback medians and p99 were
  effectively unchanged, generally within about 2%;
- work containing the fixed MASTER STRIP regressed materially: isolated strip
  mean time rose about 87%, the complete dry graph rose about 86–100% by median,
  the maximally enabled graph rose about 30–41%, and drum-plus-melody final-bus
  work rose about 17–19%; and
- every paired workload produced bit-identical output across the two
  compilers. The maximum absolute and RMS sample differences are therefore
  exactly zero.

Rust 1.97.1 is adopted as the exact repository pin as requested. That adoption
does not turn the runtime regression into a speed improvement; future compiler
changes should rerun this short comparison deliberately.

A separate 2026-07-30 follow-up, documented below, found that LLVM 22 made the
interpolator's modulo-indexed inner loop unusually expensive. Splitting the
ring scan at its wrap removed the modulo while preserving accumulation order
and bit-exact output. That source optimization recovered most of the lost
runtime performance. It does not retroactively change the compiler-only A/B
above.

## Compared source and compilers

Both release artifacts used:

- SHR-DAW base commit
  `0ea7cc787a1f624b9eb415b2a5a5d3fde5a1dc5c`;
- `Cargo.lock` SHA-256
  `6417550918d91f415ce9e7b62a8d402abc2c2ef10f42cf1699de4fb470323418`;
- SHR Drums commit
  `856cc86d64baaa3511ecbd630da4a2d504e0eb8a`;
- the same small uncommitted `compiler-ab-bench` harness, which calls the
  existing production processors without changing their settings;
- the existing release profile: stripped output, full LTO, and one codegen
  unit; and
- separate empty targets below
  `$SHSYNTH_USER_DIR/compiler-ab-20260729/target-rust-1.85.0/` and
  `target-rust-1.97.1/`.

The final-mix fixture repair, exact toolchain pin, and documentation edits were
made only after the compared artifacts existed. They are not part of one side
of the A/B.

| Compiler | Exact identity |
| --- | --- |
| Rust 1.85.0 | `rustc 1.85.0 (4d91de4e4 2025-02-17)`; Cargo `1.85.0 (d73d2caf9 2024-12-31)`; LLVM `19.1.7`; AArch64 GNU host |
| Rust 1.97.1 | `rustc 1.97.1 (8bab26f4f 2026-07-14)`; Cargo `1.97.1 (c980f4866 2026-06-30)`; LLVM `22.1.6`; AArch64 GNU host |

Rust 1.85.0 used Cargo's `--ignore-rust-version` only for this historical
experiment. The tracked minimum remains Rust 1.97.

## Machine and method

The machine was a Raspberry Pi 5 Model B Rev 1.1 with 2 GB RAM, NVMe root,
Debian 13.6, and a PREEMPT `6.18.34+rpt-rpi-2712` AArch64 kernel. All four CPUs
used the performance governor at 2.4 GHz. CPU 3 remained isolated for the
already-running JACK server; runtime commands were pinned to CPU 0, so the
measured process could not migrate. JACK remained untouched at 48 kHz, 128
frames, three periods, RT priority 95, and CPU 3. No port or physical route was
opened or changed.

Before work, no Cargo, rustc, benchmark, render, synth, or competing worker was
active. The runtime sequence used five complete runs per compiler in
interleaved order rather than completing one compiler first. Each row warmed
500 callbacks and measured 2,000 at both 64 and 128 frames. The separate strip
command used five interleaved runs of 5,000 callbacks per profile. Recorded
runtime snapshots stayed at 2.4 GHz, 55.4–59.8 °C, and firmware throttle state
`0x0`.

## Clean builds

Each build used `/usr/bin/time -v`, `--release`, `--locked`, the same
environment/features, and its own empty `CARGO_TARGET_DIR`.

| Metric | Rust 1.85.0 | Rust 1.97.1 | 1.97.1 change |
| --- | ---: | ---: | ---: |
| Wall time | 254.71 s | 151.03 s | −40.7% |
| User time | 297.03 s | 182.50 s | −38.6% |
| System time | 13.02 s | 4.74 s | −63.6% |
| Peak RSS | 1,445,264 KiB | 1,351,152 KiB | −6.5% |
| Major faults | 42,063 | 1,957 | descriptive only |
| Minor faults | 516,151 | 566,774 | +9.8% |
| Filesystem input blocks | 1,974,696 | 458,872 | descriptive only |
| Filesystem output blocks | 251,456 | 327,232 | +30.1% |
| Reported process swaps | 0 | 0 | unchanged |
| Stripped executable | 4,524,496 bytes | 4,199,264 bytes | −7.2% |

The old executable SHA-256 is
`82b5ee0b6dd8a74f1f5e256fe7d662eda321f0faf3886ff4e774c1b42d526191`;
the new executable SHA-256 is
`909c0e8ab5fe8c0486c3d53220b61b6bfece1c7e5d5c0256b2ac6e5b73024ef6`.

The 1.97.1 build ran second. Its much lower major-fault/input count shows that
filesystem cache order helped it, so the clean-build difference is useful
turnaround evidence but is not isolated enough to call the entire 40.7% a
compiler speedup.

## Controlled callback comparison

Values below are the median across five runs of each run's median and p99.
Change is Rust 1.97.1 relative to 1.85.0; positive timing is slower. `Worst`
keeps the largest observed maximum across the five runs.

| Workload | Frames | Median old → new | Change | p99 old → new | Change | Worst max old / new | Result |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Dry four-source graph + final strip | 64 | 51.018 → 94.999 µs | +86.2% | 81.407 → 126.203 µs | +55.0% | 132.962 / 176.481 µs | regression |
| Dry four-source graph + final strip | 128 | 101.870 → 203.239 µs | +99.5% | 134.351 → 248.887 µs | +85.3% | 185.258 / 307.238 µs | regression |
| `phase4-full` graph + final strip | 64 | 139.073 → 195.535 µs | +40.6% | 196.166 → 242.369 µs | +23.6% | 250.887 / 274.368 µs | regression |
| `phase4-full` graph + final strip | 128 | 302.108 → 392.516 µs | +29.9% | 342.794 → 436.497 µs | +27.3% | 872.808 / 510.793 µs | regression |
| SHR Drums dry | 64 | 367.275 → 359.552 µs | −2.1% | 424.108 → 419.182 µs | −1.2% | 1,320.805 / 989.882 µs | unchanged |
| SHR Drums dry | 128 | 728.643 → 722.828 µs | −0.8% | 786.790 → 778.383 µs | −1.1% | 861.234 / 842.252 µs | unchanged |
| SHR Drums Reverb | 64 | 197.813 → 197.720 µs | 0.0% | 244.591 → 242.980 µs | −0.7% | 304.071 / 300.146 µs | unchanged |
| SHR Drums Reverb | 128 | 399.071 → 396.960 µs | −0.5% | 443.811 → 440.792 µs | −0.7% | 506.329 / 493.366 µs | unchanged |
| SHR Drums Delay | 64 | 363.664 → 360.052 µs | −1.0% | 414.015 → 411.293 µs | −0.7% | 496.477 / 483.125 µs | unchanged |
| SHR Drums Delay | 128 | 728.791 → 722.328 µs | −0.9% | 784.401 → 774.643 µs | −1.2% | 1,505.803 / 1,061.991 µs | unchanged |
| SHR Drums Reverb + Delay | 64 | 198.702 → 197.813 µs | −0.4% | 243.183 → 243.554 µs | +0.2% | 1,058.288 / 294.294 µs | unchanged |
| SHR Drums Reverb + Delay | 128 | 399.442 → 397.442 µs | −0.5% | 441.682 → 441.367 µs | −0.1% | 6,405.413 / 504.866 µs | unchanged |
| Drums + melody + final bus | 64 | 258.183 → 302.035 µs | +17.0% | 302.775 → 342.979 µs | +13.3% | 365.238 / 393.793 µs | regression |
| Drums + melody + final bus | 128 | 511.700 → 610.958 µs | +19.4% | 553.552 → 640.680 µs | +15.7% | 629.440 / 737.143 µs | regression |

All 140 rows (14 boundaries × five runs × two compilers) were finite. There
were no 1.97.1 deadline misses. Rust 1.85.0 had one 6.405 ms maximum in the
128-frame combined drum-effects row while its run median and p99 remained
399.442 and 441.682 µs; this is retained as a scheduler/descheduling outlier,
not a steady DSP cost or a new-compiler win.

Whole-command median RSS was 5,792 KiB for 1.85.0 and 6,160 KiB for 1.97.1.
Median involuntary context switches were 1,382 and 1,458 respectively.

### Isolated MASTER STRIP

| Frames/profile | Mean old → new | Change | p99 old → new | Change | Worst max old / new |
| --- | ---: | ---: | ---: | ---: | ---: |
| 64 neutral | 53.784 → 100.586 µs | +87.0% | 93.203 → 125.388 µs | +34.5% | 133.055 / 207.369 µs |
| 64 active | 53.912 → 100.685 µs | +86.8% | 93.055 → 137.351 µs | +47.6% | 128.110 / 193.147 µs |
| 128 neutral | 107.599 → 201.257 µs | +87.0% | 144.351 → 245.368 µs | +70.0% | 193.683 / 299.498 µs |
| 128 active | 107.801 → 201.411 µs | +86.8% | 130.462 → 244.387 µs | +87.3% | 193.721 / 311.887 µs |
| 4× interpolation, 128 | 20.781 → 38.905 µs | +87.2% | 47.962 → 66.463 µs | +38.6% | 81.351 / 96.925 µs |
| 8× interpolation, 128 | 31.749 → 77.881 µs | +145.3% | 58.315 → 116.295 µs | +99.4% | 86.443 / 170.480 µs |

The agreement between the isolated strip and graph/final-bus rows localizes
the important regression to work containing the fixed final processor. This
pass does not attempt a compiler-backend diagnosis.

## Post-A/B Rust 1.97.1 optimization

On 2026-07-30 a short follow-up tested whether ordinary Rust 1.97.1 codegen
options could recover the final-strip regression before changing DSP source.
It used baseline commit
`6b352b5dad0d875d4cc00588872e76d9af74d793`, the same `Cargo.lock` hash and
SHR Drums commit recorded above, the pinned Rust 1.97.1 compiler, the existing
release profile, CPU 0 affinity, and the same deterministic commands. JACK
remained untouched on CPU 3. The Pi stayed at 2.4 GHz, 53.8–59.3 °C, and
firmware throttle state `0x0`.

### Compiler-option candidates

Three artifacts compared the existing build, `-C target-cpu=native`, and
`target-cpu=native` with `-C opt-level=2`. The default AArch64 target already
enables NEON. Native Cortex-A76 selection additionally exposed AES, CRC,
dot-product, FP16, LSE, and other Pi 5 features, but those features did not
accelerate this floating-point interpolator. Each strip value below is the
median of three interleaved 5,000-callback run means.

| Boundary | Existing Rust 1.97.1 | Cortex-A76 native | Native plus opt-level 2 |
| --- | ---: | ---: | ---: |
| 64 neutral | 100.620 µs | 101.464 µs (+0.84%) | 102.408 µs (+1.78%) |
| 64 active | 100.674 µs | 101.555 µs (+0.88%) | 102.253 µs (+1.57%) |
| 128 neutral | 201.483 µs | 202.425 µs (+0.47%) | 204.119 µs (+1.31%) |
| 128 active | 201.246 µs | 202.936 µs (+0.84%) | 204.603 µs (+1.67%) |
| 4× interpolator | 38.895 µs | 38.998 µs (+0.26%) | 39.933 µs (+2.67%) |
| 8× interpolator | 77.895 µs | 78.020 µs (+0.16%) | 79.056 µs (+1.49%) |

Four short complete-graph runs per artifact were mixed but effectively
unchanged. Most medians and p99 values stayed within about ±1%; isolated
exceptions did not repeat across buffer sizes or median and p99. All 14
workload/buffer output hashes matched exactly. The tested artifacts were:

| Build | SHA-256 | Bytes |
| --- | --- | ---: |
| Existing release | `16e1c1369caa4e5d5b5900f1e0a5e0430b83ddd8ea6c3f1fc21b8b668a0d35f1` | 4,199,264 |
| Cortex-A76 native | `6326da0ad33385d3101489c02f4024a4048a59c489c13ec19f9dda2dbd90780e` | 4,330,336 |
| Native plus opt-level 2 | `08ed1b271530b233ebec3737c22f074289117f190fbc236280ae55f6962b00e5` | 4,133,728 |

Only the existing artifact used a fresh target and took 149.35 seconds. The
two option candidates reused dependencies and took 128.10 and 121.29 seconds,
so those wall times are not clean-build comparisons.

### Focused source candidates

Forcing `TruePeakInterpolator::process` from `#[inline]` to
`#[inline(always)]` changed complete-strip means by only −0.4% to +0.4%. That
annotation was restored; inlining was not the cause.

The successful candidate kept the original tap and floating-point accumulation
order but replaced this inner-loop index:

```text
(write + tap) % INTERPOLATOR_TAPS
```

with two bounded ranges, one before and one after the ring-buffer wrap. This
removes a modulo operation for every tap and phase without changing
coefficients, state, latency, or DSP output. The source records why the ranges
must not be casually collapsed back into a modulo-indexed loop, and a focused
test compares the optimized loop bit-for-bit with the retained test-only
modulo reference.

| Boundary | Existing Rust 1.97.1 | Modulo-free scan | Change |
| --- | ---: | ---: | ---: |
| 64 neutral | 100.599 µs | 57.488 µs | −42.85% |
| 64 active | 100.842 µs | 57.382 µs | −43.10% |
| 128 neutral | 200.942 µs | 114.701 µs | −42.92% |
| 128 active | 201.262 µs | 114.794 µs | −42.96% |
| 4× interpolator | 38.919 µs | 18.619 µs | −52.16% |
| 8× interpolator | 77.874 µs | 33.630 µs | −56.81% |

Three interleaved complete-graph runs per artifact confirmed that the gain
appears only where final-strip work is present:

| Workload | Frames | Median before → after | Change | p99 before → after | Change |
| --- | ---: | ---: | ---: | ---: | ---: |
| Dry graph + final strip | 64 | 95.018 → 54.277 µs | −42.88% | 137.739 → 86.259 µs | −37.38% |
| Dry graph + final strip | 128 | 203.221 → 108.444 µs | −46.64% | 248.165 → 149.295 µs | −39.84% |
| `phase4-full` + final strip | 64 | 195.961 → 143.276 µs | −26.89% | 236.220 → 201.850 µs | −14.55% |
| `phase4-full` + final strip | 128 | 391.219 → 310.034 µs | −20.75% | 436.089 → 350.274 µs | −19.68% |
| Drums + melody + final bus | 64 | 302.183 → 261.516 µs | −13.46% | 340.849 → 305.424 µs | −10.39% |
| Drums + melody + final bus | 128 | 611.217 → 517.385 µs | −15.35% | 639.569 → 551.588 µs | −13.76% |

Standalone drum boundaries remained within observed noise. Every before/after
workload repeated the same output hash across all runs, and all 14 paired
hashes matched. The output difference is therefore exactly zero.

Comparing the optimized Rust 1.97.1 source with the original controlled Rust
1.85 artifact is **source plus compiler**, not a compiler-only result. It is
still useful for deciding whether a split production toolchain is warranted:
the optimized 1.97.1 results are within about 1–7% of the original 1.85
medians for the strip-bearing graph rows, while the 4× interpolator is about
10% faster. That small remaining mixed difference does not justify maintaining
Rust 1.85 for release builds.

The failed inline artifact SHA-256 was
`f1717d3845e344ddc584d8069369fff92f3cf5dd5b824e8d46db2a72b668a1ef`;
the successful candidate artifact SHA-256 was
`3b6e1638f38f8104017c2995d7c5fd1cb32e992fac2b1c450672d36d49f695be`.
Both builds reused dependencies and took about 128 seconds, so their build
times are descriptive only.

Ignored option-build and runtime evidence is below
`$SHSYNTH_USER_DIR/compiler-options-20260730/`. Ignored source-candidate
artifacts, interleaved runs, hashes, `/usr/bin/time -v` records, and thermal
snapshots are below
`$SHSYNTH_USER_DIR/compiler-source-options-20260730/`.

The durable lesson is to keep one pinned development and production toolchain.
When a future compiler regression localizes to one DSP boundary, first inspect
that hot loop and compare bit-exact source shapes with the existing short
harness. CPU-specific flags, weaker whole-program optimization, a second
release compiler, and arbitrary inlining annotations did not solve this case.

### Adoption validation

The requested full adoption pass used exact Rust 1.97.1 and Cargo 1.97.1:

```sh
cargo fmt --all
make docs-site
make check-docs-site
git diff --check
cargo test --locked --all-targets
cargo build --release --locked --all-targets
```

The Cargo test and build commands ran in both SHR-DAW and the live SHR Drums
path dependency. The first full SHR-DAW suite exposed four deterministic stale
test expectations left by earlier product changes: the managed graph expected
only three source nodes, the MIDI fixture and sequencer test expected
manufactured percussion note-offs after percussion became intentional
one-shots, and the final-bus list-wrap test hard-coded four rows before SHR
Drums became the fourth source. The repairs changed test expectations only:
they now derive the production source/row count, verify percussion attacks
separately from duration-owning melodic notes, and preserve the documented
one-shot contract. All four passed alone before the complete suite was rerun.

Final validation results:

- SHR-DAW: 856 passed, 0 failed, and 6 explicitly ignored private render tests;
- SHR Drums: 12 passed, 0 failed, and 0 ignored;
- SHR-DAW release all-target build: 264.20 seconds, 1,515,280 KiB peak RSS;
- SHR Drums release all-target build: 22.23 seconds, 301,120 KiB peak RSS;
- generated documentation check, formatting, and diff whitespace check passed;
  and
- the normal stripped `target/release/shr` is 4,264,800 bytes with SHA-256
  `4dced24d5f9cb5612b48825a4e4ea3fb213cfae6fedbbb1140b4cf52b8df409d`.

Ignored validation transcripts and `/usr/bin/time -v` records are below
`$SHSYNTH_USER_DIR/adopt-modulo-free-20260730/validation/`.

## Output equivalence

Every compiler/workload/buffer combination repeated one stable 64-bit hash
across all five runs, and the old/new hashes matched. Peaks and RMS values also
matched to all printed digits. That is stronger than a tolerance comparison:
the f32 output samples were bit-identical, so maximum absolute difference and
RMS difference are both `0.0`.

The existing final-mix stress command initially panicked before processing
because its synthetic fixture still allocated three source buffers after the
production bus gained SHR Drums as a fourth source. The post-measurement repair
uses the production `SOURCE_COUNT`, adds a distinguishable fourth source, and
sums every source. Its focused Rust 1.97.1 test passed with full PCM equality,
zero drops, and zero overflows. This was a stale fixture boundary, not an A/B
output difference.

## Separate historical comparison

This table is **source-plus-toolchain evolution**, not a controlled compiler
effect. The dated rows came from earlier repository states and, for the graph,
a Raspberry Pi 4 and connected JACK workload.

| Workload | Historical Rust 1.85 result | Current Rust 1.97.1 result | Interpretation |
| --- | ---: | ---: | --- |
| Pi 5 MASTER STRIP 64 neutral mean | 50.552 µs median on 2026-07-28 | 100.586 µs | Current result is about 99% slower; source and measurement revision also advanced |
| Pi 5 MASTER STRIP 128 active mean | 99.687 µs median on 2026-07-28 | 201.411 µs | Current result is about 102% slower; not compiler-only |
| Pi 4 connected `phase4-full`, 64 mean | 158.527 µs on 2026-07-19 | 197.554 µs offline median mean | Current workload is about 25% slower despite newer Pi; it now includes four sources and the fixed final strip |
| Pi 4 connected `phase4-full`, 128 mean | 313.572 µs on 2026-07-19 | 396.077 µs offline median mean | Current workload is about 26% slower; source, machine, and harness differ |

The controlled tables above, not this historical table, establish the compiler
effect.

## Raw evidence and limits

Ignored raw evidence is below
`$SHSYNTH_USER_DIR/compiler-ab-20260729/`:

- `build/`: both Cargo transcripts and `/usr/bin/time -v` records;
- `runtime/`: every raw compiler/strip run, time record, and TSV summary;
- `system/`: per-run frequency, temperature, throttle, memory, and swap
  snapshots;
- `target-rust-1.85.0/` and `target-rust-1.97.1/`: both isolated artifacts; and
- `validation-final-mix-test.log`: the focused post-repair PCM-equality test.

These are ordinary pinned-CPU, non-real-time processes. They establish
production DSP cost and deterministic output, not JACK xrun behavior,
real-time scheduling, connected 64-frame stability, physical playback,
listening quality, or interface behavior. The configured JACK period was 128;
64 frames was measured offline only. JACK was never started, stopped,
reconfigured, or connected by this pass.
