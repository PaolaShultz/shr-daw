use super::{smooth, EffectError, PARAMETER_SMOOTH_SAMPLES};
use crate::audio_graph::EffectInstance;
use crate::dsp::{FractionalDelayLine, OnePole, OnePoleMode, SmoothedValue, StereoFrame};
use crate::effect_schema;

const PREDELAY_CAPACITY_MILLISECONDS: f32 = 200.0;
const FDN_CAPACITY_MILLISECONDS: f32 = 100.0;
const EMERGENCY_LEVEL: f32 = 64.0;
const INPUT_DIFFUSION_GAIN: f32 = 0.55;
const INPUT_DIFFUSION_MS: [[f32; 2]; 2] = [[4.7, 6.3], [5.3, 7.1]];
const VOICING_LENGTHS_MS: [[f32; 4]; 3] = [
    [23.3, 29.7, 31.1, 37.9],
    [37.1, 41.1, 43.7, 47.9],
    [53.1, 61.7, 67.3, 71.9],
];

struct Diffuser {
    line: FractionalDelayLine,
    delay_samples: f32,
}

impl Diffuser {
    fn new(milliseconds: f32, sample_rate: f32) -> Result<Self, EffectError> {
        let samples = (milliseconds * sample_rate / 1_000.0).round().max(2.0) as usize;
        Ok(Self {
            line: FractionalDelayLine::new(samples)?,
            delay_samples: samples as f32,
        })
    }

    /// Schroeder all-pass diffuser: H(z) = (z^-M - g) / (1 - g z^-M).
    /// The cascade follows Dattorro's input-diffusion principle; SHR's fixed
    /// line lengths and coefficient are original bounded voicing choices.
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let delayed = self.line.read(self.delay_samples);
        let output = delayed - INPUT_DIFFUSION_GAIN * input;
        let write = input + INPUT_DIFFUSION_GAIN * output;
        if !(output.is_finite() && write.is_finite())
            || output.abs() > EMERGENCY_LEVEL
            || write.abs() > EMERGENCY_LEVEL
        {
            self.reset();
            0.0
        } else {
            self.line.push(write);
            output
        }
    }

    fn reset(&mut self) {
        self.line.reset();
    }
}

pub(super) struct Reverb {
    sample_rate: f32,
    predelay_left: FractionalDelayLine,
    predelay_right: FractionalDelayLine,
    predelay_samples: SmoothedValue,
    input_low_cut_left: OnePole,
    input_low_cut_right: OnePole,
    input_diffusion_left: [Diffuser; 2],
    input_diffusion_right: [Diffuser; 2],
    lines: [FractionalDelayLine; 4],
    damping: [OnePole; 4],
    lengths: [f32; 4],
    feedback: [f32; 4],
    voicing: usize,
    decay_seconds: f32,
    size_percent: f32,
    damping_percent: f32,
    width: SmoothedValue,
    wet: SmoothedValue,
    dry: SmoothedValue,
}

impl Reverb {
    pub(super) fn compile(effect: &EffectInstance, sample_rate: u32) -> Result<Self, EffectError> {
        let value = |name| {
            effect_schema::parameter(effect, name)
                .map_err(|error| EffectError::new(error.to_string()))
        };
        let sample_rate = sample_rate as f32;
        let predelay_capacity =
            (sample_rate * PREDELAY_CAPACITY_MILLISECONDS / 1_000.0).ceil() as usize;
        let fdn_capacity = (sample_rate * FDN_CAPACITY_MILLISECONDS / 1_000.0).ceil() as usize;
        let damping_hz = damping_frequency(value("damping_percent")?);
        let line = || FractionalDelayLine::new(fdn_capacity).map_err(EffectError::from);
        let damp = || {
            OnePole::new(OnePoleMode::LowPass, damping_hz, sample_rate).map_err(EffectError::from)
        };
        let mut reverb = Self {
            sample_rate,
            predelay_left: FractionalDelayLine::new(predelay_capacity)?,
            predelay_right: FractionalDelayLine::new(predelay_capacity)?,
            predelay_samples: smooth(value("predelay_ms")? * sample_rate / 1_000.0),
            input_low_cut_left: OnePole::new(
                OnePoleMode::HighPass,
                value("input_low_cut_hz")?,
                sample_rate,
            )?,
            input_low_cut_right: OnePole::new(
                OnePoleMode::HighPass,
                value("input_low_cut_hz")?,
                sample_rate,
            )?,
            input_diffusion_left: [
                Diffuser::new(INPUT_DIFFUSION_MS[0][0], sample_rate)?,
                Diffuser::new(INPUT_DIFFUSION_MS[0][1], sample_rate)?,
            ],
            input_diffusion_right: [
                Diffuser::new(INPUT_DIFFUSION_MS[1][0], sample_rate)?,
                Diffuser::new(INPUT_DIFFUSION_MS[1][1], sample_rate)?,
            ],
            lines: [line()?, line()?, line()?, line()?],
            damping: [damp()?, damp()?, damp()?, damp()?],
            lengths: [1.0; 4],
            feedback: [0.0; 4],
            voicing: value("type")? as usize,
            decay_seconds: value("decay_seconds")?,
            size_percent: value("size_percent")?,
            damping_percent: value("damping_percent")?,
            width: smooth(value("width_percent")? * 0.01),
            wet: smooth(value("wet_percent")? * 0.01),
            dry: smooth(value("dry_percent")? * 0.01),
        };
        reverb.update_lengths_and_feedback();
        Ok(reverb)
    }

    #[inline]
    pub(super) fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        self.process_internal(frame, true)
    }

    #[inline]
    fn process_internal(&mut self, frame: StereoFrame, diffuse_input: bool) -> StereoFrame {
        let predelay = self.predelay_samples.next_value();
        let (predelayed_left, predelayed_right) = if predelay < 1.0 {
            (frame.left, frame.right)
        } else {
            (
                self.predelay_left.read(predelay),
                self.predelay_right.read(predelay),
            )
        };
        self.predelay_left.push(frame.left);
        self.predelay_right.push(frame.right);

        let mut input_left = self.input_low_cut_left.process(predelayed_left);
        let mut input_right = self.input_low_cut_right.process(predelayed_right);
        if diffuse_input {
            for diffuser in &mut self.input_diffusion_left {
                input_left = diffuser.process(input_left);
            }
            for diffuser in &mut self.input_diffusion_right {
                input_right = diffuser.process(input_right);
            }
        }
        let mono = (input_left + input_right) * 0.25;
        let side = (input_left - input_right) * 0.25;
        let delayed = [
            self.lines[0].read(self.lengths[0]),
            self.lines[1].read(self.lengths[1]),
            self.lines[2].read(self.lengths[2]),
            self.lines[3].read(self.lengths[3]),
        ];
        let mixed = [
            (delayed[0] + delayed[1] + delayed[2] + delayed[3]) * 0.5,
            (delayed[0] - delayed[1] + delayed[2] - delayed[3]) * 0.5,
            (delayed[0] + delayed[1] - delayed[2] - delayed[3]) * 0.5,
            (delayed[0] - delayed[1] - delayed[2] + delayed[3]) * 0.5,
        ];
        let injection = [mono + side, mono - side, -mono + side, mono + side];
        let mut poisoned = false;
        for index in 0..4 {
            let feedback = self.damping[index].process(mixed[index]) * self.feedback[index];
            let write = injection[index] + feedback;
            if !write.is_finite() || write.abs() > EMERGENCY_LEVEL {
                poisoned = true;
                break;
            }
            self.lines[index].push(write);
        }
        if poisoned {
            self.reset();
            return frame.finite_or_silence();
        }

        let wet_left = (delayed[0] + delayed[1] - delayed[2] - delayed[3]) * 0.5;
        let wet_right = (delayed[0] - delayed[1] + delayed[2] - delayed[3]) * 0.5;
        let mid = (wet_left + wet_right) * 0.5;
        let side = (wet_left - wet_right) * 0.5 * self.width.next_value();
        let wet = self.wet.next_value();
        let dry = self.dry.next_value();
        StereoFrame::new(
            frame.left * dry + (mid + side) * wet,
            frame.right * dry + (mid - side) * wet,
        )
        .finite_or_silence()
    }

    pub(super) fn set_parameter(&mut self, name: &str, value: f32) -> Result<(), EffectError> {
        match name {
            "type" => {
                self.voicing = value as usize;
                self.update_lengths_and_feedback();
            }
            "predelay_ms" => self
                .predelay_samples
                .set_target(value * self.sample_rate / 1_000.0, PARAMETER_SMOOTH_SAMPLES)?,
            "decay_seconds" => {
                self.decay_seconds = value;
                self.update_lengths_and_feedback();
            }
            "size_percent" => {
                self.size_percent = value;
                self.update_lengths_and_feedback();
            }
            "damping_percent" => {
                self.damping_percent = value;
                let frequency = damping_frequency(value);
                for filter in &mut self.damping {
                    filter.configure(frequency, self.sample_rate)?;
                }
            }
            "input_low_cut_hz" => {
                self.input_low_cut_left.configure(value, self.sample_rate)?;
                self.input_low_cut_right
                    .configure(value, self.sample_rate)?;
            }
            "width_percent" => self
                .width
                .set_target(value * 0.01, PARAMETER_SMOOTH_SAMPLES)?,
            "wet_percent" => self
                .wet
                .set_target(value * 0.01, PARAMETER_SMOOTH_SAMPLES)?,
            "dry_percent" => self
                .dry
                .set_target(value * 0.01, PARAMETER_SMOOTH_SAMPLES)?,
            _ => return Err(EffectError::new(format!("unknown Reverb parameter {name}"))),
        }
        Ok(())
    }

    fn update_lengths_and_feedback(&mut self) {
        let size = 0.7 + self.size_percent * 0.006;
        for index in 0..4 {
            let milliseconds = VOICING_LENGTHS_MS[self.voicing][index] * size;
            let samples = milliseconds * self.sample_rate / 1_000.0;
            self.lengths[index] = samples.clamp(1.0, self.lines[index].maximum_delay() as f32);
            let delay_seconds = self.lengths[index] / self.sample_rate;
            self.feedback[index] = 10.0_f32
                .powf(-3.0 * delay_seconds / self.decay_seconds)
                .clamp(0.0, 0.999_9);
        }
    }

    pub(super) fn reset(&mut self) {
        self.predelay_left.reset();
        self.predelay_right.reset();
        self.input_low_cut_left.reset();
        self.input_low_cut_right.reset();
        for diffuser in &mut self.input_diffusion_left {
            diffuser.reset();
        }
        for diffuser in &mut self.input_diffusion_right {
            diffuser.reset();
        }
        for line in &mut self.lines {
            line.reset();
        }
        for filter in &mut self.damping {
            filter.reset();
        }
    }
}

fn damping_frequency(percent: f32) -> f32 {
    18_000.0 * (1.0 - percent * 0.01) + 1_500.0 * percent * 0.01
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_graph::{EffectKind, EFFECT_FORMAT_VERSION};
    use crate::dsp::allocation_test::assert_no_allocations;
    use crate::dsp::analysis::{channel, correlation, energy_decay_db, spectral_amplitudes};
    use crate::dsp::{Biquad, BiquadCoefficients};
    use crate::effects::EffectSlot;
    use std::collections::BTreeMap;

    fn effect(parameters: impl IntoIterator<Item = (&'static str, f32)>) -> EffectInstance {
        EffectInstance {
            id: 34,
            kind: EffectKind::Reverb,
            version: EFFECT_FORMAT_VERSION,
            bypass: false,
            parameters: parameters
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect::<BTreeMap<_, _>>(),
            owned_memory_bytes: 0,
        }
    }

    #[derive(Debug)]
    struct ReverbMetrics {
        voicing: usize,
        damping_percent: f32,
        rt60: [f64; 4],
        early_late_db: f64,
        density: [f64; 4],
        decay_residual_db: f64,
        modal_peaks: [(f64, f64); 3],
        correlation: [f64; 2],
        mono_delta_db: f64,
    }

    impl ReverbMetrics {
        fn is_valid(&self) -> bool {
            self.voicing <= 2
                && (0.0..=100.0).contains(&self.damping_percent)
                && self.rt60.into_iter().all(f64::is_finite)
                && self.early_late_db.is_finite()
                && self.density.into_iter().all(f64::is_finite)
                && self.decay_residual_db.is_finite()
                && self
                    .modal_peaks
                    .into_iter()
                    .all(|(frequency, prominence)| frequency.is_finite() && prominence.is_finite())
                && self.correlation.into_iter().all(f64::is_finite)
                && self.mono_delta_db.is_finite()
        }
    }

    fn render_response(
        voicing: usize,
        damping: f32,
        predelay_ms: f32,
        diffuse_input: bool,
    ) -> Vec<StereoFrame> {
        let sample_rate = 48_000;
        let mut reverb = Reverb::compile(
            &effect([
                ("type", voicing as f32),
                ("predelay_ms", predelay_ms),
                ("decay_seconds", 1.5),
                ("size_percent", 50.0),
                ("damping_percent", damping),
                ("input_low_cut_hz", 20.0),
                ("width_percent", 100.0),
                ("wet_percent", 100.0),
                ("dry_percent", 0.0),
            ]),
            sample_rate,
        )
        .unwrap();
        (0..sample_rate as usize * 4)
            .map(|index| {
                reverb.process_internal(
                    if index == 0 {
                        StereoFrame::new(1.0, 0.0)
                    } else {
                        StereoFrame::SILENCE
                    },
                    diffuse_input,
                )
            })
            .collect()
    }

    fn band_limit(samples: &[f32], low: f32, high: f32) -> Vec<f32> {
        let mut high_pass =
            Biquad::new(BiquadCoefficients::high_pass(low, 0.707, 48_000.0).unwrap());
        let mut low_pass =
            Biquad::new(BiquadCoefficients::low_pass(high, 0.707, 48_000.0).unwrap());
        samples
            .iter()
            .copied()
            .map(|sample| low_pass.process(high_pass.process(sample)))
            .collect()
    }

    fn rt60_and_smoothness(samples: &[f32]) -> (f64, f64) {
        let decay = energy_decay_db(samples);
        let points = decay
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, db)| (-35.0..=-5.0).contains(db))
            .map(|(index, db)| (index as f64 / 48_000.0, db))
            .collect::<Vec<_>>();
        assert!(points.len() > 1_000);
        let mean_time = points.iter().map(|point| point.0).sum::<f64>() / points.len() as f64;
        let mean_db = points.iter().map(|point| point.1).sum::<f64>() / points.len() as f64;
        let slope = points
            .iter()
            .map(|point| (point.0 - mean_time) * (point.1 - mean_db))
            .sum::<f64>()
            / points
                .iter()
                .map(|point| (point.0 - mean_time).powi(2))
                .sum::<f64>();
        let residual = (points
            .iter()
            .map(|point| {
                let fitted = mean_db + slope * (point.0 - mean_time);
                (point.1 - fitted).powi(2)
            })
            .sum::<f64>()
            / points.len() as f64)
            .sqrt();
        (-60.0 / slope, residual)
    }

    fn normalized_echo_density(samples: &[f32], center: usize, radius: usize) -> f64 {
        let window = &samples[center - radius..center + radius];
        let mean = window.iter().map(|sample| *sample as f64).sum::<f64>() / window.len() as f64;
        let deviation = (window
            .iter()
            .map(|sample| (*sample as f64 - mean).powi(2))
            .sum::<f64>()
            / window.len() as f64)
            .sqrt();
        let outside = window
            .iter()
            .filter(|sample| (**sample as f64 - mean).abs() > deviation)
            .count();
        outside as f64 / window.len() as f64 / 0.317_310_507_862_914_15
    }

    fn modal_prominence(samples: &[f32], low_hz: f64, high_hz: f64) -> (f64, f64) {
        let length = 65_536;
        let start = 24_000;
        let windowed = samples[start..start + length]
            .iter()
            .copied()
            .enumerate()
            .map(|(index, sample)| {
                let hann = 0.5
                    - 0.5 * (2.0 * std::f64::consts::PI * index as f64 / (length - 1) as f64).cos();
                sample * hann as f32
            })
            .collect::<Vec<_>>();
        let spectrum = spectral_amplitudes(&windowed);
        let first = (low_hz * length as f64 / 48_000.0).ceil() as usize;
        let last = (high_hz * length as f64 / 48_000.0).floor() as usize;
        let mut band = spectrum[first..=last].to_vec();
        let (peak_offset, peak) = band
            .iter()
            .copied()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(&right.1))
            .unwrap();
        band.sort_by(f64::total_cmp);
        let median = band[band.len() / 2].max(f64::MIN_POSITIVE);
        let frequency = (first + peak_offset) as f64 * 48_000.0 / length as f64;
        (frequency, 20.0 * (peak / median).log10())
    }

    #[test]
    fn feedback_is_mathematically_bounded_and_tracks_declared_rt60() {
        for voicing in 0..=2 {
            for decay in [0.2, 1.5, 8.0] {
                for size in [0.0, 50.0, 100.0] {
                    let reverb = Reverb::compile(
                        &effect([
                            ("type", voicing as f32),
                            ("decay_seconds", decay),
                            ("size_percent", size),
                        ]),
                        48_000,
                    )
                    .unwrap();
                    for index in 0..4 {
                        assert!((0.0..1.0).contains(&reverb.feedback[index]));
                        let cycles = decay * reverb.sample_rate / reverb.lengths[index];
                        let rt60_gain = reverb.feedback[index].powf(cycles);
                        assert!((20.0 * rt60_gain.log10() + 60.0).abs() < 0.01);
                    }
                }
            }
        }
    }

    #[test]
    fn predelay_and_fdn_lengths_bound_first_wet_arrival() {
        let mut slot = EffectSlot::compile(
            &effect([
                ("type", 0.0),
                ("predelay_ms", 20.0),
                ("size_percent", 50.0),
                ("wet_percent", 100.0),
                ("dry_percent", 0.0),
            ]),
            48_000,
            128,
        )
        .unwrap();
        let mut samples = vec![StereoFrame::SILENCE; 4_000];
        samples[0] = StereoFrame::new(1.0, 0.0);
        for chunk in samples.chunks_mut(73) {
            slot.process(chunk);
        }
        let first = samples
            .iter()
            .position(|frame| frame.left.abs() + frame.right.abs() > 1.0e-5)
            .unwrap();
        assert!((2_070..=2_090).contains(&first), "first arrival {first}");
    }

    #[test]
    fn three_voicings_are_distinct_stereo_and_decay_over_time() {
        let mut signatures = Vec::new();
        for voicing in 0..=2 {
            let mut slot = EffectSlot::compile(
                &effect([
                    ("type", voicing as f32),
                    ("predelay_ms", 0.0),
                    ("decay_seconds", 0.5),
                    ("wet_percent", 100.0),
                    ("dry_percent", 0.0),
                ]),
                48_000,
                128,
            )
            .unwrap();
            let mut samples = vec![StereoFrame::SILENCE; 48_000];
            samples[0] = StereoFrame::new(1.0, 0.25);
            for chunk in samples.chunks_mut(127) {
                slot.process(chunk);
            }
            let early = samples[2_000..12_000]
                .iter()
                .map(|frame| frame.left * frame.left + frame.right * frame.right)
                .sum::<f32>();
            let late = samples[38_000..48_000]
                .iter()
                .map(|frame| frame.left * frame.left + frame.right * frame.right)
                .sum::<f32>();
            assert!(early > late * 5.0, "early {early}, late {late}");
            assert!(samples
                .iter()
                .any(|frame| (frame.left - frame.right).abs() > 1.0e-4));
            signatures.push(samples[8_000]);
        }
        assert_ne!(signatures[0], signatures[1]);
        assert_ne!(signatures[1], signatures[2]);
    }

    #[test]
    fn broadband_decay_and_late_stereo_correlation_are_measured_from_output() {
        let sample_rate = 48_000;
        let mut reverb = Reverb::compile(
            &effect([
                ("type", 1.0),
                ("predelay_ms", 0.0),
                ("decay_seconds", 1.5),
                ("size_percent", 50.0),
                ("damping_percent", 0.0),
                ("input_low_cut_hz", 20.0),
                ("width_percent", 100.0),
                ("wet_percent", 100.0),
                ("dry_percent", 0.0),
            ]),
            sample_rate,
        )
        .unwrap();
        let mut response = Vec::with_capacity(sample_rate as usize * 4);
        for index in 0..sample_rate as usize * 4 {
            response.push(reverb.process(if index == 0 {
                StereoFrame::new(1.0, 0.0)
            } else {
                StereoFrame::SILENCE
            }));
        }
        let magnitude = response
            .iter()
            .map(|frame| (frame.left * frame.left + frame.right * frame.right).sqrt())
            .collect::<Vec<_>>();
        let decay = energy_decay_db(&magnitude);
        let at_5 = decay.iter().position(|db| *db <= -5.0).unwrap();
        let at_35 = decay.iter().position(|db| *db <= -35.0).unwrap();
        let measured_rt60 = (at_35 - at_5) as f64 / sample_rate as f64 * 2.0;
        assert!(
            (1.1..=1.9).contains(&measured_rt60),
            "measured broadband RT60 {measured_rt60:.3} s"
        );

        let left = channel(&response[24_000..72_000], true);
        let right = channel(&response[24_000..72_000], false);
        let late_correlation = correlation(&left, &right);
        eprintln!(
            "reverb plate: measured broadband RT60 {measured_rt60:.3} s, late correlation {late_correlation:.3}"
        );
        assert!(
            late_correlation.abs() < 0.95,
            "late stereo correlation {late_correlation:.3}"
        );
    }

    #[test]
    fn reverb_output_characterization_covers_density_bands_modes_and_ringing() {
        let mut rows = Vec::new();
        for voicing in 0..=2 {
            for damping in [0.0_f32, 50.0, 100.0] {
                let response = render_response(voicing, damping, 0.0, true);
                let left = channel(&response, true);
                let right = channel(&response, false);
                let mono = left
                    .iter()
                    .zip(&right)
                    .map(|(left, right)| (*left + *right) * 0.5)
                    .collect::<Vec<_>>();
                let low = band_limit(&mono, 80.0, 500.0);
                let mid = band_limit(&mono, 500.0, 2_000.0);
                let high = band_limit(&mono, 2_000.0, 8_000.0);
                let (broadband_rt60, smoothness) = rt60_and_smoothness(&mono);
                let (low_rt60, _) = rt60_and_smoothness(&low);
                let (mid_rt60, _) = rt60_and_smoothness(&mid);
                let (high_rt60, _) = rt60_and_smoothness(&high);
                let early_energy = mono[..24_000]
                    .iter()
                    .map(|sample| (*sample as f64).powi(2))
                    .sum::<f64>();
                let late_energy = mono[24_000..72_000]
                    .iter()
                    .map(|sample| (*sample as f64).powi(2))
                    .sum::<f64>();
                let early_correlation = correlation(&left[2_400..12_000], &right[2_400..12_000]);
                let late_correlation = correlation(&left[24_000..72_000], &right[24_000..72_000]);
                let stereo_energy = left
                    .iter()
                    .zip(&right)
                    .map(|(left, right)| ((*left as f64).powi(2) + (*right as f64).powi(2)) * 0.5)
                    .sum::<f64>();
                let mono_energy = mono
                    .iter()
                    .map(|sample| (*sample as f64).powi(2))
                    .sum::<f64>();
                let mono_delta_db = 10.0 * (mono_energy / stereo_energy).log10();
                let density = [50, 100, 250, 500]
                    .map(|milliseconds| normalized_echo_density(&mono, milliseconds * 48, 480));
                let low_peak = modal_prominence(&mono, 80.0, 500.0);
                let mid_peak = modal_prominence(&mono, 500.0, 2_000.0);
                let high_peak = modal_prominence(&mono, 2_000.0, 8_000.0);
                rows.push(ReverbMetrics {
                    voicing,
                    damping_percent: damping,
                    rt60: [broadband_rt60, low_rt60, mid_rt60, high_rt60],
                    early_late_db: 10.0 * (early_energy / late_energy).log10(),
                    density,
                    decay_residual_db: smoothness,
                    modal_peaks: [low_peak, mid_peak, high_peak],
                    correlation: [early_correlation, late_correlation],
                    mono_delta_db,
                });
                assert!([broadband_rt60, low_rt60, mid_rt60, high_rt60]
                    .into_iter()
                    .all(|value| value.is_finite() && (0.05..10.0).contains(&value)));
                assert!(density.into_iter().all(f64::is_finite));
                assert!(mono_delta_db.is_finite() && mono_delta_db > -30.0);
            }
        }

        assert!(rows.iter().all(ReverbMetrics::is_valid));

        let baseline = render_response(1, 0.0, 0.0, false);
        let improved = render_response(1, 0.0, 0.0, true);
        let baseline_mono = baseline
            .iter()
            .map(|frame| (frame.left + frame.right) * 0.5)
            .collect::<Vec<_>>();
        let improved_mono = improved
            .iter()
            .map(|frame| (frame.left + frame.right) * 0.5)
            .collect::<Vec<_>>();
        let baseline_density = [100, 250, 500]
            .map(|milliseconds| normalized_echo_density(&baseline_mono, milliseconds * 48, 480));
        let improved_density = [100, 250, 500]
            .map(|milliseconds| normalized_echo_density(&improved_mono, milliseconds * 48, 480));
        assert!(
            improved_density[1] > baseline_density[1] * 1.5,
            "baseline {baseline_density:?}, diffused {improved_density:?}"
        );

        let no_predelay = render_response(1, 50.0, 0.0, true);
        let predelayed = render_response(1, 50.0, 20.0, true);
        let first = |response: &[StereoFrame]| {
            response
                .iter()
                .position(|frame| frame.left.abs() + frame.right.abs() > 1.0e-7)
                .unwrap()
        };
        let predelay_shift = first(&predelayed) - first(&no_predelay);
        assert_eq!(predelay_shift, 960);
        eprintln!("reverb characterization rows (voice, damping %, broadband/low/mid/high RT60 s, early/late dB, NED at 50/100/250/500 ms, EDC residual dB, modal peak Hz/dB by band, early/late correlation, mono delta dB): {rows:?}");
        eprintln!("reverb plate NED before/after input diffusion at 100/250/500 ms: {baseline_density:?} / {improved_density:?}");
        eprintln!("reverb measured predelay shift: {predelay_shift} samples / 20.000 ms");

        let benchmark = |diffuse_input: bool| {
            let mut reverb = Reverb::compile(&effect([]), 48_000).unwrap();
            let start = std::time::Instant::now();
            let mut sum = 0.0_f32;
            for index in 0..200_000 {
                let input = ((index * 17 % 257) as f32 / 128.0 - 1.0) * 0.1;
                let output =
                    reverb.process_internal(StereoFrame::new(input, -input * 0.7), diffuse_input);
                sum += std::hint::black_box(output.left + output.right);
            }
            std::hint::black_box(sum);
            start.elapsed()
        };
        let baseline_time = benchmark(false);
        let diffused_time = benchmark(true);
        eprintln!(
            "reverb processor cost: baseline {baseline_time:?}, diffused {diffused_time:?}, ratio {:.3}",
            diffused_time.as_secs_f64() / baseline_time.as_secs_f64()
        );
    }

    #[test]
    fn silence_rates_limits_moves_reset_bypass_and_allocation_are_safe() {
        for sample_rate in [8_000, 44_100, 48_000, 96_000, 384_000] {
            let mut slot = EffectSlot::compile(
                &effect([
                    ("type", 2.0),
                    ("decay_seconds", 8.0),
                    ("size_percent", 100.0),
                    ("damping_percent", 0.0),
                ]),
                sample_rate,
                128,
            )
            .unwrap();
            let mut silence = [StereoFrame::SILENCE; 512];
            assert_no_allocations(|| {
                for chunk in silence.chunks_mut(37) {
                    slot.process(chunk);
                }
            });
            assert_eq!(silence, [StereoFrame::SILENCE; 512]);
            for index in 0..30 {
                slot.set_parameter("type", (index % 3) as f32).unwrap();
                slot.set_parameter("size_percent", (index * 3) as f32)
                    .unwrap();
                let mut block = [StereoFrame::new(1.0, -1.0); 31];
                slot.process(&mut block);
                assert!(block
                    .iter()
                    .all(|frame| frame.left.is_finite() && frame.right.is_finite()));
            }
            slot.reset();
            slot.set_bypass(true).unwrap();
            let mut dry = [StereoFrame::new(0.25, -0.5); 2_048];
            slot.process(&mut dry);
            assert_eq!(dry[2_047], StereoFrame::new(0.25, -0.5));
        }
    }

    #[test]
    fn bypass_hides_but_drains_tail_and_long_decay_avoids_denormals() {
        let mut slot = EffectSlot::compile(
            &effect([
                ("type", 1.0),
                ("decay_seconds", 1.5),
                ("wet_percent", 100.0),
                ("dry_percent", 0.0),
            ]),
            48_000,
            128,
        )
        .unwrap();
        let mut excitation = vec![StereoFrame::SILENCE; 6_000];
        excitation[0] = StereoFrame::new(1.0, 0.0);
        slot.process(&mut excitation);
        slot.set_bypass(true).unwrap();
        let mut hidden = vec![StereoFrame::SILENCE; 2_048];
        slot.process(&mut hidden);
        assert_eq!(hidden[2_047], StereoFrame::SILENCE);
        slot.set_bypass(false).unwrap();
        let mut resumed = vec![StereoFrame::SILENCE; 2_048];
        slot.process(&mut resumed);
        assert!(resumed
            .iter()
            .any(|frame| frame.left.abs() + frame.right.abs() > 1.0e-6));

        let mut long = Reverb::compile(
            &effect([
                ("type", 2.0),
                ("decay_seconds", 8.0),
                ("damping_percent", 100.0),
                ("wet_percent", 100.0),
                ("dry_percent", 0.0),
            ]),
            48_000,
        )
        .unwrap();
        let mut peak = 0.0_f32;
        let mut subnormal = 0_u64;
        for index in 0..48_000 * 12 {
            let output = long.process(if index == 0 {
                StereoFrame::new(1.0, -0.25)
            } else {
                StereoFrame::SILENCE
            });
            assert!(output.left.is_finite() && output.right.is_finite());
            peak = peak.max(output.left.abs()).max(output.right.abs());
            subnormal +=
                u64::from(output.left.is_subnormal()) + u64::from(output.right.is_subnormal());
        }
        assert!(peak < EMERGENCY_LEVEL && subnormal == 0);
        eprintln!("reverb bypass resumed a draining tail; 12 s stability peak {peak:.6}, subnormal outputs {subnormal}");
    }

    #[test]
    #[ignore = "writes the private level-matched reverb audition pack"]
    fn render_private_reverb_audition_pack() {
        use hound::{SampleFormat, WavSpec, WavWriter};
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::path::PathBuf;

        let destination = PathBuf::from(
            std::env::var("SHSYNTH_DSP_LAB_DIR")
                .expect("set SHSYNTH_DSP_LAB_DIR to one explicit private run directory"),
        );
        assert!(destination.is_absolute() && destination.parent().is_some());
        std::fs::create_dir_all(&destination).unwrap();

        let sample_rate = 48_000_u32;
        let dry = (0..sample_rate as usize * 3)
            .map(|index| {
                let time = index as f32 / sample_rate as f32;
                let beat = index % 12_000;
                let envelope = if beat < 2_400 {
                    (1.0 - beat as f32 / 2_400.0).powi(3)
                } else {
                    0.0
                };
                let left = envelope
                    * ((2.0 * std::f32::consts::PI * 220.0 * time).sin() * 0.2
                        + (((index * 43 % 257) as f32 / 128.0) - 1.0) * 0.06);
                let right = envelope
                    * ((2.0 * std::f32::consts::PI * 330.0 * time).sin() * 0.18
                        + (((index * 71 % 263) as f32 / 131.0) - 1.0) * 0.05);
                StereoFrame::new(left, right)
            })
            .collect::<Vec<_>>();

        let render = |voicing: f32, diffuse_input: bool| {
            let mut reverb = Reverb::compile(
                &effect([
                    ("type", voicing),
                    ("predelay_ms", 0.0),
                    ("decay_seconds", 1.5),
                    ("size_percent", 50.0),
                    ("damping_percent", 0.0),
                    ("input_low_cut_hz", 20.0),
                    ("width_percent", 100.0),
                    ("wet_percent", 100.0),
                    ("dry_percent", 0.0),
                ]),
                sample_rate,
            )
            .unwrap();
            let mut output = Vec::with_capacity(dry.len() + sample_rate as usize * 2);
            for frame in dry.iter().copied() {
                output.push(reverb.process_internal(frame, diffuse_input));
            }
            for _ in 0..sample_rate as usize * 2 {
                output.push(reverb.process_internal(StereoFrame::SILENCE, diffuse_input));
            }
            output
        };

        fn level_match(samples: &[StereoFrame], target_db: f64) -> (Vec<StereoFrame>, f64) {
            let energy = samples
                .iter()
                .map(|frame| ((frame.left as f64).powi(2) + (frame.right as f64).powi(2)) * 0.5)
                .sum::<f64>();
            let rms = (energy / samples.len() as f64).sqrt();
            let peak = samples
                .iter()
                .map(|frame| frame.left.abs().max(frame.right.abs()))
                .fold(0.0_f32, f32::max) as f64;
            let gain = (10.0_f64.powf(target_db / 20.0) / rms.max(f64::MIN_POSITIVE))
                .min(10.0_f64.powf(-1.0 / 20.0) / peak.max(1.0e-12));
            (
                samples
                    .iter()
                    .map(|frame| {
                        StereoFrame::new(frame.left * gain as f32, frame.right * gain as f32)
                    })
                    .collect(),
                20.0 * gain.log10(),
            )
        }

        fn write_wav(path: &std::path::Path, samples: &[StereoFrame]) {
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap();
            let mut writer = WavWriter::new(
                file,
                WavSpec {
                    channels: 2,
                    sample_rate: 48_000,
                    bits_per_sample: 24,
                    sample_format: SampleFormat::Int,
                },
            )
            .unwrap();
            for frame in samples {
                writer
                    .write_sample((frame.left.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32)
                    .unwrap();
                writer
                    .write_sample((frame.right.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32)
                    .unwrap();
            }
            writer.finalize().unwrap();
        }

        let impulse_baseline = render_response(1, 0.0, 0.0, false);
        let impulse_diffused = render_response(1, 0.0, 0.0, true);
        let files = [
            (
                "30-reverb-dry.wav",
                dry.clone(),
                "dry deterministic three-second percussive excitation",
                -22.0,
            ),
            (
                "31-plate-impulse-baseline.wav",
                impulse_baseline,
                "previous plate impulse response, no input diffusion",
                -50.0,
            ),
            (
                "32-plate-impulse-diffused.wav",
                impulse_diffused,
                "current plate impulse response, two input all-pass stages per channel",
                -50.0,
            ),
            (
                "33-plate-material-baseline.wav",
                render(1.0, false),
                "previous plate response to deterministic excitation",
                -22.0,
            ),
            (
                "34-plate-material-diffused.wav",
                render(1.0, true),
                "current plate response to deterministic excitation",
                -22.0,
            ),
            (
                "35-hall-material-diffused.wav",
                render(2.0, true),
                "current hall response exposing its slower density build",
                -22.0,
            ),
        ];
        let mut rows = Vec::new();
        for (name, samples, purpose, target_db) in files {
            let (matched, gain_db) = level_match(&samples, target_db);
            write_wav(&destination.join(name), &matched);
            rows.push(format!(
                "- `{name}` — {purpose}; target {target_db:.0} dBFS RMS; gain {gain_db:+.2} dB"
            ));
        }
        let mut manifest = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination.join("REVERB_MANIFEST.md"))
            .unwrap();
        writeln!(
            manifest,
            "# Private reverb audition\n\nSynthetic evidence/audition material; no listening judgment is implied.\n\n- Format: stereo 24-bit PCM WAV, 48 kHz; impulse files 4.000 s, material files 5.000 s including a two-second drain\n- Input: one-sample left impulse, or deterministic original 220/330 Hz percussive tones plus bounded deterministic noise; no private or uncleared material\n- Processor: wet-only Reverb; predelay 0 ms, decay 1.5 s, size 50%, damping 0%, input low-cut 20 Hz, width 100%; type plate except file 35 (hall)\n- Level control: impulse files are stereo-RMS-matched to -50 dBFS and dry/material files to -22 dBFS, all with a -1 dBFS peak ceiling; each direct comparison uses the same RMS target\n- Alignment: reset at frame zero with no post-render latency or phase alignment\n- Listening questions: compare 31/32 for onset density and discrete echoes, 33/34 for percussive texture, and use 35 as a deliberately exposed remaining slow-density case; metrics establish density/decay, not preference\n\n{}",
            rows.join("\n")
        )
        .unwrap();
    }
}
