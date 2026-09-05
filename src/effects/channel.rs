//! Two shelves and one compressor, preallocated only for isolated audio returns.
use super::{compressor::Compressor, eq::SmoothedStereoBiquad};
use crate::audio_graph::{EffectInstance, EffectKind};
use crate::channel_strip::{Controls, Settings};
use crate::dsp::{BiquadCoefficients, SmoothedValue, StereoFrame};
use std::sync::{atomic::Ordering, Arc};

pub struct ChannelProcessor {
    controls: Arc<Controls>,
    applied: Settings,
    generation: u32,
    reduction: Arc<super::AtomicGainReduction>,
    bass: SmoothedStereoBiquad,
    bass_coefficients: BiquadCoefficients,
    treble_coefficients: BiquadCoefficients,
    treble: SmoothedStereoBiquad,
    compressor: Compressor,
    mix: SmoothedValue,
    comp_mix: SmoothedValue,
    wet: f32,
    comp_wet: f32,
    rate: f32,
    fade: u32,
}
impl ChannelProcessor {
    pub fn new(rate: u32, controls: Arc<Controls>) -> Result<Self, String> {
        let effect = EffectInstance {
            id: 1,
            kind: EffectKind::Compressor,
            version: 1,
            bypass: false,
            parameters: crate::effect_schema::defaults(EffectKind::Compressor),
            owned_memory_bytes: 0,
        };
        let compressor = Compressor::compile(&effect, rate).map_err(|e| e.to_string())?;
        let reduction = compressor.gain_reduction();
        Ok(Self {
            controls,
            generation: 0,
            reduction,
            applied: Settings::default(),
            bass: SmoothedStereoBiquad::new(BiquadCoefficients::IDENTITY),
            bass_coefficients: BiquadCoefficients::IDENTITY,
            treble_coefficients: BiquadCoefficients::IDENTITY,
            treble: SmoothedStereoBiquad::new(BiquadCoefficients::IDENTITY),
            compressor,
            mix: SmoothedValue::new(0.0).map_err(|e| e.to_string())?,
            comp_mix: SmoothedValue::new(0.0).map_err(|e| e.to_string())?,
            wet: 0.0,
            comp_wet: 0.0,
            rate: rate as f32,
            fade: (rate / 100).max(1),
        })
    }
    pub fn reset(&mut self) {
        // Preserve the latest requested coefficients even if a crossfade had
        // another coefficient update pending when the instrument was replaced.
        self.bass = SmoothedStereoBiquad::new(self.bass_coefficients);
        self.treble = SmoothedStereoBiquad::new(self.treble_coefficients);
        self.compressor.reset();
    }
    pub fn process(&mut self, frames: &mut [StereoFrame]) {
        let (generation, next) = self.controls.snapshot();
        if generation != self.generation {
            self.reset();
            self.generation = generation;
        }
        if next != self.applied {
            if next.bass != self.applied.bass {
                if let Ok(c) =
                    BiquadCoefficients::low_shelf(120.0, 1.0, next.bass as f32 * 0.5, self.rate)
                {
                    self.bass_coefficients = c;
                    let _ = self.bass.set_coefficients(c);
                }
            }
            if next.treble != self.applied.treble {
                if let Ok(c) = BiquadCoefficients::high_shelf(
                    8000.0_f32.min(self.rate * 0.4),
                    1.0,
                    next.treble as f32 * 0.5,
                    self.rate,
                ) {
                    self.treble_coefficients = c;
                    let _ = self.treble.set_coefficients(c);
                }
            }
            if next.comp != self.applied.comp {
                let amount = next.comp as f32 * 0.01;
                let _ = self
                    .compressor
                    .set_parameter("threshold_db", -6.0 - 24.0 * amount);
                let _ = self.compressor.set_parameter("ratio", 2.0 + 2.0 * amount);
                // Deliberately modest compensation, never automatic normalization.
                let _ = self.compressor.set_parameter("makeup_db", 1.5 * amount);
                let _ = self
                    .comp_mix
                    .set_target(if next.comp == 0 { 0.0 } else { 1.0 }, self.fade);
            }
            let _ = self
                .mix
                .set_target(if next.active() { 1.0 } else { 0.0 }, self.fade);
            self.applied = next;
        }
        let mut peak = 0.0_f32;
        for frame in frames {
            // Stable flat/OFF is exact passthrough, with only peak metering.
            if self.applied.active() || self.wet > 0.0 {
                let dry = *frame;
                let mut processed = self.treble.process(self.bass.process(dry));
                self.comp_wet = self.comp_mix.next_value();
                if self.applied.comp > 0 || self.comp_wet > 0.0 {
                    let compressed = self.compressor.process(processed);
                    processed.left += (compressed.left - processed.left) * self.comp_wet;
                    processed.right += (compressed.right - processed.right) * self.comp_wet;
                }
                self.wet = self.mix.next_value();
                *frame = StereoFrame::new(
                    dry.left + (processed.left - dry.left) * self.wet,
                    dry.right + (processed.right - dry.right) * self.wet,
                )
                .finite_or_silence();
                if !self.applied.active() && self.wet == 0.0 {
                    self.reset();
                }
            }
            peak = peak.max(frame.left.abs()).max(frame.right.abs());
        }
        self.compressor.publish();
        self.controls.peak.store(peak.to_bits(), Ordering::Relaxed);
        let reduction = if self.applied.active() && self.applied.comp > 0 {
            self.reduction.load()
        } else {
            0.0
        };
        self.controls
            .reduction
            .store(reduction.to_bits(), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::allocation_test::assert_no_allocations;

    #[test]
    fn channel_rebinding_keeps_latest_pending_tone_setting() {
        let controls = Arc::new(Controls::default());
        let mut channel = ChannelProcessor::new(48000, controls.clone()).unwrap();
        for bass in [3, 12] {
            controls
                .publish(Settings {
                    bass,
                    ..Settings::default()
                })
                .unwrap();
            channel.process(&mut [StereoFrame::SILENCE; 1]);
        }
        let settings = Settings {
            enabled: true,
            bass: 12,
            ..Settings::default()
        };
        controls.publish_reset(settings);
        let reference_controls = Arc::new(Controls::default());
        reference_controls.publish(settings).unwrap();
        let mut reference = ChannelProcessor::new(48000, reference_controls).unwrap();
        let mut actual = [StereoFrame::SILENCE; 256];
        let mut expected = actual;
        for _ in 0..100 {
            actual.fill(StereoFrame::new(0.2, 0.2));
            expected = actual;
            channel.process(&mut actual);
            reference.process(&mut expected);
        }
        assert!((actual[255].left - expected[255].left).abs() < 0.0001);
    }

    #[test]
    fn channel_instrument_rebinding_clears_previous_filter_history_without_allocation() {
        let controls = Arc::new(Controls::default());
        let settings = Settings {
            enabled: true,
            bass: 10,
            treble: -5,
            comp: 60,
        };
        controls.publish(settings).unwrap();
        let mut channel = ChannelProcessor::new(48000, controls.clone()).unwrap();
        for _ in 0..100 {
            channel.process(&mut [StereoFrame::new(0.5, 0.4); 128]);
        }
        controls.publish_reset(settings);
        let mut silence = [StereoFrame::SILENCE; 128];
        assert_no_allocations(|| channel.process(&mut silence));
        assert_eq!(silence, [StereoFrame::SILENCE; 128]);
    }

    #[test]
    fn channel_strip_isolation_flat_off_and_transitions_are_bounded() {
        let controls = Arc::new(Controls::default());
        let other_controls = Arc::new(Controls::default());
        let mut channel = ChannelProcessor::new(48000, controls.clone()).unwrap();
        let mut other = ChannelProcessor::new(48000, other_controls).unwrap();
        let input = [StereoFrame::new(0.6, 0.4); 256];
        let mut flat = input;
        assert_no_allocations(|| channel.process(&mut flat));
        assert_eq!(flat, input);
        for settings in [
            Settings {
                enabled: true,
                bass: 0,
                treble: 0,
                comp: 0,
            },
            Settings {
                enabled: true,
                bass: -12,
                treble: 12,
                comp: 100,
            },
            Settings {
                enabled: true,
                bass: 12,
                treble: -12,
                comp: 50,
            },
            Settings {
                enabled: false,
                bass: 12,
                treble: -12,
                comp: 50,
            },
            Settings::default(),
        ] {
            controls.publish(settings).unwrap();
            let mut previous = flat[255];
            for _ in 0..50 {
                let mut isolated = input;
                flat = input;
                assert_no_allocations(|| {
                    channel.process(&mut flat);
                    other.process(&mut isolated);
                });
                assert_eq!(
                    isolated, input,
                    "another instrument's pre-master signal changed"
                );
                for frame in flat {
                    assert!(frame.left.is_finite() && frame.right.is_finite());
                    assert!(
                        (frame.left - previous.left).abs() < 0.15,
                        "discontinuous transition"
                    );
                    previous = frame;
                }
            }
            if !settings.active() {
                assert_eq!(flat, input);
            } else {
                assert_ne!(flat, input);
            }
        }
    }

    #[test]
    #[ignore = "offline synthetic channel cost evidence; run explicitly after DSP changes"]
    fn channel_strip_offline_cost() {
        for enabled in [false, true] {
            let controls = Arc::new(Controls::default());
            controls
                .publish(Settings {
                    enabled,
                    bass: 6,
                    treble: -6,
                    comp: 60,
                })
                .unwrap();
            let mut channel = ChannelProcessor::new(48000, controls).unwrap();
            let mut block = [StereoFrame::new(0.4, 0.3); 128];
            for _ in 0..100 {
                channel.process(&mut block);
                block.fill(StereoFrame::new(0.4, 0.3));
            }
            let start = std::time::Instant::now();
            for _ in 0..10000 {
                block.fill(StereoFrame::new(0.4, 0.3));
                channel.process(std::hint::black_box(&mut block));
                std::hint::black_box(&block);
            }
            eprintln!(
                "channel enabled={enabled} 128 frames mean_ns={} profile=test opt-level=0",
                start.elapsed().as_nanos() / 10000
            );
        }
    }
}
