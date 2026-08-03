//! Fixed four-source final performance bus.
//!
//! Source faders and the live master fader feed the Project-owned fixed
//! MASTER STRIP. The strip owns the final true-peak limiter and meters.

use crate::dsp::{db_to_gain, SmoothedValue, StereoFrame};
#[cfg(test)]
use crate::master_strip::MasterStripSettings;
use crate::master_strip::{
    MasterStripControls, MasterStripMeterSnapshot, MasterStripMeters, MasterStripProcessor,
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

pub const SOURCE_COUNT: usize = 4;
pub const SOURCE_GAIN_MIN_DB: f32 = -60.0;
pub const SOURCE_GAIN_MAX_DB: f32 = 6.0;
pub const MASTER_GAIN_MIN_DB: f32 = -60.0;
pub const MASTER_GAIN_MAX_DB: f32 = 0.0;
pub const DEFAULT_SOURCE_GAIN_DB: f32 = -6.0;
const GAIN_SMOOTH_SECONDS: f32 = 0.010;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BusSource {
    Synth = 0,
    Loop = 1,
    Input = 2,
    Drums = 3,
}

impl BusSource {
    pub const ALL: [Self; SOURCE_COUNT] = [Self::Synth, Self::Loop, Self::Input, Self::Drums];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Synth => "SYNTH",
            Self::Loop => "LOOP",
            Self::Input => "INPUT",
            Self::Drums => "DRUMS",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

struct AtomicFader {
    gain_db: AtomicU32,
    muted: AtomicBool,
}

struct AtomicSourceMeter {
    peak_left: AtomicU32,
    peak_right: AtomicU32,
}

impl AtomicSourceMeter {
    fn new() -> Self {
        Self {
            peak_left: AtomicU32::new(0.0_f32.to_bits()),
            peak_right: AtomicU32::new(0.0_f32.to_bits()),
        }
    }

    #[inline]
    fn publish(&self, peak: StereoFrame) {
        self.peak_left.store(peak.left.to_bits(), Ordering::Release);
        self.peak_right
            .store(peak.right.to_bits(), Ordering::Release);
    }

    fn snapshot(&self) -> StereoFrame {
        StereoFrame::new(
            f32::from_bits(self.peak_left.load(Ordering::Acquire)),
            f32::from_bits(self.peak_right.load(Ordering::Acquire)),
        )
        .finite_or_silence()
    }
}

impl AtomicFader {
    fn new(gain_db: f32) -> Self {
        Self {
            gain_db: AtomicU32::new(gain_db.to_bits()),
            muted: AtomicBool::new(false),
        }
    }

    fn gain_db(&self) -> f32 {
        let gain = f32::from_bits(self.gain_db.load(Ordering::Acquire));
        if gain.is_finite() {
            gain
        } else {
            0.0
        }
    }
}

pub struct BusControls {
    sources: [AtomicFader; SOURCE_COUNT],
    source_meters: [AtomicSourceMeter; SOURCE_COUNT],
    master: AtomicFader,
}

impl Default for BusControls {
    fn default() -> Self {
        Self {
            sources: std::array::from_fn(|_| AtomicFader::new(DEFAULT_SOURCE_GAIN_DB)),
            source_meters: std::array::from_fn(|_| AtomicSourceMeter::new()),
            master: AtomicFader::new(0.0),
        }
    }
}

impl BusControls {
    pub fn source_gain_db(&self, source: BusSource) -> f32 {
        self.sources[source.index()].gain_db()
    }

    pub fn set_source_gain_db(&self, source: BusSource, gain_db: f32) -> bool {
        if !gain_db.is_finite() || !(SOURCE_GAIN_MIN_DB..=SOURCE_GAIN_MAX_DB).contains(&gain_db) {
            return false;
        }
        self.sources[source.index()]
            .gain_db
            .store(gain_db.to_bits(), Ordering::Release);
        true
    }

    pub fn source_muted(&self, source: BusSource) -> bool {
        self.sources[source.index()].muted.load(Ordering::Acquire)
    }

    pub fn set_source_muted(&self, source: BusSource, muted: bool) {
        self.sources[source.index()]
            .muted
            .store(muted, Ordering::Release);
    }

    /// Latest callback-block peak after this owner's smoothed gain and mute.
    /// Pages that share an owner intentionally read this same canonical value.
    pub fn source_peak(&self, source: BusSource) -> StereoFrame {
        self.source_meters[source.index()].snapshot()
    }

    #[inline]
    fn publish_source_peak(&self, source: BusSource, peak: StereoFrame) {
        self.source_meters[source.index()].publish(peak);
    }

    pub fn master_gain_db(&self) -> f32 {
        self.master.gain_db()
    }

    pub fn set_master_gain_db(&self, gain_db: f32) -> bool {
        if !gain_db.is_finite() || !(MASTER_GAIN_MIN_DB..=MASTER_GAIN_MAX_DB).contains(&gain_db) {
            return false;
        }
        self.master
            .gain_db
            .store(gain_db.to_bits(), Ordering::Release);
        true
    }
}

pub type FinalBusMeterSnapshot = MasterStripMeterSnapshot;
pub type FinalBusMeters = MasterStripMeters;

struct RuntimeFader {
    value: SmoothedValue,
    last_target: f32,
    smoothing_samples: u32,
}

impl RuntimeFader {
    fn new(gain_db: f32, muted: bool, sample_rate: u32) -> Result<Self, String> {
        let gain = if muted {
            0.0
        } else {
            db_to_gain(gain_db).map_err(|error| error.to_string())?
        };
        Ok(Self {
            value: SmoothedValue::new(gain).map_err(|error| error.to_string())?,
            last_target: gain,
            smoothing_samples: ((sample_rate as f32 * GAIN_SMOOTH_SECONDS).round() as u32).max(1),
        })
    }

    #[inline]
    fn refresh(&mut self, gain_db: f32, muted: bool) {
        let target = if muted {
            0.0
        } else {
            db_to_gain(gain_db).unwrap_or(0.0)
        };
        if target != self.last_target {
            if self
                .value
                .set_target(target, self.smoothing_samples)
                .is_err()
            {
                let _ = self.value.reset(0.0);
            }
            self.last_target = target;
        }
    }

    #[inline]
    fn next(&mut self) -> f32 {
        self.value.next_value()
    }
}

pub struct FinalBusProcessor {
    controls: Arc<BusControls>,
    source_faders: [RuntimeFader; SOURCE_COUNT],
    master_fader: RuntimeFader,
    strip: MasterStripProcessor,
}

impl FinalBusProcessor {
    pub fn new(
        sample_rate: u32,
        maximum_frames: usize,
        controls: Arc<BusControls>,
        strip_controls: Arc<MasterStripControls>,
        meters: Arc<FinalBusMeters>,
    ) -> Result<Self, String> {
        let source_fader = |source| {
            RuntimeFader::new(
                controls.source_gain_db(source),
                controls.source_muted(source),
                sample_rate,
            )
        };
        Ok(Self {
            source_faders: [
                source_fader(BusSource::Synth)?,
                source_fader(BusSource::Loop)?,
                source_fader(BusSource::Input)?,
                source_fader(BusSource::Drums)?,
            ],
            master_fader: RuntimeFader::new(controls.master_gain_db(), false, sample_rate)?,
            strip: MasterStripProcessor::new(sample_rate, maximum_frames, strip_controls, meters)?,
            controls,
        })
    }

    #[cfg(test)]
    pub fn with_neutral_strip(
        sample_rate: u32,
        maximum_frames: usize,
        controls: Arc<BusControls>,
        meters: Arc<FinalBusMeters>,
    ) -> Result<(Self, Arc<MasterStripControls>), String> {
        let strip_controls = Arc::new(MasterStripControls::new(
            sample_rate,
            &MasterStripSettings::default(),
        )?);
        let processor = Self::new(
            sample_rate,
            maximum_frames,
            controls,
            Arc::clone(&strip_controls),
            meters,
        )?;
        Ok((processor, strip_controls))
    }

    #[cfg(test)]
    pub fn latency_samples(&self) -> usize {
        self.strip.latency_samples()
    }

    #[cfg(test)]
    pub fn lookahead_samples(&self) -> usize {
        self.strip.lookahead_samples()
    }

    #[cfg(test)]
    pub fn safety_clamp_count(&self) -> u64 {
        self.strip.safety_clamp_count()
    }

    #[inline]
    pub fn process_source(&mut self, source: BusSource, frames: &mut [StereoFrame]) {
        let index = source.index();
        self.source_faders[index].refresh(
            self.controls.source_gain_db(source),
            self.controls.source_muted(source),
        );
        let mut peak = StereoFrame::SILENCE;
        for frame in frames {
            let gain = self.source_faders[index].next();
            *frame = StereoFrame::new(frame.left * gain, frame.right * gain).finite_or_silence();
            peak.left = peak.left.max(frame.left.abs());
            peak.right = peak.right.max(frame.right.abs());
        }
        self.controls.publish_source_peak(source, peak);
    }

    #[inline]
    pub fn process_final(&mut self, frames: &mut [StereoFrame]) {
        self.master_fader
            .refresh(self.controls.master_gain_db(), false);
        for frame in frames.iter_mut() {
            let master = self.master_fader.next();
            *frame =
                StereoFrame::new(frame.left * master, frame.right * master).finite_or_silence();
        }
        self.strip.process(frames);
    }

    pub fn reset(&mut self) {
        self.strip.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::allocation_test::assert_no_allocations;

    fn processor(rate: u32, frames: usize) -> (FinalBusProcessor, Arc<BusControls>) {
        let controls = Arc::new(BusControls::default());
        for source in BusSource::ALL {
            assert!(controls.set_source_gain_db(source, 0.0));
        }
        let (processor, _) = FinalBusProcessor::with_neutral_strip(
            rate,
            frames,
            Arc::clone(&controls),
            Arc::new(FinalBusMeters::default()),
        )
        .unwrap();
        (processor, controls)
    }

    #[test]
    fn source_sum_stereo_identity_and_declared_latency_are_preserved() {
        let (mut bus, _) = processor(48_000, 256);
        let mut synth = [StereoFrame::new(0.01, 0.02); 256];
        let mut loop_frames = [StereoFrame::new(0.03, 0.04); 256];
        let mut input = [StereoFrame::new(0.05, 0.06); 256];
        bus.process_source(BusSource::Synth, &mut synth);
        bus.process_source(BusSource::Loop, &mut loop_frames);
        bus.process_source(BusSource::Input, &mut input);
        let mut sum = std::array::from_fn::<_, 256, _>(|index| {
            StereoFrame::new(
                synth[index].left + loop_frames[index].left + input[index].left,
                synth[index].right + loop_frames[index].right + input[index].right,
            )
        });
        bus.process_final(&mut sum);
        assert_eq!(bus.latency_samples(), 133);
        assert_eq!(bus.lookahead_samples(), 120);
        assert!((sum[133].left - 0.09).abs() < 1e-6);
        assert!((sum[133].right - 0.12).abs() < 1e-6);
        assert_eq!(bus.safety_clamp_count(), 0);
    }

    #[test]
    fn source_mute_master_level_finite_recovery_and_callback_are_safe() {
        let (mut bus, controls) = processor(48_000, 1024);
        assert!(!controls.set_source_gain_db(BusSource::Synth, 7.0));
        assert!(!controls.set_master_gain_db(0.1));
        controls.set_source_muted(BusSource::Synth, true);
        assert!(controls.set_master_gain_db(-6.0));
        let mut source = [StereoFrame::new(f32::NAN, f32::INFINITY); 1024];
        let mut output = [StereoFrame::SILENCE; 1024];
        assert_no_allocations(|| {
            bus.process_source(BusSource::Synth, &mut source);
            output.copy_from_slice(&source);
            bus.process_final(&mut output);
        });
        assert!(output
            .iter()
            .all(|frame| frame.left.is_finite() && frame.right.is_finite()));
        assert_eq!(controls.source_peak(BusSource::Synth), StereoFrame::SILENCE);
    }

    #[test]
    fn source_meter_is_post_smoothed_owner_gain_and_shared_with_canonical_controls() {
        let (mut bus, controls) = processor(48_000, 1024);
        assert!(controls.set_source_gain_db(BusSource::Loop, -6.0));
        let mut source = [StereoFrame::new(0.5, -0.25); 1024];
        bus.process_source(BusSource::Loop, &mut source);
        let peak = controls.source_peak(BusSource::Loop);
        assert!(peak.left > 0.24 && peak.left < 0.5);
        assert!(peak.right > 0.12 && peak.right < 0.25);
        assert_eq!(controls.source_peak(BusSource::Synth), StereoFrame::SILENCE);
    }

    #[test]
    #[ignore = "on-demand callback cost measurement"]
    fn source_meter_callback_cost_has_realtime_headroom() {
        const SAMPLE_RATE: u32 = 48_000;
        const FRAMES: usize = 256;
        const CALLBACKS: u128 = 20_000;
        let (mut bus, controls) = processor(SAMPLE_RATE, FRAMES);
        let mut source = [StereoFrame::new(0.25, -0.125); FRAMES];

        for _ in 0..128 {
            bus.process_source(BusSource::Synth, &mut source);
        }
        let started = std::time::Instant::now();
        for _ in 0..CALLBACKS {
            bus.process_source(BusSource::Synth, &mut source);
            std::hint::black_box(controls.source_peak(BusSource::Synth));
        }
        let average_ns = started.elapsed().as_nanos() / CALLBACKS;
        let callback_deadline_ns = FRAMES as u128 * 1_000_000_000 / u128::from(SAMPLE_RATE);
        let budget_ns = callback_deadline_ns / 10;
        eprintln!(
            "metered owner source: {average_ns} ns per {FRAMES}-frame callback ({:.3}% of deadline)",
            average_ns as f64 * 100.0 / callback_deadline_ns as f64
        );
        assert!(
            average_ns < budget_ns,
            "metered owner source used {average_ns} ns; 10% callback budget is {budget_ns} ns"
        );
    }

    #[test]
    fn final_processing_is_chunk_invariant() {
        let input = (0..4096)
            .map(|index| {
                StereoFrame::new(
                    ((index * 37 % 211) as f32 / 1050.0) - 0.1,
                    ((index * 61 % 197) as f32 / 980.0) - 0.1,
                )
            })
            .collect::<Vec<_>>();
        let run = |chunk: usize| {
            let (mut bus, _) = processor(44_100, chunk);
            let mut output = input.clone();
            for frames in output.chunks_mut(chunk) {
                bus.process_final(frames);
            }
            output
        };
        assert_eq!(run(64), run(127));
    }
}
