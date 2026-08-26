# Priority 7 external transport sync acceptance

Status: implemented first-version software contract; pure injected-byte and
timestamp evidence only on the development PC

## User motion and scope

Routing now exposes three machine-owned fields: `SYNC` selects `INTERNAL` or
`EXTERNAL`, `SYNC IN` selects one exact USB MIDI input, and `SYNC POS` selects
`ARRANGEMENT` or `PATTERN`. Every keyboard and four-, five-, or eight-pad
controller layout reaches those rows through the existing Routing list, edit,
Apply, and Cancel actions. These settings live in `shsynth.conf`; browsing,
refusal, waiting, loss, Stop, Cancel, and failed Apply do not touch the Project,
Pattern History, Arrangement, Pattern data, structural state, or dirty state.

External sync deliberately implements only MIDI Timing Clock, Start, and Stop.
It does not implement Continue, Song Position Pointer, clock thru/forwarding,
or another clock route.

## Exact first-version contract

### One clock owner and one source

- `INTERNAL` is the migration/default. The existing scheduler tempo, external
  MIDI clock/Start/Stop output, and optional controller-clock output behave as
  before.
- `EXTERNAL` makes the configured input the only steady-transport clock owner.
  Pattern tempo and Tempo commands are ignored only in a private playback
  clone. Swing, groove, cell microtiming, REC FEEL results, lane rate/cycle/
  direction, probability, conditions, PRE/FILL, retrigger, automation, and
  Loop placement retain their existing event-level owners.
- The configured stable `client-name:port-name` identity must resolve to
  exactly one live input. Volatile ALSA `client:port` numbers are stripped on
  save. Missing, partial, or ambiguous matches are refused; no other input is
  substituted. An address-only replacement of the same unique stable identity
  is treated as a source replacement, stops/cleans transport, reconnects, and
  requires reacquisition plus a fresh Start.
- While external owns time, SHR schedules no outgoing `F8`, `FA`, or `FC` for
  tracker MIDI destinations and suspends all optional controller-clock pulses.
  Returning to internal play resumes the configured output behavior. There is
  no input-to-output clock forwarding path.

### Byte parsing

The input callback timestamps each delivery and feeds a stateful MIDI 1.0 byte
stream parser. Each System Real-Time byte is emitted immediately without
changing channel running status, an incomplete ordinary message, or an open
SysEx collector. Note, CC, Program Change, pressure, command-pad, System Common,
and SysEx messages continue through their existing owners. Malformed ordinary
stream fragments are bounded faults and never become clock commands.

This follows the MIDI Association definition that Timing Clock is sent 24
times per quarter note and that System Real-Time messages may appear anywhere
in a stream, including inside SysEx. The relevant public protocol summaries
are [MIDI Messages](https://midi.org/about-midi-part-3midi-messages) and
[Summary of MIDI 1.0 Messages](https://midi.org/summary-of-midi-1-0-messages).
ALSA's sequencer documentation distinguishes event timestamps and queue time;
SHR retains the callback `Instant` instead of treating UI/thread delivery time
as musical time: [ALSA sequencer events](https://www.alsa-project.org/alsa-doc/alsa-lib/group___seq_events.html)
and [ALSA sequencer interface](https://www.alsa-project.org/alsa-doc/alsa-lib/seq.html).

### Acquisition, tempo, phase, and loss

- One pulse opens acquisition. Six valid pulse intervals—seven received
  clocks—establish usable tempo and phase. Start before that is refused and
  does not move or start transport.
- Supported tempo is exactly 20.00–300.00 BPM. Individual intervals outside a
  bounded USB-jitter envelope are malformed. Six consecutive malformed
  fragments or intervals create a visible fault.
- Tempo uses a rolling median of at most 24 valid intervals followed by a 7/8
  previous + 1/8 measured filter. One update may change the filtered interval
  by at most two percent. Delivery bursts at or below 2 ms do not rewrite the
  tempo estimate.
- Each running pulse corrects the scheduler origin toward the predicted phase
  by at most one eighth of one filtered pulse interval. Remaining messages and
  the next repeat are rescaled around elapsed transport time when filtered
  tempo changes. The phase correction is bounded per pulse, so jitter cannot
  accumulate as unbounded drift or trigger catch-up note bursts.
- More than 500 ms without a pulse is clock loss. The transport stops through
  the existing note owners, sends exact Note Off/cleanup and All Notes Off as
  already applicable, stops Loop/metronome owners, leaves the single managed
  engine under its existing lifecycle owner, and remains visibly lost. It
  never falls back to internal time.
- After Stop, loss, source replacement, refusal, or fault, clocks must first
  establish a usable estimate and a fresh `FA` Start is mandatory. Clocks alone
  never resume. Repeated Start while running performs one existing cleanup and
  one restart; it cannot layer or start twice.

### Start, Stop, and positioning

- `ARRANGEMENT` Start always selects Arrangement step 1, row 1 and plays the
  complete Arrangement with its existing repeat and Pattern-boundary behavior.
- `PATTERN` Start always selects row 1 of the currently selected Pattern and
  repeats that Pattern. In Live Patterns it starts the selected shaped Pattern,
  after which immediate/quantized launches and retrigger use the same external
  owner.
- Start never preserves a paused/sub-row location. Stop retains the visible
  editor selection but ends sounding transport and demands a fresh Start.
  Continue (`FB`) is visibly refused; SPP (`F2`) remains an ordinary unsupported
  System Common message. Pattern and Arrangement duration/data are unchanged.
- Internal cursor-start partial playback remains unchanged when sync is
  internal. External Start intentionally has no partial-position mode because
  truthful relocation is outside this version.

### Existing subsystems

- Pattern repeats, Arrangement boundaries, Live Pattern launches, independent
  lanes, conditions/probability, automation, retrigger, and Loop Mix all reuse
  the single existing scheduler and boundary owners.
- Metronome and managed drum tempo follow the filtered external tempo while
  running. A stopped external transport cannot start count-in recording; REC
  is refused until clock acquisition plus Start. Punch-in on an externally
  running transport retains the existing recording owner and REC FEEL capture.
- Preflight remains mandatory before Start can sound. A failed preflight stops
  the follower and requires a fresh Start. MIDI export remains Project-derived
  and therefore does not export a transient external tempo. No Project format
  migration was added.
- Stop, Panic, route loss, Project replacement, and shutdown retain the
  existing scheduler, held-note, engine, Loop, and All Notes Off owners.

## Native state and failure language

The shared status row always shows `SYNC INT` in internal mode when no more
important message is active. External mode has persistent states for source
unavailable, source ambiguous, waiting, acquiring `n/6`, ready, running with
tempo, stopped, clock lost, and clock fault. The one transport cell remains the
canonical play/stop/record indicator. External waiting/stopped/lost is Stop,
never a false Pause. Routing uses its existing scrolling body and never draws
over row 13 at 40×13.

## Focused acceptance matrix

| Area | Deterministic software evidence | Required human/hardware evidence |
| --- | --- | --- |
| Parser | 24 PPQN; real-time interleaving with note, CC, Program, pressure, command-pad, running status, and SysEx; malformed bounds | Real USB sender with interleaved traffic |
| Source | exact/stable match, missing, ambiguity, address replacement, shared performance role, unrelated source exclusion | ALSA names and hot-unplug/replug on Raspberry Pi |
| Owner/output | internal/external mutual exclusion; external schedule has no clock; controller clock fully suspended | Loop-safe cabling with the intended controller and destination |
| Acquisition | seven clocks, 20/60/120/300 BPM, median/filter bounds, jitter, delivery burst, long-run phase, missing pulse, fault | USB scheduling jitter and sustained Pi timing |
| Transport | Start/Stop/repeated Start; fresh-Start reacquisition; Arrangement/Pattern row-one material; single transport thread | Musical feel and physical Start/Stop controls |
| Scheduler | unchanged Pattern structure and feel fields; repeats, Live runtime, lanes, conditions, retrigger, automation, Loop boundaries reuse existing code | Audible note/Loop alignment and live-launch feel |
| Recording/output | stopped external REC refusal; running owner retained; preflight/export remain Project-owned; internal output preserved | Count-in/metronome/recording audition and exported-file listening |
| UI/transaction | exact Routing fields, 40×13 status row, four/five/eight layouts, Cancel/refusal/no-op leave Project and dirty state unchanged | Every physical controller layout and native display |
| Cleanup | Stop, loss, fault, source replacement, restart, and shutdown use existing note/Loop/engine cleanup | Stuck-note, unplug, and shutdown rehearsal with instruments |

Automated tests do not prove the Raspberry Pi's ALSA scheduling latency, a real
USB device's clock quality, controller mappings on physical hardware, Loop or
metronome phase by listening, recording feel, or absence of stuck notes in a
real synth chain. Those remain explicit acceptance work; no JACK, ALSA port,
synth, playback, recording, screenshot, or MIDI transmission was started for
this development-PC pass.
