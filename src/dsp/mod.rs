//! Allocation-free sample processing foundations for SHR's owned audio graph.
//!
//! Constructors and `configure` methods are control-thread operations: they
//! may allocate delay/meter memory or calculate transcendental functions.
//! `process`, `next`, `push`, and `read` methods are suitable building blocks
//! for the JACK callback and do not allocate, lock, log, or perform I/O.

use std::fmt;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[cfg(test)]
pub(crate) mod analysis;

pub const MIN_SAMPLE_RATE: f32 = 8_000.0;
pub const MAX_SAMPLE_RATE: f32 = 384_000.0;
pub const MAX_METER_WINDOW: usize = 4_096;
const DENORMAL_LIMIT: f32 = 1.0e-30;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StereoFrame {
    pub left: f32,
    pub right: f32,
}

impl StereoFrame {
    pub const SILENCE: Self = Self {
        left: 0.0,
        right: 0.0,
    };

    pub const fn new(left: f32, right: f32) -> Self {
        Self { left, right }
    }

    pub fn finite_or_silence(self) -> Self {
        Self::new(finite_or_zero(self.left), finite_or_zero(self.right))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DspError(&'static str);

impl fmt::Display for DspError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for DspError {}

pub type DspResult<T> = Result<T, DspError>;

#[inline]
pub fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() && value.abs() >= DENORMAL_LIMIT {
        value
    } else {
        0.0
    }
}

pub fn db_to_gain(db: f32) -> DspResult<f32> {
    if !db.is_finite() || !(-160.0..=60.0).contains(&db) {
        return Err(DspError("decibels must be finite and within -160..60"));
    }
    Ok(10.0_f32.powf(db / 20.0))
}

pub fn gain_to_db(gain: f32) -> DspResult<f32> {
    if !gain.is_finite() || gain < 0.0 {
        return Err(DspError("gain must be finite and non-negative"));
    }
    Ok(if gain == 0.0 {
        -160.0
    } else {
        20.0 * gain.log10()
    })
}

fn validate_sample_rate(sample_rate: f32) -> DspResult<()> {
    if !sample_rate.is_finite() || !(MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&sample_rate) {
        return Err(DspError("unsupported sample rate"));
    }
    Ok(())
}

fn nyquist_safe_frequency(frequency: f32, sample_rate: f32) -> DspResult<f32> {
    validate_sample_rate(sample_rate)?;
    if !frequency.is_finite() || frequency <= 0.0 {
        return Err(DspError("frequency must be finite and positive"));
    }
    Ok(frequency.min(sample_rate * 0.49))
}

#[derive(Clone, Copy, Debug)]
pub struct SmoothedValue {
    current: f32,
    target: f32,
    step: f32,
    remaining: u32,
}

impl SmoothedValue {
    pub fn new(value: f32) -> DspResult<Self> {
        if !value.is_finite() {
            return Err(DspError("smoothed value must be finite"));
        }
        Ok(Self {
            current: value,
            target: value,
            step: 0.0,
            remaining: 0,
        })
    }

    pub fn set_target(&mut self, target: f32, samples: u32) -> DspResult<()> {
        if !target.is_finite() {
            return Err(DspError("smoothed target must be finite"));
        }
        self.target = target;
        if samples == 0 {
            self.current = target;
            self.step = 0.0;
            self.remaining = 0;
        } else {
            self.step = (target - self.current) / samples as f32;
            self.remaining = samples;
        }
        Ok(())
    }

    #[inline]
    pub fn next_value(&mut self) -> f32 {
        if self.remaining > 0 {
            self.current += self.step;
            self.remaining -= 1;
            if self.remaining == 0 {
                self.current = self.target;
            }
        }
        self.current
    }

    pub fn current(&self) -> f32 {
        self.current
    }

    pub fn reset(&mut self, value: f32) -> DspResult<()> {
        if !value.is_finite() {
            return Err(DspError("smoothed reset value must be finite"));
        }
        self.current = value;
        self.target = value;
        self.step = 0.0;
        self.remaining = 0;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnePoleMode {
    LowPass,
    HighPass,
}

#[derive(Clone, Copy, Debug)]
pub struct OnePole {
    mode: OnePoleMode,
    pole: f32,
    low_state: f32,
}

impl OnePole {
    pub fn new(mode: OnePoleMode, frequency: f32, sample_rate: f32) -> DspResult<Self> {
        let mut filter = Self {
            mode,
            pole: 0.0,
            low_state: 0.0,
        };
        filter.configure(frequency, sample_rate)?;
        Ok(filter)
    }

    pub fn configure(&mut self, frequency: f32, sample_rate: f32) -> DspResult<()> {
        let frequency = nyquist_safe_frequency(frequency, sample_rate)?;
        self.pole = (-2.0 * std::f32::consts::PI * frequency / sample_rate).exp();
        Ok(())
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        if !input.is_finite() {
            self.reset();
            return 0.0;
        }
        self.low_state = finite_or_zero((1.0 - self.pole) * input + self.pole * self.low_state);
        match self.mode {
            OnePoleMode::LowPass => self.low_state,
            OnePoleMode::HighPass => finite_or_zero(input - self.low_state),
        }
    }

    pub fn reset(&mut self) {
        self.low_state = 0.0;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DcBlocker {
    pole: f32,
    previous_input: f32,
    previous_output: f32,
}

impl DcBlocker {
    pub fn new(frequency: f32, sample_rate: f32) -> DspResult<Self> {
        let frequency = nyquist_safe_frequency(frequency, sample_rate)?;
        Ok(Self {
            pole: (-2.0 * std::f32::consts::PI * frequency / sample_rate).exp(),
            previous_input: 0.0,
            previous_output: 0.0,
        })
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        if !input.is_finite() {
            self.reset();
            return 0.0;
        }
        let output = finite_or_zero(input - self.previous_input + self.pole * self.previous_output);
        self.previous_input = input;
        self.previous_output = output;
        output
    }

    pub fn reset(&mut self) {
        self.previous_input = 0.0;
        self.previous_output = 0.0;
    }
}

/// Normalized RBJ/W3C Audio EQ Cookbook coefficients (`a0 == 1`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiquadCoefficients {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl BiquadCoefficients {
    pub const IDENTITY: Self = Self {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    pub fn low_pass(frequency: f32, q: f32, sample_rate: f32) -> DspResult<Self> {
        let (cosine, alpha) = cookbook_terms(frequency, q, sample_rate)?;
        normalize(
            (1.0 - cosine) * 0.5,
            1.0 - cosine,
            (1.0 - cosine) * 0.5,
            1.0 + alpha,
            -2.0 * cosine,
            1.0 - alpha,
        )
    }

    pub fn high_pass(frequency: f32, q: f32, sample_rate: f32) -> DspResult<Self> {
        let (cosine, alpha) = cookbook_terms(frequency, q, sample_rate)?;
        normalize(
            (1.0 + cosine) * 0.5,
            -(1.0 + cosine),
            (1.0 + cosine) * 0.5,
            1.0 + alpha,
            -2.0 * cosine,
            1.0 - alpha,
        )
    }

    pub fn peaking(frequency: f32, q: f32, gain_db: f32, sample_rate: f32) -> DspResult<Self> {
        if !gain_db.is_finite() || !(-36.0..=36.0).contains(&gain_db) {
            return Err(DspError("biquad gain must be finite and within -36..36 dB"));
        }
        if gain_db == 0.0 {
            return Ok(Self::IDENTITY);
        }
        let (cosine, alpha) = cookbook_terms(frequency, q, sample_rate)?;
        let amplitude = 10.0_f32.powf(gain_db / 40.0);
        normalize(
            1.0 + alpha * amplitude,
            -2.0 * cosine,
            1.0 - alpha * amplitude,
            1.0 + alpha / amplitude,
            -2.0 * cosine,
            1.0 - alpha / amplitude,
        )
    }

    pub fn low_shelf(
        frequency: f32,
        slope: f32,
        gain_db: f32,
        sample_rate: f32,
    ) -> DspResult<Self> {
        shelf(frequency, slope, gain_db, sample_rate, false)
    }

    pub fn high_shelf(
        frequency: f32,
        slope: f32,
        gain_db: f32,
        sample_rate: f32,
    ) -> DspResult<Self> {
        shelf(frequency, slope, gain_db, sample_rate, true)
    }

    pub fn is_finite(self) -> bool {
        [self.b0, self.b1, self.b2, self.a1, self.a2]
            .iter()
            .all(|value| value.is_finite())
    }
}

fn cookbook_terms(frequency: f32, q: f32, sample_rate: f32) -> DspResult<(f32, f32)> {
    let frequency = nyquist_safe_frequency(frequency, sample_rate)?;
    if !q.is_finite() || !(0.05..=50.0).contains(&q) {
        return Err(DspError("biquad Q must be finite and within 0.05..50"));
    }
    let omega = 2.0 * std::f32::consts::PI * frequency / sample_rate;
    let (sine, cosine) = omega.sin_cos();
    Ok((cosine, sine / (2.0 * q)))
}

fn shelf(
    frequency: f32,
    slope: f32,
    gain_db: f32,
    sample_rate: f32,
    high: bool,
) -> DspResult<BiquadCoefficients> {
    let frequency = nyquist_safe_frequency(frequency, sample_rate)?;
    if !slope.is_finite() || !(0.1..=2.0).contains(&slope) {
        return Err(DspError("shelf slope must be finite and within 0.1..2"));
    }
    if !gain_db.is_finite() || !(-36.0..=36.0).contains(&gain_db) {
        return Err(DspError("shelf gain must be finite and within -36..36 dB"));
    }
    if gain_db == 0.0 {
        return Ok(BiquadCoefficients::IDENTITY);
    }
    let amplitude = 10.0_f32.powf(gain_db / 40.0);
    let omega = 2.0 * std::f32::consts::PI * frequency / sample_rate;
    let (sine, cosine) = omega.sin_cos();
    let alpha = sine * 0.5 * ((amplitude + 1.0 / amplitude) * (1.0 / slope - 1.0) + 2.0).sqrt();
    let beta = 2.0 * amplitude.sqrt() * alpha;
    let ap1 = amplitude + 1.0;
    let am1 = amplitude - 1.0;
    if high {
        normalize(
            amplitude * (ap1 + am1 * cosine + beta),
            -2.0 * amplitude * (am1 + ap1 * cosine),
            amplitude * (ap1 + am1 * cosine - beta),
            ap1 - am1 * cosine + beta,
            2.0 * (am1 - ap1 * cosine),
            ap1 - am1 * cosine - beta,
        )
    } else {
        normalize(
            amplitude * (ap1 - am1 * cosine + beta),
            2.0 * amplitude * (am1 - ap1 * cosine),
            amplitude * (ap1 - am1 * cosine - beta),
            ap1 + am1 * cosine + beta,
            -2.0 * (am1 + ap1 * cosine),
            ap1 + am1 * cosine - beta,
        )
    }
}

fn normalize(
    b0: f32,
    b1: f32,
    b2: f32,
    a0: f32,
    a1: f32,
    a2: f32,
) -> DspResult<BiquadCoefficients> {
    if !a0.is_finite() || a0.abs() < f32::EPSILON {
        return Err(DspError("invalid biquad denominator"));
    }
    let coefficients = BiquadCoefficients {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    };
    if !coefficients.is_finite() {
        return Err(DspError("non-finite biquad coefficients"));
    }
    Ok(coefficients)
}

#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    coefficients: BiquadCoefficients,
    state1: f32,
    state2: f32,
}

impl Biquad {
    pub const fn new(coefficients: BiquadCoefficients) -> Self {
        Self {
            coefficients,
            state1: 0.0,
            state2: 0.0,
        }
    }

    pub fn set_coefficients(&mut self, coefficients: BiquadCoefficients) -> DspResult<()> {
        if !coefficients.is_finite() {
            return Err(DspError("non-finite biquad coefficients"));
        }
        self.coefficients = coefficients;
        Ok(())
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        if !input.is_finite() {
            self.reset();
            return 0.0;
        }
        let output = self.coefficients.b0 * input + self.state1;
        let state1 = self.coefficients.b1 * input - self.coefficients.a1 * output + self.state2;
        let state2 = self.coefficients.b2 * input - self.coefficients.a2 * output;
        if !(output.is_finite() && state1.is_finite() && state2.is_finite()) {
            self.reset();
            return 0.0;
        }
        self.state1 = finite_or_zero(state1);
        self.state2 = finite_or_zero(state2);
        finite_or_zero(output)
    }

    pub fn reset(&mut self) {
        self.state1 = 0.0;
        self.state2 = 0.0;
    }
}

#[derive(Debug)]
pub struct FractionalDelayLine {
    samples: Box<[f32]>,
    write: usize,
    maximum_delay: usize,
}

impl FractionalDelayLine {
    pub fn new(maximum_delay_samples: usize) -> DspResult<Self> {
        if maximum_delay_samples == 0 {
            return Err(DspError("delay capacity must be positive"));
        }
        let length = maximum_delay_samples
            .checked_add(2)
            .ok_or(DspError("delay capacity overflow"))?;
        Ok(Self {
            samples: vec![0.0; length].into_boxed_slice(),
            write: 0,
            maximum_delay: maximum_delay_samples,
        })
    }

    #[inline]
    pub fn push(&mut self, sample: f32) {
        self.samples[self.write] = finite_or_zero(sample);
        self.write += 1;
        if self.write == self.samples.len() {
            self.write = 0;
        }
    }

    /// Read `delay_samples` behind the newest pushed sample using linear
    /// interpolation. Valid delay is `1..=maximum_delay` samples.
    #[inline]
    pub fn read(&self, delay_samples: f32) -> f32 {
        if !delay_samples.is_finite()
            || delay_samples < 1.0
            || delay_samples > self.maximum_delay as f32
        {
            return 0.0;
        }
        // Use f64 for the wrapped index. At large high-rate capacities, f32
        // can round `len - epsilon` up to `len` and produce an invalid index.
        let length = self.samples.len() as f64;
        let newest = if self.write == 0 {
            self.samples.len() - 1
        } else {
            self.write - 1
        } as f64;
        let position = (newest - (delay_samples as f64 - 1.0)).rem_euclid(length);
        let first = position.floor() as usize;
        let second = (first + 1) % self.samples.len();
        let fraction = (position - first as f64) as f32;
        finite_or_zero(
            self.samples[first] + (self.samples[second] - self.samples[first]) * fraction,
        )
    }

    pub fn maximum_delay(&self) -> usize {
        self.maximum_delay
    }

    pub fn memory_bytes(&self) -> usize {
        self.samples.len() * std::mem::size_of::<f32>()
    }

    pub fn reset(&mut self) {
        self.samples.fill(0.0);
        self.write = 0;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EnvelopeFollower {
    attack: f32,
    release: f32,
    envelope: f32,
}

impl EnvelopeFollower {
    pub fn new(attack_ms: f32, release_ms: f32, sample_rate: f32) -> DspResult<Self> {
        validate_sample_rate(sample_rate)?;
        if !attack_ms.is_finite()
            || !release_ms.is_finite()
            || !(0.01..=10_000.0).contains(&attack_ms)
            || !(0.01..=10_000.0).contains(&release_ms)
        {
            return Err(DspError(
                "envelope times must be finite and within 0.01..10000 ms",
            ));
        }
        Ok(Self {
            attack: time_coefficient(attack_ms, sample_rate),
            release: time_coefficient(release_ms, sample_rate),
            envelope: 0.0,
        })
    }

    #[inline]
    pub fn process_magnitude(&mut self, magnitude: f32) -> f32 {
        if !magnitude.is_finite() {
            self.reset();
            return 0.0;
        }
        let magnitude = magnitude.abs();
        let coefficient = if magnitude > self.envelope {
            self.attack
        } else {
            self.release
        };
        self.envelope = finite_or_zero(magnitude + coefficient * (self.envelope - magnitude));
        self.envelope
    }

    pub fn current(&self) -> f32 {
        self.envelope
    }

    pub fn reset(&mut self) {
        self.envelope = 0.0;
    }
}

fn time_coefficient(milliseconds: f32, sample_rate: f32) -> f32 {
    (-1.0 / (milliseconds * 0.001 * sample_rate)).exp()
}

/// Sine LFO implemented as a bounded complex rotation. Trigonometry occurs
/// only in `new`/`configure`, never in `next_value`.
#[derive(Clone, Copy, Debug)]
pub struct SineLfo {
    sine: f32,
    cosine: f32,
    step_sine: f32,
    step_cosine: f32,
    normalization_countdown: u16,
}

impl SineLfo {
    pub fn new(frequency: f32, phase_radians: f32, sample_rate: f32) -> DspResult<Self> {
        let mut lfo = Self {
            sine: 0.0,
            cosine: 1.0,
            step_sine: 0.0,
            step_cosine: 1.0,
            normalization_countdown: 4_096,
        };
        lfo.configure(frequency, phase_radians, sample_rate)?;
        Ok(lfo)
    }

    pub fn configure(
        &mut self,
        frequency: f32,
        phase_radians: f32,
        sample_rate: f32,
    ) -> DspResult<()> {
        if !phase_radians.is_finite() {
            return Err(DspError("invalid LFO frequency or phase"));
        }
        self.set_frequency(frequency, sample_rate)?;
        (self.sine, self.cosine) = phase_radians.sin_cos();
        self.normalization_countdown = 4_096;
        Ok(())
    }

    /// Change oscillator speed without resetting its instantaneous phase.
    /// This is the control-thread path for click-conscious live rate changes.
    pub fn set_frequency(&mut self, frequency: f32, sample_rate: f32) -> DspResult<()> {
        validate_sample_rate(sample_rate)?;
        if !frequency.is_finite() || frequency <= 0.0 || frequency > sample_rate * 0.25 {
            return Err(DspError("invalid LFO frequency or phase"));
        }
        let step = 2.0 * std::f32::consts::PI * frequency / sample_rate;
        (self.step_sine, self.step_cosine) = step.sin_cos();
        Ok(())
    }

    #[inline]
    pub fn next_value(&mut self) -> f32 {
        let output = self.sine;
        let sine = self.sine * self.step_cosine + self.cosine * self.step_sine;
        let cosine = self.cosine * self.step_cosine - self.sine * self.step_sine;
        self.sine = sine;
        self.cosine = cosine;
        self.normalization_countdown -= 1;
        if self.normalization_countdown == 0 {
            let magnitude_squared = self.sine * self.sine + self.cosine * self.cosine;
            if magnitude_squared.is_finite() && magnitude_squared > f32::EPSILON {
                let inverse = magnitude_squared.sqrt().recip();
                self.sine *= inverse;
                self.cosine *= inverse;
            } else {
                self.sine = 0.0;
                self.cosine = 1.0;
            }
            self.normalization_countdown = 4_096;
        }
        finite_or_zero(output.clamp(-1.0, 1.0))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeterSnapshot {
    pub peak: StereoFrame,
    pub rms: StereoFrame,
    pub clips: u64,
    pub non_finite: u64,
}

#[derive(Debug)]
pub struct MeterAccumulator {
    squares: Box<[StereoFrame]>,
    position: usize,
    sum: StereoFrame,
    peak: StereoFrame,
    clips: u64,
    non_finite: u64,
}

impl MeterAccumulator {
    pub fn new(window_frames: usize) -> DspResult<Self> {
        if !(1..=MAX_METER_WINDOW).contains(&window_frames) {
            return Err(DspError("meter window exceeds callback buffer bound"));
        }
        Ok(Self {
            squares: vec![StereoFrame::SILENCE; window_frames].into_boxed_slice(),
            position: 0,
            sum: StereoFrame::SILENCE,
            peak: StereoFrame::SILENCE,
            clips: 0,
            non_finite: 0,
        })
    }

    #[inline]
    pub fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        if !frame.left.is_finite() {
            self.non_finite = self.non_finite.saturating_add(1);
        }
        if !frame.right.is_finite() {
            self.non_finite = self.non_finite.saturating_add(1);
        }
        let frame = frame.finite_or_silence();
        if frame.left.abs() >= 1.0 {
            self.clips = self.clips.saturating_add(1);
        }
        if frame.right.abs() >= 1.0 {
            self.clips = self.clips.saturating_add(1);
        }
        self.peak.left = self.peak.left.max(frame.left.abs());
        self.peak.right = self.peak.right.max(frame.right.abs());
        let old = self.squares[self.position];
        let new = StereoFrame::new(frame.left * frame.left, frame.right * frame.right);
        self.squares[self.position] = new;
        self.position = (self.position + 1) % self.squares.len();
        self.sum.left = (self.sum.left + new.left - old.left).max(0.0);
        self.sum.right = (self.sum.right + new.right - old.right).max(0.0);
        frame
    }

    pub fn snapshot_and_clear_peak(&mut self) -> MeterSnapshot {
        let scale = 1.0 / self.squares.len() as f32;
        let snapshot = MeterSnapshot {
            peak: self.peak,
            rms: StereoFrame::new(
                (self.sum.left * scale).sqrt(),
                (self.sum.right * scale).sqrt(),
            ),
            clips: self.clips,
            non_finite: self.non_finite,
        };
        self.peak = StereoFrame::SILENCE;
        snapshot
    }

    pub fn reset(&mut self) {
        self.squares.fill(StereoFrame::SILENCE);
        self.position = 0;
        self.sum = StereoFrame::SILENCE;
        self.peak = StereoFrame::SILENCE;
        self.clips = 0;
        self.non_finite = 0;
    }
}

#[derive(Debug, Default)]
pub struct AtomicMeter {
    peak_left: AtomicU32,
    peak_right: AtomicU32,
    rms_left: AtomicU32,
    rms_right: AtomicU32,
    clips: AtomicU64,
    non_finite: AtomicU64,
}

impl AtomicMeter {
    pub fn publish(&self, snapshot: MeterSnapshot) {
        self.peak_left
            .store(snapshot.peak.left.to_bits(), Ordering::Release);
        self.peak_right
            .store(snapshot.peak.right.to_bits(), Ordering::Release);
        self.rms_left
            .store(snapshot.rms.left.to_bits(), Ordering::Release);
        self.rms_right
            .store(snapshot.rms.right.to_bits(), Ordering::Release);
        self.clips.store(snapshot.clips, Ordering::Release);
        self.non_finite
            .store(snapshot.non_finite, Ordering::Release);
    }

    pub fn load(&self) -> MeterSnapshot {
        MeterSnapshot {
            peak: StereoFrame::new(
                f32::from_bits(self.peak_left.load(Ordering::Acquire)),
                f32::from_bits(self.peak_right.load(Ordering::Acquire)),
            ),
            rms: StereoFrame::new(
                f32::from_bits(self.rms_left.load(Ordering::Acquire)),
                f32::from_bits(self.rms_right.load(Ordering::Acquire)),
            ),
            clips: self.clips.load(Ordering::Acquire),
            non_finite: self.non_finite.load(Ordering::Acquire),
        }
    }
}

#[cfg(test)]
pub(crate) mod allocation_test {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static TRACKING: Cell<bool> = const { Cell::new(false) };
        static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    }

    struct TrackingAllocator;

    unsafe impl GlobalAlloc for TrackingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            TRACKING.with(|tracking| {
                if tracking.get() {
                    ALLOCATIONS.with(|count| count.set(count.get() + 1));
                }
            });
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe { System.dealloc(pointer, layout) }
        }

        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            TRACKING.with(|tracking| {
                if tracking.get() {
                    ALLOCATIONS.with(|count| count.set(count.get() + 1));
                }
            });
            unsafe { System.realloc(pointer, layout, size) }
        }
    }

    #[global_allocator]
    static GLOBAL: TrackingAllocator = TrackingAllocator;

    pub(crate) fn assert_no_allocations(action: impl FnOnce()) {
        ALLOCATIONS.with(|count| count.set(0));
        TRACKING.with(|tracking| tracking.set(true));
        action();
        TRACKING.with(|tracking| tracking.set(false));
        let allocations = ALLOCATIONS.with(Cell::get);
        assert_eq!(allocations, 0, "real-time path allocated");
    }
}

#[cfg(test)]
mod tests {
    use super::allocation_test::assert_no_allocations;
    use super::analysis::spectral_amplitude;
    use super::*;

    fn close(left: f32, right: f32, tolerance: f32) {
        assert!(
            (left - right).abs() <= tolerance,
            "{left} is not within {tolerance} of {right}"
        );
    }

    #[test]
    fn decibel_conversion_and_smoothing_are_exact_at_useful_points() {
        close(db_to_gain(0.0).unwrap(), 1.0, 1.0e-7);
        close(db_to_gain(6.0206).unwrap(), 2.0, 1.0e-4);
        close(gain_to_db(0.5).unwrap(), -6.0206, 1.0e-4);
        assert!(db_to_gain(f32::NAN).is_err());

        let mut smooth = SmoothedValue::new(0.0).unwrap();
        smooth.set_target(1.0, 4).unwrap();
        assert_eq!(
            (0..5).map(|_| smooth.next_value()).collect::<Vec<_>>(),
            [0.25, 0.5, 0.75, 1.0, 1.0]
        );
        assert!(smooth.set_target(f32::INFINITY, 10).is_err());
    }

    #[test]
    fn one_pole_and_dc_blocker_reject_poison_and_reset() {
        let mut low = OnePole::new(OnePoleMode::LowPass, 1_000.0, 48_000.0).unwrap();
        let response: Vec<_> = (0..32).map(|_| low.process(1.0)).collect();
        assert!(response.windows(2).all(|pair| pair[1] >= pair[0]));
        assert!(response.last().unwrap() < &1.0);
        assert_eq!(low.process(f32::NAN), 0.0);
        low.reset();
        assert_eq!(low.process(0.0), 0.0);

        let mut dc = DcBlocker::new(10.0, 48_000.0).unwrap();
        let mut output = 0.0;
        for _ in 0..48_000 {
            output = dc.process(0.5);
        }
        assert!(output.abs() < 0.001);
        dc.reset();
        assert_eq!(dc.process(0.0), 0.0);
    }

    #[test]
    fn cookbook_unity_filters_and_biquad_state_are_finite() {
        assert_eq!(
            BiquadCoefficients::peaking(1_000.0, 0.9, 0.0, 48_000.0).unwrap(),
            BiquadCoefficients::IDENTITY
        );
        let low = BiquadCoefficients::low_shelf(200.0, 1.0, 0.0, 48_000.0).unwrap();
        let high = BiquadCoefficients::high_shelf(5_000.0, 1.0, 0.0, 48_000.0).unwrap();
        for coefficients in [low, high] {
            close(coefficients.b0, 1.0, 1.0e-6);
            close(coefficients.b1, coefficients.a1, 1.0e-6);
            close(coefficients.b2, coefficients.a2, 1.0e-6);
        }
        let coefficients = BiquadCoefficients::high_pass(100.0, 0.707, 48_000.0).unwrap();
        let mut filter = Biquad::new(coefficients);
        let mut maximum = 0.0_f32;
        for index in 0..200_000 {
            let input = if index == 0 { 1.0 } else { 0.0 };
            maximum = maximum.max(filter.process(input).abs());
        }
        assert!(maximum <= 1.0);
        assert_eq!(filter.process(f32::INFINITY), 0.0);
        assert_eq!(filter.process(0.0), 0.0);
    }

    #[test]
    fn cookbook_clamps_frequency_below_nyquist_at_supported_rates() {
        for sample_rate in [8_000.0, 44_100.0, 48_000.0, 96_000.0, 384_000.0] {
            for coefficients in [
                BiquadCoefficients::low_pass(100_000.0, 0.707, sample_rate).unwrap(),
                BiquadCoefficients::high_pass(100_000.0, 1.306_563, sample_rate).unwrap(),
                BiquadCoefficients::peaking(100_000.0, 0.9, 18.0, sample_rate).unwrap(),
            ] {
                assert!(coefficients.is_finite());
            }
        }
        assert!(BiquadCoefficients::low_pass(1_000.0, 0.0, 48_000.0).is_err());
    }

    #[test]
    fn fractional_delay_impulse_fraction_and_reset_are_deterministic() {
        let mut delay = FractionalDelayLine::new(8).unwrap();
        delay.push(1.0);
        assert_eq!(delay.read(1.0), 1.0);
        delay.push(0.0);
        close(delay.read(1.5), 0.5, 1.0e-7);
        delay.push(0.0);
        assert_eq!(delay.read(3.0), 1.0);
        assert_eq!(delay.read(9.0), 0.0);
        assert_eq!(delay.memory_bytes(), 10 * std::mem::size_of::<f32>());
        delay.reset();
        assert_eq!(delay.read(1.0), 0.0);

        let mut high_rate = FractionalDelayLine::new(768_000).unwrap();
        high_rate.push(1.0);
        assert_eq!(high_rate.read(768_000.0), 0.0);
        assert!(high_rate.read(767_999.5).is_finite());
    }

    #[test]
    fn linear_fractional_delay_high_frequency_loss_matches_its_known_response() {
        let sample_rate = 48_000.0;
        let frequency = 10_000.0;
        let mut delay = FractionalDelayLine::new(8).unwrap();
        let mut output = Vec::with_capacity(43_200);
        for index in 0..48_000 {
            let input = (2.0 * std::f32::consts::PI * frequency * index as f32 / sample_rate).sin();
            delay.push(input);
            let delayed = delay.read(1.5);
            if index >= 4_800 {
                output.push(delayed);
            }
        }
        let measured = spectral_amplitude(&output, 9_000);
        let expected = (std::f64::consts::PI * frequency as f64 / sample_rate as f64).cos();
        assert!((measured - expected).abs() < 1.0e-5);
        let loss_db = 20.0 * measured.log10();
        assert!((-2.02..=-2.00).contains(&loss_db), "{loss_db:.3} dB");
    }

    #[test]
    fn envelope_attack_and_release_are_monotonic_and_resettable() {
        let mut envelope = EnvelopeFollower::new(10.0, 100.0, 48_000.0).unwrap();
        let rising: Vec<_> = (0..100).map(|_| envelope.process_magnitude(1.0)).collect();
        assert!(rising.windows(2).all(|pair| pair[1] >= pair[0]));
        let falling: Vec<_> = (0..100).map(|_| envelope.process_magnitude(0.0)).collect();
        assert!(falling.windows(2).all(|pair| pair[1] <= pair[0]));
        assert_eq!(envelope.process_magnitude(f32::NAN), 0.0);
        envelope.reset();
        assert_eq!(envelope.current(), 0.0);
    }

    #[test]
    fn lfo_is_periodic_bounded_and_long_run_finite() {
        let mut lfo = SineLfo::new(1.0, 0.0, 48_000.0).unwrap();
        let first = lfo.next_value();
        for _ in 1..48_000 {
            let value = lfo.next_value();
            assert!(value.is_finite() && (-1.0..=1.0).contains(&value));
        }
        close(lfo.next_value(), first, 2.0e-4);
        for _ in 0..1_000_000 {
            assert!(lfo.next_value().is_finite());
        }
    }

    #[test]
    fn lfo_rate_change_preserves_instantaneous_phase() {
        let mut changed = SineLfo::new(0.5, 0.0, 48_000.0).unwrap();
        let mut reference = SineLfo::new(0.5, 0.0, 48_000.0).unwrap();
        for _ in 0..12_345 {
            close(changed.next_value(), reference.next_value(), 1.0e-7);
        }
        changed.set_frequency(5.0, 48_000.0).unwrap();
        close(changed.next_value(), reference.next_value(), 1.0e-7);
        assert!((changed.next_value() - reference.next_value()).abs() < 0.001);
        assert!(changed.set_frequency(f32::NAN, 48_000.0).is_err());
    }

    #[test]
    fn meter_tracks_stereo_independently_and_counts_bad_samples() {
        let mut meter = MeterAccumulator::new(4).unwrap();
        for frame in [
            StereoFrame::new(1.0, 0.5),
            StereoFrame::new(-1.0, -0.5),
            StereoFrame::new(f32::NAN, 0.5),
            StereoFrame::new(0.0, f32::INFINITY),
        ] {
            meter.process(frame);
        }
        let snapshot = meter.snapshot_and_clear_peak();
        assert_eq!(snapshot.peak, StereoFrame::new(1.0, 0.5));
        close(snapshot.rms.left, 2.0_f32.sqrt() * 0.5, 1.0e-6);
        close(snapshot.rms.right, (3.0_f32 / 16.0).sqrt(), 1.0e-6);
        assert_eq!(snapshot.clips, 2);
        assert_eq!(snapshot.non_finite, 2);

        let atomic = AtomicMeter::default();
        atomic.publish(snapshot);
        assert_eq!(atomic.load(), snapshot);
        meter.reset();
        assert_eq!(meter.snapshot_and_clear_peak(), MeterSnapshot::default());
    }

    #[test]
    fn processing_primitives_allocate_nothing() {
        let mut smooth = SmoothedValue::new(0.0).unwrap();
        smooth.set_target(1.0, 64).unwrap();
        let mut biquad =
            Biquad::new(BiquadCoefficients::low_pass(2_000.0, 0.707, 48_000.0).unwrap());
        let mut delay = FractionalDelayLine::new(128).unwrap();
        let mut envelope = EnvelopeFollower::new(2.0, 100.0, 48_000.0).unwrap();
        let mut lfo = SineLfo::new(2.0, 0.0, 48_000.0).unwrap();
        let mut meter = MeterAccumulator::new(64).unwrap();
        assert_no_allocations(|| {
            for index in 0..10_000 {
                let input = (index as f32 * 0.001).fract() * 2.0 - 1.0;
                let filtered = biquad.process(input * smooth.next_value());
                delay.push(filtered);
                let delayed = delay.read(32.25);
                envelope.process_magnitude(delayed);
                meter.process(StereoFrame::new(delayed, delayed * lfo.next_value()));
            }
        });
    }

    #[test]
    fn filter_results_are_chunk_size_invariant() {
        let coefficients = BiquadCoefficients::low_pass(4_000.0, 0.707, 48_000.0).unwrap();
        let input: Vec<_> = (0..4_096)
            .map(|index| ((index * 17 % 101) as f32 / 50.0) - 1.0)
            .collect();
        let mut continuous = Biquad::new(coefficients);
        let expected: Vec<_> = input
            .iter()
            .map(|sample| continuous.process(*sample))
            .collect();
        let mut chunked = Biquad::new(coefficients);
        let mut actual = Vec::with_capacity(input.len());
        for chunk in input.chunks(37) {
            actual.extend(chunk.iter().map(|sample| chunked.process(*sample)));
        }
        assert_eq!(actual, expected);
    }
}
