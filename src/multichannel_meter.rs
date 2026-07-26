//! Fixed 18-channel recording-meter presentation.
//!
//! The recorder callback publishes raw bounded snapshots. This module owns the
//! slower UI-side RMS smoothing, sample-peak hold/decay, dBFS ladder, and clip
//! hold used by the native overview.

use crate::audio_recorder::{RecorderMeterSample, RecorderMeterSnapshot, MONITOR_CHANNELS};
use crate::performance_meter::{LedState, MeterColor};
use std::time::{Duration, Instant};

pub const METER_THRESHOLDS_DBFS: [f32; 9] =
    [-48.0, -36.0, -30.0, -24.0, -18.0, -12.0, -6.0, -3.0, -1.0];
pub const PEAK_HOLD: Duration = Duration::from_millis(900);
pub const CLIP_HOLD: Duration = Duration::from_secs(2);
pub const PEAK_DECAY_DB_PER_SECOND: f32 = 24.0;
const FLOOR_DBFS: f32 = -120.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelLevel {
    pub rms_dbfs: f32,
    pub peak_dbfs: f32,
    pub clipping: bool,
    pub non_finite: bool,
}

impl Default for ChannelLevel {
    fn default() -> Self {
        Self {
            rms_dbfs: FLOOR_DBFS,
            peak_dbfs: FLOOR_DBFS,
            clipping: false,
            non_finite: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeterCell {
    pub threshold_dbfs: f32,
    pub color: MeterColor,
    pub state: LedState,
}

#[derive(Clone, Copy, Debug, Default)]
struct ChannelState {
    level: ChannelLevel,
    peak_hold_until: Option<Instant>,
    clip_until: Option<Instant>,
    clip_count: u64,
    non_finite_count: u64,
    initialized: bool,
}

#[derive(Debug)]
pub struct MultichannelMeter {
    channels: [ChannelState; MONITOR_CHANNELS],
    last_update: Option<Instant>,
}

impl Default for MultichannelMeter {
    fn default() -> Self {
        Self {
            channels: [ChannelState::default(); MONITOR_CHANNELS],
            last_update: None,
        }
    }
}

pub const fn threshold_color(threshold_dbfs: f32) -> MeterColor {
    if threshold_dbfs <= -18.0 {
        MeterColor::Green
    } else if threshold_dbfs <= -3.0 {
        MeterColor::Yellow
    } else {
        MeterColor::Red
    }
}

fn level_dbfs(amplitude: f32) -> f32 {
    if amplitude.is_finite() && amplitude > 0.0 {
        (20.0 * amplitude.log10()).max(FLOOR_DBFS)
    } else {
        FLOOR_DBFS
    }
}

impl MultichannelMeter {
    pub fn update(&mut self, snapshot: RecorderMeterSnapshot, now: Instant) {
        let elapsed = self
            .last_update
            .map(|last| now.saturating_duration_since(last))
            .unwrap_or_default();
        self.last_update = Some(now);
        for (state, sample) in self.channels.iter_mut().zip(snapshot.channels) {
            update_channel(state, sample, now, elapsed);
        }
    }

    pub fn levels(&self, now: Instant) -> [ChannelLevel; MONITOR_CHANNELS] {
        std::array::from_fn(|index| {
            let state = self.channels[index];
            ChannelLevel {
                clipping: state.clip_until.is_some_and(|until| now < until),
                non_finite: state.level.non_finite,
                ..state.level
            }
        })
    }

    pub fn cells(&self, channel: usize) -> [MeterCell; 9] {
        let level = self
            .channels
            .get(channel)
            .map(|state| state.level)
            .unwrap_or_default();
        meter_cells(level.rms_dbfs, level.peak_dbfs)
    }

    pub fn clear_holds(&mut self) {
        for state in &mut self.channels {
            state.level.peak_dbfs = state.level.rms_dbfs;
            state.peak_hold_until = None;
            state.clip_until = None;
            state.clip_count = 0;
        }
    }

    pub fn seed(&mut self, snapshot: RecorderMeterSnapshot, now: Instant) {
        self.channels = [ChannelState::default(); MONITOR_CHANNELS];
        self.last_update = None;
        self.update(snapshot, now);
    }
}

fn update_channel(
    state: &mut ChannelState,
    sample: RecorderMeterSample,
    now: Instant,
    elapsed: Duration,
) {
    let target_rms = level_dbfs(sample.rms);
    let target_peak = level_dbfs(sample.sample_peak);
    let non_finite = sample.non_finite_count > state.non_finite_count
        || (state.non_finite_count == 0 && sample.non_finite_count > 0);
    if non_finite {
        state.level.non_finite = true;
    }
    state.non_finite_count = sample.non_finite_count;
    if sample.clip_count > state.clip_count
        || (state.clip_count == 0 && sample.clip_count > 0)
        || (sample.sample_peak.is_finite() && sample.sample_peak >= 1.0)
    {
        state.clip_until = Some(now + CLIP_HOLD);
    }
    state.clip_count = sample.clip_count;

    if !state.initialized {
        state.level.rms_dbfs = target_rms;
        state.level.peak_dbfs = target_peak;
        state.peak_hold_until = (target_peak > FLOOR_DBFS).then_some(now + PEAK_HOLD);
        state.initialized = true;
        return;
    }
    let tau = if target_rms > state.level.rms_dbfs {
        0.08
    } else {
        0.35
    };
    let alpha = 1.0 - (-elapsed.as_secs_f32() / tau).exp();
    state.level.rms_dbfs += alpha * (target_rms - state.level.rms_dbfs);
    if target_peak >= state.level.peak_dbfs {
        state.level.peak_dbfs = target_peak;
        state.peak_hold_until = Some(now + PEAK_HOLD);
    } else if state.peak_hold_until.is_none_or(|until| now >= until) {
        state.level.peak_dbfs = (state.level.peak_dbfs
            - PEAK_DECAY_DB_PER_SECOND * elapsed.as_secs_f32())
        .max(target_peak);
    }
}

pub fn meter_cells(rms_dbfs: f32, peak_dbfs: f32) -> [MeterCell; 9] {
    let rms_dbfs = if rms_dbfs.is_finite() {
        rms_dbfs
    } else {
        FLOOR_DBFS
    };
    let peak_dbfs = if peak_dbfs.is_finite() {
        peak_dbfs
    } else {
        FLOOR_DBFS
    };
    let peak_cell = METER_THRESHOLDS_DBFS
        .iter()
        .rposition(|threshold| peak_dbfs >= *threshold);
    std::array::from_fn(|index| {
        let threshold_dbfs = METER_THRESHOLDS_DBFS[index];
        MeterCell {
            threshold_dbfs,
            color: threshold_color(threshold_dbfs),
            state: if peak_cell == Some(index) {
                LedState::Peak
            } else if rms_dbfs >= threshold_dbfs {
                LedState::Level
            } else {
                LedState::Off
            },
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(channel: usize, rms: f32, peak: f32) -> RecorderMeterSnapshot {
        let mut snapshot = RecorderMeterSnapshot::default();
        snapshot.channels[channel] = RecorderMeterSample {
            rms,
            sample_peak: peak,
            ..RecorderMeterSample::default()
        };
        snapshot
    }

    #[test]
    fn ladder_has_all_nine_ordered_recording_thresholds() {
        assert_eq!(
            METER_THRESHOLDS_DBFS,
            [-48.0, -36.0, -30.0, -24.0, -18.0, -12.0, -6.0, -3.0, -1.0]
        );
        assert!(METER_THRESHOLDS_DBFS
            .windows(2)
            .all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn threshold_colors_transition_green_yellow_red_exactly() {
        assert_eq!(
            METER_THRESHOLDS_DBFS.map(threshold_color),
            [
                MeterColor::Green,
                MeterColor::Green,
                MeterColor::Green,
                MeterColor::Green,
                MeterColor::Green,
                MeterColor::Yellow,
                MeterColor::Yellow,
                MeterColor::Yellow,
                MeterColor::Red,
            ]
        );
    }

    #[test]
    fn rms_fills_and_peak_marks_one_same_color_threshold() {
        let cells = meter_cells(-18.0, -3.0);
        assert!(cells[..5].iter().all(|cell| cell.state == LedState::Level));
        assert_eq!(cells[7].state, LedState::Peak);
        assert_eq!(cells[7].color, MeterColor::Yellow);
        assert_eq!(cells[8].state, LedState::Off);
    }

    #[test]
    fn silence_extremes_nan_and_infinity_stay_finite() {
        let now = Instant::now();
        for (rms, peak) in [
            (0.0, 0.0),
            (f32::MAX, f32::MAX),
            (f32::NAN, f32::NAN),
            (f32::INFINITY, f32::NEG_INFINITY),
        ] {
            let mut meter = MultichannelMeter::default();
            meter.update(snapshot(0, rms, peak), now);
            let level = meter.levels(now)[0];
            assert!(level.rms_dbfs.is_finite());
            assert!(level.peak_dbfs.is_finite());
            assert!(meter
                .cells(0)
                .iter()
                .all(|cell| cell.threshold_dbfs.is_finite()));
        }
    }

    #[test]
    fn peak_holds_then_decays_predictably() {
        let now = Instant::now();
        let mut meter = MultichannelMeter::default();
        meter.update(snapshot(0, 0.1, 0.5), now);
        let held = meter.levels(now)[0].peak_dbfs;
        meter.update(snapshot(0, 0.01, 0.01), now + PEAK_HOLD / 2);
        assert_eq!(meter.levels(now + PEAK_HOLD / 2)[0].peak_dbfs, held);
        meter.update(
            snapshot(0, 0.01, 0.01),
            now + PEAK_HOLD + Duration::from_secs(1),
        );
        let decayed = meter.levels(now + PEAK_HOLD + Duration::from_secs(1))[0].peak_dbfs;
        assert!(decayed < held);
        let expected = (held - PEAK_DECAY_DB_PER_SECOND * 1.45).max(-40.0);
        assert!((decayed - expected).abs() < 0.1);
    }

    #[test]
    fn clipping_has_a_distinct_hold_from_near_peak() {
        let now = Instant::now();
        let mut near = MultichannelMeter::default();
        near.update(snapshot(0, 0.5, 0.99), now);
        assert!(!near.levels(now)[0].clipping);

        let mut clipped = MultichannelMeter::default();
        let mut clip = snapshot(0, 0.5, 1.0);
        clip.channels[0].clip_count = 1;
        clipped.update(clip, now);
        assert!(clipped.levels(now)[0].clipping);
        assert!(!clipped.levels(now + CLIP_HOLD)[0].clipping);
    }
}
