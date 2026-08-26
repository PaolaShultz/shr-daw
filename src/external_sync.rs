//! Deterministic MIDI Clock byte parsing and bounded external-clock tracking.
//!
//! This module owns no ALSA port and performs no I/O. Runtime callbacks feed
//! bytes and `Instant` timestamps into it; tests use only injected bytes and
//! timestamps.

use crate::tempo::Bpm;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub const MIDI_TIMING_CLOCK: u8 = 0xf8;
pub const MIDI_START: u8 = 0xfa;
pub const MIDI_CONTINUE: u8 = 0xfb;
pub const MIDI_STOP: u8 = 0xfc;
pub const ACQUISITION_INTERVALS: usize = 6;
pub const CLOCK_LOSS_TIMEOUT: Duration = Duration::from_millis(500);
const FILTER_INTERVALS: usize = 24;
const MAX_CONSECUTIVE_MALFORMED: u8 = 6;
const BURST_WINDOW: Duration = Duration::from_millis(2);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RealtimeMessage {
    TimingClock,
    Start,
    Continue,
    Stop,
    ActiveSensing,
    Reset,
    Undefined(u8),
}

impl RealtimeMessage {
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            MIDI_TIMING_CLOCK => Some(Self::TimingClock),
            0xf9 | 0xfd => Some(Self::Undefined(byte)),
            MIDI_START => Some(Self::Start),
            MIDI_CONTINUE => Some(Self::Continue),
            MIDI_STOP => Some(Self::Stop),
            0xfe => Some(Self::ActiveSensing),
            0xff => Some(Self::Reset),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamItem {
    Message(Vec<u8>),
    Realtime(RealtimeMessage),
    Malformed,
}

/// Stateful MIDI 1.0 byte-stream parser. System Real-Time bytes are emitted
/// immediately without disturbing running status, pending channel messages,
/// or SysEx collection.
#[derive(Clone, Debug, Default)]
pub struct MidiByteStream {
    running_status: Option<u8>,
    pending: Vec<u8>,
    pending_len: usize,
    sysex: Option<Vec<u8>>,
}

impl MidiByteStream {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<StreamItem> {
        let mut items = Vec::new();
        for &byte in bytes {
            if let Some(message) = RealtimeMessage::from_byte(byte) {
                items.push(StreamItem::Realtime(message));
                continue;
            }
            if let Some(sysex) = self.sysex.as_mut() {
                if byte == 0xf7 {
                    sysex.push(byte);
                    items.push(StreamItem::Message(
                        self.sysex.take().expect("SysEx collector exists"),
                    ));
                } else if byte & 0x80 != 0 {
                    self.sysex = None;
                    self.running_status = None;
                    items.push(StreamItem::Malformed);
                    self.begin_status(byte, &mut items);
                } else {
                    sysex.push(byte);
                }
                continue;
            }
            if byte & 0x80 != 0 {
                if !self.pending.is_empty() {
                    self.pending.clear();
                    self.pending_len = 0;
                    items.push(StreamItem::Malformed);
                }
                self.begin_status(byte, &mut items);
                continue;
            }
            if self.pending.is_empty() {
                let Some(status) = self.running_status else {
                    items.push(StreamItem::Malformed);
                    continue;
                };
                self.pending.push(status);
                self.pending_len = message_len(status);
            }
            self.pending.push(byte);
            self.finish_pending(&mut items);
        }
        items
    }

    fn begin_status(&mut self, status: u8, items: &mut Vec<StreamItem>) {
        match status {
            0xf0 => {
                self.running_status = None;
                self.sysex = Some(vec![status]);
            }
            0xf7 => {
                self.running_status = None;
                items.push(StreamItem::Malformed);
            }
            0x80..=0xef => {
                self.running_status = Some(status);
                self.pending.push(status);
                self.pending_len = message_len(status);
                self.finish_pending(items);
            }
            _ => {
                self.running_status = None;
                self.pending.push(status);
                self.pending_len = message_len(status);
                self.finish_pending(items);
            }
        }
    }

    fn finish_pending(&mut self, items: &mut Vec<StreamItem>) {
        if self.pending_len != 0 && self.pending.len() == self.pending_len {
            items.push(StreamItem::Message(std::mem::take(&mut self.pending)));
            self.pending_len = 0;
        }
    }
}

const fn message_len(status: u8) -> usize {
    match status {
        0x80..=0xbf | 0xe0..=0xef | 0xf2 => 3,
        0xc0..=0xdf | 0xf1 | 0xf3 => 2,
        0xf4..=0xf6 => 1,
        _ => 1,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FollowerState {
    Waiting,
    Acquiring { intervals: usize },
    Ready,
    Running,
    Stopped,
    Lost,
    Fault,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PulseUpdate {
    pub tempo: Bpm,
    /// Signed correction to the transport origin, bounded to one eighth of
    /// the filtered pulse interval for each received clock.
    pub phase_correction_nanos: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FollowerAction {
    None,
    Ready(Bpm),
    Start(Bpm),
    Restart(Bpm),
    Stop,
    Pulse(PulseUpdate),
    StartRefused,
    ContinueRefused,
    Lost,
    Fault,
}

#[derive(Clone, Debug)]
pub struct ExternalClockFollower {
    state: FollowerState,
    intervals: VecDeque<Duration>,
    last_received: Option<Instant>,
    filtered_interval: Option<Duration>,
    predicted_phase: Option<Instant>,
    malformed: u8,
}

impl Default for ExternalClockFollower {
    fn default() -> Self {
        Self {
            state: FollowerState::Waiting,
            intervals: VecDeque::with_capacity(FILTER_INTERVALS),
            last_received: None,
            filtered_interval: None,
            predicted_phase: None,
            malformed: 0,
        }
    }
}

impl ExternalClockFollower {
    pub const fn state(&self) -> FollowerState {
        self.state
    }

    pub fn tempo(&self) -> Option<Bpm> {
        self.filtered_interval.and_then(interval_tempo)
    }

    pub fn reset_source(&mut self) {
        *self = Self::default();
    }

    pub fn local_stop(&mut self) {
        self.state = FollowerState::Stopped;
        self.predicted_phase = None;
    }

    pub fn receive(&mut self, message: RealtimeMessage, received: Instant) -> FollowerAction {
        match message {
            RealtimeMessage::TimingClock => self.clock(received),
            RealtimeMessage::Start => self.start(received),
            RealtimeMessage::Stop => {
                let was_running = self.state == FollowerState::Running;
                self.state = FollowerState::Stopped;
                self.predicted_phase = None;
                if was_running {
                    FollowerAction::Stop
                } else {
                    FollowerAction::None
                }
            }
            RealtimeMessage::Continue => FollowerAction::ContinueRefused,
            RealtimeMessage::ActiveSensing
            | RealtimeMessage::Reset
            | RealtimeMessage::Undefined(_) => FollowerAction::None,
        }
    }

    pub fn malformed(&mut self) -> FollowerAction {
        self.malformed = self.malformed.saturating_add(1);
        if self.malformed < MAX_CONSECUTIVE_MALFORMED {
            return FollowerAction::None;
        }
        let was_running = self.state == FollowerState::Running;
        self.intervals.clear();
        self.filtered_interval = None;
        self.predicted_phase = None;
        self.state = FollowerState::Fault;
        if was_running || self.malformed == MAX_CONSECUTIVE_MALFORMED {
            FollowerAction::Fault
        } else {
            FollowerAction::None
        }
    }

    pub fn check_timeout(&mut self, now: Instant) -> FollowerAction {
        let Some(last) = self.last_received else {
            return FollowerAction::None;
        };
        if now.saturating_duration_since(last) <= CLOCK_LOSS_TIMEOUT {
            return FollowerAction::None;
        }
        self.last_received = None;
        self.intervals.clear();
        self.filtered_interval = None;
        self.predicted_phase = None;
        let was_running = self.state == FollowerState::Running;
        let was_stopped = self.state == FollowerState::Stopped;
        self.state = if was_running {
            FollowerState::Lost
        } else if was_stopped {
            FollowerState::Stopped
        } else {
            FollowerState::Waiting
        };
        if was_running {
            FollowerAction::Lost
        } else {
            FollowerAction::None
        }
    }

    fn start(&mut self, received: Instant) -> FollowerAction {
        if self
            .last_received
            .is_none_or(|last| received.saturating_duration_since(last) > CLOCK_LOSS_TIMEOUT)
            || self.filtered_interval.is_none()
        {
            self.state = FollowerState::Waiting;
            self.predicted_phase = None;
            return FollowerAction::StartRefused;
        }
        let restart = self.state == FollowerState::Running;
        self.state = FollowerState::Running;
        self.predicted_phase = Some(received);
        self.malformed = 0;
        let tempo = self.tempo().unwrap_or(Bpm::DEFAULT);
        if restart {
            FollowerAction::Restart(tempo)
        } else {
            FollowerAction::Start(tempo)
        }
    }

    fn clock(&mut self, received: Instant) -> FollowerAction {
        let previous = self.last_received.replace(received);
        let Some(previous) = previous else {
            if self.state != FollowerState::Running {
                self.state = FollowerState::Acquiring { intervals: 0 };
            }
            return FollowerAction::None;
        };
        let interval = received.saturating_duration_since(previous);
        if interval <= BURST_WINDOW {
            self.malformed = 0;
            return self.running_pulse(received);
        }
        let nanos = interval.as_nanos();
        // Include ordinary USB delivery variation around the strict 20..=300
        // BPM musical bound; the filtered median itself must remain in range.
        if !(6_000_000..=150_000_000).contains(&nanos) {
            return self.malformed();
        }
        self.malformed = 0;
        if self.intervals.len() == FILTER_INTERVALS {
            self.intervals.pop_front();
        }
        self.intervals.push_back(interval);
        let median = median_interval(&self.intervals);
        let Some(measured_tempo) = interval_tempo(median) else {
            return self.malformed();
        };
        self.filtered_interval = Some(match self.filtered_interval {
            None => median,
            Some(current) => smooth_interval(current, median),
        });
        if self.intervals.len() < ACQUISITION_INTERVALS {
            if self.state != FollowerState::Running {
                self.state = FollowerState::Acquiring {
                    intervals: self.intervals.len(),
                };
            }
            return self.running_pulse(received);
        }
        if self.state != FollowerState::Running {
            let was_ready = matches!(self.state, FollowerState::Ready | FollowerState::Stopped);
            self.state = FollowerState::Ready;
            if !was_ready {
                return FollowerAction::Ready(measured_tempo);
            }
        }
        self.running_pulse(received)
    }

    fn running_pulse(&mut self, received: Instant) -> FollowerAction {
        if self.state != FollowerState::Running {
            return FollowerAction::None;
        }
        let Some(interval) = self.filtered_interval else {
            return FollowerAction::None;
        };
        let predicted = self
            .predicted_phase
            .map_or(received, |phase| phase + interval);
        let error = signed_duration_nanos(received, predicted);
        let limit = i64::try_from(interval.as_nanos() / 8).unwrap_or(i64::MAX);
        let correction = error.clamp(-limit, limit);
        self.predicted_phase = Some(shift_instant(predicted, correction));
        FollowerAction::Pulse(PulseUpdate {
            tempo: self.tempo().unwrap_or(Bpm::DEFAULT),
            phase_correction_nanos: correction,
        })
    }
}

fn median_interval(intervals: &VecDeque<Duration>) -> Duration {
    let mut values = intervals.iter().map(Duration::as_nanos).collect::<Vec<_>>();
    values.sort_unstable();
    Duration::from_nanos(u64::try_from(values[values.len() / 2]).unwrap_or(u64::MAX))
}

fn smooth_interval(current: Duration, measured: Duration) -> Duration {
    let current = current.as_nanos() as f64;
    let measured = measured.as_nanos() as f64;
    let target = (current * 7.0 + measured) / 8.0;
    let bounded = target.clamp(current * 0.98, current * 1.02);
    Duration::from_nanos(bounded.round().clamp(1.0, u64::MAX as f64) as u64)
}

fn interval_tempo(interval: Duration) -> Option<Bpm> {
    if interval.is_zero() {
        return None;
    }
    let hundredths =
        (6_000_000_000_000u128 + interval.as_nanos() * 12) / (interval.as_nanos() * 24);
    Bpm::from_hundredths(u16::try_from(hundredths).ok()?)
}

fn signed_duration_nanos(actual: Instant, predicted: Instant) -> i64 {
    if actual >= predicted {
        i64::try_from(actual.duration_since(predicted).as_nanos()).unwrap_or(i64::MAX)
    } else {
        -i64::try_from(predicted.duration_since(actual).as_nanos()).unwrap_or(i64::MAX)
    }
}

pub fn shift_instant(value: Instant, nanos: i64) -> Instant {
    if nanos >= 0 {
        value + Duration::from_nanos(nanos as u64)
    } else {
        value
            .checked_sub(Duration::from_nanos(nanos.unsigned_abs()))
            .unwrap_or(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(origin: Instant, millis: u64) -> Instant {
        origin + Duration::from_millis(millis)
    }

    #[test]
    fn realtime_bytes_do_not_corrupt_note_cc_program_running_status_or_sysex() {
        let mut parser = MidiByteStream::default();
        let items = parser.push(&[
            0x90,
            60,
            MIDI_TIMING_CLOCK,
            100,
            61,
            99,
            0xb2,
            74,
            MIDI_START,
            64,
            0xc3,
            12,
            0xf0,
            0x01,
            MIDI_TIMING_CLOCK,
            0x02,
            0xf7,
            MIDI_STOP,
        ]);
        assert_eq!(
            items,
            [
                StreamItem::Realtime(RealtimeMessage::TimingClock),
                StreamItem::Message(vec![0x90, 60, 100]),
                StreamItem::Message(vec![0x90, 61, 99]),
                StreamItem::Realtime(RealtimeMessage::Start),
                StreamItem::Message(vec![0xb2, 74, 64]),
                StreamItem::Message(vec![0xc3, 12]),
                StreamItem::Realtime(RealtimeMessage::TimingClock),
                StreamItem::Message(vec![0xf0, 0x01, 0x02, 0xf7]),
                StreamItem::Realtime(RealtimeMessage::Stop),
            ]
        );
    }

    #[test]
    fn command_pad_note_on_off_and_pressure_remain_complete() {
        let mut parser = MidiByteStream::default();
        let items = parser.push(&[
            0x99,
            40,
            MIDI_TIMING_CLOCK,
            127,
            0xa9,
            40,
            50,
            0x89,
            MIDI_TIMING_CLOCK,
            40,
            0,
        ]);
        let messages = items
            .into_iter()
            .filter_map(|item| match item {
                StreamItem::Message(bytes) => Some(bytes),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            messages,
            [vec![0x99, 40, 127], vec![0xa9, 40, 50], vec![0x89, 40, 0]]
        );
    }

    #[test]
    fn six_intervals_acquire_exact_twenty_four_ppqn_tempo() {
        for bpm in [20.0, 60.0, 120.0, 173.0, 300.0] {
            let origin = Instant::now();
            let interval = Duration::from_secs_f64(60.0 / bpm / 24.0);
            let mut follower = ExternalClockFollower::default();
            let mut ready = None;
            for pulse in 0..=ACQUISITION_INTERVALS {
                let action = follower.receive(
                    RealtimeMessage::TimingClock,
                    origin + interval.mul_f64(pulse as f64),
                );
                if let FollowerAction::Ready(tempo) = action {
                    ready = Some(tempo);
                }
            }
            let actual = ready.expect("tempo was not acquired").as_f64();
            assert!((actual - bpm).abs() <= 0.02, "{bpm} became {actual}");
        }
    }

    #[test]
    fn start_stop_restart_and_loss_require_fresh_start() {
        let origin = Instant::now();
        let mut follower = ExternalClockFollower::default();
        assert_eq!(
            follower.receive(RealtimeMessage::Start, origin),
            FollowerAction::StartRefused
        );
        for pulse in 0..=ACQUISITION_INTERVALS {
            follower.receive(
                RealtimeMessage::TimingClock,
                origin + Duration::from_millis(21 * pulse as u64),
            );
        }
        assert!(matches!(
            follower.receive(RealtimeMessage::Start, at(origin, 130)),
            FollowerAction::Start(_)
        ));
        assert!(matches!(
            follower.receive(RealtimeMessage::Start, at(origin, 131)),
            FollowerAction::Restart(_)
        ));
        assert_eq!(
            follower.receive(RealtimeMessage::Stop, at(origin, 132)),
            FollowerAction::Stop
        );
        assert_eq!(
            follower.receive(RealtimeMessage::Continue, at(origin, 133)),
            FollowerAction::ContinueRefused
        );
        follower.receive(RealtimeMessage::Start, at(origin, 134));
        assert_eq!(
            follower.check_timeout(at(origin, 700)),
            FollowerAction::Lost
        );
        assert_eq!(
            follower.receive(RealtimeMessage::Start, at(origin, 701)),
            FollowerAction::StartRefused
        );
    }

    #[test]
    fn jitter_bursts_and_outliers_keep_tempo_and_phase_bounded() {
        let origin = Instant::now();
        let mut follower = ExternalClockFollower::default();
        let jitter = [0i64, 1, -1, 2, -2, 0, 1, -1, 0, 2, -2, 0];
        let mut time = origin;
        for pulse in 0..48 {
            let delta = 21i64 + jitter[pulse % jitter.len()];
            time += Duration::from_millis(delta as u64);
            follower.receive(RealtimeMessage::TimingClock, time);
            if pulse == 20 {
                // Two coalesced USB deliveries do not alter the tempo filter
                // or create an unbounded phase correction.
                follower.receive(RealtimeMessage::TimingClock, time);
            }
        }
        let tempo = follower.tempo().unwrap().as_f64();
        assert!((tempo - 120.0).abs() < 4.0, "jitter tempo {tempo}");
        assert!(matches!(
            follower.receive(RealtimeMessage::Start, time),
            FollowerAction::Start(_)
        ));
        let next = time + Duration::from_millis(45);
        let FollowerAction::Pulse(update) = follower.receive(RealtimeMessage::TimingClock, next)
        else {
            panic!("running pulse was not published");
        };
        let limit = follower.filtered_interval.unwrap().as_nanos() / 8;
        assert!(update.phase_correction_nanos.unsigned_abs() as u128 <= limit);
    }

    #[test]
    fn long_run_filter_does_not_accumulate_clock_period_drift() {
        let origin = Instant::now();
        let interval = Duration::from_secs_f64(60.0 / 123.45 / 24.0);
        let mut follower = ExternalClockFollower::default();
        for pulse in 0..=ACQUISITION_INTERVALS {
            follower.receive(
                RealtimeMessage::TimingClock,
                origin + interval.mul_f64(pulse as f64),
            );
        }
        assert!(matches!(
            follower.receive(
                RealtimeMessage::Start,
                origin + interval.mul_f64(ACQUISITION_INTERVALS as f64)
            ),
            FollowerAction::Start(_)
        ));
        let mut last = origin;
        for pulse in ACQUISITION_INTERVALS + 1..=24 * 60 * 10 {
            last = origin + interval.mul_f64(pulse as f64);
            assert!(matches!(
                follower.receive(RealtimeMessage::TimingClock, last),
                FollowerAction::Pulse(_)
            ));
        }
        let tempo = follower.tempo().unwrap().as_f64();
        assert!((tempo - 123.45).abs() <= 0.02, "long-run tempo {tempo}");
        let phase_error = signed_duration_nanos(last, follower.predicted_phase.unwrap());
        assert!(
            phase_error.unsigned_abs() <= 2,
            "phase drift {phase_error} ns"
        );
    }

    #[test]
    fn malformed_clock_fails_boundedly_and_good_pulses_reacquire() {
        let mut follower = ExternalClockFollower::default();
        for _ in 0..MAX_CONSECUTIVE_MALFORMED - 1 {
            assert_eq!(follower.malformed(), FollowerAction::None);
        }
        assert_eq!(follower.malformed(), FollowerAction::Fault);
        assert_eq!(follower.state(), FollowerState::Fault);
        let origin = Instant::now();
        for pulse in 0..=ACQUISITION_INTERVALS {
            follower.receive(
                RealtimeMessage::TimingClock,
                origin + Duration::from_millis(20 * pulse as u64),
            );
        }
        assert_eq!(follower.state(), FollowerState::Ready);
        assert!(matches!(
            follower.receive(RealtimeMessage::Start, at(origin, 130)),
            FollowerAction::Start(_)
        ));
    }
}
