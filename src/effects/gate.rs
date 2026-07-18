use super::EffectError;
use crate::audio_graph::EffectInstance;
use crate::dsp::{db_to_gain, finite_or_zero, EnvelopeFollower, StereoFrame};
use crate::effect_schema;

const DETECTOR_ATTACK_MS: f32 = 0.5;
const DETECTOR_RELEASE_MS: f32 = 10.0;

pub(super) struct Gate {
    sample_rate: f32,
    open_threshold: f32,
    close_threshold: f32,
    range_gain: f32,
    attack_coefficient: f32,
    release_coefficient: f32,
    hold_samples: u32,
    hold_remaining: u32,
    open: bool,
    gain: f32,
    detector: EnvelopeFollower,
}

impl Gate {
    pub(super) fn compile(effect: &EffectInstance, sample_rate: u32) -> Result<Self, EffectError> {
        let value = |name| {
            effect_schema::parameter(effect, name)
                .map_err(|error| EffectError::new(error.to_string()))
        };
        let sample_rate = sample_rate as f32;
        let threshold_db = value("threshold_db")?;
        let hysteresis_db = value("hysteresis_db")?;
        let range_gain = db_to_gain(value("range_db")?)?;
        let hold_samples = milliseconds_to_samples(value("hold_ms")?, sample_rate);
        Ok(Self {
            sample_rate,
            open_threshold: db_to_gain(threshold_db)?,
            close_threshold: db_to_gain(threshold_db - hysteresis_db)?,
            range_gain,
            attack_coefficient: time_coefficient(value("attack_ms")?, sample_rate),
            release_coefficient: time_coefficient(value("release_ms")?, sample_rate),
            hold_samples,
            hold_remaining: 0,
            open: false,
            gain: range_gain,
            detector: EnvelopeFollower::new(DETECTOR_ATTACK_MS, DETECTOR_RELEASE_MS, sample_rate)?,
        })
    }

    #[inline]
    pub(super) fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        let magnitude = frame.left.abs().max(frame.right.abs());
        let envelope = self.detector.process_magnitude(magnitude);
        self.update_gate(envelope);
        StereoFrame::new(frame.left * self.gain, frame.right * self.gain).finite_or_silence()
    }

    #[inline]
    fn update_gate(&mut self, envelope: f32) {
        if envelope >= self.open_threshold {
            self.open = true;
            self.hold_remaining = self.hold_samples;
        } else if self.open && envelope < self.close_threshold {
            if self.hold_remaining > 0 {
                self.hold_remaining -= 1;
            } else {
                self.open = false;
            }
        } else if self.open {
            self.hold_remaining = self.hold_samples;
        }
        let target = if self.open { 1.0 } else { self.range_gain };
        let coefficient = if target > self.gain {
            self.attack_coefficient
        } else {
            self.release_coefficient
        };
        self.gain = finite_or_zero(target + coefficient * (self.gain - target))
            .clamp(self.range_gain.min(1.0), 1.0);
    }

    pub(super) fn set_parameter(&mut self, name: &str, value: f32) -> Result<(), EffectError> {
        match name {
            "threshold_db" => {
                let hysteresis_db = 20.0 * (self.open_threshold / self.close_threshold).log10();
                self.open_threshold = db_to_gain(value)?;
                self.close_threshold = db_to_gain(value - hysteresis_db)?;
            }
            "hysteresis_db" => {
                let threshold_db = 20.0 * self.open_threshold.log10();
                self.close_threshold = db_to_gain(threshold_db - value)?;
            }
            "range_db" => self.range_gain = db_to_gain(value)?,
            "attack_ms" => self.attack_coefficient = time_coefficient(value, self.sample_rate),
            "hold_ms" => self.hold_samples = milliseconds_to_samples(value, self.sample_rate),
            "release_ms" => self.release_coefficient = time_coefficient(value, self.sample_rate),
            _ => return Err(EffectError::new(format!("unknown Gate parameter {name}"))),
        }
        Ok(())
    }

    pub(super) fn reset(&mut self) {
        self.detector.reset();
        self.open = false;
        self.hold_remaining = 0;
        self.gain = self.range_gain;
    }
}

fn time_coefficient(milliseconds: f32, sample_rate: f32) -> f32 {
    (-1.0 / (milliseconds * 0.001 * sample_rate)).exp()
}

fn milliseconds_to_samples(milliseconds: f32, sample_rate: f32) -> u32 {
    (milliseconds * 0.001 * sample_rate).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_graph::{EffectKind, EFFECT_FORMAT_VERSION};
    use crate::dsp::allocation_test::assert_no_allocations;
    use crate::effects::EffectSlot;

    fn effect(parameters: impl IntoIterator<Item = (&'static str, f32)>) -> EffectInstance {
        EffectInstance {
            id: 6,
            kind: EffectKind::Gate,
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
    fn hysteresis_and_hold_state_machine_are_exact() {
        let mut gate = Gate::compile(
            &effect([
                ("threshold_db", -20.0),
                ("hysteresis_db", 6.0),
                ("hold_ms", 1.0),
            ]),
            48_000,
        )
        .unwrap();
        gate.update_gate(gate.open_threshold * 0.99);
        assert!(!gate.open);
        gate.update_gate(gate.open_threshold);
        assert!(gate.open);
        gate.update_gate((gate.open_threshold + gate.close_threshold) * 0.5);
        assert!(gate.open);
        for _ in 0..gate.hold_samples {
            gate.update_gate(0.0);
            assert!(gate.open);
        }
        gate.update_gate(0.0);
        assert!(!gate.open);
    }

    #[test]
    fn opening_and_closing_envelopes_are_monotonic_and_linked() {
        let mut gate = Gate::compile(
            &effect([
                ("threshold_db", -40.0),
                ("range_db", -60.0),
                ("attack_ms", 10.0),
                ("hold_ms", 0.0),
                ("release_ms", 100.0),
            ]),
            48_000,
        )
        .unwrap();
        let mut previous = gate.gain;
        for _ in 0..480 {
            gate.update_gate(1.0);
            assert!(gate.gain >= previous);
            previous = gate.gain;
        }
        assert!((gate.gain - (1.0 - (-1.0_f32).exp())).abs() < 0.01);
        for _ in 0..4_800 {
            let before = gate.gain;
            gate.update_gate(0.0);
            assert!(gate.gain <= before);
        }
        assert!(gate.gain < 0.38);

        let mut linked = EffectSlot::compile(
            &effect([
                ("threshold_db", -30.0),
                ("range_db", -60.0),
                ("attack_ms", 0.1),
            ]),
            48_000,
            128,
        )
        .unwrap();
        let mut block = [StereoFrame::new(1.0, 0.1); 128];
        linked.process(&mut block);
        for frame in block.iter().skip(64) {
            assert!((frame.left * 0.1 - frame.right).abs() < 1.0e-6);
        }
    }

    #[test]
    fn configured_range_settles_to_the_declared_attenuation() {
        let mut gate = Gate::compile(
            &effect([("range_db", -40.0), ("hold_ms", 0.0), ("release_ms", 20.0)]),
            48_000,
        )
        .unwrap();
        gate.open = true;
        gate.gain = 1.0;
        for _ in 0..48_000 {
            gate.update_gate(0.0);
        }
        assert!((20.0 * gate.gain.log10() + 40.0).abs() < 0.01);
    }

    #[test]
    fn silence_random_chunks_reset_bypass_and_allocation_are_safe() {
        let configured = effect([
            ("threshold_db", -36.0),
            ("hysteresis_db", 8.0),
            ("range_db", -50.0),
            ("attack_ms", 2.0),
            ("hold_ms", 25.0),
            ("release_ms", 120.0),
        ]);
        let input = (0..4_096)
            .map(|index| {
                let value = ((index * 23 % 251) as f32 / 125.0) - 1.0;
                StereoFrame::new(value, value * -0.2)
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
        assert!(actual
            .iter()
            .all(|frame| frame.left.is_finite() && frame.right.is_finite()));

        odd.reset();
        let mut silence = [StereoFrame::SILENCE; 256];
        odd.process(&mut silence);
        assert_eq!(silence, [StereoFrame::SILENCE; 256]);
        odd.set_bypass(true).unwrap();
        let mut dry = [StereoFrame::new(0.25, -0.5); 256];
        odd.process(&mut dry);
        assert_eq!(dry[255], StereoFrame::new(0.25, -0.5));
    }

    #[test]
    fn all_rates_limits_and_rapid_moves_remain_finite() {
        for sample_rate in [8_000, 44_100, 48_000, 96_000, 384_000] {
            let mut slot = EffectSlot::compile(&effect([]), sample_rate, 63).unwrap();
            for index in 0..100 {
                slot.set_parameter("threshold_db", -80.0 + (index % 81) as f32)
                    .unwrap();
                slot.set_parameter("hysteresis_db", (index % 25) as f32)
                    .unwrap();
                slot.set_parameter("range_db", -80.0 + (index % 81) as f32)
                    .unwrap();
                let mut block = [StereoFrame::new(1.0, -1.0); 63];
                slot.process(&mut block);
                assert!(block
                    .iter()
                    .all(|frame| frame.left.is_finite() && frame.right.is_finite()));
            }
        }
    }
}
