//! Deterministic, test-only measurements for objective DSP assertions.
//!
//! Coherent-bin stimuli avoid window leakage: every measured sinusoid completes
//! an integer number of cycles in the analyzed block. Accumulation uses `f64`
//! so the analyzer's error stays well below the tolerances used by effect tests.

use super::StereoFrame;
use std::f64::consts::TAU;

pub(crate) fn coherent_sine(length: usize, bin: usize, amplitude: f32) -> Vec<f32> {
    assert!(length > 0 && bin > 0 && bin < length / 2);
    (0..length)
        .map(|index| (TAU * bin as f64 * index as f64 / length as f64).sin() as f32 * amplitude)
        .collect()
}

/// Peak amplitude of one exact DFT bin for a real coherent signal.
pub(crate) fn spectral_amplitude(samples: &[f32], bin: usize) -> f64 {
    assert!(!samples.is_empty() && bin <= samples.len() / 2);
    let length = samples.len() as f64;
    let (real, imaginary) = samples.iter().copied().enumerate().fold(
        (0.0, 0.0),
        |(real, imaginary), (index, sample)| {
            let phase = TAU * bin as f64 * index as f64 / length;
            (
                real + sample as f64 * phase.cos(),
                imaginary - sample as f64 * phase.sin(),
            )
        },
    );
    let scale = if bin == 0 || bin * 2 == samples.len() {
        1.0
    } else {
        2.0
    };
    scale * real.hypot(imaginary) / length
}

pub(crate) fn rms(samples: &[f32]) -> f64 {
    assert!(!samples.is_empty());
    (samples
        .iter()
        .map(|sample| (*sample as f64).powi(2))
        .sum::<f64>()
        / samples.len() as f64)
        .sqrt()
}

pub(crate) fn mean(samples: &[f32]) -> f64 {
    assert!(!samples.is_empty());
    samples.iter().map(|sample| *sample as f64).sum::<f64>() / samples.len() as f64
}

pub(crate) fn peak(samples: &[f32]) -> f32 {
    samples.iter().copied().map(f32::abs).fold(0.0, f32::max)
}

pub(crate) fn maximum_step(samples: &[f32]) -> f32 {
    samples
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0, f32::max)
}

pub(crate) fn correlation(left: &[f32], right: &[f32]) -> f64 {
    assert!(!left.is_empty() && left.len() == right.len());
    let left_mean = mean(left);
    let right_mean = mean(right);
    let mut product = 0.0;
    let mut left_energy = 0.0;
    let mut right_energy = 0.0;
    for (&left, &right) in left.iter().zip(right) {
        let left = left as f64 - left_mean;
        let right = right as f64 - right_mean;
        product += left * right;
        left_energy += left * left;
        right_energy += right * right;
    }
    if left_energy == 0.0 || right_energy == 0.0 {
        0.0
    } else {
        product / (left_energy * right_energy).sqrt()
    }
}

/// Schroeder backward-integrated energy decay, normalized to 0 dB at start.
pub(crate) fn energy_decay_db(samples: &[f32]) -> Vec<f64> {
    assert!(!samples.is_empty());
    let mut energy = vec![0.0; samples.len()];
    let mut accumulated = 0.0;
    for (index, sample) in samples.iter().copied().enumerate().rev() {
        accumulated += (sample as f64).powi(2);
        energy[index] = accumulated;
    }
    let reference = energy[0].max(f64::MIN_POSITIVE);
    energy
        .into_iter()
        .map(|value| 10.0 * (value.max(f64::MIN_POSITIVE) / reference).log10())
        .collect()
}

pub(crate) fn channel(frame: &[StereoFrame], left: bool) -> Vec<f32> {
    frame
        .iter()
        .map(|frame| if left { frame.left } else { frame.right })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coherent_measurements_recover_known_sine_dc_rms_and_peak() {
        let mut samples = coherent_sine(4_096, 137, 0.8);
        assert!((spectral_amplitude(&samples, 137) - 0.8).abs() < 1.0e-7);
        assert!(spectral_amplitude(&samples, 138) < 1.0e-8);
        assert!((rms(&samples) - 0.8 / 2.0_f64.sqrt()).abs() < 1.0e-7);
        assert!(mean(&samples).abs() < 1.0e-9);
        assert!((peak(&samples) - 0.8).abs() < 1.0e-5);

        samples.iter_mut().for_each(|sample| *sample += 0.125);
        assert!((mean(&samples) - 0.125).abs() < 1.0e-8);
    }

    #[test]
    fn correlation_and_discontinuity_metrics_have_independent_references() {
        let left = coherent_sine(1_024, 7, 0.5);
        let right = left.iter().map(|sample| -*sample).collect::<Vec<_>>();
        let unrelated = coherent_sine(1_024, 11, 0.5);
        assert!((correlation(&left, &left) - 1.0).abs() < 1.0e-12);
        assert!((correlation(&left, &right) + 1.0).abs() < 1.0e-12);
        assert!(correlation(&left, &unrelated).abs() < 1.0e-7);
        assert_eq!(maximum_step(&[0.0, 0.25, -0.5, -0.25]), 0.75);
    }

    #[test]
    fn backward_energy_decay_recovers_an_exponential_time_constant() {
        let sample_rate = 1_000.0_f64;
        let rt60_seconds = 1.5_f64;
        let ratio = 10.0_f64.powf(-3.0 / (rt60_seconds * sample_rate));
        let samples = (0..3_000)
            .map(|index| ratio.powi(index) as f32)
            .collect::<Vec<_>>();
        let decay = energy_decay_db(&samples);
        assert!((decay[1_500] + 60.0).abs() < 0.01, "{} dB", decay[1_500]);
    }
}
