use super::{smooth, EffectError, PARAMETER_SMOOTH_SAMPLES};
use crate::audio_graph::EffectInstance;
use crate::dsp::{db_to_gain, Biquad, BiquadCoefficients, SmoothedValue, StereoFrame};
use crate::effect_schema;

const LOW_CUT_Q1: f32 = 0.541_196_1;
const LOW_CUT_Q2: f32 = 1.306_563;
const BELL_Q: f32 = 0.9;
const SHELF_SLOPE: f32 = 1.0;

#[derive(Clone, Copy)]
struct StereoBiquad {
    left: Biquad,
    right: Biquad,
}

impl StereoBiquad {
    const fn new(coefficients: BiquadCoefficients) -> Self {
        Self {
            left: Biquad::new(coefficients),
            right: Biquad::new(coefficients),
        }
    }

    fn set_coefficients(&mut self, coefficients: BiquadCoefficients) -> Result<(), EffectError> {
        self.left.set_coefficients(coefficients)?;
        self.right.set_coefficients(coefficients)?;
        Ok(())
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

/// Crossfade between two individually stable filters. Coefficients are
/// calculated on the control side; the sample path only processes and mixes.
struct SmoothedStereoBiquad {
    current: StereoBiquad,
    next: StereoBiquad,
    mix: SmoothedValue,
    transitioning: bool,
    pending: Option<BiquadCoefficients>,
}

impl SmoothedStereoBiquad {
    fn new(coefficients: BiquadCoefficients) -> Self {
        Self {
            current: StereoBiquad::new(coefficients),
            next: StereoBiquad::new(coefficients),
            mix: smooth(0.0),
            transitioning: false,
            pending: None,
        }
    }

    fn set_coefficients(&mut self, coefficients: BiquadCoefficients) -> Result<(), EffectError> {
        if self.transitioning {
            self.pending = Some(coefficients);
            return Ok(());
        }
        self.begin_transition(coefficients)
    }

    fn begin_transition(&mut self, coefficients: BiquadCoefficients) -> Result<(), EffectError> {
        self.next = self.current;
        self.next.set_coefficients(coefficients)?;
        self.mix.reset(0.0)?;
        self.mix.set_target(1.0, PARAMETER_SMOOTH_SAMPLES)?;
        self.transitioning = true;
        Ok(())
    }

    #[inline]
    fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        if !self.transitioning {
            return self.current.process(frame);
        }
        let old = self.current.process(frame);
        let new = self.next.process(frame);
        let mix = self.mix.next_value();
        let output = StereoFrame::new(
            old.left + (new.left - old.left) * mix,
            old.right + (new.right - old.right) * mix,
        );
        if mix >= 1.0 {
            self.current = self.next;
            if let Some(coefficients) = self.pending.take() {
                self.next = self.current;
                self.transitioning = self.next.set_coefficients(coefficients).is_ok()
                    && self.mix.reset(0.0).is_ok()
                    && self.mix.set_target(1.0, PARAMETER_SMOOTH_SAMPLES).is_ok();
            } else {
                self.transitioning = false;
            }
        }
        output
    }

    fn reset(&mut self) {
        self.current.reset();
        self.next.reset();
        self.pending = None;
    }
}

#[derive(Clone, Copy)]
struct Parameters {
    low_cut_enabled: bool,
    low_cut_hz: f32,
    low_shelf_hz: f32,
    low_shelf_db: f32,
    low_mid_hz: f32,
    low_mid_db: f32,
    high_mid_hz: f32,
    high_mid_db: f32,
    high_shelf_hz: f32,
    high_shelf_db: f32,
}

pub(super) struct Eq {
    sample_rate: f32,
    parameters: Parameters,
    low_cut_1: SmoothedStereoBiquad,
    low_cut_2: SmoothedStereoBiquad,
    low_shelf: SmoothedStereoBiquad,
    low_mid: SmoothedStereoBiquad,
    high_mid: SmoothedStereoBiquad,
    high_shelf: SmoothedStereoBiquad,
    output_trim: SmoothedValue,
}

impl Eq {
    pub(super) fn compile(effect: &EffectInstance, sample_rate: u32) -> Result<Self, EffectError> {
        let value = |name| {
            effect_schema::parameter(effect, name)
                .map_err(|error| EffectError::new(error.to_string()))
        };
        let parameters = Parameters {
            low_cut_enabled: value("low_cut_enabled")? == 1.0,
            low_cut_hz: value("low_cut_hz")?,
            low_shelf_hz: value("low_shelf_hz")?,
            low_shelf_db: value("low_shelf_db")?,
            low_mid_hz: value("low_mid_hz")?,
            low_mid_db: value("low_mid_db")?,
            high_mid_hz: value("high_mid_hz")?,
            high_mid_db: value("high_mid_db")?,
            high_shelf_hz: value("high_shelf_hz")?,
            high_shelf_db: value("high_shelf_db")?,
        };
        let sample_rate = sample_rate as f32;
        let (low_cut_1, low_cut_2) = low_cut_coefficients(parameters, sample_rate)?;
        Ok(Self {
            sample_rate,
            parameters,
            low_cut_1: SmoothedStereoBiquad::new(low_cut_1),
            low_cut_2: SmoothedStereoBiquad::new(low_cut_2),
            low_shelf: SmoothedStereoBiquad::new(BiquadCoefficients::low_shelf(
                parameters.low_shelf_hz,
                SHELF_SLOPE,
                parameters.low_shelf_db,
                sample_rate,
            )?),
            low_mid: SmoothedStereoBiquad::new(BiquadCoefficients::peaking(
                parameters.low_mid_hz,
                BELL_Q,
                parameters.low_mid_db,
                sample_rate,
            )?),
            high_mid: SmoothedStereoBiquad::new(BiquadCoefficients::peaking(
                parameters.high_mid_hz,
                BELL_Q,
                parameters.high_mid_db,
                sample_rate,
            )?),
            high_shelf: SmoothedStereoBiquad::new(BiquadCoefficients::high_shelf(
                parameters.high_shelf_hz,
                SHELF_SLOPE,
                parameters.high_shelf_db,
                sample_rate,
            )?),
            output_trim: smooth(db_to_gain(value("output_trim_db")?)?),
        })
    }

    #[inline]
    pub(super) fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        let frame = self.low_cut_1.process(frame);
        let frame = self.low_cut_2.process(frame);
        let frame = self.low_shelf.process(frame);
        let frame = self.low_mid.process(frame);
        let frame = self.high_mid.process(frame);
        let frame = self.high_shelf.process(frame);
        let trim = self.output_trim.next_value();
        StereoFrame::new(frame.left * trim, frame.right * trim).finite_or_silence()
    }

    pub(super) fn set_parameter(&mut self, name: &str, value: f32) -> Result<(), EffectError> {
        match name {
            "low_cut_enabled" => {
                self.parameters.low_cut_enabled = value == 1.0;
                self.update_low_cut()
            }
            "low_cut_hz" => {
                self.parameters.low_cut_hz = value;
                self.update_low_cut()
            }
            "low_shelf_hz" => {
                self.parameters.low_shelf_hz = value;
                self.update_low_shelf()
            }
            "low_shelf_db" => {
                self.parameters.low_shelf_db = value;
                self.update_low_shelf()
            }
            "low_mid_hz" => {
                self.parameters.low_mid_hz = value;
                self.update_low_mid()
            }
            "low_mid_db" => {
                self.parameters.low_mid_db = value;
                self.update_low_mid()
            }
            "high_mid_hz" => {
                self.parameters.high_mid_hz = value;
                self.update_high_mid()
            }
            "high_mid_db" => {
                self.parameters.high_mid_db = value;
                self.update_high_mid()
            }
            "high_shelf_hz" => {
                self.parameters.high_shelf_hz = value;
                self.update_high_shelf()
            }
            "high_shelf_db" => {
                self.parameters.high_shelf_db = value;
                self.update_high_shelf()
            }
            "output_trim_db" => {
                self.output_trim
                    .set_target(db_to_gain(value)?, PARAMETER_SMOOTH_SAMPLES)?;
                Ok(())
            }
            _ => Err(EffectError::new(format!("unknown Eq parameter {name}"))),
        }
    }

    fn update_low_cut(&mut self) -> Result<(), EffectError> {
        let (first, second) = low_cut_coefficients(self.parameters, self.sample_rate)?;
        self.low_cut_1.set_coefficients(first)?;
        self.low_cut_2.set_coefficients(second)
    }

    fn update_low_shelf(&mut self) -> Result<(), EffectError> {
        self.low_shelf
            .set_coefficients(BiquadCoefficients::low_shelf(
                self.parameters.low_shelf_hz,
                SHELF_SLOPE,
                self.parameters.low_shelf_db,
                self.sample_rate,
            )?)
    }

    fn update_low_mid(&mut self) -> Result<(), EffectError> {
        self.low_mid.set_coefficients(BiquadCoefficients::peaking(
            self.parameters.low_mid_hz,
            BELL_Q,
            self.parameters.low_mid_db,
            self.sample_rate,
        )?)
    }

    fn update_high_mid(&mut self) -> Result<(), EffectError> {
        self.high_mid.set_coefficients(BiquadCoefficients::peaking(
            self.parameters.high_mid_hz,
            BELL_Q,
            self.parameters.high_mid_db,
            self.sample_rate,
        )?)
    }

    fn update_high_shelf(&mut self) -> Result<(), EffectError> {
        self.high_shelf
            .set_coefficients(BiquadCoefficients::high_shelf(
                self.parameters.high_shelf_hz,
                SHELF_SLOPE,
                self.parameters.high_shelf_db,
                self.sample_rate,
            )?)
    }

    pub(super) fn reset(&mut self) {
        self.low_cut_1.reset();
        self.low_cut_2.reset();
        self.low_shelf.reset();
        self.low_mid.reset();
        self.high_mid.reset();
        self.high_shelf.reset();
    }
}

fn low_cut_coefficients(
    parameters: Parameters,
    sample_rate: f32,
) -> Result<(BiquadCoefficients, BiquadCoefficients), EffectError> {
    if !parameters.low_cut_enabled {
        return Ok((BiquadCoefficients::IDENTITY, BiquadCoefficients::IDENTITY));
    }
    Ok((
        BiquadCoefficients::high_pass(parameters.low_cut_hz, LOW_CUT_Q1, sample_rate)?,
        BiquadCoefficients::high_pass(parameters.low_cut_hz, LOW_CUT_Q2, sample_rate)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_graph::{EffectKind, EFFECT_FORMAT_VERSION};
    use crate::dsp::allocation_test::assert_no_allocations;
    use crate::effect_schema;
    use crate::effects::EffectSlot;

    fn effect(parameters: impl IntoIterator<Item = (&'static str, f32)>) -> EffectInstance {
        EffectInstance {
            id: 2,
            kind: EffectKind::Eq,
            version: EFFECT_FORMAT_VERSION,
            bypass: false,
            parameters: parameters
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
            owned_memory_bytes: 0,
        }
    }

    fn magnitude(coefficients: BiquadCoefficients, frequency: f64, sample_rate: f64) -> f64 {
        let omega = 2.0 * std::f64::consts::PI * frequency / sample_rate;
        let cos_1 = omega.cos();
        let sin_1 = omega.sin();
        let cos_2 = (2.0 * omega).cos();
        let sin_2 = (2.0 * omega).sin();
        let numerator_real = coefficients.b0 as f64
            + coefficients.b1 as f64 * cos_1
            + coefficients.b2 as f64 * cos_2;
        let numerator_imag = -coefficients.b1 as f64 * sin_1 - coefficients.b2 as f64 * sin_2;
        let denominator_real =
            1.0 + coefficients.a1 as f64 * cos_1 + coefficients.a2 as f64 * cos_2;
        let denominator_imag = -coefficients.a1 as f64 * sin_1 - coefficients.a2 as f64 * sin_2;
        ((numerator_real * numerator_real + numerator_imag * numerator_imag)
            / (denominator_real * denominator_real + denominator_imag * denominator_imag))
            .sqrt()
    }

    fn db(gain: f64) -> f64 {
        20.0 * gain.log10()
    }

    #[test]
    fn fourth_order_low_cut_has_butterworth_corner_and_twenty_four_db_slope() {
        let first = BiquadCoefficients::high_pass(400.0, LOW_CUT_Q1, 48_000.0).unwrap();
        let second = BiquadCoefficients::high_pass(400.0, LOW_CUT_Q2, 48_000.0).unwrap();
        let response = |frequency| {
            magnitude(first, frequency, 48_000.0) * magnitude(second, frequency, 48_000.0)
        };
        assert!((db(response(400.0)) + 3.0103).abs() < 0.01);
        let octave_slope = db(response(100.0)) - db(response(50.0));
        assert!((octave_slope - 24.0).abs() < 0.25, "{octave_slope}");
        assert!(db(response(4_000.0)).abs() < 0.001);
    }

    #[test]
    fn shelves_and_bells_hit_declared_gain_targets() {
        let low = BiquadCoefficients::low_shelf(200.0, SHELF_SLOPE, 12.0, 48_000.0).unwrap();
        assert!((db(magnitude(low, 2.0, 48_000.0)) - 12.0).abs() < 0.02);
        assert!(db(magnitude(low, 10_000.0, 48_000.0)).abs() < 0.01);

        let bell = BiquadCoefficients::peaking(1_000.0, BELL_Q, -9.0, 48_000.0).unwrap();
        assert!((db(magnitude(bell, 1_000.0, 48_000.0)) + 9.0).abs() < 0.01);

        let high = BiquadCoefficients::high_shelf(5_000.0, SHELF_SLOPE, 6.0, 48_000.0).unwrap();
        assert!((db(magnitude(high, 20_000.0, 48_000.0)) - 6.0).abs() < 0.1);
        assert!(db(magnitude(high, 20.0, 48_000.0)).abs() < 0.01);
    }

    #[test]
    fn unity_eq_is_sample_exact_silent_and_allocation_free() {
        let mut slot = EffectSlot::compile(&effect([]), 48_000, 128).unwrap();
        let mut input = [StereoFrame::SILENCE; 128];
        for (index, frame) in input.iter_mut().enumerate() {
            let value = ((index * 37 % 101) as f32 / 50.0) - 1.0;
            *frame = StereoFrame::new(value, -value * 0.75);
        }
        let expected = input;
        assert_no_allocations(|| slot.process(&mut input));
        assert_eq!(input, expected);
        let mut silence = [StereoFrame::SILENCE; 128];
        slot.process(&mut silence);
        assert_eq!(silence, [StereoFrame::SILENCE; 128]);
    }

    #[test]
    fn impulse_random_maximum_and_sample_rate_extremes_remain_bounded() {
        for sample_rate in [8_000, 44_100, 48_000, 96_000, 384_000] {
            let mut slot = EffectSlot::compile(
                &effect([
                    ("low_cut_enabled", 1.0),
                    ("low_cut_hz", 500.0),
                    ("low_shelf_db", 18.0),
                    ("low_mid_db", 18.0),
                    ("high_mid_db", 18.0),
                    ("high_shelf_hz", 20_000.0),
                    ("high_shelf_db", 18.0),
                    ("output_trim_db", 12.0),
                ]),
                sample_rate,
                127,
            )
            .unwrap();
            let mut state = 0x1234_5678_u32;
            let mut maximum = 0.0_f32;
            for block_index in 0..200 {
                let mut block = [StereoFrame::SILENCE; 127];
                for (index, frame) in block.iter_mut().enumerate() {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let random = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                    let input = if block_index == 0 && index == 0 {
                        1.0
                    } else if block_index == 1 {
                        if index % 2 == 0 {
                            1.0
                        } else {
                            -1.0
                        }
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
            assert!(maximum < 1_000.0, "{sample_rate}: {maximum}");
            slot.reset();
            let mut silence = [StereoFrame::SILENCE; 127];
            slot.process(&mut silence);
            assert_eq!(silence, [StereoFrame::SILENCE; 127]);
        }
    }

    #[test]
    fn parameter_sweeps_and_chunk_sizes_stay_finite_and_invariant() {
        let configured = effect([
            ("low_cut_enabled", 1.0),
            ("low_cut_hz", 120.0),
            ("low_shelf_db", 5.0),
            ("low_mid_db", -4.0),
            ("high_mid_db", 3.0),
            ("high_shelf_db", -2.0),
        ]);
        let input = (0..4_096)
            .map(|index| {
                let value = ((index * 19 % 257) as f32 / 128.0) - 1.0;
                StereoFrame::new(value, value * 0.37)
            })
            .collect::<Vec<_>>();
        let mut whole = EffectSlot::compile(&configured, 48_000, 256).unwrap();
        let mut expected = input.clone();
        for chunk in expected.chunks_mut(256) {
            whole.process(chunk);
        }
        let mut odd = EffectSlot::compile(&configured, 48_000, 256).unwrap();
        let mut actual = input;
        for chunk in actual.chunks_mut(37) {
            odd.process(chunk);
        }
        assert_eq!(actual, expected);

        for index in 0..200 {
            odd.set_parameter("low_cut_hz", 20.0 + (index % 100) as f32 * 4.8)
                .unwrap();
            odd.set_parameter("high_mid_db", if index % 2 == 0 { -18.0 } else { 18.0 })
                .unwrap();
            let mut block = [StereoFrame::new(1.0, -1.0); 31];
            odd.process(&mut block);
            assert!(block
                .iter()
                .all(|frame| frame.left.is_finite() && frame.right.is_finite()));
        }
    }

    #[test]
    fn every_schema_limit_compiles_and_bypass_reaches_dry() {
        for spec in effect_schema::schema(EffectKind::Eq) {
            for value in [spec.default, spec.minimum, spec.maximum] {
                if spec.accepts(value) {
                    EffectSlot::compile(&effect([(spec.name, value)]), 48_000, 256).unwrap();
                }
            }
        }
        let mut slot = EffectSlot::compile(
            &effect([("low_shelf_db", 18.0), ("output_trim_db", 12.0)]),
            48_000,
            256,
        )
        .unwrap();
        slot.set_bypass(true).unwrap();
        let mut block = [StereoFrame::new(0.25, -0.5); 256];
        slot.process(&mut block);
        assert_eq!(block[255], StereoFrame::new(0.25, -0.5));
    }
}
