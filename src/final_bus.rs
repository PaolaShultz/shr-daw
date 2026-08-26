//! Fixed four-source final performance bus.
//!
//! Source faders and the live master fader feed the Project-owned fixed
//! MASTER STRIP. The strip owns the final true-peak limiter and meters.

use crate::dsp::{db_to_gain, SmoothedValue, StereoFrame};
#[cfg(test)]
use crate::master_strip::MasterStripSettings;
use crate::master_strip::{
    MasterStripControls, MasterStripMeterSnapshot, MasterStripMeters, MasterStripProcessor,
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

pub const SOURCE_COUNT: usize = 4;
pub const SOURCE_GAIN_MIN_DB: f32 = -60.0;
pub const SOURCE_GAIN_MAX_DB: f32 = 6.0;
pub const MASTER_GAIN_MIN_DB: f32 = -60.0;
pub const MASTER_GAIN_MAX_DB: f32 = 0.0;
pub const DEFAULT_SOURCE_GAIN_DB: f32 = -6.0;
pub const INPUT_PAN_MIN: f32 = -1.0;
pub const INPUT_PAN_MAX: f32 = 1.0;
const GAIN_SMOOTH_SECONDS: f32 = 0.010;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InputMixMode {
    #[default]
    Stereo,
    DualMono,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputChannel {
    One,
    Two,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BusSource {
    Synth = 0,
    Loop = 1,
    Input = 2,
    Drums = 3,
}

impl BusSource {
    pub const ALL: [Self; SOURCE_COUNT] = [Self::Synth, Self::Loop, Self::Input, Self::Drums];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Synth => "SYNTH",
            Self::Loop => "LOOP",
            Self::Input => "INPUT",
            Self::Drums => "DRUMS",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

struct AtomicFader {
    gain_db: AtomicU32,
    muted: AtomicBool,
}

struct AtomicSourceMeter {
    peak_left: AtomicU32,
    peak_right: AtomicU32,
}

impl AtomicSourceMeter {
    fn new() -> Self {
        Self {
            peak_left: AtomicU32::new(0.0_f32.to_bits()),
            peak_right: AtomicU32::new(0.0_f32.to_bits()),
        }
    }

    #[inline]
    fn publish(&self, peak: StereoFrame) {
        self.peak_left.store(peak.left.to_bits(), Ordering::Release);
        self.peak_right
            .store(peak.right.to_bits(), Ordering::Release);
    }

    fn snapshot(&self) -> StereoFrame {
        StereoFrame::new(
            f32::from_bits(self.peak_left.load(Ordering::Acquire)),
            f32::from_bits(self.peak_right.load(Ordering::Acquire)),
        )
        .finite_or_silence()
    }
}

impl AtomicFader {
    fn new(gain_db: f32) -> Self {
        Self {
            gain_db: AtomicU32::new(gain_db.to_bits()),
            muted: AtomicBool::new(false),
        }
    }

    fn gain_db(&self) -> f32 {
        let gain = f32::from_bits(self.gain_db.load(Ordering::Acquire));
        if gain.is_finite() {
            gain
        } else {
            0.0
        }
    }
}

pub struct BusControls {
    sources: [AtomicFader; SOURCE_COUNT],
    source_meters: [AtomicSourceMeter; SOURCE_COUNT],
    input_dual_mono: AtomicBool,
    input_one_pan: AtomicU32,
    input_two_pan: AtomicU32,
    input_one_left: AtomicU32,
    input_one_right: AtomicU32,
    input_two_left: AtomicU32,
    input_two_right: AtomicU32,
    master: AtomicFader,
    metronome_enabled: AtomicBool,
    metronome_bpm: AtomicU32,
    metronome_meter: AtomicU32,
    metronome_generation: AtomicU32,
}

impl Default for BusControls {
    fn default() -> Self {
        Self {
            sources: std::array::from_fn(|_| AtomicFader::new(DEFAULT_SOURCE_GAIN_DB)),
            source_meters: std::array::from_fn(|_| AtomicSourceMeter::new()),
            input_dual_mono: AtomicBool::new(false),
            input_one_pan: AtomicU32::new(INPUT_PAN_MIN.to_bits()),
            input_two_pan: AtomicU32::new(INPUT_PAN_MAX.to_bits()),
            input_one_left: AtomicU32::new(1.0_f32.to_bits()),
            input_one_right: AtomicU32::new(0.0_f32.to_bits()),
            input_two_left: AtomicU32::new(0.0_f32.to_bits()),
            input_two_right: AtomicU32::new(1.0_f32.to_bits()),
            master: AtomicFader::new(0.0),
            metronome_enabled: AtomicBool::new(false),
            metronome_bpm: AtomicU32::new(120.0_f32.to_bits()),
            metronome_meter: AtomicU32::new(4),
            metronome_generation: AtomicU32::new(0),
        }
    }
}

impl BusControls {
    pub fn source_gain_db(&self, source: BusSource) -> f32 {
        self.sources[source.index()].gain_db()
    }

    pub fn set_source_gain_db(&self, source: BusSource, gain_db: f32) -> bool {
        if !gain_db.is_finite() || !(SOURCE_GAIN_MIN_DB..=SOURCE_GAIN_MAX_DB).contains(&gain_db) {
            return false;
        }
        self.sources[source.index()]
            .gain_db
            .store(gain_db.to_bits(), Ordering::Release);
        true
    }

    pub fn source_muted(&self, source: BusSource) -> bool {
        self.sources[source.index()].muted.load(Ordering::Acquire)
    }

    pub fn set_source_muted(&self, source: BusSource, muted: bool) {
        self.sources[source.index()]
            .muted
            .store(muted, Ordering::Release);
    }

    /// Latest callback-block peak after this owner's smoothed gain and mute.
    /// Pages that share an owner intentionally read this same canonical value.
    pub fn source_peak(&self, source: BusSource) -> StereoFrame {
        self.source_meters[source.index()].snapshot()
    }

    #[inline]
    fn publish_source_peak(&self, source: BusSource, peak: StereoFrame) {
        self.source_meters[source.index()].publish(peak);
    }

    pub fn input_mix_mode(&self) -> InputMixMode {
        if self.input_dual_mono.load(Ordering::Acquire) {
            InputMixMode::DualMono
        } else {
            InputMixMode::Stereo
        }
    }

    pub fn set_input_mix_mode(&self, mode: InputMixMode) {
        self.input_dual_mono
            .store(mode == InputMixMode::DualMono, Ordering::Release);
    }

    pub fn input_pan(&self, channel: InputChannel) -> f32 {
        let bits = match channel {
            InputChannel::One => self.input_one_pan.load(Ordering::Acquire),
            InputChannel::Two => self.input_two_pan.load(Ordering::Acquire),
        };
        let pan = f32::from_bits(bits);
        if pan.is_finite() && (INPUT_PAN_MIN..=INPUT_PAN_MAX).contains(&pan) {
            pan
        } else {
            match channel {
                InputChannel::One => INPUT_PAN_MIN,
                InputChannel::Two => INPUT_PAN_MAX,
            }
        }
    }

    pub fn set_input_pan(&self, channel: InputChannel, pan: f32) -> bool {
        if !pan.is_finite() || !(INPUT_PAN_MIN..=INPUT_PAN_MAX).contains(&pan) {
            return false;
        }
        let [left, right] = mono_pan_gains(pan);
        match channel {
            InputChannel::One => {
                self.input_one_left.store(left.to_bits(), Ordering::Release);
                self.input_one_right
                    .store(right.to_bits(), Ordering::Release);
                self.input_one_pan.store(pan.to_bits(), Ordering::Release);
            }
            InputChannel::Two => {
                self.input_two_left.store(left.to_bits(), Ordering::Release);
                self.input_two_right
                    .store(right.to_bits(), Ordering::Release);
                self.input_two_pan.store(pan.to_bits(), Ordering::Release);
            }
        }
        true
    }

    fn input_pan_gains(&self, channel: InputChannel) -> [f32; 2] {
        let (left, right, fallback) = match channel {
            InputChannel::One => (&self.input_one_left, &self.input_one_right, [1.0, 0.0]),
            InputChannel::Two => (&self.input_two_left, &self.input_two_right, [0.0, 1.0]),
        };
        let gains = [
            f32::from_bits(left.load(Ordering::Acquire)),
            f32::from_bits(right.load(Ordering::Acquire)),
        ];
        if gains
            .iter()
            .all(|gain| gain.is_finite() && (0.0..=1.0).contains(gain))
        {
            gains
        } else {
            fallback
        }
    }

    pub fn master_gain_db(&self) -> f32 {
        self.master.gain_db()
    }

    pub fn set_master_gain_db(&self, gain_db: f32) -> bool {
        if !gain_db.is_finite() || !(MASTER_GAIN_MIN_DB..=MASTER_GAIN_MAX_DB).contains(&gain_db) {
            return false;
        }
        self.master
            .gain_db
            .store(gain_db.to_bits(), Ordering::Release);
        true
    }

    pub fn start_metronome(&self, bpm: f32, meter: u8) -> bool {
        if !bpm.is_finite() || !(20.0..=300.0).contains(&bpm) || !(1..=32).contains(&meter) {
            return false;
        }
        self.metronome_bpm.store(bpm.to_bits(), Ordering::Release);
        self.metronome_meter
            .store(u32::from(meter), Ordering::Release);
        self.metronome_enabled.store(true, Ordering::Release);
        self.metronome_generation.fetch_add(1, Ordering::AcqRel);
        true
    }

    pub fn set_metronome_tempo(&self, bpm: f32) -> bool {
        if !bpm.is_finite() || !(20.0..=300.0).contains(&bpm) {
            return false;
        }
        self.metronome_bpm.store(bpm.to_bits(), Ordering::Release);
        true
    }

    pub fn stop_metronome(&self) {
        self.metronome_enabled.store(false, Ordering::Release);
        self.metronome_generation.fetch_add(1, Ordering::AcqRel);
    }
}

pub type FinalBusMeterSnapshot = MasterStripMeterSnapshot;
pub type FinalBusMeters = MasterStripMeters;

struct RuntimeFader {
    value: SmoothedValue,
    last_target: f32,
    smoothing_samples: u32,
}

impl RuntimeFader {
    fn new(gain_db: f32, muted: bool, sample_rate: u32) -> Result<Self, String> {
        let gain = if muted {
            0.0
        } else {
            db_to_gain(gain_db).map_err(|error| error.to_string())?
        };
        Self::new_linear(gain, sample_rate)
    }

    fn new_linear(gain: f32, sample_rate: u32) -> Result<Self, String> {
        if !gain.is_finite() || gain < 0.0 {
            return Err("linear gain must be finite and non-negative".into());
        }
        Ok(Self {
            value: SmoothedValue::new(gain).map_err(|error| error.to_string())?,
            last_target: gain,
            smoothing_samples: ((sample_rate as f32 * GAIN_SMOOTH_SECONDS).round() as u32).max(1),
        })
    }

    #[inline]
    fn refresh(&mut self, gain_db: f32, muted: bool) {
        let target = if muted {
            0.0
        } else {
            db_to_gain(gain_db).unwrap_or(0.0)
        };
        self.refresh_linear(target);
    }

    #[inline]
    fn refresh_linear(&mut self, target: f32) {
        let target = if target.is_finite() && target >= 0.0 {
            target
        } else {
            0.0
        };
        if target != self.last_target {
            if self
                .value
                .set_target(target, self.smoothing_samples)
                .is_err()
            {
                let _ = self.value.reset(0.0);
            }
            self.last_target = target;
        }
    }

    #[inline]
    fn next(&mut self) -> f32 {
        self.value.next_value()
    }
}

pub struct FinalBusProcessor {
    controls: Arc<BusControls>,
    source_faders: [RuntimeFader; SOURCE_COUNT],
    input_matrix: [RuntimeFader; 4],
    master_fader: RuntimeFader,
    strip: MasterStripProcessor,
    sample_rate: u32,
    metronome_generation: u32,
    metronome_until_beat: u32,
    metronome_click_left: u32,
    metronome_y1: f32,
    metronome_y2: f32,
    metronome_coefficient: f32,
    metronome_accent_recurrence: [f32; 2],
    metronome_regular_recurrence: [f32; 2],
    metronome_beat: u32,
}

impl FinalBusProcessor {
    pub fn new(
        sample_rate: u32,
        maximum_frames: usize,
        controls: Arc<BusControls>,
        strip_controls: Arc<MasterStripControls>,
        meters: Arc<FinalBusMeters>,
    ) -> Result<Self, String> {
        let source_fader = |source| {
            RuntimeFader::new(
                controls.source_gain_db(source),
                controls.source_muted(source),
                sample_rate,
            )
        };
        let matrix = input_matrix_targets(&controls);
        Ok(Self {
            source_faders: [
                source_fader(BusSource::Synth)?,
                source_fader(BusSource::Loop)?,
                source_fader(BusSource::Input)?,
                source_fader(BusSource::Drums)?,
            ],
            input_matrix: [
                RuntimeFader::new_linear(matrix[0], sample_rate)?,
                RuntimeFader::new_linear(matrix[1], sample_rate)?,
                RuntimeFader::new_linear(matrix[2], sample_rate)?,
                RuntimeFader::new_linear(matrix[3], sample_rate)?,
            ],
            master_fader: RuntimeFader::new(controls.master_gain_db(), false, sample_rate)?,
            strip: MasterStripProcessor::new(sample_rate, maximum_frames, strip_controls, meters)?,
            sample_rate,
            metronome_generation: controls.metronome_generation.load(Ordering::Acquire),
            metronome_until_beat: 0,
            metronome_click_left: 0,
            metronome_y1: 0.0,
            metronome_y2: 0.0,
            metronome_coefficient: 0.0,
            metronome_accent_recurrence: oscillator_recurrence(1_760.0, sample_rate),
            metronome_regular_recurrence: oscillator_recurrence(1_320.0, sample_rate),
            metronome_beat: 0,
            controls,
        })
    }

    #[cfg(test)]
    pub fn with_neutral_strip(
        sample_rate: u32,
        maximum_frames: usize,
        controls: Arc<BusControls>,
        meters: Arc<FinalBusMeters>,
    ) -> Result<(Self, Arc<MasterStripControls>), String> {
        let strip_controls = Arc::new(MasterStripControls::new(
            sample_rate,
            &MasterStripSettings::default(),
        )?);
        let processor = Self::new(
            sample_rate,
            maximum_frames,
            controls,
            Arc::clone(&strip_controls),
            meters,
        )?;
        Ok((processor, strip_controls))
    }

    #[cfg(test)]
    pub fn latency_samples(&self) -> usize {
        self.strip.latency_samples()
    }

    #[cfg(test)]
    pub fn lookahead_samples(&self) -> usize {
        self.strip.lookahead_samples()
    }

    #[cfg(test)]
    pub fn safety_clamp_count(&self) -> u64 {
        self.strip.safety_clamp_count()
    }

    #[inline]
    pub fn process_source(&mut self, source: BusSource, frames: &mut [StereoFrame]) {
        let index = source.index();
        self.source_faders[index].refresh(
            self.controls.source_gain_db(source),
            self.controls.source_muted(source),
        );
        let mut peak = StereoFrame::SILENCE;
        if source == BusSource::Input {
            for (smoother, target) in self
                .input_matrix
                .iter_mut()
                .zip(input_matrix_targets(&self.controls))
            {
                smoother.refresh_linear(target);
            }
            for frame in frames {
                let gain = self.source_faders[index].next();
                let source_frame = *frame;
                let input_one_left = self.input_matrix[0].next();
                let input_one_right = self.input_matrix[1].next();
                let input_two_left = self.input_matrix[2].next();
                let input_two_right = self.input_matrix[3].next();
                *frame = StereoFrame::new(
                    (source_frame.left * input_one_left + source_frame.right * input_two_left)
                        * gain,
                    (source_frame.left * input_one_right + source_frame.right * input_two_right)
                        * gain,
                )
                .finite_or_silence();
                peak.left = peak.left.max(frame.left.abs());
                peak.right = peak.right.max(frame.right.abs());
            }
        } else {
            for frame in frames {
                let gain = self.source_faders[index].next();
                *frame =
                    StereoFrame::new(frame.left * gain, frame.right * gain).finite_or_silence();
                peak.left = peak.left.max(frame.left.abs());
                peak.right = peak.right.max(frame.right.abs());
            }
        }
        self.controls.publish_source_peak(source, peak);
    }

    #[inline]
    pub fn process_final(&mut self, frames: &mut [StereoFrame]) {
        self.mix_metronome(frames);
        self.master_fader
            .refresh(self.controls.master_gain_db(), false);
        for frame in frames.iter_mut() {
            let master = self.master_fader.next();
            *frame =
                StereoFrame::new(frame.left * master, frame.right * master).finite_or_silence();
        }
        self.strip.process(frames);
    }

    #[inline]
    fn mix_metronome(&mut self, frames: &mut [StereoFrame]) {
        let generation = self.controls.metronome_generation.load(Ordering::Acquire);
        if generation != self.metronome_generation {
            self.metronome_generation = generation;
            self.metronome_until_beat = 0;
            self.metronome_click_left = 0;
            self.metronome_y1 = 0.0;
            self.metronome_y2 = 0.0;
            self.metronome_beat = 0;
        }
        if !self.controls.metronome_enabled.load(Ordering::Acquire) {
            return;
        }
        let bpm = f32::from_bits(self.controls.metronome_bpm.load(Ordering::Acquire));
        let bpm = if bpm.is_finite() {
            bpm.clamp(20.0, 300.0)
        } else {
            120.0
        };
        let meter = self
            .controls
            .metronome_meter
            .load(Ordering::Acquire)
            .clamp(1, 32);
        let beat_samples = ((self.sample_rate as f32 * 60.0 / bpm).round() as u32).max(1);
        let click_samples = (self.sample_rate / 80).max(1);
        for frame in frames {
            if self.metronome_until_beat == 0 {
                self.metronome_click_left = click_samples;
                self.metronome_until_beat = beat_samples;
                let recurrence = if self.metronome_beat % meter == 0 {
                    self.metronome_accent_recurrence
                } else {
                    self.metronome_regular_recurrence
                };
                self.metronome_coefficient = recurrence[0];
                self.metronome_y1 = 0.0;
                self.metronome_y2 = recurrence[1];
            }
            if self.metronome_click_left > 0 {
                let accent = self.metronome_beat % meter == 0;
                let level = if accent { 0.18 } else { 0.11 };
                let envelope = self.metronome_click_left as f32 / click_samples as f32;
                let oscillator = self.metronome_coefficient * self.metronome_y1 - self.metronome_y2;
                self.metronome_y2 = self.metronome_y1;
                self.metronome_y1 = oscillator;
                let sample = oscillator * level * envelope;
                frame.left += sample;
                frame.right += sample;
                self.metronome_click_left -= 1;
            }
            self.metronome_until_beat -= 1;
            if self.metronome_until_beat == 0 {
                self.metronome_beat = self.metronome_beat.wrapping_add(1);
            }
            *frame = frame.finite_or_silence();
        }
    }

    pub fn reset(&mut self) {
        self.strip.reset();
    }
}

fn oscillator_recurrence(frequency: f32, sample_rate: u32) -> [f32; 2] {
    let radians = std::f32::consts::TAU * frequency / sample_rate as f32;
    [2.0 * radians.cos(), -radians.sin()]
}

fn input_matrix_targets(controls: &BusControls) -> [f32; 4] {
    match controls.input_mix_mode() {
        InputMixMode::Stereo => [1.0, 0.0, 0.0, 1.0],
        InputMixMode::DualMono => {
            let [one_left, one_right] = controls.input_pan_gains(InputChannel::One);
            let [two_left, two_right] = controls.input_pan_gains(InputChannel::Two);
            [one_left, one_right, two_left, two_right]
        }
    }
}

fn mono_pan_gains(pan: f32) -> [f32; 2] {
    let pan = pan.clamp(INPUT_PAN_MIN, INPUT_PAN_MAX);
    if pan == INPUT_PAN_MIN {
        [1.0, 0.0]
    } else if pan == INPUT_PAN_MAX {
        [0.0, 1.0]
    } else {
        let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
        [angle.cos(), angle.sin()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::allocation_test::assert_no_allocations;

    fn processor(rate: u32, frames: usize) -> (FinalBusProcessor, Arc<BusControls>) {
        let controls = Arc::new(BusControls::default());
        for source in BusSource::ALL {
            assert!(controls.set_source_gain_db(source, 0.0));
        }
        let (processor, _) = FinalBusProcessor::with_neutral_strip(
            rate,
            frames,
            Arc::clone(&controls),
            Arc::new(FinalBusMeters::default()),
        )
        .unwrap();
        (processor, controls)
    }

    #[test]
    fn source_sum_stereo_identity_and_declared_latency_are_preserved() {
        let (mut bus, _) = processor(48_000, 256);
        let mut synth = [StereoFrame::new(0.01, 0.02); 256];
        let mut loop_frames = [StereoFrame::new(0.03, 0.04); 256];
        let mut input = [StereoFrame::new(0.05, 0.06); 256];
        bus.process_source(BusSource::Synth, &mut synth);
        bus.process_source(BusSource::Loop, &mut loop_frames);
        bus.process_source(BusSource::Input, &mut input);
        let mut sum = std::array::from_fn::<_, 256, _>(|index| {
            StereoFrame::new(
                synth[index].left + loop_frames[index].left + input[index].left,
                synth[index].right + loop_frames[index].right + input[index].right,
            )
        });
        bus.process_final(&mut sum);
        assert_eq!(bus.latency_samples(), 133);
        assert_eq!(bus.lookahead_samples(), 120);
        assert!((sum[133].left - 0.09).abs() < 1e-6);
        assert!((sum[133].right - 0.12).abs() < 1e-6);
        assert_eq!(bus.safety_clamp_count(), 0);
    }

    #[test]
    fn input_defaults_to_stereo_and_dual_mono_starts_with_identical_hard_pans() {
        let (mut bus, controls) = processor(48_000, 4);
        assert_eq!(controls.input_mix_mode(), InputMixMode::Stereo);
        assert_eq!(controls.input_pan(InputChannel::One), -1.0);
        assert_eq!(controls.input_pan(InputChannel::Two), 1.0);

        controls.set_input_mix_mode(InputMixMode::DualMono);
        let mut input = [StereoFrame::new(0.25, -0.5); 4];
        bus.process_source(BusSource::Input, &mut input);
        assert_eq!(input, [StereoFrame::new(0.25, -0.5); 4]);
    }

    #[test]
    fn dual_mono_inputs_have_independent_equal_power_pan() {
        let controls = Arc::new(BusControls::default());
        assert!(controls.set_source_gain_db(BusSource::Input, 0.0));
        controls.set_input_mix_mode(InputMixMode::DualMono);
        assert!(controls.set_input_pan(InputChannel::One, 0.0));
        assert!(controls.set_input_pan(InputChannel::Two, 0.0));
        assert!(!controls.set_input_pan(InputChannel::One, -1.01));
        assert!(!controls.set_input_pan(InputChannel::Two, f32::NAN));
        let (mut bus, _) = FinalBusProcessor::with_neutral_strip(
            48_000,
            1,
            Arc::clone(&controls),
            Arc::new(FinalBusMeters::default()),
        )
        .unwrap();

        let mut input = [StereoFrame::new(0.5, 0.25)];
        assert_no_allocations(|| bus.process_source(BusSource::Input, &mut input));
        let expected = 0.75 * std::f32::consts::FRAC_1_SQRT_2;
        assert!((input[0].left - expected).abs() < 1.0e-6);
        assert!((input[0].right - expected).abs() < 1.0e-6);
        assert_eq!(controls.source_peak(BusSource::Input), input[0]);
    }

    #[test]
    fn live_input_pan_change_is_smoothed_to_its_equal_power_target() {
        let (mut bus, controls) = processor(48_000, 480);
        controls.set_input_mix_mode(InputMixMode::DualMono);
        assert!(controls.set_input_pan(InputChannel::One, 0.0));
        let mut input = [StereoFrame::new(1.0, 0.0); 480];

        bus.process_source(BusSource::Input, &mut input);

        assert!(input[0].left < 1.0 && input[0].left > std::f32::consts::FRAC_1_SQRT_2);
        assert!(input[0].right > 0.0 && input[0].right < std::f32::consts::FRAC_1_SQRT_2);
        assert!((input[479].left - std::f32::consts::FRAC_1_SQRT_2).abs() < 1.0e-6);
        assert!((input[479].right - std::f32::consts::FRAC_1_SQRT_2).abs() < 1.0e-6);
    }

    #[test]
    fn source_mute_master_level_finite_recovery_and_callback_are_safe() {
        let (mut bus, controls) = processor(48_000, 1024);
        assert!(!controls.set_source_gain_db(BusSource::Synth, 7.0));
        assert!(!controls.set_master_gain_db(0.1));
        controls.set_source_muted(BusSource::Synth, true);
        assert!(controls.set_master_gain_db(-6.0));
        let mut source = [StereoFrame::new(f32::NAN, f32::INFINITY); 1024];
        let mut output = [StereoFrame::SILENCE; 1024];
        assert_no_allocations(|| {
            bus.process_source(BusSource::Synth, &mut source);
            output.copy_from_slice(&source);
            bus.process_final(&mut output);
        });
        assert!(output
            .iter()
            .all(|frame| frame.left.is_finite() && frame.right.is_finite()));
        assert_eq!(controls.source_peak(BusSource::Synth), StereoFrame::SILENCE);
    }

    #[test]
    fn source_meter_is_post_smoothed_owner_gain_and_shared_with_canonical_controls() {
        let (mut bus, controls) = processor(48_000, 1024);
        assert!(controls.set_source_gain_db(BusSource::Loop, -6.0));
        let mut source = [StereoFrame::new(0.5, -0.25); 1024];
        bus.process_source(BusSource::Loop, &mut source);
        let peak = controls.source_peak(BusSource::Loop);
        assert!(peak.left > 0.24 && peak.left < 0.5);
        assert!(peak.right > 0.12 && peak.right < 0.25);
        assert_eq!(controls.source_peak(BusSource::Synth), StereoFrame::SILENCE);
    }

    #[test]
    fn metronome_is_internal_accented_finite_and_callback_bounded() {
        let (mut bus, controls) = processor(48_000, 256);
        assert!(controls.start_metronome(240.0, 4));
        let mut frames = vec![StereoFrame::SILENCE; 24_000];
        assert_no_allocations(|| {
            for block in frames.chunks_mut(256) {
                bus.mix_metronome(block);
            }
        });
        let first = frames[..600]
            .iter()
            .map(|frame| frame.left.abs())
            .fold(0.0_f32, f32::max);
        let second = frames[12_000..12_600]
            .iter()
            .map(|frame| frame.left.abs())
            .fold(0.0_f32, f32::max);
        assert!(first > second && second > 0.0);
        assert!(frames
            .iter()
            .all(|frame| frame.left.is_finite() && frame.right.is_finite()));
        controls.stop_metronome();
        let mut silence = [StereoFrame::SILENCE; 64];
        bus.mix_metronome(&mut silence);
        assert_eq!(silence, [StereoFrame::SILENCE; 64]);
    }

    #[test]
    #[ignore = "on-demand callback cost measurement"]
    fn source_meter_callback_cost_has_realtime_headroom() {
        const SAMPLE_RATE: u32 = 48_000;
        const FRAMES: usize = 256;
        const CALLBACKS: u128 = 20_000;
        let (mut bus, controls) = processor(SAMPLE_RATE, FRAMES);
        let mut source = [StereoFrame::new(0.25, -0.125); FRAMES];

        for _ in 0..128 {
            bus.process_source(BusSource::Synth, &mut source);
        }
        let started = std::time::Instant::now();
        for _ in 0..CALLBACKS {
            bus.process_source(BusSource::Synth, &mut source);
            std::hint::black_box(controls.source_peak(BusSource::Synth));
        }
        let average_ns = started.elapsed().as_nanos() / CALLBACKS;
        let callback_deadline_ns = FRAMES as u128 * 1_000_000_000 / u128::from(SAMPLE_RATE);
        let budget_ns = callback_deadline_ns / 10;
        eprintln!(
            "metered owner source: {average_ns} ns per {FRAMES}-frame callback ({:.3}% of deadline)",
            average_ns as f64 * 100.0 / callback_deadline_ns as f64
        );
        assert!(
            average_ns < budget_ns,
            "metered owner source used {average_ns} ns; 10% callback budget is {budget_ns} ns"
        );
    }

    #[test]
    fn final_processing_is_chunk_invariant() {
        let input = (0..4096)
            .map(|index| {
                StereoFrame::new(
                    ((index * 37 % 211) as f32 / 1050.0) - 0.1,
                    ((index * 61 % 197) as f32 / 980.0) - 0.1,
                )
            })
            .collect::<Vec<_>>();
        let run = |chunk: usize| {
            let (mut bus, _) = processor(44_100, chunk);
            let mut output = input.clone();
            for frames in output.chunks_mut(chunk) {
                bus.process_final(frames);
            }
            output
        };
        assert_eq!(run(64), run(127));
    }
}
