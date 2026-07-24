# Raspberry Pi audio-system policy

This document owns SHR-DAW's installation and host-optimization decisions.
It separates what is configured from what the running kernel, services, and
processes actually do. It is not a generic Linux-audio tuning checklist.

Evidence labels used below are:

- **observed**: current repository source, isolated fixtures, or direct
  read-only inspection of the development Pi;
- **authoritative**: the primary sources listed under Provenance;
- **project judgment**: a conservative SHR-DAW choice where no source proves a
  universal performance win;
- **physical gate**: requires a real Pi/interface workload and is not proved by
  fixtures.

## Supported motion

On both platforms, `install.sh` shows consequences before changes. Package and
service changes require one explicit consent; `--yes` is the non-interactive
equivalent. Real-time permission changes are a separate consent. `shr-setup`
then detects an existing JACK owner, live RT limits, CPU topology, kernel
features, and existing tuning before it offers changes. Every system-changing
setup choice defaults to no.

Patchbox OS already supplies a shared system `jack.service`, `/etc/jackdrc`,
the `jack` service account, RT service limits, and usually the Debian
`@audio` PAM limits. SHR retains that ownership and lifecycle. It may add only
its separately owned CPU-affinity drop-in and performance/isolation profile
after consent. Patchbox documentation still advertises an old RT-kernel image,
so the name “Patchbox” is never accepted as proof that the installed kernel is
RT, full-tickless, or otherwise tuned.

Stock Raspberry Pi OS Lite 64-bit receives the required packages, Debian/JACK
RT policy and audio-group membership when missing and accepted, and a backed-up
`~/.jackdrc`. SHR does not invent a competing system JACK service: JACK remains
an explicit musician/admin lifecycle on stock. The optional dedicated-CPU
profile is the same managed profile on both systems.

`shr-audio-tune plan` is read-only. `install` never starts or restarts JACK and
now enables the governor service without starting it live. A reboot activates
boot isolation and starts that governor service. `doctor` reports persistent
intent and live state separately. `recover` rolls an interrupted transaction
back. `remove` deletes only exact owned boot tokens and unchanged owned files;
later administrator edits are retained.

## Optimization matrix: baseline and decision

| ID | Candidate and change | Current SHR implementation | Patchbox baseline | Stock Lite 64 baseline | Detection | Decision, measurable benefit, and overlap |
|---|---|---|---|---|---|---|
| A1 | RT scheduling: `rtprio=95`, unlimited `memlock` | Installer/setup can call owned `permissions-install`; doctor checks files and the live login separately | Debian `audio.conf` and JACK unit limits were **observed** correct | `jackd2` can install `audio.conf`; current login may still lack policy until logout | Parse every limits file; inspect user group and live limits | **Necessary.** Enables JACK/client FIFO scheduling and memory locking. It does not require an RT kernel. Unit limits do not replace client-user PAM limits. |
| A2 | Audio group and file capabilities | Owned membership is added only after consent; no executable capabilities are added | Development user is already in `audio` and `jack` | Image/user groups vary; never assume | `/etc/group`, `id`, `/proc/self/limits`; inspect unexpected file capabilities if diagnosing | **Audio group necessary when PAM policy uses it. Capabilities rejected.** `CAP_SYS_NICE`/`CAP_IPC_LOCK` on broad binaries expands privilege and duplicates scoped limits. |
| A3 | PREEMPT versus PREEMPT_RT kernel | Doctor reads the installed kernel config, not `uname` text alone | Current Pi is `CONFIG_PREEMPT=y`, not RT; old Patchbox docs are stale | Current Raspberry Pi OS kernels vary by release/model | `/boot/config-$(uname -r)`, `/proc/config*`, `/sys/kernel/realtime` when present | **PREEMPT is supported; PREEMPT_RT is optional, not required.** Test RT only against a measured workload; Raspberry Pi maintainers describe RT builds as experimental. |
| A4 | CPU frequency governor | Owned oneshot service records/restores governors and selects `performance` at boot | Not a guaranteed Patchbox property; current Pi is active through SHR's service | Usually dynamic governor | Service configured/active state plus every policy's live `scaling_governor`; frequency/throttle telemetry is separate | **Optional with dedicated-core profile.** Removes frequency-selection delay/jitter; costs power and heat. `schedutil` already raises frequency for RT tasks, so benefit must be measured. |
| A5 | `isolcpus=domain,managed_irq`, `irqaffinity`, housekeeping set | Managed boot tokens; JACK drop-in; managed synth inherits `audio.engine_cpu`; ordinary SHR/writers/builds stay elsewhere | Not assumed; current Pi has SHR-owned CPU 3 isolation | Absent by default | Selected boot file, `/proc/cmdline`, `/sys/devices/system/cpu/isolated`, IRQ masks, JACK and synth process affinity | **Optional, retained design for demanding 18×18 work.** Stronger than affinity alone because unrelated scheduler work is excluded. Costs one general/build CPU until removal and reboot. |
| A6 | `nohz_full` and `rcu_nocbs` | Now conditional on `CONFIG_NO_HZ_FULL` and `CONFIG_RCU_NOCB_CPU`; owned stale tokens are removed on repeated install | Current Pi's 6.12 kernel rejects both despite their command-line text | Often not built in | Kernel config, boot log, live nohz sysfs, and configured/live command lines | **Conditional only.** Can reduce tick/RCU jitter, but adds no benefit when unsupported. `nohz_full` already offloads RCU callbacks, making a separate `rcu_nocbs` token partly redundant on supporting kernels. |
| A7 | systemd `CPUAffinity`, `AllowedCPUs`, RT/memlock/scheduling directives | JACK drop-in uses `CPUAffinity`, `LimitRTPRIO`, `LimitMEMLOCK`; JACK itself requests FIFO priority | Patchbox JACK unit already provides RT/memlock; SHR adds affinity only | No SHR-owned system JACK service | Unit fragment/drop-ins, `systemctl show`, `MainPID`, `/proc/PID/status` and limits | **Keep CPUAffinity and limits. Reject duplicate AllowedCPUs and forced unit FIFO.** `AllowedCPUs` is a cgroup cpuset control with parent constraints; affinity already supplies the required process placement. JACK owns its RT-thread policy. |
| A8 | JACK owner and boot lifecycle | Detect/retain external owner; stock uses `.jackdrc`; no setup start/restart | Shared system service enabled at boot is intended and retained | No service is assumed or created | Unit fragment, enabled/active state, process count, shared-server environment, `.jackdrc` | **One owner only.** Retain healthy Patchbox boot lifecycle; keep stock lifecycle explicit. Duplicate servers can compete for ALSA and shared resources. |
| A9 | JACK device, rate, period, periods, priority, timeout/watchdog | Setup validates and backs up `.jackdrc` only without an external owner; doctor compares the persistent launch command with the live process command | `/etc/jackdrc` is Patchbox-owned; current measured setting is 48 kHz, 128, 3, priority 95 | Must be selected for the interface/workload | Exact command, unit process arguments, `jack_lsp`, logs, xruns, callback deadlines | **48 kHz and three periods are safe USB defaults; period size is workload-specific.** Larger periods lower xrun risk and raise latency. Keep JACK's watchdog/timeout defaults unless measured disconnects justify a change. |
| A10 | IRQ placement, threaded IRQs, USB topology | Default IRQ affinity excludes audio CPU; no per-device IRQ rewriting | Current USB host IRQ is effectively on housekeeping CPU; Pi kernel exposes forced-threading support | Hardware/kernel dependent | `/proc/interrupts`, default/effective affinity, kernel config, `lsusb -t` | **Global housekeeping mask retained. Per-IRQ/rtirq recipes rejected by default.** Moving the wrong storage/network/timer IRQ can reduce reliability; managed IRQ affinity is best-effort and topology-specific. |
| A11 | USB audio/MIDI autosuspend | Detect only; no global udev or `usbcore.autosuspend=-1` rule | Current leaf audio/MIDI devices are **observed** `power/control=on` | Device/driver dependent | USB topology, per-device `power/control`, disconnect/reset logs | **No blanket change.** Disable autosuspend only for an identified device with measured suspend/reset failures. Global disable costs power and can mask another fault. |
| A12 | PipeWire, PulseAudio, FluidSynth, MIDI auto-patchers | Exact FluidSynth/amidiminder units are offered for masking; unrelated services remain | Shared JACK can coexist with PipeWire/Pulse when they do not own the same ALSA device; current standalone synth/router units are masked | Desktop services may be absent on Lite | Unit/process owner, ALSA device use, JACK-provider/process count, routes | **Prevent duplicate owner, not package presence.** Mask exact known synth/router conflicts only after consent. Do not call active PipeWire alone a duplicate JACK server. |
| A13 | Swap, zram, swappiness | No changes | Current Pi has a small swap file and default swappiness | Image/release dependent | `swapon`, `zramctl`, memory/swap pressure, OOM and callback measurements | **Retain OS policy.** Zram may help memory pressure but consumes CPU; disabling swap can turn pressure into OOM. No measured audio win supports a universal value. |
| A14 | Dirty writeback sysctls | No changes | Current defaults retained | Kernel defaults | Configured/live sysctls, writer high-water, fsync time, drops, storage latency | **Reject generic ratios/intervals.** Lower thresholds may smooth bursts but increase ongoing I/O; higher thresholds enlarge durability loss and later stalls. SHR's bounded writer and fsync/publication contract is the owner. |
| A15 | Filesystem, microSD/NVMe, mount advice | Recording uses bounded writers, fsync, manifests, and atomic no-replace publication; no mount changes | Current Pi 4 evidence is microSD; Pi 5 NVMe remains unmeasured | Storage chosen by user | Mount source/options, free space, SMART/NVMe data where available, recorder stress and real-device tests | **No universal mount tweak.** NVMe should improve throughput/endurance but requires physical measurement. Never trade fsync/durability for benchmark latency. |
| A16 | journald/log suppression | No changes | Retained | Retained | Persistent/volatile mode, disk use, rate-limit drops, service logs | **Retain logs.** Logs are required for JACK, power, USB and recovery diagnosis. Rate/size policy may be adjusted only for demonstrated storage pressure. |
| A17 | cooling, throttling, power supply | Doctor/state inspection can report evidence; tuning does not overclock | Current Pi showed no throttle flags during this audit; temperature is only a momentary observation | Hardware/supply/cooling dependent | `vcgencmd get_throttled`, thermal sysfs, kernel undervoltage messages, sustained workload | **Necessary operational prerequisite, not a software “optimization.”** Performance governor raises power/heat; active cooling and a suitable supply prevent clocks being capped. |
| A18 | clock stability, high-resolution timers, tick rate | Detect kernel capability; do not set clocks/timers | Current kernel has high-resolution timers and 250 Hz base tick | Kernel dependent | Kernel config, clocksource, latency/xrun measurements | **Retain kernel defaults.** Audio clocks come from the interface/JACK rate. Overclocking, arbitrary timer-source switches, and `HZ=1000` rebuilds lack evidence here. |
| A19 | Disable networking/Bluetooth/SSH/services | No blanket disabling | Patchbox uses networking for remote control/modules; SHR Help can use LAN | Lite may need SSH, updates and remote setup | Actual IRQ/CPU/network load during reference workload | **Rejected by default.** Product and recovery cost is immediate; benefit is unproved. Isolate a demonstrated offender only for a named performance session. |
| A20 | MIDI/controller background services | SHR owns selected inputs/routes and consumes command-pad on/off; setup masks only exact conflicting auto-services | Patchbox may supply amidiauto/amidiminder-style routing | Usually absent unless installed | ALSA clients/routes, exact units, selected controller/performance roles | **Exact ownership required.** Prevent doubled notes/unintended routes without disabling unrelated MIDI hardware or transmit paths. |
| A21 | Shutdown, All Notes Off, interruption and restoration | Existing bounded shutdown sends all-channel panic, stops only owned engines, restores owned routes; tuner has transaction recovery | Retained | Same application contract | Process/route snapshots, recovery fixtures, physical tests only with approval | **Necessary safety invariant.** Optimization may not bypass cleanup, kill unowned engines, or replace durable recording publication. |
| A22 | Pi 4/Pi 5, 32/64-bit, boot layout | Contiguous topology required; both command-line locations supported; Pi optimization refuses non-aarch64 | Pi image/history varies; current Pi 4 is aarch64 and uses `/boot/firmware` | Current official Lite target is 64-bit; Bookworm/Trixie use `/boot/firmware` while older images may use `/boot` | Architecture, model, online CPU list, actual boot files, config-selected cmdline, kernel release | **64-bit Pi 4/5 supported path.** Refuse unusual/hotplug layouts. Never edit both boot files or infer the live file from version alone. |

## Mechanics, persistence, recovery, and validation

| IDs | Correct mechanism and persistence | Default / consent | Backup, ownership, idempotency, rollback | Doctor and validation |
|---|---|---|---|---|
| A1–A3 | Use distro limits when sufficient; otherwise helper-owned `95-shr-audio.conf` and audio membership. Login boundary activates PAM policy. Kernel replacement is outside normal setup. | RT policy prompt: no. RT kernel: never automatic. | Record user, pre/post group hashes and policy hash. Removal touches only matching owned state; later group/limits edits are retained. | Files versus current group/live limits; fixture missing policy, repeat, removal and admin edit; real JACK RT success remains physical/live validation. |
| A4–A7 | One managed governor service; boot tokens in the one detected cmdline; JACK systemd drop-in. Reboot for isolation/governor; next JACK start for affinity. | Dedicated profile prompt: no, after read-only preview. | Original cmdline retained; manifest records exact tokens and hashes. Same-CPU repeat converges. Transaction rollback/recover handles failures. Removal deletes exact tokens/files only. | Boot file, `/proc/cmdline`, sysfs isolation/nohz, kernel config, service configured/active, process affinity/limits. Fixtures cover both boot paths, conflicts, unsupported features/topology, retry and reboot. |
| A8–A12, A20 | Preserve one JACK owner. Patchbox edits remain Patchbox/admin work; stock `.jackdrc` is user-owned and backed up only when neither a service nor live `jackd` owns lifecycle. Exact service masks only. | Existing owner retained automatically. New `.jackdrc`, masks and routes: separate default-no choices. | Dated config/`.jackdrc` backups. Tuner never owns `/etc/jackdrc`. Setup recovery names only changes completed by that run. | Unit fragment/enabled/active, process count/affinity/limits, routes and ports. Fixtures cover JACK absent/disabled/inactive/active/duplicate/external; automated tests never start services or hardware. |
| A13–A19 | Observe OS/storage/power state; apply no generic persistent change. | No prompt because no change is proposed. Workload-specific experiments require a new explicit request and baseline. | OS/admin ownership remains intact. | Synthetic recorder/final-mix tests are storage/software evidence only; real interface, power, thermals, IRQ and xrun acceptance require the exact Pi workload. |
| A21–A22 | Application owns engine/route/recording recovery; helper owns only its ledger. Boot path and CPU topology are discovered each run. | Safety cleanup is automatic for owned state; hardware tests require explicit permission. | No user data or unrelated routes/processes are touched. | Fixtures plus existing shutdown tests; real Pi 4/Pi 5 and 18×18 acceptance remain physical gates. |

## Workload effects and tradeoffs

| Setting | Audio | Compilation | Network/general use | Storage/durability | Power/thermals | Maintenance |
|---|---|---|---|---|---|---|
| Dedicated CPU and IRQ housekeeping | Expected lower worst-case scheduler/IRQ interference; high-channel benefit must be measured | Three rather than four general CPUs; parallel compile can slow; final Rust link remains mostly serial | One CPU permanently unavailable until removal/reboot | Writer stays on housekeeping CPUs | Performance governor increases draw/heat | Strong boot-time protection, but requires kernel-aware doctor and reboot |
| Full tickless/RCU offload when supported | May further reduce OS jitter | Negligible direct gain; more housekeeping work elsewhere | Can complicate general-purpose tuning | No durability change | May change idle efficiency | Kernel-build dependent; unsupported text is harmful false confidence |
| Performance governor | Avoids frequency ramp delay | Faster sustained compile while active | Less energy-efficient general use | No direct durability change | Higher heat and supply demand | Managed service restores prior values when stopped |
| RT limits/memlock | Required for reliable low-latency JACK/client threads | No material effect | Grants selected users elevated scheduling/locking rights | Prevents paging of locked audio memory | Can let a broken RT task monopolize CPU | Scoped group policy, explicit consent and logout boundary |
| JACK buffer/rate | Smaller period lowers latency but raises deadline risk; three periods is robust for USB | None | None | Higher channel/rate increases recording bandwidth | More callback work at small periods | Must match device and material; validate xruns, not folklore |
| Rejected generic VM/filesystem/service tweaks | No established benefit | May reduce build/cache/network convenience | Can break SSH, updates, Bluetooth, desktop or remote Help | May weaken fsync durability or create write bursts | Often trades power for unproved latency | Avoids a second hidden system-policy owner |

## Provenance

Primary sources supporting policy-affecting claims:

- Linux kernel command-line parameters:
  <https://www.kernel.org/doc/html/latest/admin-guide/kernel-parameters.html>
  (`isolcpus`, `managed_irq`, `nohz_full`, `rcu_nocbs`, `irqaffinity` and their
  required build options).
- Linux CPU isolation guide:
  <https://docs.kernel.org/admin-guide/cpu-isolation.html>.
- Linux CPU frequency policy/governors:
  <https://www.kernel.org/doc/html/latest/admin-guide/pm/cpufreq.html>.
- Linux VM/writeback sysctls:
  <https://www.kernel.org/doc/html/latest/admin-guide/sysctl/vm.html>.
- Linux USB runtime power management:
  <https://www.kernel.org/doc/html/latest/driver-api/usb/power-management.html>.
- systemd execution and cgroup controls:
  <https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html>
  and
  <https://www.freedesktop.org/software/systemd/man/latest/systemd.resource-control.html>.
- JACK real-time setup:
  <https://jackaudio.org/faq/linux_rt_config.html>; JACK/Debian option
  semantics:
  <https://manpages.debian.org/bookworm/jackd2/jackd.1.en.html>.
- Debian JACK policy:
  <https://wiki.debian.org/JACK> and the installed `jackd2` package's
  `/etc/security/limits.d/audio.conf`.
- Raspberry Pi boot layout and live command line:
  <https://www.raspberrypi.com/documentation/computers/configuration.html>;
  kernel/config selection:
  <https://www.raspberrypi.com/documentation/computers/config_txt.html>.
- Raspberry Pi power, thermal and throttle reporting:
  <https://www.raspberrypi.com/documentation/computers/os.html#vcgencmd> and
  <https://www.raspberrypi.com/documentation/computers/raspberry-pi.html#power-supply>.
- Official Raspberry Pi OS image generator:
  <https://github.com/RPi-Distro/pi-gen>.
- Patchbox shared-JACK baseline and current configuration model:
  <https://blokas.io/patchbox-os/docs/> and
  <https://github.com/BlokasLabs/patchbox-os-debs/tree/master/blokas-jack>.

The development Pi audit on 2026-07-24 observed Debian 12/Patchbox packages,
arm64 Raspberry Pi kernel `6.12.93+rpt-rpi-v8`, `CONFIG_PREEMPT=y`,
`CONFIG_NO_HZ_FULL` unset, `CONFIG_RCU_NOCB_CPU` unset, active scheduler-domain
isolation of CPU 3, default/effective IRQ housekeeping on CPUs 0–2, active
performance governor, one Patchbox-owned JACK server confined to CPU 3 with
FIFO priority 95, correct RT/memlock limits, and no current throttle flag. Its
boot log explicitly rejected `nohz_full=3` and `rcu_nocbs=3`; this is machine
evidence for the repaired feature detection, not a universal Raspberry Pi
kernel claim.

## Physical gates

Fixtures prove file ownership, parsing, transaction, cancellation defaults and
configured/live classification. They cannot prove xrun performance, USB
stability, thermal headroom, supply quality, microSD/NVMe durability, Pi 5
behavior, or 18-channel recording/playback. Follow the focused measurement and
MR18 plans after explicit hardware authorization. Do not convert a synthetic
or read-only pass into a physical-support claim.
