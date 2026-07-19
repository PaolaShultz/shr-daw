use super::{smooth, EffectError, PARAMETER_SMOOTH_SAMPLES};
use crate::audio_graph::EffectInstance;
use crate::dsp::{FractionalDelayLine, OnePole, OnePoleMode, SmoothedValue, StereoFrame};
use crate::effect_schema;

const MAXIMUM_DELAY_SECONDS: f32 = 2.0;
const TIME_CHANGE_MILLISECONDS: f32 = 20.0;
const EMERGENCY_LEVEL: f32 = 64.0;
const DIVISION_BEATS: [f32; 8] = [0.0625, 0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Stereo,
    PingPong,
    MonoToStereo,
}

impl Mode {
    fn from_parameter(value: f32) -> Self {
        match value as u8 {
            0 => Self::Stereo,
            1 => Self::PingPong,
            _ => Self::MonoToStereo,
        }
    }
}

pub(super) struct Delay {
    sample_rate: f32,
    left: FractionalDelayLine,
    right: FractionalDelayLine,
    left_time: SmoothedValue,
    right_time: SmoothedValue,
    mode: Mode,
    tempo_sync: bool,
    tempo_bpm: f32,
    division: usize,
    time_ms: f32,
    stereo_ratio: f32,
    feedback: SmoothedValue,
    feedback_parameter: f32,
    tone_left: OnePole,
    tone_right: OnePole,
    wet: SmoothedValue,
    dry: SmoothedValue,
    input_gain: SmoothedValue,
    tail_on_bypass: bool,
    tail_bypassed: bool,
    wet_only_tail_bypass: bool,
    clear_after: u32,
}

impl Delay {
    pub(super) fn compile(effect: &EffectInstance, sample_rate: u32) -> Result<Self, EffectError> {
        let value = |name| {
            effect_schema::parameter(effect, name)
                .map_err(|error| EffectError::new(error.to_string()))
        };
        let sample_rate = sample_rate as f32;
        let capacity = (sample_rate * MAXIMUM_DELAY_SECONDS).ceil() as usize;
        let tempo_sync = value("tempo_sync")? == 1.0;
        let tempo_bpm = value("tempo_bpm")?;
        let division = value("division")? as usize;
        let time_ms = value("time_ms")?;
        let stereo_ratio = value("stereo_ratio")?;
        let (left_time, right_time) = delay_samples(
            sample_rate,
            tempo_sync,
            tempo_bpm,
            division,
            time_ms,
            stereo_ratio,
            capacity,
        );
        let feedback_parameter = value("feedback_percent")? * 0.01;
        Ok(Self {
            sample_rate,
            left: FractionalDelayLine::new(capacity)?,
            right: FractionalDelayLine::new(capacity)?,
            left_time: smooth(left_time),
            right_time: smooth(right_time),
            mode: Mode::from_parameter(value("mode")?),
            tempo_sync,
            tempo_bpm,
            division,
            time_ms,
            stereo_ratio,
            feedback: smooth(feedback_parameter),
            feedback_parameter,
            tone_left: OnePole::new(OnePoleMode::LowPass, value("tone_hz")?, sample_rate)?,
            tone_right: OnePole::new(OnePoleMode::LowPass, value("tone_hz")?, sample_rate)?,
            wet: smooth(value("wet_percent")? * 0.01),
            dry: smooth(value("dry_percent")? * 0.01),
            input_gain: smooth(1.0),
            tail_on_bypass: value("tail_on_bypass")? == 1.0,
            tail_bypassed: false,
            wet_only_tail_bypass: false,
            clear_after: 0,
        })
    }

    #[inline]
    pub(super) fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        let left_delay = self.left.read(self.left_time.next_value());
        let right_delay = self.right.read(self.right_time.next_value());
        let left_feedback = self.tone_left.process(left_delay);
        let right_feedback = self.tone_right.process(right_delay);
        let feedback = self.feedback.next_value();
        let input_gain = self.input_gain.next_value();
        let (input_left, input_right) = if self.mode == Mode::MonoToStereo {
            let mono = (frame.left + frame.right) * 0.5 * input_gain;
            (mono, mono)
        } else {
            (frame.left * input_gain, frame.right * input_gain)
        };
        let (write_left, write_right) = if self.mode == Mode::Stereo {
            (
                input_left + left_feedback * feedback,
                input_right + right_feedback * feedback,
            )
        } else {
            (
                input_left + right_feedback * feedback,
                input_right + left_feedback * feedback,
            )
        };
        if write_left.abs() > EMERGENCY_LEVEL
            || write_right.abs() > EMERGENCY_LEVEL
            || !write_left.is_finite()
            || !write_right.is_finite()
        {
            self.reset_lines();
        } else {
            self.left.push(write_left);
            self.right.push(write_right);
        }
        if self.clear_after > 0 {
            self.clear_after -= 1;
            if self.clear_after == 0 {
                self.reset_lines();
            }
        }
        let wet = self.wet.next_value();
        let dry = if self.tail_bypassed {
            if self.wet_only_tail_bypass {
                0.0
            } else {
                1.0
            }
        } else {
            self.dry.next_value()
        };
        StereoFrame::new(
            frame.left * dry + left_delay * wet,
            frame.right * dry + right_delay * wet,
        )
        .finite_or_silence()
    }

    pub(super) fn set_parameter(&mut self, name: &str, value: f32) -> Result<(), EffectError> {
        match name {
            "mode" => self.mode = Mode::from_parameter(value),
            "tempo_sync" => {
                self.tempo_sync = value == 1.0;
                self.update_time()?;
            }
            "tempo_bpm" => {
                self.tempo_bpm = value;
                self.update_time()?;
            }
            "division" => {
                self.division = value as usize;
                self.update_time()?;
            }
            "time_ms" => {
                self.time_ms = value;
                self.update_time()?;
            }
            "feedback_percent" => {
                self.feedback_parameter = value * 0.01;
                self.feedback
                    .set_target(self.feedback_parameter, PARAMETER_SMOOTH_SAMPLES)?;
            }
            "stereo_ratio" => {
                self.stereo_ratio = value;
                self.update_time()?;
            }
            "tone_hz" => {
                self.tone_left.configure(value, self.sample_rate)?;
                self.tone_right.configure(value, self.sample_rate)?;
            }
            "wet_percent" => self
                .wet
                .set_target(value * 0.01, PARAMETER_SMOOTH_SAMPLES)?,
            "dry_percent" => self
                .dry
                .set_target(value * 0.01, PARAMETER_SMOOTH_SAMPLES)?,
            "tail_on_bypass" => self.tail_on_bypass = value == 1.0,
            _ => return Err(EffectError::new(format!("unknown Delay parameter {name}"))),
        }
        Ok(())
    }

    pub(super) fn set_bypass(
        &mut self,
        bypass: bool,
        fade_samples: u32,
        wet_only_tail: bool,
    ) -> bool {
        if bypass && self.tail_on_bypass {
            self.tail_bypassed = true;
            self.wet_only_tail_bypass = wet_only_tail;
            let _ = self.input_gain.set_target(0.0, fade_samples);
            return true;
        }
        self.tail_bypassed = false;
        self.wet_only_tail_bypass = false;
        let _ = self
            .input_gain
            .set_target(if bypass { 0.0 } else { 1.0 }, fade_samples);
        if bypass {
            self.clear_after = fade_samples;
        } else {
            self.clear_after = 0;
        }
        false
    }

    fn update_time(&mut self) -> Result<(), EffectError> {
        let capacity = self.left.maximum_delay();
        let (left, right) = delay_samples(
            self.sample_rate,
            self.tempo_sync,
            self.tempo_bpm,
            self.division,
            self.time_ms,
            self.stereo_ratio,
            capacity,
        );
        let samples = (self.sample_rate * TIME_CHANGE_MILLISECONDS * 0.001).round() as u32;
        self.left_time.set_target(left, samples.max(1))?;
        self.right_time.set_target(right, samples.max(1))?;
        Ok(())
    }

    pub(super) fn reset(&mut self) {
        self.reset_lines();
        self.clear_after = 0;
    }

    fn reset_lines(&mut self) {
        self.left.reset();
        self.right.reset();
        self.tone_left.reset();
        self.tone_right.reset();
    }
}

fn delay_samples(
    sample_rate: f32,
    tempo_sync: bool,
    tempo_bpm: f32,
    division: usize,
    time_ms: f32,
    stereo_ratio: f32,
    capacity: usize,
) -> (f32, f32) {
    let milliseconds = if tempo_sync {
        60_000.0 / tempo_bpm * DIVISION_BEATS[division]
    } else {
        time_ms
    };
    let left = (milliseconds * sample_rate / 1_000.0).clamp(1.0, capacity as f32);
    let right = (left * stereo_ratio).clamp(1.0, capacity as f32);
    (left, right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_graph::{EffectKind, EFFECT_FORMAT_VERSION};
    use crate::dsp::allocation_test::assert_no_allocations;
    use crate::effects::EffectSlot;
    use std::collections::BTreeMap;

    fn effect(parameters: impl IntoIterator<Item = (&'static str, f32)>) -> EffectInstance {
        EffectInstance {
            id: 30,
            kind: EffectKind::Delay,
            version: EFFECT_FORMAT_VERSION,
            bypass: false,
            parameters: parameters
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect::<BTreeMap<_, _>>(),
            owned_memory_bytes: 0,
        }
    }

    #[test]
    fn free_and_tempo_synced_impulses_land_on_the_declared_sample() {
        for configured in [
            effect([
                ("time_ms", 10.0),
                ("wet_percent", 100.0),
                ("dry_percent", 0.0),
            ]),
            effect([
                ("tempo_sync", 1.0),
                ("tempo_bpm", 120.0),
                ("division", 0.0),
                ("wet_percent", 100.0),
                ("dry_percent", 0.0),
            ]),
        ] {
            let expected = if configured.parameters.contains_key("tempo_sync") {
                1_500
            } else {
                480
            };
            let mut slot = EffectSlot::compile(&configured, 48_000, 64).unwrap();
            let mut samples = vec![StereoFrame::SILENCE; expected + 2];
            samples[0] = StereoFrame::new(1.0, 1.0);
            for chunk in samples.chunks_mut(64) {
                slot.process(chunk);
            }
            assert!(samples[..expected]
                .iter()
                .all(|frame| frame.left.abs() < 1.0e-7));
            let measured = samples
                .iter()
                .enumerate()
                .max_by(|left, right| left.1.left.total_cmp(&right.1.left))
                .map(|(index, frame)| (index, frame.left))
                .unwrap();
            assert!(
                (samples[expected].left - 1.0).abs() < 1.0e-4,
                "expected {expected}, measured {measured:?}"
            );
        }
    }

    #[test]
    fn ping_pong_echo_crosses_channels_and_feedback_decays() {
        let mut slot = EffectSlot::compile(
            &effect([
                ("mode", 1.0),
                ("time_ms", 1.0),
                ("feedback_percent", 50.0),
                ("tone_hz", 18_000.0),
                ("wet_percent", 100.0),
                ("dry_percent", 0.0),
            ]),
            48_000,
            64,
        )
        .unwrap();
        let mut samples = vec![StereoFrame::SILENCE; 160];
        samples[0].left = 1.0;
        for chunk in samples.chunks_mut(37) {
            slot.process(chunk);
        }
        assert!(samples[48].left > 0.99 && samples[48].right.abs() < 1.0e-6);
        assert!(samples[96].right > 0.3 && samples[96].left.abs() < 0.01);
        assert!(samples[144].left > 0.1 && samples[144].left < samples[96].right);
    }

    #[test]
    fn tail_bypass_drains_while_normal_bypass_clears_to_exact_dry() {
        let mut tail = EffectSlot::compile(
            &effect([
                ("time_ms", 1.0),
                ("feedback_percent", 80.0),
                ("wet_percent", 100.0),
                ("tail_on_bypass", 1.0),
            ]),
            48_000,
            256,
        )
        .unwrap();
        let mut excite = [StereoFrame::SILENCE; 64];
        excite[0] = StereoFrame::new(1.0, 1.0);
        tail.process(&mut excite);
        tail.set_bypass(true).unwrap();
        let mut drain = [StereoFrame::SILENCE; 256];
        tail.process(&mut drain);
        assert!(drain.iter().any(|frame| frame.left.abs() > 0.01));

        let mut clear = EffectSlot::compile(
            &effect([
                ("time_ms", 1.0),
                ("feedback_percent", 80.0),
                ("wet_percent", 100.0),
            ]),
            48_000,
            256,
        )
        .unwrap();
        clear.process(&mut excite);
        clear.set_bypass(true).unwrap();
        let mut dry = [StereoFrame::new(0.25, -0.5); 512];
        clear.process(&mut dry);
        assert_eq!(dry[511], StereoFrame::new(0.25, -0.5));
    }

    #[test]
    fn limits_chunks_reset_poison_and_callback_allocation_are_safe() {
        for sample_rate in [8_000, 44_100, 48_000, 96_000, 384_000] {
            let configured = effect([
                ("mode", 2.0),
                ("time_ms", 2_000.0),
                ("feedback_percent", 92.0),
                ("stereo_ratio", 2.0),
                ("wet_percent", 100.0),
                ("dry_percent", 0.0),
            ]);
            let mut whole = EffectSlot::compile(&configured, sample_rate, 128).unwrap();
            let mut input = (0..1_024)
                .map(|index| {
                    let value = (index as f32 * 0.071).sin();
                    StereoFrame::new(value, -value)
                })
                .collect::<Vec<_>>();
            assert_no_allocations(|| {
                for chunk in input.chunks_mut(128) {
                    whole.process(chunk);
                }
            });
            assert!(input
                .iter()
                .all(|frame| frame.left.is_finite() && frame.right.is_finite()));
            let mut poison = [StereoFrame::new(f32::NAN, f32::INFINITY); 2];
            whole.process(&mut poison);
            assert!(poison
                .iter()
                .all(|frame| frame.left.is_finite() && frame.right.is_finite()));
            whole.reset();
        }
    }
}
