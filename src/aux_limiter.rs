//! Optional stereo-linked AUX return peak control. No lookahead or allocation.
use crate::dsp::StereoFrame;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub struct AuxLimiter {
    pub control: Arc<AtomicBool>,
    gain: f32,
    mix: f32,
    mix_step: f32,
    release: f32,
    enabled: bool,
}

impl AuxLimiter {
    pub fn new(sample_rate: u32, enabled: bool) -> Self {
        Self {
            control: Arc::new(AtomicBool::new(enabled)),
            gain: 1.0,
            mix: if enabled { 1.0 } else { 0.0 },
            mix_step: 1.0 / (sample_rate as f32 * 0.005).max(1.0),
            release: (-1.0 / (sample_rate as f32 * 0.050).max(1.0)).exp(),
            enabled,
        }
    }

    pub fn prepare(&mut self) {
        self.enabled = self.control.load(Ordering::Acquire);
    }

    #[inline]
    pub fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        if !self.enabled && self.mix == 0.0 {
            self.gain = 1.0;
            return frame;
        }
        // C1 quadratic knee in amplitude space, spanning approximately 6 dB.
        // The upper segment bounds the linked stereo peak to -1 dBFS.
        const CEILING: f32 = 0.891_250_9;
        let peak = frame.left.abs().max(frame.right.abs()) / CEILING;
        let target = if peak <= 2.0 / 3.0 {
            1.0
        } else if peak >= 4.0 / 3.0 {
            peak.recip()
        } else {
            let knee = peak - 2.0 / 3.0;
            (peak - 0.75 * knee * knee) / peak
        };
        self.gain = target.min(1.0 - (1.0 - self.gain) * self.release);
        self.mix = if self.enabled {
            (self.mix + self.mix_step).min(1.0)
        } else {
            (self.mix - self.mix_step).max(0.0)
        };
        let gain = (1.0 - self.mix) + self.mix * self.gain;
        StereoFrame::new(frame.left * gain, frame.right * gain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aux_limiter_off_is_exact_and_linked_on_bounds_hot_stereo() {
        let mut limiter = AuxLimiter::new(48_000, false);
        for value in [-64.0, -0.5, 0.0, 0.1, 32.0] {
            let frame = StereoFrame::new(value, -value * 0.25);
            assert_eq!(limiter.process(frame), frame);
        }
        limiter.control.store(true, Ordering::Release);
        limiter.prepare();
        for i in 0..48_000 {
            let value = (i as f32 * 0.04).sin() * 32.0;
            let out = limiter.process(StereoFrame::new(value, value * 0.25));
            assert_eq!(out.right, out.left * 0.25);
            if i > 240 {
                assert!(out.left.abs() <= 0.891_252);
            }
        }
        limiter.control.store(false, Ordering::Release);
        limiter.prepare();
        for _ in 0..250 {
            limiter.process(StereoFrame::new(4.0, 2.0));
        }
        assert_eq!(
            limiter.process(StereoFrame::new(4.0, 2.0)),
            StereoFrame::new(4.0, 2.0)
        );
    }

    #[test]
    fn aux_limiter_quiet_audio_and_release() {
        let mut limiter = AuxLimiter::new(48_000, true);
        assert_eq!(
            limiter.process(StereoFrame::new(0.1, -0.2)),
            StereoFrame::new(0.1, -0.2)
        );
        limiter.process(StereoFrame::new(16.0, 8.0));
        let first = limiter.process(StereoFrame::new(0.1, 0.1)).left;
        for _ in 0..24_000 {
            limiter.process(StereoFrame::new(0.1, 0.1));
        }
        let recovered = limiter.process(StereoFrame::new(0.1, 0.1)).left;
        assert!(first < 0.01 && recovered > 0.099);
    }
}
