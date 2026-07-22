use super::{smooth, EffectError, PARAMETER_SMOOTH_SAMPLES};
use crate::audio_graph::EffectInstance;
use crate::dsp::{db_to_gain, finite_or_zero, SmoothedValue, StereoFrame};
use crate::effect_schema;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Low,
    Band,
    High,
}

impl Mode {
    fn from_parameter(value: f32) -> Self {
        match value as u8 {
            0 => Self::Low,
            1 => Self::Band,
            _ => Self::High,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct State {
    integrator_1: f32,
    integrator_2: f32,
}

impl State {
    #[inline]
    fn process(&mut self, input: f32, g: f32, damping: f32) -> Outputs {
        let denominator = 1.0 + g * (g + damping);
        let a1 = denominator.recip();
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = input - self.integrator_2;
        let band = a1 * self.integrator_1 + a2 * v3;
        let low = self.integrator_2 + a2 * self.integrator_1 + a3 * v3;
        self.integrator_1 = finite_or_zero(2.0 * band - self.integrator_1);
        self.integrator_2 = finite_or_zero(2.0 * low - self.integrator_2);
        let high = input - damping * band - low;
        if !(low.is_finite() && band.is_finite() && high.is_finite()) {
            self.reset();
            Outputs::default()
        } else {
            Outputs { low, band, high }
        }
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Default)]
struct Outputs {
    low: f32,
    band: f32,
    high: f32,
}

impl Outputs {
    fn mode(self, mode: Mode) -> f32 {
        match mode {
            Mode::Low => self.low,
            Mode::Band => self.band,
            Mode::High => self.high,
        }
    }
}

pub(super) struct Filter {
    sample_rate: f32,
    left: State,
    right: State,
    g: SmoothedValue,
    damping: SmoothedValue,
    drive_gain: SmoothedValue,
    drive_mix: SmoothedValue,
    drive_previous_left: Option<f32>,
    drive_previous_right: Option<f32>,
    mix: SmoothedValue,
    current_mode: Mode,
    next_mode: Mode,
    mode_mix: SmoothedValue,
    transitioning: bool,
    pending_mode: Option<Mode>,
}

impl Filter {
    pub(super) fn compile(effect: &EffectInstance, sample_rate: u32) -> Result<Self, EffectError> {
        let value = |name| {
            effect_schema::parameter(effect, name)
                .map_err(|error| EffectError::new(error.to_string()))
        };
        let sample_rate = sample_rate as f32;
        let mode = Mode::from_parameter(value("mode")?);
        let drive_db = value("drive_db")?;
        Ok(Self {
            sample_rate,
            left: State::default(),
            right: State::default(),
            g: smooth(cutoff_coefficient(value("cutoff_hz")?, sample_rate)),
            damping: smooth(resonance_damping(value("resonance")?)),
            drive_gain: smooth(db_to_gain(drive_db)?),
            drive_mix: smooth(drive_db / 12.0),
            drive_previous_left: None,
            drive_previous_right: None,
            mix: smooth(value("mix_percent")? * 0.01),
            current_mode: mode,
            next_mode: mode,
            mode_mix: smooth(0.0),
            transitioning: false,
            pending_mode: None,
        })
    }

    #[inline]
    pub(super) fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        let drive_gain = self.drive_gain.next_value();
        let drive_mix = self.drive_mix.next_value();
        let driven_left = drive(
            frame.left,
            drive_gain,
            drive_mix,
            &mut self.drive_previous_left,
        );
        let driven_right = drive(
            frame.right,
            drive_gain,
            drive_mix,
            &mut self.drive_previous_right,
        );
        let g = self.g.next_value();
        let damping = self.damping.next_value();
        let left = self.left.process(driven_left, g, damping);
        let right = self.right.process(driven_right, g, damping);
        let mut filtered =
            StereoFrame::new(left.mode(self.current_mode), right.mode(self.current_mode));
        if self.transitioning {
            let next = StereoFrame::new(left.mode(self.next_mode), right.mode(self.next_mode));
            let mode_mix = self.mode_mix.next_value();
            filtered = StereoFrame::new(
                filtered.left + (next.left - filtered.left) * mode_mix,
                filtered.right + (next.right - filtered.right) * mode_mix,
            );
            if mode_mix >= 1.0 {
                self.current_mode = self.next_mode;
                if let Some(mode) = self.pending_mode.take() {
                    self.begin_mode_transition(mode);
                } else {
                    self.transitioning = false;
                }
            }
        }
        let mix = self.mix.next_value();
        StereoFrame::new(
            frame.left + (filtered.left - frame.left) * mix,
            frame.right + (filtered.right - frame.right) * mix,
        )
        .finite_or_silence()
    }

    pub(super) fn set_parameter(&mut self, name: &str, value: f32) -> Result<(), EffectError> {
        match name {
            "mode" => self.set_mode(Mode::from_parameter(value)),
            "cutoff_hz" => self.g.set_target(
                cutoff_coefficient(value, self.sample_rate),
                PARAMETER_SMOOTH_SAMPLES,
            )?,
            "resonance" => self
                .damping
                .set_target(resonance_damping(value), PARAMETER_SMOOTH_SAMPLES)?,
            "drive_db" => {
                self.drive_gain
                    .set_target(db_to_gain(value)?, PARAMETER_SMOOTH_SAMPLES)?;
                self.drive_mix
                    .set_target(value / 12.0, PARAMETER_SMOOTH_SAMPLES)?;
            }
            "mix_percent" => self
                .mix
                .set_target(value * 0.01, PARAMETER_SMOOTH_SAMPLES)?,
            _ => return Err(EffectError::new(format!("unknown Filter parameter {name}"))),
        }
        Ok(())
    }

    fn set_mode(&mut self, mode: Mode) {
        if self.transitioning {
            self.pending_mode = Some(mode);
        } else if mode != self.current_mode {
            self.begin_mode_transition(mode);
        }
    }

    fn begin_mode_transition(&mut self, mode: Mode) {
        self.next_mode = mode;
        if self.mode_mix.reset(0.0).is_ok()
            && self
                .mode_mix
                .set_target(1.0, PARAMETER_SMOOTH_SAMPLES)
                .is_ok()
        {
            self.transitioning = true;
        }
    }

    pub(super) fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
        self.drive_previous_left = None;
        self.drive_previous_right = None;
        self.current_mode = self.pending_mode.unwrap_or(if self.transitioning {
            self.next_mode
        } else {
            self.current_mode
        });
        self.next_mode = self.current_mode;
        let _ = self.mode_mix.reset(0.0);
        self.transitioning = false;
        self.pending_mode = None;
    }
}

fn cutoff_coefficient(frequency: f32, sample_rate: f32) -> f32 {
    let safe = frequency.min(sample_rate * 0.45);
    (std::f32::consts::PI * safe / sample_rate).tan()
}

fn resonance_damping(resonance_percent: f32) -> f32 {
    let normalized = resonance_percent / 90.0;
    let q = 0.5 + normalized * normalized * 7.5;
    q.recip()
}

#[inline]
fn drive(input: f32, gain: f32, mix: f32, previous: &mut Option<f32>) -> f32 {
    let driven = input * gain;
    if mix <= 0.0 {
        *previous = Some(driven);
        return input;
    }
    let saturated = antialiased_cubic(driven, *previous);
    *previous = Some(driven);
    finite_or_zero(input + (saturated - input) * mix)
}

/// First-order ADAA for the filter's cubic pre-drive. This is the same
/// antiderivative method used by Distortion, applied before the resonant TPT
/// state-variable filter so folded products are not amplified by that stage.
#[inline]
fn antialiased_cubic(input: f32, previous: Option<f32>) -> f32 {
    let Some(previous) = previous else {
        return cubic_transfer(input);
    };
    if input >= 1.0 && previous >= 1.0 {
        return 1.0;
    }
    if input <= -1.0 && previous <= -1.0 {
        return -1.0;
    }
    let difference = input - previous;
    if difference.abs() <= 1.0e-4 {
        cubic_transfer((input + previous) * 0.5)
    } else {
        (cubic_antiderivative(input) - cubic_antiderivative(previous)) / difference
    }
}

#[inline]
fn cubic_transfer(input: f32) -> f32 {
    if input >= 1.0 {
        1.0
    } else if input <= -1.0 {
        -1.0
    } else {
        1.5 * (input - input * input * input / 3.0)
    }
}

#[inline]
fn cubic_antiderivative(input: f32) -> f32 {
    if input >= 1.0 {
        input - 0.375
    } else if input <= -1.0 {
        -input - 0.375
    } else {
        let squared = input * input;
        0.75 * squared - 0.125 * squared * squared
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_graph::{EffectKind, EFFECT_FORMAT_VERSION};
    use crate::dsp::allocation_test::assert_no_allocations;
    use crate::dsp::analysis::{coherent_sine, harmonic_alias_ratio, rms};
    use crate::effects::EffectSlot;
    use std::f32::consts::PI;

    fn effect(parameters: impl IntoIterator<Item = (&'static str, f32)>) -> EffectInstance {
        EffectInstance {
            id: 7,
            kind: EffectKind::Filter,
            version: EFFECT_FORMAT_VERSION,
            bypass: false,
            parameters: parameters
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
            owned_memory_bytes: 0,
        }
    }

    fn measured_gain(mode: u8, frequency: f32, cutoff: f32) -> f32 {
        let mut filter = Filter::compile(
            &effect([
                ("mode", mode as f32),
                ("cutoff_hz", cutoff),
                ("resonance", 15.0),
            ]),
            48_000,
        )
        .unwrap();
        let mut input_energy = 0.0;
        let mut output_energy = 0.0;
        for index in 0..96_000 {
            let input = (2.0 * PI * frequency * index as f32 / 48_000.0).sin() * 0.25;
            let output = filter.process(StereoFrame::new(input, input)).left;
            if index >= 48_000 {
                input_energy += input * input;
                output_energy += output * output;
            }
        }
        (output_energy / input_energy).sqrt()
    }

    fn legacy_drive(input: f32, gain: f32, mix: f32) -> f32 {
        if mix <= 0.0 {
            return input;
        }
        let saturated = cubic_transfer(input * gain);
        finite_or_zero(input + (saturated - input) * mix)
    }

    fn render_driven_filter(
        input: &[f32],
        sample_rate: f32,
        cutoff: f32,
        resonance: f32,
        drive_db: f32,
        antialiased: bool,
    ) -> Vec<f32> {
        let mut state = State::default();
        let g = cutoff_coefficient(cutoff, sample_rate);
        let damping = resonance_damping(resonance);
        let gain = db_to_gain(drive_db).unwrap();
        let mix = drive_db / 12.0;
        let mut previous = None;
        input
            .iter()
            .copied()
            .map(|sample| {
                let shaped = if antialiased {
                    drive(sample, gain, mix, &mut previous)
                } else {
                    legacy_drive(sample, gain, mix)
                };
                state.process(shaped, g, damping).low
            })
            .collect()
    }

    #[test]
    fn three_modes_have_the_expected_measured_response() {
        let low_pass_low = measured_gain(0, 100.0, 1_000.0);
        let low_pass_high = measured_gain(0, 10_000.0, 1_000.0);
        let band_low = measured_gain(1, 100.0, 1_000.0);
        let band_center = measured_gain(1, 1_000.0, 1_000.0);
        let band_high = measured_gain(1, 10_000.0, 1_000.0);
        let high_pass_low = measured_gain(2, 100.0, 1_000.0);
        let high_pass_high = measured_gain(2, 10_000.0, 1_000.0);
        assert!(low_pass_low > 0.98 && low_pass_high < 0.02);
        assert!(band_center > band_low * 5.0 && band_center > band_high * 5.0);
        assert!(high_pass_low < 0.02 && high_pass_high > 0.98);
    }

    #[test]
    fn zero_drive_is_exactly_linear_before_filtering() {
        let mut previous = None;
        for index in 0..=20_000 {
            let input = -1.0 + index as f32 * 0.0001;
            assert_eq!(drive(input, 1.0, 0.0, &mut previous), input);
        }
        previous = None;
        assert!(drive(0.75, db_to_gain(12.0).unwrap(), 1.0, &mut previous) <= 1.0);
    }

    #[test]
    fn filter_drive_alias_is_measured_through_the_resonant_tpt_stage() {
        const LENGTH: usize = 4_096;
        let bins = [700, 900, 1_300];
        let mut rows = Vec::new();
        for sample_rate in [44_100_u32, 48_000, 96_000] {
            for cutoff in [5_000.0_f32, 15_000.0] {
                for resonance in [20.0_f32, 70.0] {
                    for drive_db in [4.0_f32, 8.0, 12.0] {
                        let mut legacy_alias = 0.0_f64;
                        let mut adaa_alias = 0.0_f64;
                        let mut rms_delta = 0.0_f64;
                        for bin in bins {
                            let input = coherent_sine(LENGTH * 2, bin * 2, 0.5);
                            let legacy = render_driven_filter(
                                &input,
                                sample_rate as f32,
                                cutoff,
                                resonance,
                                drive_db,
                                false,
                            )
                            .split_off(LENGTH);
                            let adaa = render_driven_filter(
                                &input,
                                sample_rate as f32,
                                cutoff,
                                resonance,
                                drive_db,
                                true,
                            )
                            .split_off(LENGTH);
                            legacy_alias += harmonic_alias_ratio(&legacy, bin, 63).powi(2);
                            adaa_alias += harmonic_alias_ratio(&adaa, bin, 63).powi(2);
                            rms_delta += 20.0 * (rms(&adaa) / rms(&legacy)).log10();
                        }
                        legacy_alias = (legacy_alias / bins.len() as f64).sqrt();
                        adaa_alias = (adaa_alias / bins.len() as f64).sqrt();
                        rows.push((
                            sample_rate,
                            cutoff,
                            resonance,
                            drive_db,
                            20.0 * legacy_alias.max(1.0e-15).log10(),
                            20.0 * adaa_alias.max(1.0e-15).log10(),
                            rms_delta / bins.len() as f64,
                        ));
                        assert!(legacy_alias.is_finite() && adaa_alias.is_finite());
                        assert!(
                            adaa_alias < legacy_alias * 0.5,
                            "rate {sample_rate}, cutoff {cutoff}, resonance {resonance}, drive {drive_db}: legacy {legacy_alias}, ADAA {adaa_alias}"
                        );
                    }
                }
            }
        }
        eprintln!("filter-drive alias rows (rate, cutoff Hz, resonance, drive dB, legacy alias dBc, ADAA alias dBc, ADAA RMS delta dB): {rows:?}");

        let input = coherent_sine(65_536, 13_003, 0.5);
        let legacy_start = std::time::Instant::now();
        let mut sum = 0.0_f32;
        for _ in 0..16 {
            for sample in input.iter().copied() {
                sum += std::hint::black_box(legacy_drive(sample, 4.0, 1.0));
            }
        }
        std::hint::black_box(sum);
        let legacy_time = legacy_start.elapsed();
        let adaa_start = std::time::Instant::now();
        let mut previous = None;
        let mut sum = 0.0_f32;
        for _ in 0..16 {
            for sample in input.iter().copied() {
                sum += std::hint::black_box(drive(sample, 4.0, 1.0, &mut previous));
            }
        }
        std::hint::black_box(sum);
        let adaa_time = adaa_start.elapsed();
        eprintln!(
            "filter drive core cost: legacy {legacy_time:?}, ADAA {adaa_time:?}, ratio {:.3}",
            adaa_time.as_secs_f64() / legacy_time.as_secs_f64()
        );
    }

    #[test]
    fn maximum_resonance_impulse_and_random_input_remain_stable_at_all_rates() {
        for sample_rate in [8_000, 44_100, 48_000, 96_000, 384_000] {
            for mode in 0..=2 {
                for cutoff in [20.0, 20_000.0] {
                    let mut slot = EffectSlot::compile(
                        &effect([
                            ("mode", mode as f32),
                            ("cutoff_hz", cutoff),
                            ("resonance", 90.0),
                            ("drive_db", 12.0),
                        ]),
                        sample_rate,
                        127,
                    )
                    .unwrap();
                    let mut state = 0x1357_9bdf_u32;
                    let mut maximum = 0.0_f32;
                    for block_index in 0..500 {
                        let mut block = [StereoFrame::SILENCE; 127];
                        for (index, frame) in block.iter_mut().enumerate() {
                            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                            let random = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                            let input = if block_index == 0 && index == 0 {
                                1.0
                            } else {
                                random
                            };
                            *frame = StereoFrame::new(input, -input);
                        }
                        slot.process(&mut block);
                        for frame in block {
                            assert!(frame.left.is_finite() && frame.right.is_finite());
                            maximum = maximum.max(frame.left.abs()).max(frame.right.abs());
                        }
                    }
                    assert!(maximum < 100.0, "{sample_rate} {mode} {cutoff}: {maximum}");
                    slot.reset();
                    let mut silence = [StereoFrame::SILENCE; 127];
                    slot.process(&mut silence);
                    assert_eq!(silence, [StereoFrame::SILENCE; 127]);
                }
            }
        }
    }

    #[test]
    fn chunks_modes_sweeps_bypass_and_allocation_are_safe() {
        let configured = effect([
            ("mode", 1.0),
            ("cutoff_hz", 2_000.0),
            ("resonance", 60.0),
            ("drive_db", 4.0),
            ("mix_percent", 75.0),
        ]);
        let input = (0..4_096)
            .map(|index| {
                let value = ((index * 29 % 257) as f32 / 128.0) - 1.0;
                StereoFrame::new(value, value * -0.4)
            })
            .collect::<Vec<_>>();
        let mut whole = EffectSlot::compile(&configured, 48_000, 256).unwrap();
        let mut expected = input.clone();
        assert_no_allocations(|| {
            for chunk in expected.chunks_mut(256) {
                whole.process(chunk);
            }
        });
        let mut odd = EffectSlot::compile(&configured, 48_000, 256).unwrap();
        let mut actual = input;
        for chunk in actual.chunks_mut(37) {
            odd.process(chunk);
        }
        assert_eq!(actual, expected);

        for index in 0..200 {
            odd.set_parameter("mode", (index % 3) as f32).unwrap();
            odd.set_parameter("cutoff_hz", 20.0 + (index % 100) as f32 * 199.8)
                .unwrap();
            odd.set_parameter("resonance", (index % 91) as f32).unwrap();
            let mut block = [StereoFrame::new(1.0, -1.0); 31];
            odd.process(&mut block);
            assert!(block
                .iter()
                .all(|frame| frame.left.is_finite() && frame.right.is_finite()));
        }
        odd.set_bypass(true).unwrap();
        let mut dry = [StereoFrame::new(0.25, -0.5); 256];
        odd.process(&mut dry);
        assert_eq!(dry[255], StereoFrame::new(0.25, -0.5));
    }

    #[test]
    #[ignore = "writes the private level-matched filter-drive audition pack"]
    fn render_private_filter_drive_audition_pack() {
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
                let sweep_frequency = 2_000.0 * (8.0_f32).powf(time / 3.0);
                (2.0 * PI * sweep_frequency * time).sin() * 0.22
                    + (2.0 * PI * 10_500.0 * time).sin() * 0.08
                    + (((index * 47 % 257) as f32 / 128.0) - 1.0) * 0.025
            })
            .collect::<Vec<_>>();
        let render = |cutoff: f32, resonance: f32, legacy: bool| {
            render_driven_filter(&dry, sample_rate as f32, cutoff, resonance, 12.0, !legacy)
        };

        fn level_match(samples: &[f32]) -> (Vec<f32>, f64) {
            let peak = samples
                .iter()
                .copied()
                .map(f32::abs)
                .fold(0.0, f32::max)
                .max(1.0e-12);
            let gain = (10.0_f64.powf(-18.0 / 20.0) / rms(samples).max(f64::MIN_POSITIVE))
                .min(10.0_f64.powf(-1.0 / 20.0) / f64::from(peak));
            (
                samples.iter().map(|sample| *sample * gain as f32).collect(),
                20.0 * gain.log10(),
            )
        }

        fn write_wav(path: &std::path::Path, samples: &[f32]) {
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
            for sample in samples {
                let quantized = (sample.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
                writer.write_sample(quantized).unwrap();
                writer.write_sample(quantized).unwrap();
            }
            writer.finalize().unwrap();
        }

        let files = [
            (
                "40-filter-drive-dry.wav",
                dry.clone(),
                "unprocessed deterministic reference",
            ),
            (
                "41-filter-drive-5k-legacy.wav",
                render(5_000.0, 20.0, true),
                "previous cubic drive, low-pass 5 kHz, resonance 20%, drive +12 dB",
            ),
            (
                "42-filter-drive-5k-adaa.wav",
                render(5_000.0, 20.0, false),
                "current ADAA cubic drive, low-pass 5 kHz, resonance 20%, drive +12 dB",
            ),
            (
                "43-filter-drive-15k-legacy.wav",
                render(15_000.0, 70.0, true),
                "previous cubic drive, low-pass 15 kHz, resonance 70%, drive +12 dB",
            ),
            (
                "44-filter-drive-15k-adaa.wav",
                render(15_000.0, 70.0, false),
                "current ADAA cubic drive, low-pass 15 kHz, resonance 70%, drive +12 dB",
            ),
        ];
        let mut rows = Vec::new();
        for (name, samples, purpose) in files {
            let (matched, gain_db) = level_match(&samples);
            write_wav(&destination.join(name), &matched);
            rows.push(format!("- `{name}` — {purpose}; gain {gain_db:+.2} dB"));
        }
        let mut manifest = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination.join("FILTER_MANIFEST.md"))
            .unwrap();
        writeln!(
            manifest,
            "# Private filter-drive audition\n\nSynthetic evidence/audition material; no listening judgment is implied.\n\n- Format: stereo 24-bit PCM WAV, 48 kHz, 3.000 s\n- Input: deterministic rising 2–16 kHz sinusoid, fixed 10.5 kHz tone, and low bounded deterministic noise\n- Processor: isolated SHR cubic pre-drive and low-pass TPT state; drive +12 dB and 100% effect mix; cutoff/resonance as named\n- Level control: every file is independently RMS-matched to -18 dBFS with a -1 dBFS peak ceiling\n- Alignment: reset at frame zero with no post-render latency or phase alignment; ADAA phase/amplitude consequences remain exposed\n- Listening questions: compare 41/42 for low-frequency foldback passing a 5 kHz cutoff and 43/44 for the higher-cutoff resonant case; 41 deliberately exposes the strongest measured baseline artifact\n\n{}",
            rows.join("\n")
        )
        .unwrap();
    }
}
