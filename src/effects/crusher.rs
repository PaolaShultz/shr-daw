use super::{EffectError, PARAMETER_SMOOTH_SAMPLES};
use crate::audio_graph::EffectInstance;
use crate::dsp::{SmoothedValue, StereoFrame};
use crate::effect_schema;

pub(super) struct Crusher {
    bit_depth: u8,
    hold_factor: u8,
    dither: bool,
    mix: SmoothedValue,
    counter: u8,
    held: StereoFrame,
    random_left: u32,
    random_right: u32,
}

impl Crusher {
    pub(super) fn compile(effect: &EffectInstance) -> Result<Self, EffectError> {
        let value = |name| {
            effect_schema::parameter(effect, name)
                .map_err(|error| EffectError::new(error.to_string()))
        };
        Ok(Self {
            bit_depth: value("bit_depth")? as u8,
            hold_factor: value("hold_factor")? as u8,
            dither: value("dither")? == 1.0,
            mix: SmoothedValue::new(value("mix_percent")? * 0.01)?,
            counter: 0,
            held: StereoFrame::SILENCE,
            random_left: 0x1234_5678,
            random_right: 0x9abc_def0,
        })
    }

    #[inline]
    pub(super) fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        if self.counter == 0 {
            self.held = StereoFrame::new(
                quantize(
                    frame.left,
                    self.bit_depth,
                    self.dither,
                    &mut self.random_left,
                ),
                quantize(
                    frame.right,
                    self.bit_depth,
                    self.dither,
                    &mut self.random_right,
                ),
            );
        }
        self.counter += 1;
        if self.counter >= self.hold_factor {
            self.counter = 0;
        }
        let mix = self.mix.next_value();
        if mix >= 1.0 {
            self.held
        } else {
            StereoFrame::new(
                frame.left + (self.held.left - frame.left) * mix,
                frame.right + (self.held.right - frame.right) * mix,
            )
            .finite_or_silence()
        }
    }

    pub(super) fn set_parameter(&mut self, name: &str, value: f32) -> Result<(), EffectError> {
        match name {
            "bit_depth" => self.bit_depth = value as u8,
            "hold_factor" => {
                self.hold_factor = value as u8;
                self.counter = 0;
            }
            "dither" => self.dither = value == 1.0,
            "mix_percent" => {
                self.mix
                    .set_target(value * 0.01, PARAMETER_SMOOTH_SAMPLES)?;
            }
            _ => {
                return Err(EffectError::new(format!(
                    "unknown Crusher parameter {name}"
                )))
            }
        }
        Ok(())
    }

    pub(super) fn reset(&mut self) {
        self.counter = 0;
        self.held = StereoFrame::SILENCE;
        self.random_left = 0x1234_5678;
        self.random_right = 0x9abc_def0;
    }
}

#[inline]
fn quantize(input: f32, bits: u8, dither: bool, random: &mut u32) -> f32 {
    let scale = (1_u32 << (bits - 1)) as f32;
    let dither_value = if dither {
        let first = uniform(random);
        let second = uniform(random);
        (first - second) / scale
    } else {
        0.0
    };
    let minimum = -scale;
    let maximum = scale - 1.0;
    ((input.clamp(-1.0, 1.0) + dither_value) * scale)
        .round()
        .clamp(minimum, maximum)
        / scale
}

#[inline]
fn uniform(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*state >> 8) as f32 / 16_777_215.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_graph::{EffectKind, EFFECT_FORMAT_VERSION};
    use crate::dsp::allocation_test::assert_no_allocations;
    use crate::effects::EffectSlot;

    fn effect(parameters: impl IntoIterator<Item = (&'static str, f32)>) -> EffectInstance {
        EffectInstance {
            id: 5,
            kind: EffectKind::Crusher,
            version: EFFECT_FORMAT_VERSION,
            bypass: false,
            parameters: parameters
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
            owned_memory_bytes: 0,
        }
    }

    #[test]
    fn signed_pcm_quantization_has_exact_steps_and_bounds() {
        let mut random = 1;
        let values = (-1_000..=1_000)
            .map(|index| quantize(index as f32 * 0.001, 4, false, &mut random))
            .collect::<Vec<_>>();
        assert!(values
            .iter()
            .all(|value| { (-1.0..=0.875).contains(value) && (value * 8.0).fract() == 0.0 }));
        assert_eq!(quantize(-1.0, 4, false, &mut random), -1.0);
        assert_eq!(quantize(0.0, 4, false, &mut random), 0.0);
        assert_eq!(quantize(1.0, 4, false, &mut random), 0.875);
        assert_eq!(quantize(1.0, 16, false, &mut random), 32_767.0 / 32_768.0);
    }

    #[test]
    fn hold_factor_owns_exact_sample_windows() {
        let mut slot = EffectSlot::compile(
            &effect([("bit_depth", 4.0), ("hold_factor", 4.0)]),
            48_000,
            8,
        )
        .unwrap();
        let mut block = [
            StereoFrame::new(1.0, -1.0),
            StereoFrame::SILENCE,
            StereoFrame::SILENCE,
            StereoFrame::SILENCE,
            StereoFrame::new(0.5, -0.5),
            StereoFrame::SILENCE,
            StereoFrame::SILENCE,
            StereoFrame::SILENCE,
        ];
        slot.process(&mut block);
        assert_eq!(block[..4], [StereoFrame::new(0.875, -1.0); 4]);
        assert_eq!(block[4..], [StereoFrame::new(0.5, -0.5); 4]);
    }

    #[test]
    fn deterministic_tpdf_dither_is_bounded_and_chunk_invariant() {
        let configured = effect([("bit_depth", 4.0), ("hold_factor", 1.0), ("dither", 1.0)]);
        let mut continuous = EffectSlot::compile(&configured, 48_000, 256).unwrap();
        let mut expected = [StereoFrame::new(0.01, -0.01); 4_096];
        for chunk in expected.chunks_mut(256) {
            continuous.process(chunk);
        }
        let mut odd = EffectSlot::compile(&configured, 48_000, 256).unwrap();
        let mut actual = [StereoFrame::new(0.01, -0.01); 4_096];
        for chunk in actual.chunks_mut(37) {
            odd.process(chunk);
        }
        assert_eq!(actual, expected);
        assert!(actual.iter().all(|frame| {
            (-1.0..=0.875).contains(&frame.left) && (-1.0..=0.875).contains(&frame.right)
        }));
        assert!(actual.iter().any(|frame| *frame != actual[0]));
    }

    #[test]
    fn silence_random_limits_reset_bypass_and_allocation_are_safe() {
        let configured = effect([("bit_depth", 4.0), ("hold_factor", 32.0)]);
        let mut slot = EffectSlot::compile(&configured, 48_000, 256).unwrap();
        let mut silence = [StereoFrame::SILENCE; 256];
        assert_no_allocations(|| slot.process(&mut silence));
        assert_eq!(silence, [StereoFrame::SILENCE; 256]);

        let mut state = 0x2468_ace0_u32;
        for _ in 0..200 {
            let mut block = [StereoFrame::SILENCE; 127];
            for frame in &mut block {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let value = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                *frame = StereoFrame::new(value, -value);
            }
            slot.process(&mut block);
            assert!(block.iter().all(|frame| {
                frame.left.is_finite()
                    && frame.right.is_finite()
                    && (-1.0..=0.875).contains(&frame.left)
                    && (-1.0..=0.875).contains(&frame.right)
            }));
        }
        slot.reset();
        slot.set_bypass(true).unwrap();
        let mut dry = [StereoFrame::new(0.25, -0.5); 256];
        slot.process(&mut dry);
        assert_eq!(dry[255], StereoFrame::new(0.25, -0.5));
    }

    #[test]
    fn rapid_valid_moves_and_all_sample_rates_remain_finite() {
        for sample_rate in [8_000, 44_100, 48_000, 96_000, 384_000] {
            let mut slot = EffectSlot::compile(&effect([]), sample_rate, 64).unwrap();
            for index in 0..100 {
                slot.set_parameter("bit_depth", 4.0 + (index % 13) as f32)
                    .unwrap();
                slot.set_parameter("hold_factor", 1.0 + (index % 32) as f32)
                    .unwrap();
                slot.set_parameter("dither", (index % 2) as f32).unwrap();
                let mut block = [StereoFrame::new(1.0, -1.0); 64];
                slot.process(&mut block);
                assert!(block
                    .iter()
                    .all(|frame| frame.left.is_finite() && frame.right.is_finite()));
            }
        }
    }
}
