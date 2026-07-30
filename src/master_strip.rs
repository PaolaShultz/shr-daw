//! Fixed Project-owned stereo MASTER STRIP.
//!
//! Parameter preparation happens on the owner thread. The audio callback reads
//! atomics, advances fixed state, and publishes bounded lock-free meters.

use crate::dsp::{
    db_to_gain, finite_or_zero, AtomicMeter, Biquad, BiquadCoefficients, DcBlocker,
    MeterAccumulator, MeterSnapshot, SmoothedValue, StereoFrame,
};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub const MASTER_STRIP_FORMAT_VERSION: u8 = 1;
pub const OVERSAMPLE_FACTOR: usize = 8;
pub const INTERPOLATOR_TAPS: usize = 24;
pub const INTERPOLATOR_DELAY_SAMPLES: usize = 12;
pub const COLOR_ALIGNMENT_SAMPLES: usize = 1;
pub const LOOKAHEAD_SECONDS: f32 = 0.0025;
pub const LIMITER_HOLD_SECONDS: f32 = 0.001;
pub const LIMITER_RELEASE_SECONDS: f32 = 0.100;
pub const TRUE_PEAK_GUARD_DB: f32 = 0.25;
#[cfg(test)]
pub const TRUE_PEAK_TOLERANCE_DB: f32 = 0.30;
const PARAMETER_SMOOTH_SECONDS: f32 = 0.010;
const BYPASS_SMOOTH_SECONDS: f32 = 0.005;
const GLUE_KNEE_DB: f32 = 6.0;
const MIN_LEVEL_DB: f32 = -120.0;
const LOUDNESS_HISTOGRAM_BINS: usize = 1_001;
const LOUDNESS_HISTOGRAM_MIN_TENTHS: i32 = -700;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HpfFrequency {
    #[default]
    Off,
    Hz20,
    Hz30,
    Hz40,
}

impl HpfFrequency {
    pub const ALL: [Self; 4] = [Self::Off, Self::Hz20, Self::Hz30, Self::Hz40];

    pub const fn hz(self) -> Option<f32> {
        match self {
            Self::Off => None,
            Self::Hz20 => Some(20.0),
            Self::Hz30 => Some(30.0),
            Self::Hz40 => Some(40.0),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Hz20 => "20Hz",
            Self::Hz30 => "30Hz",
            Self::Hz40 => "40Hz",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LowShelfFrequency {
    Hz30,
    #[default]
    Hz50,
    Hz70,
    Hz90,
}

impl LowShelfFrequency {
    pub const ALL: [Self; 4] = [Self::Hz30, Self::Hz50, Self::Hz70, Self::Hz90];

    pub const fn hz(self) -> f32 {
        match self {
            Self::Hz30 => 30.0,
            Self::Hz50 => 50.0,
            Self::Hz70 => 70.0,
            Self::Hz90 => 90.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HighShelfFrequency {
    Hz8000,
    #[default]
    Hz12000,
    Hz16000,
    Hz20000,
}

impl HighShelfFrequency {
    pub const ALL: [Self; 4] = [Self::Hz8000, Self::Hz12000, Self::Hz16000, Self::Hz20000];

    pub const fn hz(self) -> f32 {
        match self {
            Self::Hz8000 => 8_000.0,
            Self::Hz12000 => 12_000.0,
            Self::Hz16000 => 16_000.0,
            Self::Hz20000 => 20_000.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlueRatio {
    Ratio1_5,
    #[default]
    Ratio2,
    Ratio4,
}

impl GlueRatio {
    pub const ALL: [Self; 3] = [Self::Ratio1_5, Self::Ratio2, Self::Ratio4];

    pub const fn value(self) -> f32 {
        match self {
            Self::Ratio1_5 => 1.5,
            Self::Ratio2 => 2.0,
            Self::Ratio4 => 4.0,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ratio1_5 => "1.5:1",
            Self::Ratio2 => "2:1",
            Self::Ratio4 => "4:1",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlueAttack {
    Ms10,
    #[default]
    Ms30,
    Ms100,
}

impl GlueAttack {
    pub const ALL: [Self; 3] = [Self::Ms10, Self::Ms30, Self::Ms100];

    pub const fn milliseconds(self) -> f32 {
        match self {
            Self::Ms10 => 10.0,
            Self::Ms30 => 30.0,
            Self::Ms100 => 100.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlueRelease {
    Ms100,
    #[default]
    Ms300,
    Ms600,
}

impl GlueRelease {
    pub const ALL: [Self; 3] = [Self::Ms100, Self::Ms300, Self::Ms600];

    pub const fn milliseconds(self) -> f32 {
        match self {
            Self::Ms100 => 100.0,
            Self::Ms300 => 300.0,
            Self::Ms600 => 600.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlueSidechainHpf {
    #[default]
    Off,
    Hz60,
    Hz90,
    Hz120,
}

impl GlueSidechainHpf {
    pub const ALL: [Self; 4] = [Self::Off, Self::Hz60, Self::Hz90, Self::Hz120];

    pub const fn hz(self) -> Option<f32> {
        match self {
            Self::Off => None,
            Self::Hz60 => Some(60.0),
            Self::Hz90 => Some(90.0),
            Self::Hz120 => Some(120.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageSideHpf {
    Hz120,
    #[default]
    Hz180,
    Hz250,
}

impl ImageSideHpf {
    pub const ALL: [Self; 3] = [Self::Hz120, Self::Hz180, Self::Hz250];

    pub const fn hz(self) -> f32 {
        match self {
            Self::Hz120 => 120.0,
            Self::Hz180 => 180.0,
            Self::Hz250 => 250.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MasterStripSettings {
    pub version: u8,
    pub compare: bool,
    pub input_bypass: bool,
    pub input_trim_db: f32,
    pub input_hpf: HpfFrequency,
    pub tone_bypass: bool,
    pub low_shelf_frequency: LowShelfFrequency,
    pub low_shelf_db: f32,
    pub high_shelf_frequency: HighShelfFrequency,
    pub high_shelf_db: f32,
    pub glue_bypass: bool,
    pub glue_threshold_db: f32,
    pub glue_ratio: GlueRatio,
    pub glue_attack: GlueAttack,
    pub glue_release: GlueRelease,
    pub glue_sidechain_hpf: GlueSidechainHpf,
    pub glue_mix_percent: f32,
    pub glue_makeup_db: f32,
    pub color_bypass: bool,
    pub color_drive_db: f32,
    pub color_character_percent: f32,
    pub color_mix_percent: f32,
    pub color_trim_db: f32,
    pub image_bypass: bool,
    pub image_width_percent: f32,
    pub image_side_hpf: ImageSideHpf,
    pub loud_db: f32,
    pub ceiling_dbtp: f32,
}

impl Default for MasterStripSettings {
    fn default() -> Self {
        Self {
            version: MASTER_STRIP_FORMAT_VERSION,
            compare: false,
            input_bypass: true,
            input_trim_db: 0.0,
            input_hpf: HpfFrequency::Off,
            tone_bypass: true,
            low_shelf_frequency: LowShelfFrequency::Hz50,
            low_shelf_db: 0.0,
            high_shelf_frequency: HighShelfFrequency::Hz12000,
            high_shelf_db: 0.0,
            glue_bypass: true,
            glue_threshold_db: -18.0,
            glue_ratio: GlueRatio::Ratio2,
            glue_attack: GlueAttack::Ms30,
            glue_release: GlueRelease::Ms300,
            glue_sidechain_hpf: GlueSidechainHpf::Off,
            glue_mix_percent: 100.0,
            glue_makeup_db: 0.0,
            color_bypass: true,
            color_drive_db: 0.0,
            color_character_percent: 0.0,
            color_mix_percent: 100.0,
            color_trim_db: 0.0,
            image_bypass: true,
            image_width_percent: 100.0,
            image_side_hpf: ImageSideHpf::Hz180,
            loud_db: 0.0,
            ceiling_dbtp: -1.0,
        }
    }
}

impl MasterStripSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != MASTER_STRIP_FORMAT_VERSION {
            return Err(format!("unsupported MASTER STRIP version {}", self.version));
        }
        for (name, value, range) in [
            ("input trim", self.input_trim_db, -12.0..=12.0),
            ("low shelf", self.low_shelf_db, -6.0..=6.0),
            ("high shelf", self.high_shelf_db, -6.0..=6.0),
            ("GLUE threshold", self.glue_threshold_db, -30.0..=0.0),
            ("GLUE mix", self.glue_mix_percent, 0.0..=100.0),
            ("GLUE makeup", self.glue_makeup_db, 0.0..=6.0),
            ("COLOR drive", self.color_drive_db, 0.0..=12.0),
            (
                "COLOR character",
                self.color_character_percent,
                -100.0..=100.0,
            ),
            ("COLOR mix", self.color_mix_percent, 0.0..=100.0),
            ("COLOR trim", self.color_trim_db, -6.0..=0.0),
            ("IMAGE width", self.image_width_percent, 50.0..=150.0),
            ("LOUD", self.loud_db, 0.0..=6.0),
            ("ceiling", self.ceiling_dbtp, -2.0..=-0.5),
        ] {
            if !value.is_finite() || !range.contains(&value) {
                return Err(format!("{name} is outside its supported range"));
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct AtomicF32(AtomicU32);

impl AtomicF32 {
    fn new(value: f32) -> Self {
        Self(AtomicU32::new(value.to_bits()))
    }

    fn load(&self) -> f32 {
        let value = f32::from_bits(self.0.load(Ordering::Acquire));
        if value.is_finite() {
            value
        } else {
            0.0
        }
    }

    fn store(&self, value: f32) {
        self.0.store(value.to_bits(), Ordering::Release);
    }
}

struct AtomicCoefficients {
    values: [AtomicU32; 5],
    revision: AtomicU32,
}

impl AtomicCoefficients {
    fn new(coefficients: BiquadCoefficients) -> Self {
        Self {
            values: [
                AtomicU32::new(coefficients.b0.to_bits()),
                AtomicU32::new(coefficients.b1.to_bits()),
                AtomicU32::new(coefficients.b2.to_bits()),
                AtomicU32::new(coefficients.a1.to_bits()),
                AtomicU32::new(coefficients.a2.to_bits()),
            ],
            revision: AtomicU32::new(0),
        }
    }

    fn store(&self, coefficients: BiquadCoefficients) {
        self.revision.fetch_add(1, Ordering::AcqRel);
        for (target, value) in self.values.iter().zip([
            coefficients.b0,
            coefficients.b1,
            coefficients.b2,
            coefficients.a1,
            coefficients.a2,
        ]) {
            target.store(value.to_bits(), Ordering::Relaxed);
        }
        self.revision.fetch_add(1, Ordering::Release);
    }

    fn load(&self) -> Option<(u32, BiquadCoefficients)> {
        let before = self.revision.load(Ordering::Acquire);
        if before & 1 != 0 {
            return None;
        }
        let value = |index: usize| f32::from_bits(self.values[index].load(Ordering::Relaxed));
        let coefficients = BiquadCoefficients {
            b0: value(0),
            b1: value(1),
            b2: value(2),
            a1: value(3),
            a2: value(4),
        };
        let after = self.revision.load(Ordering::Acquire);
        (before == after && after & 1 == 0).then_some((after, coefficients))
    }
}

pub struct MasterStripControls {
    sample_rate: f32,
    compare: AtomicBool,
    input_bypass: AtomicBool,
    input_trim_gain: AtomicF32,
    input_hpf: AtomicCoefficients,
    tone_bypass: AtomicBool,
    low_shelf: AtomicCoefficients,
    high_shelf: AtomicCoefficients,
    glue_bypass: AtomicBool,
    glue_threshold_db: AtomicF32,
    glue_ratio: AtomicF32,
    glue_attack_coefficient: AtomicF32,
    glue_release_coefficient: AtomicF32,
    glue_sidechain_hpf: AtomicCoefficients,
    glue_mix: AtomicF32,
    glue_makeup_gain: AtomicF32,
    color_bypass: AtomicBool,
    color_drive_gain: AtomicF32,
    color_character: AtomicF32,
    color_mix: AtomicF32,
    color_trim_gain: AtomicF32,
    image_bypass: AtomicBool,
    image_width: AtomicF32,
    image_side_hpf: AtomicCoefficients,
    loud_gain: AtomicF32,
    internal_ceiling_gain: AtomicF32,
    sample_ceiling_gain: AtomicF32,
    reset_loudness_generation: AtomicU32,
}

impl MasterStripControls {
    pub fn new(sample_rate: u32, settings: &MasterStripSettings) -> Result<Self, String> {
        if !(8_000..=384_000).contains(&sample_rate) {
            return Err("unsupported MASTER STRIP sample rate".into());
        }
        settings.validate()?;
        let controls = Self {
            sample_rate: sample_rate as f32,
            compare: AtomicBool::new(false),
            input_bypass: AtomicBool::new(true),
            input_trim_gain: AtomicF32::new(1.0),
            input_hpf: AtomicCoefficients::new(BiquadCoefficients::IDENTITY),
            tone_bypass: AtomicBool::new(true),
            low_shelf: AtomicCoefficients::new(BiquadCoefficients::IDENTITY),
            high_shelf: AtomicCoefficients::new(BiquadCoefficients::IDENTITY),
            glue_bypass: AtomicBool::new(true),
            glue_threshold_db: AtomicF32::new(-18.0),
            glue_ratio: AtomicF32::new(2.0),
            glue_attack_coefficient: AtomicF32::new(0.0),
            glue_release_coefficient: AtomicF32::new(0.0),
            glue_sidechain_hpf: AtomicCoefficients::new(BiquadCoefficients::IDENTITY),
            glue_mix: AtomicF32::new(1.0),
            glue_makeup_gain: AtomicF32::new(1.0),
            color_bypass: AtomicBool::new(true),
            color_drive_gain: AtomicF32::new(1.0),
            color_character: AtomicF32::new(0.0),
            color_mix: AtomicF32::new(1.0),
            color_trim_gain: AtomicF32::new(1.0),
            image_bypass: AtomicBool::new(true),
            image_width: AtomicF32::new(1.0),
            image_side_hpf: AtomicCoefficients::new(BiquadCoefficients::IDENTITY),
            loud_gain: AtomicF32::new(1.0),
            internal_ceiling_gain: AtomicF32::new(
                db_to_gain(-1.0 - TRUE_PEAK_GUARD_DB).expect("bounded default ceiling"),
            ),
            sample_ceiling_gain: AtomicF32::new(db_to_gain(-1.0).expect("bounded default ceiling")),
            reset_loudness_generation: AtomicU32::new(0),
        };
        controls.apply(settings)?;
        Ok(controls)
    }

    pub fn apply(&self, settings: &MasterStripSettings) -> Result<(), String> {
        settings.validate()?;
        let coefficient =
            |milliseconds: f32| (-1.0 / (milliseconds * 0.001 * self.sample_rate)).exp();
        self.compare.store(settings.compare, Ordering::Release);
        self.input_bypass
            .store(settings.input_bypass, Ordering::Release);
        self.input_trim_gain
            .store(db_to_gain(settings.input_trim_db).map_err(|error| error.to_string())?);
        self.input_hpf.store(match settings.input_hpf.hz() {
            Some(hz) => {
                BiquadCoefficients::high_pass(hz, std::f32::consts::FRAC_1_SQRT_2, self.sample_rate)
                    .map_err(|error| error.to_string())?
            }
            None => BiquadCoefficients::IDENTITY,
        });
        self.tone_bypass
            .store(settings.tone_bypass, Ordering::Release);
        self.low_shelf.store(
            BiquadCoefficients::low_shelf(
                settings.low_shelf_frequency.hz(),
                0.5,
                settings.low_shelf_db,
                self.sample_rate,
            )
            .map_err(|error| error.to_string())?,
        );
        self.high_shelf.store(
            BiquadCoefficients::high_shelf(
                settings.high_shelf_frequency.hz(),
                0.5,
                settings.high_shelf_db,
                self.sample_rate,
            )
            .map_err(|error| error.to_string())?,
        );
        self.glue_bypass
            .store(settings.glue_bypass, Ordering::Release);
        self.glue_threshold_db.store(settings.glue_threshold_db);
        self.glue_ratio.store(settings.glue_ratio.value());
        self.glue_attack_coefficient
            .store(coefficient(settings.glue_attack.milliseconds()));
        self.glue_release_coefficient
            .store(coefficient(settings.glue_release.milliseconds()));
        self.glue_sidechain_hpf
            .store(match settings.glue_sidechain_hpf.hz() {
                Some(hz) => BiquadCoefficients::high_pass(
                    hz,
                    std::f32::consts::FRAC_1_SQRT_2,
                    self.sample_rate,
                )
                .map_err(|error| error.to_string())?,
                None => BiquadCoefficients::IDENTITY,
            });
        self.glue_mix.store(settings.glue_mix_percent * 0.01);
        self.glue_makeup_gain
            .store(db_to_gain(settings.glue_makeup_db).map_err(|error| error.to_string())?);
        self.color_bypass
            .store(settings.color_bypass, Ordering::Release);
        self.color_drive_gain
            .store(db_to_gain(settings.color_drive_db).map_err(|error| error.to_string())?);
        self.color_character
            .store(settings.color_character_percent * 0.01);
        self.color_mix.store(settings.color_mix_percent * 0.01);
        self.color_trim_gain
            .store(db_to_gain(settings.color_trim_db).map_err(|error| error.to_string())?);
        self.image_bypass
            .store(settings.image_bypass, Ordering::Release);
        self.image_width.store(settings.image_width_percent * 0.01);
        self.image_side_hpf.store(
            BiquadCoefficients::high_pass(
                settings.image_side_hpf.hz(),
                std::f32::consts::FRAC_1_SQRT_2,
                self.sample_rate,
            )
            .map_err(|error| error.to_string())?,
        );
        self.loud_gain
            .store(db_to_gain(settings.loud_db).map_err(|error| error.to_string())?);
        self.internal_ceiling_gain.store(
            db_to_gain(settings.ceiling_dbtp - TRUE_PEAK_GUARD_DB)
                .map_err(|error| error.to_string())?,
        );
        self.sample_ceiling_gain
            .store(db_to_gain(settings.ceiling_dbtp).map_err(|error| error.to_string())?);
        Ok(())
    }

    pub fn reset_loudness(&self) {
        self.reset_loudness_generation
            .fetch_add(1, Ordering::Release);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MasterStripMeterSnapshot {
    pub input: MeterSnapshot,
    pub output: MeterSnapshot,
    pub output_true_peak_dbtp: f32,
    pub glue_gain_reduction_db: f32,
    pub limiter_gain_reduction_db: f32,
    pub correlation: f32,
    pub loudness_m_lufs: f32,
    pub loudness_s_lufs: f32,
    pub loudness_i_lufs: f32,
}

impl Default for MasterStripMeterSnapshot {
    fn default() -> Self {
        Self {
            input: MeterSnapshot::default(),
            output: MeterSnapshot::default(),
            output_true_peak_dbtp: MIN_LEVEL_DB,
            glue_gain_reduction_db: 0.0,
            limiter_gain_reduction_db: 0.0,
            correlation: 1.0,
            loudness_m_lufs: MIN_LEVEL_DB,
            loudness_s_lufs: MIN_LEVEL_DB,
            loudness_i_lufs: MIN_LEVEL_DB,
        }
    }
}

pub struct MasterStripMeters {
    input: AtomicMeter,
    output: AtomicMeter,
    true_peak: AtomicF32,
    glue_reduction: AtomicF32,
    limiter_reduction: AtomicF32,
    correlation: AtomicF32,
    loudness_m: AtomicF32,
    loudness_s: AtomicF32,
    loudness_i: AtomicF32,
}

impl Default for MasterStripMeters {
    fn default() -> Self {
        let defaults = MasterStripMeterSnapshot::default();
        Self {
            input: AtomicMeter::default(),
            output: AtomicMeter::default(),
            true_peak: AtomicF32::new(defaults.output_true_peak_dbtp),
            glue_reduction: AtomicF32::new(0.0),
            limiter_reduction: AtomicF32::new(0.0),
            correlation: AtomicF32::new(1.0),
            loudness_m: AtomicF32::new(defaults.loudness_m_lufs),
            loudness_s: AtomicF32::new(defaults.loudness_s_lufs),
            loudness_i: AtomicF32::new(defaults.loudness_i_lufs),
        }
    }
}

impl MasterStripMeters {
    pub fn snapshot(&self) -> MasterStripMeterSnapshot {
        MasterStripMeterSnapshot {
            input: self.input.load(),
            output: self.output.load(),
            output_true_peak_dbtp: self.true_peak.load().clamp(MIN_LEVEL_DB, 24.0),
            glue_gain_reduction_db: self.glue_reduction.load().clamp(0.0, 160.0),
            limiter_gain_reduction_db: self.limiter_reduction.load().clamp(0.0, 160.0),
            correlation: self.correlation.load().clamp(-1.0, 1.0),
            loudness_m_lufs: self.loudness_m.load().clamp(MIN_LEVEL_DB, 24.0),
            loudness_s_lufs: self.loudness_s.load().clamp(MIN_LEVEL_DB, 24.0),
            loudness_i_lufs: self.loudness_i.load().clamp(MIN_LEVEL_DB, 24.0),
        }
    }

    fn clear(&self) {
        let default = MasterStripMeterSnapshot::default();
        self.input.publish(default.input);
        self.output.publish(default.output);
        self.true_peak.store(default.output_true_peak_dbtp);
        self.glue_reduction.store(0.0);
        self.limiter_reduction.store(0.0);
        self.correlation.store(1.0);
        self.loudness_m.store(default.loudness_m_lufs);
        self.loudness_s.store(default.loudness_s_lufs);
        self.loudness_i.store(default.loudness_i_lufs);
    }
}

#[derive(Clone, Copy)]
struct StereoBiquad {
    left: Biquad,
    right: Biquad,
}

impl StereoBiquad {
    fn new(coefficients: BiquadCoefficients) -> Self {
        Self {
            left: Biquad::new(coefficients),
            right: Biquad::new(coefficients),
        }
    }

    fn set_coefficients(&mut self, coefficients: BiquadCoefficients) {
        if self.left.set_coefficients(coefficients).is_err()
            || self.right.set_coefficients(coefficients).is_err()
        {
            self.reset();
        }
    }

    #[inline]
    fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        StereoFrame::new(
            self.left.process(frame.left),
            self.right.process(frame.right),
        )
    }

    fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }
}

struct AtomicSmoothedBiquad {
    current: StereoBiquad,
    next: StereoBiquad,
    mix: SmoothedValue,
    revision: u32,
    smoothing_samples: u32,
    transitioning: bool,
}

impl AtomicSmoothedBiquad {
    fn new(source: &AtomicCoefficients, sample_rate: u32) -> Self {
        let (revision, coefficients) = source
            .load()
            .expect("unshared coefficient source is stable during construction");
        Self {
            current: StereoBiquad::new(coefficients),
            next: StereoBiquad::new(coefficients),
            mix: SmoothedValue::new(0.0).expect("finite"),
            revision,
            smoothing_samples: smoothing_samples(sample_rate, BYPASS_SMOOTH_SECONDS),
            transitioning: false,
        }
    }

    fn refresh(&mut self, source: &AtomicCoefficients) {
        let Some((revision, coefficients)) = source.load() else {
            return;
        };
        if revision == self.revision || !coefficients.is_finite() {
            return;
        }
        self.revision = revision;
        self.next = self.current;
        self.next.set_coefficients(coefficients);
        let _ = self.mix.reset(0.0);
        let _ = self.mix.set_target(1.0, self.smoothing_samples);
        self.transitioning = true;
    }

    #[inline]
    fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        if !self.transitioning {
            return self.current.process(frame);
        }
        let old = self.current.process(frame);
        let new = self.next.process(frame);
        let mix = self.mix.next_value();
        if mix >= 1.0 {
            self.current = self.next;
            self.transitioning = false;
            return new;
        }
        StereoFrame::new(
            old.left + (new.left - old.left) * mix,
            old.right + (new.right - old.right) * mix,
        )
        .finite_or_silence()
    }

    fn reset(&mut self) {
        self.current.reset();
        self.next.reset();
        self.transitioning = false;
        let _ = self.mix.reset(0.0);
    }
}

struct SmoothedParameter {
    value: SmoothedValue,
    target: f32,
    samples: u32,
}

impl SmoothedParameter {
    fn new(value: f32, sample_rate: u32) -> Self {
        Self {
            value: SmoothedValue::new(value).expect("finite"),
            target: value,
            samples: smoothing_samples(sample_rate, PARAMETER_SMOOTH_SECONDS),
        }
    }

    fn refresh(&mut self, target: f32) {
        if target.is_finite() && target != self.target {
            if self.value.set_target(target, self.samples).is_err() {
                let _ = self.value.reset(0.0);
            }
            self.target = target;
        }
    }

    #[inline]
    fn next(&mut self) -> f32 {
        self.value.next_value()
    }

    fn reset(&mut self, value: f32) {
        self.target = value;
        let _ = self.value.reset(value);
    }
}

struct SectionBypass {
    mix: SmoothedParameter,
}

impl SectionBypass {
    fn new(bypassed: bool, sample_rate: u32) -> Self {
        let mut mix = SmoothedParameter::new(if bypassed { 0.0 } else { 1.0 }, sample_rate);
        mix.samples = smoothing_samples(sample_rate, BYPASS_SMOOTH_SECONDS);
        Self { mix }
    }

    fn refresh(&mut self, bypassed: bool, compare: bool) {
        self.mix
            .refresh(if bypassed || compare { 0.0 } else { 1.0 });
    }

    #[inline]
    fn apply(&mut self, dry: StereoFrame, wet: StereoFrame) -> StereoFrame {
        let mix = self.mix.next();
        StereoFrame::new(
            dry.left + (wet.left - dry.left) * mix,
            dry.right + (wet.right - dry.right) * mix,
        )
        .finite_or_silence()
    }

    fn reset(&mut self, bypassed: bool) {
        self.mix.reset(if bypassed { 0.0 } else { 1.0 });
    }
}

struct InputSection {
    trim: SmoothedParameter,
    hpf: AtomicSmoothedBiquad,
    bypass: SectionBypass,
}

impl InputSection {
    fn new(controls: &MasterStripControls, sample_rate: u32) -> Self {
        Self {
            trim: SmoothedParameter::new(controls.input_trim_gain.load(), sample_rate),
            hpf: AtomicSmoothedBiquad::new(&controls.input_hpf, sample_rate),
            bypass: SectionBypass::new(
                controls.input_bypass.load(Ordering::Acquire)
                    || controls.compare.load(Ordering::Acquire),
                sample_rate,
            ),
        }
    }

    fn refresh(&mut self, controls: &MasterStripControls, compare: bool) {
        self.trim.refresh(controls.input_trim_gain.load());
        self.hpf.refresh(&controls.input_hpf);
        self.bypass
            .refresh(controls.input_bypass.load(Ordering::Acquire), compare);
    }

    #[inline]
    fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        let gain = self.trim.next();
        let wet = self
            .hpf
            .process(StereoFrame::new(frame.left * gain, frame.right * gain));
        self.bypass.apply(frame, wet)
    }

    fn reset(&mut self, controls: &MasterStripControls) {
        self.trim.reset(controls.input_trim_gain.load());
        self.hpf.reset();
        self.bypass.reset(
            controls.input_bypass.load(Ordering::Acquire)
                || controls.compare.load(Ordering::Acquire),
        );
    }
}

struct ToneSection {
    low: AtomicSmoothedBiquad,
    high: AtomicSmoothedBiquad,
    bypass: SectionBypass,
}

impl ToneSection {
    fn new(controls: &MasterStripControls, sample_rate: u32) -> Self {
        Self {
            low: AtomicSmoothedBiquad::new(&controls.low_shelf, sample_rate),
            high: AtomicSmoothedBiquad::new(&controls.high_shelf, sample_rate),
            bypass: SectionBypass::new(
                controls.tone_bypass.load(Ordering::Acquire)
                    || controls.compare.load(Ordering::Acquire),
                sample_rate,
            ),
        }
    }

    fn refresh(&mut self, controls: &MasterStripControls, compare: bool) {
        self.low.refresh(&controls.low_shelf);
        self.high.refresh(&controls.high_shelf);
        self.bypass
            .refresh(controls.tone_bypass.load(Ordering::Acquire), compare);
    }

    #[inline]
    fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        let wet = self.high.process(self.low.process(frame));
        self.bypass.apply(frame, wet)
    }

    fn reset(&mut self, controls: &MasterStripControls) {
        self.low.reset();
        self.high.reset();
        self.bypass.reset(
            controls.tone_bypass.load(Ordering::Acquire)
                || controls.compare.load(Ordering::Acquire),
        );
    }
}

struct GlueSection {
    rms_coefficient: f32,
    sidechain: AtomicSmoothedBiquad,
    detector_power: f32,
    gain_db: f32,
    threshold_db: f32,
    ratio: f32,
    attack_coefficient: f32,
    release_coefficient: f32,
    mix: SmoothedParameter,
    makeup: SmoothedParameter,
    bypass: SectionBypass,
    maximum_reduction_db: f32,
    gain_table: GainTable,
}

impl GlueSection {
    fn new(controls: &MasterStripControls, sample_rate: u32) -> Self {
        Self {
            rms_coefficient: (-1.0 / (0.010 * sample_rate as f32)).exp(),
            sidechain: AtomicSmoothedBiquad::new(&controls.glue_sidechain_hpf, sample_rate),
            detector_power: 0.0,
            gain_db: 0.0,
            threshold_db: controls.glue_threshold_db.load(),
            ratio: controls.glue_ratio.load(),
            attack_coefficient: controls.glue_attack_coefficient.load(),
            release_coefficient: controls.glue_release_coefficient.load(),
            mix: SmoothedParameter::new(controls.glue_mix.load(), sample_rate),
            makeup: SmoothedParameter::new(controls.glue_makeup_gain.load(), sample_rate),
            bypass: SectionBypass::new(
                controls.glue_bypass.load(Ordering::Acquire)
                    || controls.compare.load(Ordering::Acquire),
                sample_rate,
            ),
            maximum_reduction_db: 0.0,
            gain_table: GainTable::new(),
        }
    }

    fn refresh(&mut self, controls: &MasterStripControls, compare: bool) {
        self.sidechain.refresh(&controls.glue_sidechain_hpf);
        self.threshold_db = controls.glue_threshold_db.load().clamp(-30.0, 0.0);
        self.ratio = controls.glue_ratio.load().clamp(1.0, 4.0);
        self.attack_coefficient = controls.glue_attack_coefficient.load().clamp(0.0, 1.0);
        self.release_coefficient = controls.glue_release_coefficient.load().clamp(0.0, 1.0);
        self.mix.refresh(controls.glue_mix.load().clamp(0.0, 1.0));
        self.makeup
            .refresh(controls.glue_makeup_gain.load().clamp(1.0, 2.0));
        self.bypass
            .refresh(controls.glue_bypass.load(Ordering::Acquire), compare);
        self.maximum_reduction_db = 0.0;
    }

    #[inline]
    fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        let sidechain = self.sidechain.process(frame);
        let instantaneous = sidechain.left * sidechain.left + sidechain.right * sidechain.right;
        // A 10 ms quasi-RMS energy detector avoids the polarity and crest
        // sensitivity of sample-peak detection while remaining stereo linked.
        self.detector_power = finite_or_zero(
            instantaneous + self.rms_coefficient * (self.detector_power - instantaneous),
        )
        .max(0.0);
        let level_db = if self.detector_power > 0.0 {
            10.0 * self.detector_power.log10()
        } else {
            MIN_LEVEL_DB
        };
        let target_db = glue_curve_gain_db(level_db, self.threshold_db, self.ratio, GLUE_KNEE_DB);
        let coefficient = if target_db < self.gain_db {
            self.attack_coefficient
        } else {
            self.release_coefficient
        };
        self.gain_db =
            finite_or_zero(target_db + coefficient * (self.gain_db - target_db)).clamp(-48.0, 0.0);
        self.maximum_reduction_db = self.maximum_reduction_db.max(-self.gain_db);
        let gain = self.gain_table.gain(self.gain_db) * self.makeup.next();
        let parallel = self.mix.next();
        let wet = StereoFrame::new(
            frame.left + (frame.left * gain - frame.left) * parallel,
            frame.right + (frame.right * gain - frame.right) * parallel,
        )
        .finite_or_silence();
        self.bypass.apply(frame, wet)
    }

    fn reset(&mut self, controls: &MasterStripControls) {
        self.sidechain.reset();
        self.detector_power = 0.0;
        self.gain_db = 0.0;
        self.maximum_reduction_db = 0.0;
        self.mix.reset(controls.glue_mix.load());
        self.makeup.reset(controls.glue_makeup_gain.load());
        self.bypass.reset(
            controls.glue_bypass.load(Ordering::Acquire)
                || controls.compare.load(Ordering::Acquire),
        );
    }
}

const GAIN_TABLE_MIN_DB: f32 = -48.0;
const GAIN_TABLE_STEPS: usize = 480;

struct GainTable {
    values: [f32; GAIN_TABLE_STEPS + 1],
}

impl GainTable {
    fn new() -> Self {
        let mut values = [0.0; GAIN_TABLE_STEPS + 1];
        for (index, value) in values.iter_mut().enumerate() {
            let db = GAIN_TABLE_MIN_DB * (1.0 - index as f32 / GAIN_TABLE_STEPS as f32);
            *value = db_to_gain(db).expect("bounded gain-table value");
        }
        Self { values }
    }

    #[inline]
    fn gain(&self, db: f32) -> f32 {
        let normalized = ((db.clamp(GAIN_TABLE_MIN_DB, 0.0) - GAIN_TABLE_MIN_DB)
            / -GAIN_TABLE_MIN_DB)
            * GAIN_TABLE_STEPS as f32;
        let first = normalized.floor() as usize;
        let second = (first + 1).min(GAIN_TABLE_STEPS);
        let fraction = normalized - first as f32;
        self.values[first] + (self.values[second] - self.values[first]) * fraction
    }
}

fn glue_curve_gain_db(level_db: f32, threshold_db: f32, ratio: f32, knee_db: f32) -> f32 {
    let slope = ratio.recip() - 1.0;
    let over = level_db - threshold_db;
    if knee_db > 0.0 && over.abs() <= knee_db * 0.5 {
        slope * (over + knee_db * 0.5).powi(2) / (2.0 * knee_db)
    } else if over > 0.0 {
        slope * over
    } else {
        0.0
    }
}

struct ColorSection {
    previous_left: Option<f32>,
    previous_right: Option<f32>,
    dry_left: f32,
    dry_right: f32,
    dc_left: DcBlocker,
    dc_right: DcBlocker,
    drive: SmoothedParameter,
    character: SmoothedParameter,
    mix: SmoothedParameter,
    trim: SmoothedParameter,
    bypass: SectionBypass,
}

impl ColorSection {
    fn new(controls: &MasterStripControls, sample_rate: u32) -> Result<Self, String> {
        Ok(Self {
            previous_left: None,
            previous_right: None,
            dry_left: 0.0,
            dry_right: 0.0,
            dc_left: DcBlocker::new(5.0, sample_rate as f32).map_err(|error| error.to_string())?,
            dc_right: DcBlocker::new(5.0, sample_rate as f32).map_err(|error| error.to_string())?,
            drive: SmoothedParameter::new(controls.color_drive_gain.load(), sample_rate),
            character: SmoothedParameter::new(controls.color_character.load(), sample_rate),
            mix: SmoothedParameter::new(controls.color_mix.load(), sample_rate),
            trim: SmoothedParameter::new(controls.color_trim_gain.load(), sample_rate),
            bypass: SectionBypass::new(
                controls.color_bypass.load(Ordering::Acquire)
                    || controls.compare.load(Ordering::Acquire),
                sample_rate,
            ),
        })
    }

    fn refresh(&mut self, controls: &MasterStripControls, compare: bool) {
        self.drive
            .refresh(controls.color_drive_gain.load().clamp(1.0, 4.0));
        self.character
            .refresh(controls.color_character.load().clamp(-1.0, 1.0));
        self.mix.refresh(controls.color_mix.load().clamp(0.0, 1.0));
        self.trim
            .refresh(controls.color_trim_gain.load().clamp(0.5, 1.0));
        self.bypass
            .refresh(controls.color_bypass.load(Ordering::Acquire), compare);
    }

    #[inline]
    fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        let drive = self.drive.next();
        let character = self.character.next();
        let mix = self.mix.next();
        let trim = self.trim.next();
        let left_input = frame.left * drive;
        let right_input = frame.right * drive;
        let left = color_adaa(left_input, self.previous_left, character) / drive;
        let right = color_adaa(right_input, self.previous_right, character) / drive;
        self.previous_left = Some(left_input);
        self.previous_right = Some(right_input);
        let wet = StereoFrame::new(
            self.dc_left.process(left) * trim,
            self.dc_right.process(right) * trim,
        );
        // First-order ADAA is a midpoint operation. Delay the dry comparison by
        // one sample so partial wet/dry settings do not exaggerate the top end.
        let delayed_dry = StereoFrame::new(self.dry_left, self.dry_right);
        self.dry_left = frame.left;
        self.dry_right = frame.right;
        let colored = StereoFrame::new(
            delayed_dry.left + (wet.left - delayed_dry.left) * mix,
            delayed_dry.right + (wet.right - delayed_dry.right) * mix,
        )
        .finite_or_silence();
        // The one-sample dry alignment remains in circuit while bypassed, so
        // section bypass and whole-strip comparison keep one fixed latency.
        self.bypass.apply(delayed_dry, colored)
    }

    fn reset(&mut self, controls: &MasterStripControls) {
        self.previous_left = None;
        self.previous_right = None;
        self.dry_left = 0.0;
        self.dry_right = 0.0;
        self.dc_left.reset();
        self.dc_right.reset();
        self.drive.reset(controls.color_drive_gain.load());
        self.character.reset(controls.color_character.load());
        self.mix.reset(controls.color_mix.load());
        self.trim.reset(controls.color_trim_gain.load());
        self.bypass.reset(
            controls.color_bypass.load(Ordering::Acquire)
                || controls.compare.load(Ordering::Acquire),
        );
    }
}

#[inline]
fn color_adaa(input: f32, previous: Option<f32>, character: f32) -> f32 {
    let Some(previous) = previous else {
        return color_transfer(input, character);
    };
    let difference = input - previous;
    if difference.abs() <= 1.0e-4 {
        color_transfer((input + previous) * 0.5, character)
    } else {
        finite_or_zero(
            (color_antiderivative(input, character) - color_antiderivative(previous, character))
                / difference,
        )
    }
}

#[inline]
fn color_transfer(input: f32, character: f32) -> f32 {
    let odd = soft_cubic_unity_slope(input);
    let even = squared_plateau(input);
    (odd + character * 0.25 * even) / (1.0 + character.abs() * 0.25)
}

#[inline]
fn color_antiderivative(input: f32, character: f32) -> f32 {
    (soft_cubic_unity_slope_antiderivative(input)
        + character * 0.25 * squared_plateau_antiderivative(input))
        / (1.0 + character.abs() * 0.25)
}

#[inline]
fn soft_cubic_unity_slope(input: f32) -> f32 {
    if input >= 1.0 {
        2.0 / 3.0
    } else if input <= -1.0 {
        -2.0 / 3.0
    } else {
        input - input * input * input / 3.0
    }
}

#[inline]
fn soft_cubic_unity_slope_antiderivative(input: f32) -> f32 {
    if input >= 1.0 {
        input * (2.0 / 3.0) - 0.25
    } else if input <= -1.0 {
        -input * (2.0 / 3.0) - 0.25
    } else {
        0.5 * input * input - input.powi(4) / 12.0
    }
}

#[inline]
fn squared_plateau(input: f32) -> f32 {
    input.clamp(-1.0, 1.0).powi(2)
}

#[inline]
fn squared_plateau_antiderivative(input: f32) -> f32 {
    if input >= 1.0 {
        input - 2.0 / 3.0
    } else if input <= -1.0 {
        input + 2.0 / 3.0
    } else {
        input.powi(3) / 3.0
    }
}

struct ImageSection {
    side_hpf: AtomicSmoothedBiquad,
    width: SmoothedParameter,
    bypass: SectionBypass,
}

impl ImageSection {
    fn new(controls: &MasterStripControls, sample_rate: u32) -> Self {
        Self {
            side_hpf: AtomicSmoothedBiquad::new(&controls.image_side_hpf, sample_rate),
            width: SmoothedParameter::new(controls.image_width.load(), sample_rate),
            bypass: SectionBypass::new(
                controls.image_bypass.load(Ordering::Acquire)
                    || controls.compare.load(Ordering::Acquire),
                sample_rate,
            ),
        }
    }

    fn refresh(&mut self, controls: &MasterStripControls, compare: bool) {
        self.side_hpf.refresh(&controls.image_side_hpf);
        self.width
            .refresh(controls.image_width.load().clamp(0.5, 1.5));
        self.bypass
            .refresh(controls.image_bypass.load(Ordering::Acquire), compare);
    }

    #[inline]
    fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        let width = self.width.next();
        if width == 1.0 && self.bypass.mix.value.current() >= 1.0 {
            return frame;
        }
        let mid = (frame.left + frame.right) * 0.5;
        let side = (frame.left - frame.right) * 0.5;
        let filtered = self.side_hpf.process(StereoFrame::new(side, side)).left;
        let adjusted_side = if width > 1.0 {
            side + filtered * (width - 1.0)
        } else {
            side * width
        };
        let wet = StereoFrame::new(mid + adjusted_side, mid - adjusted_side);
        self.bypass.apply(frame, wet)
    }

    fn reset(&mut self, controls: &MasterStripControls) {
        self.side_hpf.reset();
        self.width.reset(controls.image_width.load());
        self.bypass.reset(
            controls.image_bypass.load(Ordering::Acquire)
                || controls.compare.load(Ordering::Acquire),
        );
    }
}

#[derive(Clone)]
struct TruePeakInterpolator {
    factor: usize,
    kernels: [[f32; INTERPOLATOR_TAPS]; OVERSAMPLE_FACTOR],
    history: [StereoFrame; INTERPOLATOR_TAPS],
    write: usize,
}

impl TruePeakInterpolator {
    fn new(factor: usize) -> Result<Self, String> {
        if factor != 4 && factor != 8 {
            return Err("true-peak interpolation factor must be 4 or 8".into());
        }
        let mut kernels = [[0.0; INTERPOLATOR_TAPS]; OVERSAMPLE_FACTOR];
        let center = (INTERPOLATOR_TAPS as f64 - 1.0) * 0.5;
        for (phase, kernel) in kernels.iter_mut().enumerate().take(factor) {
            let fraction = phase as f64 / factor as f64;
            let mut sum = 0.0_f64;
            for (tap, coefficient) in kernel.iter_mut().enumerate() {
                let position = tap as f64 - center + fraction;
                let sinc = if position.abs() < 1.0e-12 {
                    1.0
                } else {
                    (std::f64::consts::PI * position).sin() / (std::f64::consts::PI * position)
                };
                let window = 0.42
                    - 0.5
                        * (2.0 * std::f64::consts::PI * tap as f64
                            / (INTERPOLATOR_TAPS - 1) as f64)
                            .cos()
                    + 0.08
                        * (4.0 * std::f64::consts::PI * tap as f64
                            / (INTERPOLATOR_TAPS - 1) as f64)
                            .cos();
                *coefficient = (sinc * window) as f32;
                sum += f64::from(*coefficient);
            }
            if sum.abs() < f64::EPSILON {
                return Err("invalid true-peak interpolation kernel".into());
            }
            for coefficient in kernel {
                *coefficient = (*coefficient as f64 / sum) as f32;
            }
        }
        Ok(Self {
            factor,
            kernels,
            history: [StereoFrame::SILENCE; INTERPOLATOR_TAPS],
            write: 0,
        })
    }

    #[inline]
    fn process(&mut self, input: StereoFrame) -> [StereoFrame; OVERSAMPLE_FACTOR] {
        if self.write >= self.history.len() {
            self.reset();
        }
        self.history[self.write] = input.finite_or_silence();
        self.write += 1;
        if self.write == self.history.len() {
            self.write = 0;
        }
        let mut output = [StereoFrame::SILENCE; OVERSAMPLE_FACTOR];
        for (phase, frame) in output.iter_mut().enumerate().take(self.factor) {
            let mut left = 0.0_f32;
            let mut right = 0.0_f32;
            // Preserve tap order but split at the ring wrap. Replacing these
            // ranges with `(write + tap) % INTERPOLATOR_TAPS` made this loop
            // 2.1–2.3× slower with Rust 1.97.1/LLVM 22 on the Raspberry Pi 5.
            let first_count = INTERPOLATOR_TAPS - self.write;
            for tap in 0..first_count {
                let index = self.write + tap;
                let coefficient = self.kernels[phase][tap];
                left += self.history[index].left * coefficient;
                right += self.history[index].right * coefficient;
            }
            for tap in first_count..INTERPOLATOR_TAPS {
                let index = tap - first_count;
                let coefficient = self.kernels[phase][tap];
                left += self.history[index].left * coefficient;
                right += self.history[index].right * coefficient;
            }
            *frame = StereoFrame::new(left, right).finite_or_silence();
        }
        output
    }

    fn reset(&mut self) {
        self.history.fill(StereoFrame::SILENCE);
        self.write = 0;
    }
}

struct TruePeakLimiter {
    detector: TruePeakInterpolator,
    delay: Box<[StereoFrame]>,
    write: usize,
    lookahead_samples: usize,
    hold_samples: usize,
    hold_remaining: usize,
    release_coefficient: f32,
    gain: f32,
    attack_target: f32,
    attack_remaining: usize,
    maximum_reduction_db: f32,
    safety_clamp_count: u64,
}

impl TruePeakLimiter {
    fn new(sample_rate: u32, factor: usize) -> Result<Self, String> {
        let lookahead_samples = ((sample_rate as f32 * LOOKAHEAD_SECONDS).round() as usize).max(1);
        let delay_samples = INTERPOLATOR_DELAY_SAMPLES + lookahead_samples;
        Ok(Self {
            detector: TruePeakInterpolator::new(factor)?,
            delay: vec![StereoFrame::SILENCE; delay_samples].into_boxed_slice(),
            write: 0,
            lookahead_samples,
            hold_samples: ((sample_rate as f32 * LIMITER_HOLD_SECONDS).round() as usize).max(1),
            hold_remaining: 0,
            release_coefficient: (-1.0 / (sample_rate as f32 * LIMITER_RELEASE_SECONDS)).exp(),
            gain: 1.0,
            attack_target: 1.0,
            attack_remaining: 0,
            maximum_reduction_db: 0.0,
            safety_clamp_count: 0,
        })
    }

    #[cfg(test)]
    fn total_latency_samples(&self) -> usize {
        self.delay.len()
    }

    #[inline]
    fn process(
        &mut self,
        input: StereoFrame,
        internal_ceiling: f32,
        sample_ceiling: f32,
    ) -> (StereoFrame, f32) {
        if self.write >= self.delay.len()
            || !self.gain.is_finite()
            || !(0.0..=1.0).contains(&self.gain)
            || !self.attack_target.is_finite()
            || !(0.0..=1.0).contains(&self.attack_target)
            || self.attack_remaining > self.lookahead_samples
            || self.hold_remaining > self.hold_samples
        {
            self.reset();
        }
        let input = input.finite_or_silence();
        let delayed = self.delay[self.write].finite_or_silence();
        self.delay[self.write] = input;
        self.write += 1;
        if self.write == self.delay.len() {
            self.write = 0;
        }

        let phases = self.detector.process(input);
        let detector = phases
            .iter()
            .take(self.detector.factor)
            .fold(0.0_f32, |peak, frame| {
                peak.max(frame.left.abs()).max(frame.right.abs())
            });
        let required = if detector > internal_ceiling && detector.is_finite() {
            (internal_ceiling / detector).clamp(0.0, 1.0)
        } else {
            1.0
        };
        if required < self.attack_target {
            self.attack_target = required;
            if self.attack_remaining == 0 {
                self.attack_remaining = self.lookahead_samples;
            }
            self.hold_remaining = self.hold_samples;
        }
        if self.attack_remaining > 0 {
            let step = (self.gain - self.attack_target) / self.attack_remaining as f32;
            self.gain = (self.gain - step).max(self.attack_target);
            self.attack_remaining -= 1;
        } else if self.hold_remaining > 0 {
            self.hold_remaining -= 1;
            self.gain = self.gain.min(self.attack_target);
        } else {
            // Release only toward the gain currently required by the future
            // detector sample. This cannot outrun a sustained inter-sample
            // peak, while a genuinely quieter future releases smoothly.
            self.gain = required + self.release_coefficient * (self.gain - required);
            self.attack_target = self.gain;
        }
        self.gain = self.gain.clamp(0.0, 1.0);
        let raw = StereoFrame::new(delayed.left * self.gain, delayed.right * self.gain)
            .finite_or_silence();
        if raw.left.abs() > sample_ceiling || raw.right.abs() > sample_ceiling {
            self.safety_clamp_count = self.safety_clamp_count.saturating_add(1);
        }
        let output = StereoFrame::new(
            raw.left.clamp(-sample_ceiling, sample_ceiling),
            raw.right.clamp(-sample_ceiling, sample_ceiling),
        );
        let reduction = if self.gain > 0.0 {
            (-20.0 * self.gain.log10()).clamp(0.0, 160.0)
        } else {
            160.0
        };
        self.maximum_reduction_db = self.maximum_reduction_db.max(reduction);
        (output, reduction)
    }

    fn begin_block(&mut self) {
        self.maximum_reduction_db = 0.0;
    }

    fn reset(&mut self) {
        self.detector.reset();
        self.delay.fill(StereoFrame::SILENCE);
        self.write = 0;
        self.hold_remaining = 0;
        self.gain = 1.0;
        self.attack_target = 1.0;
        self.attack_remaining = 0;
        self.maximum_reduction_db = 0.0;
        self.safety_clamp_count = 0;
    }
}

#[derive(Clone, Copy)]
struct KWeighting {
    shelf: StereoBiquad,
    highpass: StereoBiquad,
}

impl KWeighting {
    fn new(sample_rate: u32) -> Result<Self, String> {
        let rate = sample_rate as f64;
        // Bilinear designs matching the BS.1770 48 kHz reference response.
        let shelf = high_shelf_from_analog(1_681.974_450_955_533, 3.999_843_853_973_347, rate)?;
        let highpass = high_pass_from_analog(38.135_470_876_024_44, 0.500_327_037_323_877_3, rate)?;
        Ok(Self {
            shelf: StereoBiquad::new(shelf),
            highpass: StereoBiquad::new(highpass),
        })
    }

    #[inline]
    fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        self.highpass.process(self.shelf.process(frame))
    }

    fn reset(&mut self) {
        self.shelf.reset();
        self.highpass.reset();
    }
}

fn high_shelf_from_analog(
    frequency: f64,
    gain_db: f64,
    sample_rate: f64,
) -> Result<BiquadCoefficients, String> {
    // The RBJ shelf with S=1 is response-equivalent to the reference shelf
    // after frequency prewarping. Coefficients are prepared off callback.
    let warped = sample_rate / PI as f64 * (PI as f64 * frequency / sample_rate).tan();
    BiquadCoefficients::high_shelf(warped as f32, 1.0, gain_db as f32, sample_rate as f32)
        .map_err(|error| error.to_string())
}

fn high_pass_from_analog(
    frequency: f64,
    q: f64,
    sample_rate: f64,
) -> Result<BiquadCoefficients, String> {
    let warped = sample_rate / PI as f64 * (PI as f64 * frequency / sample_rate).tan();
    BiquadCoefficients::high_pass(warped as f32, q as f32, sample_rate as f32)
        .map_err(|error| error.to_string())
}

struct LoudnessMeter {
    weighting: KWeighting,
    subblock_samples: usize,
    subblock_position: usize,
    subblock_energy: f64,
    subblocks: [f64; 30],
    subblock_write: usize,
    subblock_count: usize,
    histogram_count: [u64; LOUDNESS_HISTOGRAM_BINS],
    histogram_energy: [f64; LOUDNESS_HISTOGRAM_BINS],
    momentary: f32,
    short_term: f32,
    integrated: f32,
    reset_generation: u32,
    integrated_update_countdown: usize,
}

impl LoudnessMeter {
    fn new(sample_rate: u32, controls: &MasterStripControls) -> Result<Self, String> {
        let subblock_samples = ((sample_rate as f32 * 0.100).round() as usize).max(1);
        Ok(Self {
            weighting: KWeighting::new(sample_rate)?,
            subblock_samples,
            subblock_position: 0,
            subblock_energy: 0.0,
            subblocks: [0.0; 30],
            subblock_write: 0,
            subblock_count: 0,
            histogram_count: [0; LOUDNESS_HISTOGRAM_BINS],
            histogram_energy: [0.0; LOUDNESS_HISTOGRAM_BINS],
            momentary: MIN_LEVEL_DB,
            short_term: MIN_LEVEL_DB,
            integrated: MIN_LEVEL_DB,
            reset_generation: controls.reset_loudness_generation.load(Ordering::Acquire),
            integrated_update_countdown: 10,
        })
    }

    #[inline]
    fn process(&mut self, frame: StereoFrame, controls: &MasterStripControls) {
        let generation = controls.reset_loudness_generation.load(Ordering::Acquire);
        if generation != self.reset_generation {
            self.reset_generation = generation;
            self.reset_measurement();
        }
        let weighted = self.weighting.process(frame);
        self.subblock_energy +=
            f64::from(weighted.left * weighted.left + weighted.right * weighted.right);
        self.subblock_position += 1;
        if self.subblock_position == self.subblock_samples {
            self.finish_subblock();
        }
    }

    fn finish_subblock(&mut self) {
        let energy = self.subblock_energy / self.subblock_samples as f64;
        self.subblocks[self.subblock_write] = energy;
        self.subblock_write = (self.subblock_write + 1) % self.subblocks.len();
        self.subblock_count = (self.subblock_count + 1).min(self.subblocks.len());
        self.subblock_position = 0;
        self.subblock_energy = 0.0;
        self.momentary = self.window_loudness(4);
        self.short_term = self.window_loudness(30);
        if self.subblock_count >= 4 {
            let block_energy = self.window_energy(4);
            let block_loudness = energy_to_loudness(block_energy);
            if block_loudness >= -70.0 {
                let tenths = (block_loudness * 10.0).round() as i32;
                let index = (tenths - LOUDNESS_HISTOGRAM_MIN_TENTHS)
                    .clamp(0, LOUDNESS_HISTOGRAM_BINS as i32 - 1)
                    as usize;
                self.histogram_count[index] = self.histogram_count[index].saturating_add(1);
                self.histogram_energy[index] += block_energy;
            }
        }
        self.integrated_update_countdown = self.integrated_update_countdown.saturating_sub(1);
        if self.integrated_update_countdown == 0 {
            self.integrated = self.calculate_integrated();
            self.integrated_update_countdown = 10;
        }
    }

    fn window_energy(&self, wanted: usize) -> f64 {
        let count = wanted.min(self.subblock_count);
        if count == 0 {
            return 0.0;
        }
        let sum = (0..count)
            .map(|offset| {
                let index = (self.subblock_write + self.subblocks.len() - 1 - offset)
                    % self.subblocks.len();
                self.subblocks[index]
            })
            .sum::<f64>();
        sum / count as f64
    }

    fn window_loudness(&self, wanted: usize) -> f32 {
        energy_to_loudness(self.window_energy(wanted))
    }

    fn calculate_integrated(&self) -> f32 {
        let (absolute_sum, absolute_count) = self
            .histogram_energy
            .iter()
            .zip(self.histogram_count)
            .fold((0.0_f64, 0_u64), |(energy, count), (bin, bin_count)| {
                (energy + *bin, count.saturating_add(bin_count))
            });
        if absolute_count == 0 {
            return MIN_LEVEL_DB;
        }
        let absolute_loudness = energy_to_loudness(absolute_sum / absolute_count as f64);
        let relative_gate = (absolute_loudness - 10.0).max(-70.0);
        let threshold_index = ((relative_gate * 10.0).ceil() as i32 - LOUDNESS_HISTOGRAM_MIN_TENTHS)
            .clamp(0, LOUDNESS_HISTOGRAM_BINS as i32 - 1) as usize;
        let (sum, count) = self.histogram_energy[threshold_index..]
            .iter()
            .zip(&self.histogram_count[threshold_index..])
            .fold((0.0_f64, 0_u64), |(energy, count), (bin, bin_count)| {
                (energy + *bin, count.saturating_add(*bin_count))
            });
        if count == 0 {
            MIN_LEVEL_DB
        } else {
            energy_to_loudness(sum / count as f64)
        }
    }

    fn reset_measurement(&mut self) {
        self.subblocks.fill(0.0);
        self.subblock_write = 0;
        self.subblock_count = 0;
        self.subblock_position = 0;
        self.subblock_energy = 0.0;
        self.histogram_count.fill(0);
        self.histogram_energy.fill(0.0);
        self.momentary = MIN_LEVEL_DB;
        self.short_term = MIN_LEVEL_DB;
        self.integrated = MIN_LEVEL_DB;
        self.integrated_update_countdown = 10;
        self.weighting.reset();
    }
}

fn energy_to_loudness(energy: f64) -> f32 {
    if energy.is_finite() && energy > 0.0 {
        (-0.691 + 10.0 * energy.log10()) as f32
    } else {
        MIN_LEVEL_DB
    }
}

pub struct MasterStripProcessor {
    controls: Arc<MasterStripControls>,
    meters: Arc<MasterStripMeters>,
    input: InputSection,
    tone: ToneSection,
    glue: GlueSection,
    color: ColorSection,
    image: ImageSection,
    loud: SmoothedParameter,
    limiter: TruePeakLimiter,
    output_true_peak: TruePeakInterpolator,
    input_meter: MeterAccumulator,
    output_meter: MeterAccumulator,
    loudness: LoudnessMeter,
}

impl MasterStripProcessor {
    pub fn new(
        sample_rate: u32,
        maximum_frames: usize,
        controls: Arc<MasterStripControls>,
        meters: Arc<MasterStripMeters>,
    ) -> Result<Self, String> {
        Ok(Self {
            input: InputSection::new(&controls, sample_rate),
            tone: ToneSection::new(&controls, sample_rate),
            glue: GlueSection::new(&controls, sample_rate),
            color: ColorSection::new(&controls, sample_rate)?,
            image: ImageSection::new(&controls, sample_rate),
            loud: SmoothedParameter::new(
                if controls.compare.load(Ordering::Acquire) {
                    1.0
                } else {
                    controls.loud_gain.load()
                },
                sample_rate,
            ),
            limiter: TruePeakLimiter::new(sample_rate, OVERSAMPLE_FACTOR)?,
            output_true_peak: TruePeakInterpolator::new(OVERSAMPLE_FACTOR)?,
            input_meter: MeterAccumulator::new(maximum_frames)
                .map_err(|error| error.to_string())?,
            output_meter: MeterAccumulator::new(maximum_frames)
                .map_err(|error| error.to_string())?,
            loudness: LoudnessMeter::new(sample_rate, &controls)?,
            controls,
            meters,
        })
    }

    #[cfg(test)]
    pub fn latency_samples(&self) -> usize {
        COLOR_ALIGNMENT_SAMPLES + self.limiter.total_latency_samples()
    }

    #[cfg(test)]
    pub fn lookahead_samples(&self) -> usize {
        self.limiter.lookahead_samples
    }

    #[cfg(test)]
    pub fn safety_clamp_count(&self) -> u64 {
        self.limiter.safety_clamp_count
    }

    #[inline]
    pub fn process(&mut self, frames: &mut [StereoFrame]) {
        let compare = self.controls.compare.load(Ordering::Acquire);
        self.input.refresh(&self.controls, compare);
        self.tone.refresh(&self.controls, compare);
        self.glue.refresh(&self.controls, compare);
        self.color.refresh(&self.controls, compare);
        self.image.refresh(&self.controls, compare);
        self.loud.refresh(if compare {
            1.0
        } else {
            self.controls.loud_gain.load().clamp(1.0, 2.0)
        });
        self.limiter.begin_block();
        let internal_ceiling = self.controls.internal_ceiling_gain.load().clamp(0.0, 1.0);
        let sample_ceiling = self.controls.sample_ceiling_gain.load().clamp(0.0, 1.0);
        let mut true_peak = 0.0_f32;
        let mut sum_lr = 0.0_f64;
        let mut sum_l2 = 0.0_f64;
        let mut sum_r2 = 0.0_f64;
        for frame in frames.iter_mut() {
            let input = self.input_meter.process(frame.finite_or_silence());
            let mut output = self.input.process(input);
            output = self.tone.process(output);
            output = self.glue.process(output);
            output = self.color.process(output);
            output = self.image.process(output);
            let loud = self.loud.next();
            output = StereoFrame::new(output.left * loud, output.right * loud).finite_or_silence();
            output = self
                .limiter
                .process(output, internal_ceiling, sample_ceiling)
                .0;
            let phases = self.output_true_peak.process(output);
            for phase in phases.iter().take(OVERSAMPLE_FACTOR) {
                true_peak = true_peak.max(phase.left.abs()).max(phase.right.abs());
            }
            output = self.output_meter.process(output);
            self.loudness.process(output, &self.controls);
            sum_lr += f64::from(output.left) * f64::from(output.right);
            sum_l2 += f64::from(output.left) * f64::from(output.left);
            sum_r2 += f64::from(output.right) * f64::from(output.right);
            *frame = output;
        }
        let correlation = if sum_l2 > 0.0 && sum_r2 > 0.0 {
            (sum_lr / (sum_l2 * sum_r2).sqrt()).clamp(-1.0, 1.0) as f32
        } else {
            1.0
        };
        self.meters
            .input
            .publish(self.input_meter.snapshot_and_clear_peak());
        self.meters
            .output
            .publish(self.output_meter.snapshot_and_clear_peak());
        self.meters.true_peak.store(if true_peak > 0.0 {
            20.0 * true_peak.log10()
        } else {
            MIN_LEVEL_DB
        });
        self.meters
            .glue_reduction
            .store(self.glue.maximum_reduction_db);
        self.meters
            .limiter_reduction
            .store(self.limiter.maximum_reduction_db);
        self.meters.correlation.store(correlation);
        self.meters.loudness_m.store(self.loudness.momentary);
        self.meters.loudness_s.store(self.loudness.short_term);
        self.meters.loudness_i.store(self.loudness.integrated);
    }

    pub fn reset(&mut self) {
        self.input.reset(&self.controls);
        self.tone.reset(&self.controls);
        self.glue.reset(&self.controls);
        self.color.reset(&self.controls);
        self.image.reset(&self.controls);
        self.loud
            .reset(if self.controls.compare.load(Ordering::Acquire) {
                1.0
            } else {
                self.controls.loud_gain.load()
            });
        self.limiter.reset();
        self.output_true_peak.reset();
        self.input_meter.reset();
        self.output_meter.reset();
        self.loudness.reset_measurement();
        self.meters.clear();
    }
}

fn smoothing_samples(sample_rate: u32, seconds: f32) -> u32 {
    ((sample_rate as f32 * seconds).round() as u32).max(1)
}

#[derive(Clone, Copy, Debug)]
pub struct BenchmarkStats {
    pub mean_microseconds: f64,
    pub p95_microseconds: f64,
    pub p99_microseconds: f64,
    pub maximum_microseconds: f64,
    pub mean_deadline_percent: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct MasterStripBenchmarkRow {
    pub callback_frames: usize,
    pub neutral: BenchmarkStats,
    pub active: BenchmarkStats,
}

#[derive(Clone, Copy, Debug)]
pub struct OversamplingBenchmarkRow {
    pub factor: usize,
    pub stats: BenchmarkStats,
}

pub struct MasterStripBenchmarkReport {
    pub sample_rate: u32,
    pub callbacks: usize,
    pub total_latency_samples: usize,
    pub processor_state_bytes: usize,
    pub limiter_delay_bytes: usize,
    pub rows: [MasterStripBenchmarkRow; 2],
    pub oversampling: [OversamplingBenchmarkRow; 2],
}

/// Hardware-independent release-mode comparison used by the maintainer
/// measurement path. It performs no I/O, audio-device access, or pacing.
pub fn benchmark(sample_rate: u32, callbacks: usize) -> Result<MasterStripBenchmarkReport, String> {
    if callbacks < 1_000 {
        return Err("MASTER STRIP benchmark requires at least 1000 callbacks".into());
    }
    let profile_row = |callback_frames| -> Result<MasterStripBenchmarkRow, String> {
        let neutral = benchmark_profile(
            sample_rate,
            callback_frames,
            callbacks,
            &MasterStripSettings::default(),
        )?;
        let active = MasterStripSettings {
            input_bypass: false,
            input_trim_db: 12.0,
            input_hpf: HpfFrequency::Hz40,
            tone_bypass: false,
            low_shelf_frequency: LowShelfFrequency::Hz90,
            low_shelf_db: 6.0,
            high_shelf_frequency: HighShelfFrequency::Hz8000,
            high_shelf_db: 6.0,
            glue_bypass: false,
            glue_threshold_db: -30.0,
            glue_ratio: GlueRatio::Ratio4,
            glue_attack: GlueAttack::Ms10,
            glue_release: GlueRelease::Ms100,
            glue_sidechain_hpf: GlueSidechainHpf::Hz120,
            glue_mix_percent: 100.0,
            glue_makeup_db: 6.0,
            color_bypass: false,
            color_drive_db: 12.0,
            color_character_percent: 100.0,
            color_mix_percent: 100.0,
            color_trim_db: 0.0,
            image_bypass: false,
            image_width_percent: 150.0,
            image_side_hpf: ImageSideHpf::Hz250,
            loud_db: 6.0,
            ceiling_dbtp: -0.5,
            ..MasterStripSettings::default()
        };
        active.validate()?;
        let active = benchmark_profile(sample_rate, callback_frames, callbacks, &active)?;
        Ok(MasterStripBenchmarkRow {
            callback_frames,
            neutral,
            active,
        })
    };
    let rows = [profile_row(64)?, profile_row(128)?];
    let oversampling_row = |factor| -> Result<OversamplingBenchmarkRow, String> {
        Ok(OversamplingBenchmarkRow {
            factor,
            stats: benchmark_interpolator(sample_rate, 128, callbacks, factor)?,
        })
    };
    Ok(MasterStripBenchmarkReport {
        sample_rate,
        callbacks,
        total_latency_samples: COLOR_ALIGNMENT_SAMPLES
            + INTERPOLATOR_DELAY_SAMPLES
            + (sample_rate as f32 * LOOKAHEAD_SECONDS).round() as usize,
        processor_state_bytes: std::mem::size_of::<MasterStripProcessor>(),
        limiter_delay_bytes: (INTERPOLATOR_DELAY_SAMPLES
            + (sample_rate as f32 * LOOKAHEAD_SECONDS).round() as usize)
            * std::mem::size_of::<StereoFrame>(),
        rows,
        oversampling: [oversampling_row(4)?, oversampling_row(8)?],
    })
}

fn benchmark_profile(
    sample_rate: u32,
    callback_frames: usize,
    callbacks: usize,
    settings: &MasterStripSettings,
) -> Result<BenchmarkStats, String> {
    let controls = Arc::new(MasterStripControls::new(sample_rate, settings)?);
    let meters = Arc::new(MasterStripMeters::default());
    let mut processor = MasterStripProcessor::new(sample_rate, callback_frames, controls, meters)?;
    let source = benchmark_signal(sample_rate, callback_frames);
    let mut work = source.clone();
    for _ in 0..1_000 {
        work.copy_from_slice(&source);
        processor.process(black_box(&mut work));
    }
    let mut durations = Vec::with_capacity(callbacks);
    for _ in 0..callbacks {
        work.copy_from_slice(&source);
        let started = Instant::now();
        processor.process(black_box(&mut work));
        durations.push(started.elapsed().as_nanos() as u64);
    }
    Ok(summarize_benchmark(durations, sample_rate, callback_frames))
}

fn benchmark_interpolator(
    sample_rate: u32,
    callback_frames: usize,
    callbacks: usize,
    factor: usize,
) -> Result<BenchmarkStats, String> {
    let mut interpolator = TruePeakInterpolator::new(factor)?;
    let source = benchmark_signal(sample_rate, callback_frames);
    for _ in 0..1_000 {
        for frame in &source {
            black_box(interpolator.process(*frame));
        }
    }
    let mut durations = Vec::with_capacity(callbacks);
    for _ in 0..callbacks {
        let started = Instant::now();
        for frame in &source {
            black_box(interpolator.process(*frame));
        }
        durations.push(started.elapsed().as_nanos() as u64);
    }
    Ok(summarize_benchmark(durations, sample_rate, callback_frames))
}

fn benchmark_signal(sample_rate: u32, frames: usize) -> Vec<StereoFrame> {
    (0..frames)
        .map(|index| {
            let time = index as f32 / sample_rate as f32;
            let left = (2.0 * PI * 997.0 * time + 0.31).sin() * 0.47
                + (2.0 * PI * 15_911.0 * time + 1.13).sin() * 0.19;
            let right = (2.0 * PI * 997.0 * time - 0.23).sin() * 0.41
                + (2.0 * PI * 17_003.0 * time + 0.71).sin() * 0.23;
            StereoFrame::new(left, right)
        })
        .collect()
}

fn summarize_benchmark(
    mut durations: Vec<u64>,
    sample_rate: u32,
    callback_frames: usize,
) -> BenchmarkStats {
    durations.sort_unstable();
    let percentile = |percent: usize| {
        let index = ((durations.len() - 1) * percent) / 100;
        durations[index] as f64 / 1_000.0
    };
    let mean_nanoseconds =
        durations.iter().map(|value| *value as f64).sum::<f64>() / durations.len() as f64;
    let deadline_nanoseconds = callback_frames as f64 / sample_rate as f64 * 1_000_000_000.0;
    BenchmarkStats {
        mean_microseconds: mean_nanoseconds / 1_000.0,
        p95_microseconds: percentile(95),
        p99_microseconds: percentile(99),
        maximum_microseconds: durations.last().copied().unwrap_or_default() as f64 / 1_000.0,
        mean_deadline_percent: mean_nanoseconds / deadline_nanoseconds * 100.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::allocation_test::assert_no_allocations;
    use crate::dsp::analysis::{
        coherent_sine, harmonic_alias_ratio, maximum_step, mean, rms, spectral_amplitude,
    };

    fn processor(
        rate: u32,
        frames: usize,
        settings: &MasterStripSettings,
    ) -> (
        MasterStripProcessor,
        Arc<MasterStripControls>,
        Arc<MasterStripMeters>,
    ) {
        let controls = Arc::new(MasterStripControls::new(rate, settings).unwrap());
        let meters = Arc::new(MasterStripMeters::default());
        (
            MasterStripProcessor::new(rate, frames, Arc::clone(&controls), Arc::clone(&meters))
                .unwrap(),
            controls,
            meters,
        )
    }

    fn run(
        rate: u32,
        settings: &MasterStripSettings,
        input: &[StereoFrame],
        chunk: usize,
    ) -> (Vec<StereoFrame>, MasterStripMeterSnapshot, u64) {
        let (mut processor, _, meters) = processor(rate, chunk.max(1), settings);
        let mut output = input.to_vec();
        for frames in output.chunks_mut(chunk) {
            processor.process(frames);
        }
        (output, meters.snapshot(), processor.safety_clamp_count())
    }

    #[test]
    fn settings_are_strict_bounded_and_neutral_by_default() {
        let settings = MasterStripSettings::default();
        settings.validate().unwrap();
        let encoded = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            serde_json::from_str::<MasterStripSettings>(&encoded).unwrap(),
            settings
        );
        assert!(
            serde_json::from_str::<MasterStripSettings>(&encoded.replacen(
                '{',
                "{\"unknown\":1,",
                1
            ))
            .is_err()
        );
        let mut invalid = settings;
        invalid.loud_db = f32::NAN;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn coefficient_publication_is_bounded_and_never_returns_a_torn_set() {
        let first = BiquadCoefficients {
            b0: 0.11,
            b1: 0.12,
            b2: 0.13,
            a1: 0.14,
            a2: 0.15,
        };
        let second = BiquadCoefficients {
            b0: 0.21,
            b1: 0.22,
            b2: 0.23,
            a1: 0.24,
            a2: 0.25,
        };
        let source = Arc::new(AtomicCoefficients::new(BiquadCoefficients::IDENTITY));
        let writer_source = Arc::clone(&source);
        let writer = std::thread::spawn(move || {
            for index in 0..50_000 {
                writer_source.store(if index & 1 == 0 { first } else { second });
            }
        });
        for _ in 0..100_000 {
            if let Some((revision, coefficients)) = source.load() {
                assert_eq!(revision & 1, 0);
                assert!(
                    coefficients == BiquadCoefficients::IDENTITY
                        || coefficients == first
                        || coefficients == second
                );
            }
        }
        writer.join().unwrap();
        assert!(source.load().is_some());
    }

    #[test]
    fn graph_sample_rate_contract_and_latency_are_explicit() {
        let settings = MasterStripSettings::default();
        for rate in [
            8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400,
            192_000, 352_800, 384_000,
        ] {
            let (processor, _, _) = processor(rate, 64, &settings);
            assert_eq!(
                processor.latency_samples(),
                COLOR_ALIGNMENT_SAMPLES
                    + INTERPOLATOR_DELAY_SAMPLES
                    + (rate as f32 * LOOKAHEAD_SECONDS).round() as usize
            );
        }
        assert!(MasterStripControls::new(7_999, &settings).is_err());
        assert!(MasterStripControls::new(384_001, &settings).is_err());
    }

    #[test]
    fn neutral_strip_is_exact_delayed_reconstruction() {
        let settings = MasterStripSettings::default();
        let input = (0..4096)
            .map(|index| {
                StereoFrame::new(
                    ((index * 37 % 101) as f32 - 50.0) * 0.001,
                    ((index * 53 % 97) as f32 - 48.0) * 0.001,
                )
            })
            .collect::<Vec<_>>();
        let (output, _, clamp_count) = run(48_000, &settings, &input, 127);
        let (processor, _, _) = processor(48_000, 128, &settings);
        let latency = processor.latency_samples();
        assert_eq!(latency, 133);
        assert_eq!(processor.lookahead_samples(), 120);
        for index in latency..input.len() {
            assert_eq!(output[index], input[index - latency]);
        }
        assert_eq!(clamp_count, 0);
    }

    #[test]
    fn whole_strip_comparison_is_latency_matched_and_keeps_the_limiter() {
        let mut settings = MasterStripSettings {
            compare: true,
            input_bypass: false,
            input_trim_db: 12.0,
            tone_bypass: false,
            low_shelf_db: 6.0,
            glue_bypass: false,
            glue_threshold_db: -30.0,
            color_bypass: false,
            color_drive_db: 12.0,
            image_bypass: false,
            image_width_percent: 150.0,
            loud_db: 6.0,
            ..MasterStripSettings::default()
        };
        let input = (0..4_096)
            .map(|index| {
                StereoFrame::new(
                    ((index * 37 % 101) as f32 - 50.0) * 0.001,
                    ((index * 53 % 97) as f32 - 48.0) * 0.001,
                )
            })
            .collect::<Vec<_>>();
        let (compared, _, _) = run(48_000, &settings, &input, 91);
        for index in 133..input.len() {
            assert_eq!(compared[index], input[index - 133]);
        }

        settings.loud_db = 6.0;
        let hot = vec![StereoFrame::new(1.5, -1.5); 4_096];
        let (protected, meter, _) = run(48_000, &settings, &hot, 91);
        let ceiling = db_to_gain(settings.ceiling_dbtp).unwrap() + 1e-6;
        assert!(protected
            .iter()
            .all(|frame| frame.left.abs() <= ceiling && frame.right.abs() <= ceiling));
        assert!(meter.limiter_gain_reduction_db > 0.0);
    }

    #[test]
    fn hpf_and_broad_shelves_have_expected_response() {
        let measure = |settings: &MasterStripSettings, frequency: usize| {
            let samples = coherent_sine(48_000, frequency, 0.1);
            let frames = samples
                .into_iter()
                .map(|sample| StereoFrame::new(sample, sample))
                .collect::<Vec<_>>();
            let (output, _, _) = run(48_000, settings, &frames, 191);
            let latency = 133;
            rms(&output[latency + 4096..]
                .iter()
                .map(|frame| frame.left)
                .collect::<Vec<_>>())
        };
        let hpf = MasterStripSettings {
            input_bypass: false,
            input_hpf: HpfFrequency::Hz40,
            ..MasterStripSettings::default()
        };
        let cutoff = measure(&hpf, 40);
        let pass = measure(&hpf, 400);
        assert!((20.0 * (cutoff / pass).log10() + 3.01).abs() < 0.2);

        let tone = MasterStripSettings {
            tone_bypass: false,
            low_shelf_frequency: LowShelfFrequency::Hz50,
            low_shelf_db: 6.0,
            ..MasterStripSettings::default()
        };
        let boosted = measure(&tone, 10);
        let neutral = measure(&MasterStripSettings::default(), 10);
        let shelf_gain = 20.0 * (boosted / neutral).log10();
        assert!((shelf_gain - 6.0).abs() < 0.25, "{shelf_gain:.3} dB");
    }

    #[test]
    fn glue_curve_timing_stereo_link_sidechain_and_bypass() {
        assert!((glue_curve_gain_db(0.0, -20.0, 4.0, 0.0) + 15.0).abs() < 1e-6);
        assert!((glue_curve_gain_db(-20.0, -20.0, 4.0, 6.0) + 0.5625).abs() < 1e-6);
        let mut settings = MasterStripSettings {
            glue_bypass: false,
            glue_threshold_db: -30.0,
            glue_ratio: GlueRatio::Ratio4,
            glue_attack: GlueAttack::Ms10,
            ..MasterStripSettings::default()
        };
        let frames = vec![StereoFrame::new(0.5, 0.05); 12_000];
        let (output, meter, _) = run(48_000, &settings, &frames, 113);
        assert!(meter.glue_gain_reduction_db > 10.0);
        let settled = output.last().unwrap();
        assert!((settled.right / settled.left - 0.1).abs() < 1e-4);

        let mut bypassed = settings.clone();
        bypassed.glue_bypass = true;
        let (dry, _, _) = run(48_000, &bypassed, &frames, 113);
        assert_eq!(dry[133], frames[0]);

        let bass = (0..48_000)
            .map(|index| {
                let sample = (2.0 * PI * 30.0 * index as f32 / 48_000.0).sin() * 0.8;
                StereoFrame::new(sample, sample)
            })
            .collect::<Vec<_>>();
        let (_, off_meter, _) = run(48_000, &settings, &bass, 256);
        settings.glue_sidechain_hpf = GlueSidechainHpf::Hz120;
        let (_, filtered_meter, _) = run(48_000, &settings, &bass, 256);
        assert!(filtered_meter.glue_gain_reduction_db + 3.0 < off_meter.glue_gain_reduction_db);
    }

    #[test]
    fn color_harmonics_dc_alias_level_and_bypass_are_bounded() {
        let settings = MasterStripSettings {
            color_bypass: false,
            color_drive_db: 9.0,
            color_character_percent: 60.0,
            ..MasterStripSettings::default()
        };
        let input = (0..52_229)
            .map(|index| {
                let sample = (2.0 * PI * 1_000.0 * index as f32 / 48_000.0).sin() * 0.35;
                StereoFrame::new(sample, sample)
            })
            .collect::<Vec<_>>();
        let (output, _, _) = run(48_000, &settings, &input, 127);
        let settled = output[133 + 4096..133 + 4096 + 48_000]
            .iter()
            .map(|frame| frame.left)
            .collect::<Vec<_>>();
        let fundamental = spectral_amplitude(&settled, 1_000);
        let second = spectral_amplitude(&settled, 2_000);
        let third = spectral_amplitude(&settled, 3_000);
        assert!(second > fundamental * 0.005);
        assert!(third > fundamental * 0.005);
        assert!(mean(&settled).abs() < 0.001);
        assert!(settled.iter().all(|sample| sample.is_finite()));

        let high = coherent_sine(48_000, 10_000, 0.35);
        let mut previous = None;
        let adaa = high
            .iter()
            .map(|sample| {
                let value = color_adaa(*sample * 2.0, previous, 0.0) * 0.5;
                previous = Some(*sample * 2.0);
                value
            })
            .collect::<Vec<_>>();
        let naive = high
            .iter()
            .map(|sample| color_transfer(*sample * 2.0, 0.0) * 0.5)
            .collect::<Vec<_>>();
        let adaa_alias = harmonic_alias_ratio(&adaa, 10_000, 15);
        let naive_alias = harmonic_alias_ratio(&naive, 10_000, 15);
        assert!(adaa_alias.is_finite() && adaa_alias < naive_alias);

        let two_tone = (0..48_000)
            .map(|index| {
                let phase = index as f32 * 2.0 * PI / 48_000.0;
                ((phase * 18_000.0).sin() + (phase * 19_000.0).sin()) * 0.2
            })
            .collect::<Vec<_>>();
        let mut previous = None;
        let processed = two_tone
            .iter()
            .map(|sample| {
                let value = color_adaa(*sample * 2.0, previous, 0.5) * 0.5;
                previous = Some(*sample * 2.0);
                value
            })
            .collect::<Vec<_>>();
        let carrier =
            spectral_amplitude(&processed, 18_000).max(spectral_amplitude(&processed, 19_000));
        let difference_product = spectral_amplitude(&processed, 1_000);
        assert!(difference_product.is_finite() && difference_product < carrier * 0.25);

        let transfer = (-2_000..=2_000)
            .map(|step| color_transfer(step as f32 * 0.001, 0.6))
            .collect::<Vec<_>>();
        assert!(maximum_step(&transfer) < 0.002);

        let low_level = coherent_sine(48_000, 1_000, 0.01)
            .into_iter()
            .map(|sample| StereoFrame::new(sample, sample))
            .collect::<Vec<_>>();
        let compensated = |drive_db| {
            let settings = MasterStripSettings {
                color_bypass: false,
                color_drive_db: drive_db,
                color_character_percent: 0.0,
                ..MasterStripSettings::default()
            };
            let (output, _, _) = run(48_000, &settings, &low_level, 127);
            rms(&output[4_229..]
                .iter()
                .map(|frame| frame.left)
                .collect::<Vec<_>>())
        };
        let level_delta_db = 20.0 * (compensated(12.0) / compensated(0.0)).log10();
        assert!(level_delta_db.abs() < 0.1, "{level_delta_db:.3} dB");

        let mut bypassed = settings;
        bypassed.color_bypass = true;
        bypassed.color_drive_db = 12.0;
        bypassed.color_character_percent = 100.0;
        let (dry, _, _) = run(48_000, &bypassed, &input, 127);
        for index in 133..input.len() {
            assert_eq!(dry[index], input[index - 133]);
        }
    }

    #[test]
    fn image_unity_width_correlation_and_mono_are_conservative() {
        let mut settings = MasterStripSettings {
            image_bypass: false,
            ..MasterStripSettings::default()
        };
        let input = (0..4096)
            .map(|index| {
                let phase = index as f32 * 2.0 * PI * 1000.0 / 48_000.0;
                StereoFrame::new(phase.sin() * 0.2, phase.cos() * 0.2)
            })
            .collect::<Vec<_>>();
        let (unity, _, _) = run(48_000, &settings, &input, 128);
        for index in 133..input.len() {
            assert!((unity[index].left - input[index - 133].left).abs() < 1e-7);
            assert!((unity[index].right - input[index - 133].right).abs() < 1e-7);
        }
        settings.image_width_percent = 150.0;
        let (wide, meter, _) = run(48_000, &settings, &input, 128);
        assert!(meter.correlation < 0.1);
        for index in 512..input.len() {
            let unity_mono = unity[index].left + unity[index].right;
            let wide_mono = wide[index].left + wide[index].right;
            assert!((unity_mono - wide_mono).abs() < 1e-5);
        }
    }

    #[test]
    fn true_peak_limiter_is_linked_shaped_chunk_invariant_and_ceiling_safe() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let settings = MasterStripSettings {
                loud_db: 6.0,
                ceiling_dbtp: -1.0,
                ..MasterStripSettings::default()
            };
            let input = (0..rate as usize / 2)
                .map(|index| {
                    let phase = (index as f32 + 0.37) * 2.0 * PI * 0.49;
                    StereoFrame::new(phase.sin() * 0.9, phase.sin() * 0.45)
                })
                .collect::<Vec<_>>();
            let (a, meter, clamp_count) = run(rate, &settings, &input, 64);
            let (b, _, _) = run(rate, &settings, &input, 127);
            assert_eq!(a, b);
            assert!(meter.output_true_peak_dbtp <= settings.ceiling_dbtp + 0.05);
            assert!(meter.limiter_gain_reduction_db > 0.0);
            assert_eq!(clamp_count, 0);
            let linked = a
                .iter()
                .skip(512)
                .find(|frame| frame.left.abs() > 1e-4)
                .unwrap();
            assert!((linked.right / linked.left - 0.5).abs() < 1e-3);
        }
    }

    #[test]
    fn true_peak_limiter_handles_impulses_bursts_and_adversarial_phases() {
        for rate in [44_100, 48_000] {
            for phase_offset in [0.0_f32, 0.17, 0.37, 0.73] {
                let settings = MasterStripSettings {
                    loud_db: 6.0,
                    ceiling_dbtp: -1.0,
                    ..MasterStripSettings::default()
                };
                let mut input = (0..rate as usize)
                    .map(|index| {
                        let phase = (index as f32 + phase_offset) * 2.0 * PI * rate as f32 * 0.49
                            / rate as f32;
                        let burst = if (rate as usize / 4..rate as usize / 2).contains(&index) {
                            1.0
                        } else {
                            0.25
                        };
                        StereoFrame::new(phase.sin() * burst, phase.cos() * burst * 0.8)
                    })
                    .collect::<Vec<_>>();
                input[rate as usize / 8] = StereoFrame::new(1.8, -1.4);
                let (output, _, clamp_count) = run(rate, &settings, &input, 73);
                let mut verifier = TruePeakInterpolator::new(OVERSAMPLE_FACTOR).unwrap();
                let measured = output.iter().fold(0.0_f32, |peak, frame| {
                    verifier
                        .process(*frame)
                        .iter()
                        .take(OVERSAMPLE_FACTOR)
                        .fold(peak, |peak, phase| {
                            peak.max(phase.left.abs()).max(phase.right.abs())
                        })
                });
                let allowed = db_to_gain(settings.ceiling_dbtp + TRUE_PEAK_TOLERANCE_DB).unwrap();
                assert!(
                    measured <= allowed,
                    "{rate} Hz phase {phase_offset}: {} dBTP",
                    20.0 * measured.log10()
                );
                let sample_ceiling = db_to_gain(settings.ceiling_dbtp).unwrap() + 1e-6;
                assert!(output.iter().all(|frame| {
                    frame.left.abs() <= sample_ceiling && frame.right.abs() <= sample_ceiling
                }));
                assert_eq!(clamp_count, 0);
            }
        }
    }

    #[test]
    fn shaped_attack_reduces_before_delayed_impulse_without_a_gain_step() {
        let settings = MasterStripSettings::default();
        let (mut limiter, _, _) = processor(48_000, 256, &settings);
        let latency = limiter.latency_samples();
        let impulse = 1_024;
        let mut frames = vec![StereoFrame::new(0.5, 0.5); latency + impulse + 6_000];
        frames[impulse] = StereoFrame::new(2.0, 2.0);
        limiter.process(&mut frames);
        let event = impulse + latency;
        assert!(frames[event].left.abs() <= db_to_gain(-1.0).unwrap());
        let ramp = &frames[event - limiter.lookahead_samples()..event];
        assert!(ramp
            .windows(2)
            .any(|window| window[1].left < window[0].left));
        assert!(ramp
            .windows(2)
            .all(|window| (window[1].left - window[0].left).abs() < 0.05));

        let after_hold = frames[event + 64].left.abs();
        let during_release = frames[event + 1_000].left.abs();
        let late_release = frames[event + 4_000].left.abs();
        assert!(after_hold <= during_release && during_release < late_release);
    }

    #[test]
    fn loudness_windows_integrated_reset_and_true_peak_labels_are_valid() {
        let settings = MasterStripSettings::default();
        let (mut processor, controls, meters) = processor(48_000, 256, &settings);
        let amplitude = db_to_gain(-18.0).unwrap();
        for offset in (0..48_000 * 4).step_by(256) {
            let mut block = [StereoFrame::SILENCE; 256];
            for (index, frame) in block.iter_mut().enumerate() {
                let phase = (offset + index) as f32 * 2.0 * PI * 1000.0 / 48_000.0;
                *frame = StereoFrame::new(phase.sin() * amplitude, phase.sin() * amplitude);
            }
            processor.process(&mut block);
        }
        let snapshot = meters.snapshot();
        assert!(
            (snapshot.loudness_m_lufs + 18.0).abs() < 0.3,
            "{snapshot:?}"
        );
        assert!(
            (snapshot.loudness_s_lufs + 18.0).abs() < 0.3,
            "{snapshot:?}"
        );
        assert!(
            (snapshot.loudness_i_lufs + 18.0).abs() < 0.3,
            "{snapshot:?}"
        );
        controls.reset_loudness();
        let mut silence = [StereoFrame::SILENCE; 256];
        processor.process(&mut silence);
        assert_eq!(meters.snapshot().loudness_i_lufs, MIN_LEVEL_DB);
    }

    #[test]
    fn parameter_movement_reset_non_finite_and_callback_are_safe() {
        let settings = MasterStripSettings::default();
        let (mut processor, controls, _) = processor(48_000, 256, &settings);
        let mut changed = settings;
        changed.input_bypass = false;
        changed.input_trim_db = 12.0;
        changed.tone_bypass = false;
        changed.low_shelf_db = 6.0;
        changed.color_bypass = false;
        changed.color_drive_db = 12.0;
        controls.apply(&changed).unwrap();
        let mut frames = [StereoFrame::new(f32::NAN, f32::INFINITY); 256];
        assert_no_allocations(|| processor.process(&mut frames));
        assert!(frames
            .iter()
            .all(|frame| frame.left.is_finite() && frame.right.is_finite()));
        processor.limiter.write = usize::MAX;
        processor.limiter.gain = f32::NAN;
        let mut recovery = [StereoFrame::new(0.1, -0.1); 256];
        processor.process(&mut recovery);
        assert!(recovery
            .iter()
            .all(|frame| frame.left.is_finite() && frame.right.is_finite()));
        processor.reset();
        let mut silence = [StereoFrame::SILENCE; 256];
        processor.process(&mut silence);
        assert!(silence.iter().all(|frame| *frame == StereoFrame::SILENCE));

        let mut audible = [StereoFrame::new(0.1, -0.1); 1_024];
        processor.process(&mut audible);
        let left = audible.iter().map(|frame| frame.left).collect::<Vec<_>>();
        assert!(maximum_step(&left) < 0.2);
    }

    #[test]
    fn split_ring_interpolation_is_bit_exact_with_modulo_reference() {
        fn reference_process(
            interpolator: &mut TruePeakInterpolator,
            input: StereoFrame,
        ) -> [StereoFrame; OVERSAMPLE_FACTOR] {
            interpolator.history[interpolator.write] = input.finite_or_silence();
            interpolator.write = (interpolator.write + 1) % INTERPOLATOR_TAPS;
            let mut output = [StereoFrame::SILENCE; OVERSAMPLE_FACTOR];
            for (phase, frame) in output.iter_mut().enumerate().take(interpolator.factor) {
                let mut left = 0.0_f32;
                let mut right = 0.0_f32;
                for tap in 0..INTERPOLATOR_TAPS {
                    let index = (interpolator.write + tap) % INTERPOLATOR_TAPS;
                    let coefficient = interpolator.kernels[phase][tap];
                    left += interpolator.history[index].left * coefficient;
                    right += interpolator.history[index].right * coefficient;
                }
                *frame = StereoFrame::new(left, right).finite_or_silence();
            }
            output
        }

        for factor in [4, 8] {
            let mut optimized = TruePeakInterpolator::new(factor).unwrap();
            let mut reference = optimized.clone();
            for index in 0..4_096 {
                let phase = index as f32 * 0.037;
                let input = StereoFrame::new(phase.sin() * 0.73, phase.cos() * -0.61);
                assert_eq!(
                    optimized.process(input),
                    reference_process(&mut reference, input)
                );
            }
        }
    }

    #[test]
    fn four_and_eight_times_candidates_measure_expected_theoretical_tradeoff() {
        let mut four = TruePeakInterpolator::new(4).unwrap();
        let mut eight = TruePeakInterpolator::new(8).unwrap();
        let mut four_peak = 0.0_f32;
        let mut eight_peak = 0.0_f32;
        for index in 0..8192 {
            let phase = (index as f32 + 0.5) * PI * 0.98;
            let frame = StereoFrame::new(phase.sin(), phase.sin());
            for sample in four.process(frame).iter().take(4) {
                four_peak = four_peak.max(sample.left.abs());
            }
            for sample in eight.process(frame).iter().take(8) {
                eight_peak = eight_peak.max(sample.left.abs());
            }
        }
        assert!(eight_peak >= four_peak);
        assert!(20.0 * eight_peak.log10() > -0.25);
    }
}
