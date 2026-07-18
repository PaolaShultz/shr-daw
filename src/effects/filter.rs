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
        let driven_left = drive(frame.left, drive_gain, drive_mix);
        let driven_right = drive(frame.right, drive_gain, drive_mix);
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
fn drive(input: f32, gain: f32, mix: f32) -> f32 {
    if mix <= 0.0 {
        return input;
    }
    let driven = input * gain;
    let saturated = if driven >= 1.0 {
        1.0
    } else if driven <= -1.0 {
        -1.0
    } else {
        1.5 * (driven - driven * driven * driven / 3.0)
    };
    finite_or_zero(input + (saturated - input) * mix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_graph::{EffectKind, EFFECT_FORMAT_VERSION};
    use crate::dsp::allocation_test::assert_no_allocations;
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
        for index in 0..=20_000 {
            let input = -1.0 + index as f32 * 0.0001;
            assert_eq!(drive(input, 1.0, 0.0), input);
        }
        assert!(drive(0.75, db_to_gain(12.0).unwrap(), 1.0) <= 1.0);
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
}
