//! Preallocated, allocation-free insert-effect runtime slots.

mod compressor;
mod crusher;
mod delay;
mod distortion;
mod eq;
mod filter;
mod gate;
mod modulated_delay;
mod phaser;
mod reverb;
mod tremolo_pan;

use crate::audio_graph::{EffectId, EffectInstance, EffectKind};
use crate::dsp::{db_to_gain, AtomicMeter, MeterAccumulator, SmoothedValue, StereoFrame};
use crate::effect_schema;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

pub use compressor::AtomicGainReduction;
use compressor::Compressor;
use crusher::Crusher;
use delay::Delay;
use distortion::Distortion;
use eq::Eq;
use filter::Filter;
use gate::Gate;
use modulated_delay::ModulatedDelay;
use phaser::Phaser;
use reverb::Reverb;
use tremolo_pan::TremoloPan;

const PARAMETER_SMOOTH_SAMPLES: u32 = 64;
const BYPASS_FADE_MILLISECONDS: f32 = 5.0;
const MAX_RUNTIME_PARAMETERS: usize = 32;
const BYPASS_DIRTY_BIT: u64 = 1 << 63;

/// Validated control-thread publisher for one running effect identity. The
/// callback reads a fixed atomic array and never allocates or locks.
pub struct EffectControl {
    id: EffectId,
    kind: EffectKind,
    version: u32,
    active: AtomicBool,
    values: [AtomicU32; MAX_RUNTIME_PARAMETERS],
    bypass: AtomicBool,
    dirty: AtomicU64,
}

impl EffectControl {
    fn new(effect: &EffectInstance) -> Arc<Self> {
        Arc::new(Self {
            id: effect.id,
            kind: effect.kind,
            version: effect.version,
            active: AtomicBool::new(true),
            values: std::array::from_fn(|index| {
                let value = effect_schema::schema(effect.kind)
                    .get(index)
                    .map(|spec| {
                        effect
                            .parameters
                            .get(spec.name)
                            .copied()
                            .unwrap_or(spec.default)
                    })
                    .unwrap_or_default();
                AtomicU32::new(value.to_bits())
            }),
            bypass: AtomicBool::new(effect.bypass),
            dirty: AtomicU64::new(0),
        })
    }

    pub const fn id(&self) -> EffectId {
        self.id
    }

    pub const fn kind(&self) -> EffectKind {
        self.kind
    }

    pub const fn version(&self) -> u32 {
        self.version
    }

    pub fn publish_normalized(&self, name: &str, normalized: u16) -> Result<(), EffectError> {
        if !self.active.load(Ordering::Acquire) {
            return Err(EffectError::new("effect control target is stale"));
        }
        let (index, spec) = effect_schema::schema(self.kind)
            .iter()
            .enumerate()
            .find(|(_, spec)| spec.name == name)
            .ok_or_else(|| EffectError::new(format!("unknown {:?} parameter {name}", self.kind)))?;
        if index >= MAX_RUNTIME_PARAMETERS {
            return Err(EffectError::new(
                "effect parameter exceeds runtime control bound",
            ));
        }
        let unit = f32::from(normalized) / 65_535.0;
        let value = match spec.value_type {
            effect_schema::ParameterType::Continuous
                if self.kind == EffectKind::Eq && spec.unit == "Hz" =>
            {
                spec.minimum * (spec.maximum / spec.minimum).powf(unit)
            }
            effect_schema::ParameterType::Continuous => {
                spec.minimum + unit * (spec.maximum - spec.minimum)
            }
            effect_schema::ParameterType::Integer => {
                (spec.minimum + unit * (spec.maximum - spec.minimum)).round()
            }
            effect_schema::ParameterType::Toggle => (normalized >= 32_768) as u8 as f32,
            effect_schema::ParameterType::Choices(choices) => {
                let choice = usize::from(normalized) * choices.len() / 65_536;
                f32::from(choices[choice.min(choices.len().saturating_sub(1))])
            }
        };
        if !spec.accepts(value) {
            return Err(EffectError::new("prepared effect value is incompatible"));
        }
        self.values[index].store(value.to_bits(), Ordering::Release);
        self.dirty.fetch_or(1u64 << index, Ordering::AcqRel);
        Ok(())
    }

    pub fn publish_bypass(&self, bypass: bool) -> Result<(), EffectError> {
        if !self.active.load(Ordering::Acquire) {
            return Err(EffectError::new("effect control target is stale"));
        }
        self.bypass.store(bypass, Ordering::Release);
        self.dirty.fetch_or(BYPASS_DIRTY_BIT, Ordering::AcqRel);
        Ok(())
    }

    fn deactivate(&self) {
        self.active.store(false, Ordering::Release);
        self.dirty.store(0, Ordering::Release);
    }
}

#[derive(Default)]
struct EffectControlRegistry {
    controls: BTreeMap<EffectId, Arc<EffectControl>>,
    graph_ids: BTreeSet<EffectId>,
    drum_ids: BTreeSet<EffectId>,
}

#[derive(Default)]
pub struct EffectControlHub {
    controls: RwLock<EffectControlRegistry>,
}

impl EffectControlHub {
    pub fn replace_graph(
        &self,
        controls: impl IntoIterator<Item = (EffectId, Arc<EffectControl>)>,
    ) {
        if let Ok(mut active) = self.controls.write() {
            let controls = controls.into_iter().collect::<BTreeMap<_, _>>();
            let ids = controls.keys().copied().collect::<BTreeSet<_>>();
            for id in std::mem::take(&mut active.graph_ids) {
                if !active.drum_ids.contains(&id) {
                    active.controls.remove(&id);
                }
            }
            active.controls.extend(controls);
            active.graph_ids = ids;
        }
    }

    pub fn replace_drums(
        &self,
        controls: impl IntoIterator<Item = (EffectId, Arc<EffectControl>)>,
    ) {
        if let Ok(mut active) = self.controls.write() {
            let controls = controls.into_iter().collect::<BTreeMap<_, _>>();
            let ids = controls.keys().copied().collect::<BTreeSet<_>>();
            for id in std::mem::take(&mut active.drum_ids) {
                if !active.graph_ids.contains(&id) {
                    active.controls.remove(&id);
                }
            }
            active.controls.extend(controls);
            active.drum_ids = ids;
        }
    }

    pub fn clear_graph(&self) {
        if let Ok(mut active) = self.controls.write() {
            for id in std::mem::take(&mut active.graph_ids) {
                if !active.drum_ids.contains(&id) {
                    active.controls.remove(&id);
                }
            }
        }
    }

    pub fn clear_drums(&self) {
        if let Ok(mut active) = self.controls.write() {
            for id in std::mem::take(&mut active.drum_ids) {
                if !active.graph_ids.contains(&id) {
                    active.controls.remove(&id);
                }
            }
        }
    }

    pub fn publish_normalized(
        &self,
        id: EffectId,
        kind: EffectKind,
        version: u32,
        parameter: Option<&str>,
        value: u16,
    ) -> Result<(), EffectError> {
        let control = self
            .controls
            .read()
            .map_err(|_| EffectError::new("effect control registry is unavailable"))?
            .controls
            .get(&id)
            .cloned()
            .ok_or_else(|| EffectError::new("effect automation target is stale or missing"))?;
        if control.kind() != kind || control.version() != version {
            return Err(EffectError::new("effect automation schema is incompatible"));
        }
        if let Some(parameter) = parameter {
            control.publish_normalized(parameter, value)
        } else {
            control.publish_bypass(value >= 32_768)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BypassMode {
    DryPassthrough,
    Silence,
    WetTail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectError(String);

impl EffectError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for EffectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for EffectError {}

#[derive(Clone)]
pub struct MeterHandles {
    pub input: Arc<AtomicMeter>,
    pub output: Arc<AtomicMeter>,
    pub gain_reduction: Option<Arc<AtomicGainReduction>>,
}

enum Processor {
    Utility(Utility),
    Eq(Box<Eq>),
    Compressor(Box<Compressor>),
    Distortion(Box<Distortion>),
    Delay(Box<Delay>),
    Chorus(Box<ModulatedDelay>),
    Flanger(Box<ModulatedDelay>),
    Phaser(Box<Phaser>),
    TremoloPan(Box<TremoloPan>),
    Reverb(Box<Reverb>),
    Crusher(Box<Crusher>),
    Gate(Box<Gate>),
    Filter(Box<Filter>),
}

impl Processor {
    fn compile(effect: &EffectInstance, sample_rate: u32) -> Result<Self, EffectError> {
        match effect.kind {
            EffectKind::Utility => Ok(Self::Utility(Utility::compile(effect)?)),
            EffectKind::Eq => Ok(Self::Eq(Box::new(Eq::compile(effect, sample_rate)?))),
            EffectKind::Compressor => Ok(Self::Compressor(Box::new(Compressor::compile(
                effect,
                sample_rate,
            )?))),
            EffectKind::Distortion => Ok(Self::Distortion(Box::new(Distortion::compile(
                effect,
                sample_rate,
            )?))),
            EffectKind::Delay => Ok(Self::Delay(Box::new(Delay::compile(effect, sample_rate)?))),
            EffectKind::Chorus => Ok(Self::Chorus(Box::new(ModulatedDelay::compile(
                effect,
                sample_rate,
            )?))),
            EffectKind::Flanger => Ok(Self::Flanger(Box::new(ModulatedDelay::compile(
                effect,
                sample_rate,
            )?))),
            EffectKind::Phaser => Ok(Self::Phaser(Box::new(Phaser::compile(
                effect,
                sample_rate,
            )?))),
            EffectKind::TremoloPan => Ok(Self::TremoloPan(Box::new(TremoloPan::compile(
                effect,
                sample_rate,
            )?))),
            EffectKind::Reverb => Ok(Self::Reverb(Box::new(Reverb::compile(
                effect,
                sample_rate,
            )?))),
            EffectKind::Crusher => Ok(Self::Crusher(Box::new(Crusher::compile(effect)?))),
            EffectKind::Gate => Ok(Self::Gate(Box::new(Gate::compile(effect, sample_rate)?))),
            EffectKind::Filter => Ok(Self::Filter(Box::new(Filter::compile(
                effect,
                sample_rate,
            )?))),
        }
    }

    #[inline]
    fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        match self {
            Self::Utility(effect) => effect.process(frame),
            Self::Eq(effect) => effect.process(frame),
            Self::Compressor(effect) => effect.process(frame),
            Self::Distortion(effect) => effect.process(frame),
            Self::Delay(effect) => effect.process(frame),
            Self::Chorus(effect) | Self::Flanger(effect) => effect.process(frame),
            Self::Phaser(effect) => effect.process(frame),
            Self::TremoloPan(effect) => effect.process(frame),
            Self::Reverb(effect) => effect.process(frame),
            Self::Crusher(effect) => effect.process(frame),
            Self::Gate(effect) => effect.process(frame),
            Self::Filter(effect) => effect.process(frame),
        }
    }

    fn set_parameter(&mut self, name: &str, value: f32) -> Result<(), EffectError> {
        match self {
            Self::Utility(effect) => effect.set_parameter(name, value),
            Self::Eq(effect) => effect.set_parameter(name, value),
            Self::Compressor(effect) => effect.set_parameter(name, value),
            Self::Distortion(effect) => effect.set_parameter(name, value),
            Self::Delay(effect) => effect.set_parameter(name, value),
            Self::Chorus(effect) | Self::Flanger(effect) => effect.set_parameter(name, value),
            Self::Phaser(effect) => effect.set_parameter(name, value),
            Self::TremoloPan(effect) => effect.set_parameter(name, value),
            Self::Reverb(effect) => effect.set_parameter(name, value),
            Self::Crusher(effect) => effect.set_parameter(name, value),
            Self::Gate(effect) => effect.set_parameter(name, value),
            Self::Filter(effect) => effect.set_parameter(name, value),
        }
    }

    fn reset(&mut self) {
        match self {
            Self::Utility(effect) => effect.reset(),
            Self::Eq(effect) => effect.reset(),
            Self::Compressor(effect) => effect.reset(),
            Self::Distortion(effect) => effect.reset(),
            Self::Delay(effect) => effect.reset(),
            Self::Chorus(effect) | Self::Flanger(effect) => effect.reset(),
            Self::Phaser(effect) => effect.reset(),
            Self::TremoloPan(effect) => effect.reset(),
            Self::Reverb(effect) => effect.reset(),
            Self::Crusher(effect) => effect.reset(),
            Self::Gate(effect) => effect.reset(),
            Self::Filter(effect) => effect.reset(),
        }
    }

    fn gain_reduction(&self) -> Option<Arc<AtomicGainReduction>> {
        match self {
            Self::Compressor(effect) => Some(effect.gain_reduction()),
            Self::Utility(_)
            | Self::Eq(_)
            | Self::Distortion(_)
            | Self::Delay(_)
            | Self::Chorus(_)
            | Self::Flanger(_)
            | Self::Phaser(_)
            | Self::TremoloPan(_)
            | Self::Reverb(_)
            | Self::Crusher(_)
            | Self::Gate(_)
            | Self::Filter(_) => None,
        }
    }

    fn publish(&self) {
        if let Self::Compressor(effect) = self {
            effect.publish();
        }
    }

    fn memory_bytes(&self) -> usize {
        match self {
            Self::Delay(effect) => effect.memory_bytes(),
            Self::Reverb(effect) => effect.memory_bytes(),
            _ => 0,
        }
    }

    fn set_bypass(&mut self, bypass: bool, fade_samples: u32, wet_only_tail: bool) -> bool {
        match self {
            Self::Delay(effect) => effect.set_bypass(bypass, fade_samples, wet_only_tail),
            Self::Reverb(effect) => {
                effect.set_bypass(bypass, fade_samples);
                false
            }
            Self::Compressor(effect) => {
                effect.set_bypass(bypass);
                false
            }
            _ => false,
        }
    }
}

pub struct EffectSlot {
    id: EffectId,
    kind: EffectKind,
    processor: Processor,
    processed_mix: SmoothedValue,
    bypass_mode: BypassMode,
    wet_only: bool,
    bypass_fade_samples: u32,
    input_meter: MeterAccumulator,
    output_meter: MeterAccumulator,
    published_input: Arc<AtomicMeter>,
    published_output: Arc<AtomicMeter>,
    control: Arc<EffectControl>,
}

impl EffectSlot {
    pub fn compile(
        effect: &EffectInstance,
        sample_rate: u32,
        meter_window: usize,
    ) -> Result<Self, EffectError> {
        Self::compile_with_placement(
            effect,
            sample_rate,
            meter_window,
            false,
            BypassMode::DryPassthrough,
        )
    }

    pub(crate) fn compile_with_placement(
        effect: &EffectInstance,
        sample_rate: u32,
        meter_window: usize,
        wet_only: bool,
        bypass_mode: BypassMode,
    ) -> Result<Self, EffectError> {
        effect_schema::validate(effect).map_err(|error| EffectError::new(error.to_string()))?;
        if !(8_000..=384_000).contains(&sample_rate) {
            return Err(EffectError::new("unsupported effect sample rate"));
        }
        let bypass_fade_samples =
            ((sample_rate as f32 * BYPASS_FADE_MILLISECONDS * 0.001).round() as u32).max(1);
        let mut slot = Self {
            id: effect.id,
            kind: effect.kind,
            processor: Processor::compile(effect, sample_rate)?,
            processed_mix: SmoothedValue::new(
                if effect.bypass && !matches!(bypass_mode, BypassMode::WetTail) {
                    0.0
                } else {
                    1.0
                },
            )
            .map_err(|error| EffectError::new(error.to_string()))?,
            bypass_mode,
            wet_only,
            bypass_fade_samples,
            input_meter: MeterAccumulator::new(meter_window)
                .map_err(|error| EffectError::new(error.to_string()))?,
            output_meter: MeterAccumulator::new(meter_window)
                .map_err(|error| EffectError::new(error.to_string()))?,
            published_input: Arc::new(AtomicMeter::default()),
            published_output: Arc::new(AtomicMeter::default()),
            control: EffectControl::new(effect),
        };
        slot.processor.set_bypass(
            effect.bypass,
            slot.bypass_fade_samples,
            effect.bypass && matches!(bypass_mode, BypassMode::WetTail),
        );
        Ok(slot)
    }

    pub const fn id(&self) -> EffectId {
        self.id
    }

    pub const fn kind(&self) -> EffectKind {
        self.kind
    }

    pub fn memory_bytes(&self) -> usize {
        self.processor.memory_bytes()
    }

    /// Apply a compatible persisted instance while retaining recursive DSP
    /// history, smoothing state, deterministic noise state, and meter handles.
    pub fn apply_instance(&mut self, effect: &EffectInstance) -> Result<(), EffectError> {
        self.apply_instance_with_placement(effect, false, BypassMode::DryPassthrough)
    }

    pub(crate) fn apply_instance_with_placement(
        &mut self,
        effect: &EffectInstance,
        wet_only: bool,
        bypass_mode: BypassMode,
    ) -> Result<(), EffectError> {
        if effect.id != self.id || effect.kind != self.kind {
            return Err(EffectError::new("effect instance is not state-compatible"));
        }
        effect_schema::validate(effect).map_err(|error| EffectError::new(error.to_string()))?;
        self.wet_only = wet_only;
        for spec in effect_schema::schema(effect.kind) {
            self.set_parameter(
                spec.name,
                effect
                    .parameters
                    .get(spec.name)
                    .copied()
                    .unwrap_or(spec.default),
            )?;
        }
        self.set_bypass_with_mode(effect.bypass, bypass_mode)
    }

    pub fn meters(&self) -> MeterHandles {
        MeterHandles {
            input: Arc::clone(&self.published_input),
            output: Arc::clone(&self.published_output),
            gain_reduction: self.processor.gain_reduction(),
        }
    }

    pub fn control(&self) -> Arc<EffectControl> {
        Arc::clone(&self.control)
    }

    pub fn set_bypass(&mut self, bypass: bool) -> Result<(), EffectError> {
        self.set_bypass_with_mode(bypass, BypassMode::DryPassthrough)
    }

    pub(crate) fn set_bypass_with_mode(
        &mut self,
        bypass: bool,
        bypass_mode: BypassMode,
    ) -> Result<(), EffectError> {
        self.bypass_mode = bypass_mode;
        let preserve_tail = self.processor.set_bypass(
            bypass,
            self.bypass_fade_samples,
            bypass && matches!(bypass_mode, BypassMode::WetTail),
        );
        self.processed_mix
            .set_target(
                if bypass && !preserve_tail { 0.0 } else { 1.0 },
                self.bypass_fade_samples,
            )
            .map_err(|error| EffectError::new(error.to_string()))
    }

    pub fn set_parameter(&mut self, name: &str, value: f32) -> Result<(), EffectError> {
        let spec = effect_schema::schema(self.kind)
            .iter()
            .find(|spec| spec.name == name)
            .ok_or_else(|| EffectError::new(format!("unknown {:?} parameter {name}", self.kind)))?;
        if !spec.accepts(value) {
            return Err(EffectError::new(format!(
                "invalid {:?} parameter {name}",
                self.kind
            )));
        }
        self.processor.set_parameter(name, value)
    }

    /// Process an in-place stereo block without allocation, locks, logging,
    /// I/O, blocking, or unbounded coefficient work.
    pub fn process(&mut self, frames: &mut [StereoFrame]) {
        self.consume_control();
        for frame in frames.iter_mut() {
            let dry = self.input_meter.process(*frame);
            let processed = self.processor.process(dry);
            let processed = if processed.left.is_finite() && processed.right.is_finite() {
                processed
            } else {
                self.processor.reset();
                if self.wet_only {
                    StereoFrame::SILENCE
                } else {
                    dry
                }
            };
            let wet = self.processed_mix.next_value();
            let bypass = match self.bypass_mode {
                BypassMode::DryPassthrough => dry,
                BypassMode::Silence | BypassMode::WetTail => StereoFrame::SILENCE,
            };
            let output = if wet <= 0.0 {
                bypass
            } else if wet >= 1.0 {
                processed
            } else {
                StereoFrame::new(
                    bypass.left + (processed.left - bypass.left) * wet,
                    bypass.right + (processed.right - bypass.right) * wet,
                )
                .finite_or_silence()
            };
            *frame = self.output_meter.process(output);
        }
        self.published_input
            .publish(self.input_meter.snapshot_and_clear_peak());
        self.published_output
            .publish(self.output_meter.snapshot_and_clear_peak());
        self.processor.publish();
    }

    #[inline]
    fn consume_control(&mut self) {
        let dirty = self.control.dirty.swap(0, Ordering::AcqRel);
        for (index, spec) in effect_schema::schema(self.kind).iter().enumerate() {
            if dirty & (1u64 << index) == 0 {
                continue;
            }
            let value = f32::from_bits(self.control.values[index].load(Ordering::Acquire));
            let _ = self.processor.set_parameter(spec.name, value);
        }
        if dirty & BYPASS_DIRTY_BIT != 0 {
            let bypass = self.control.bypass.load(Ordering::Acquire);
            let _ = self.set_bypass_with_mode(bypass, self.bypass_mode);
        }
    }

    pub fn reset(&mut self) {
        self.processor.reset();
        self.input_meter.reset();
        self.output_meter.reset();
        self.published_input.publish(Default::default());
        self.published_output.publish(Default::default());
    }
}

impl Drop for EffectSlot {
    fn drop(&mut self) {
        self.control.deactivate();
    }
}

struct Utility {
    trim: SmoothedValue,
    left_pan: SmoothedValue,
    right_pan: SmoothedValue,
    width: SmoothedValue,
    invert_left: SmoothedValue,
    invert_right: SmoothedValue,
    mute: SmoothedValue,
}

impl Utility {
    fn compile(effect: &EffectInstance) -> Result<Self, EffectError> {
        let value = |name| {
            effect_schema::parameter(effect, name)
                .map_err(|error| EffectError::new(error.to_string()))
        };
        let (left_pan, right_pan) = stereo_pan_gains(value("pan")?);
        Ok(Self {
            trim: smooth(db_to_gain(value("trim_db")?)?),
            left_pan: smooth(left_pan),
            right_pan: smooth(right_pan),
            width: smooth(value("width_percent")? * 0.01),
            invert_left: smooth(polarity(value("invert_left")?)),
            invert_right: smooth(polarity(value("invert_right")?)),
            mute: smooth(1.0 - value("mute")?),
        })
    }

    #[inline]
    fn process(&mut self, frame: StereoFrame) -> StereoFrame {
        let trim = self.trim.next_value() * self.mute.next_value();
        let left = frame.left * trim * self.left_pan.next_value() * self.invert_left.next_value();
        let right =
            frame.right * trim * self.right_pan.next_value() * self.invert_right.next_value();
        let mid = (left + right) * 0.5;
        let side = (left - right) * 0.5 * self.width.next_value();
        StereoFrame::new(mid + side, mid - side).finite_or_silence()
    }

    fn set_parameter(&mut self, name: &str, value: f32) -> Result<(), EffectError> {
        if name == "pan" {
            let (left, right) = stereo_pan_gains(value);
            set_smooth(&mut self.left_pan, left)?;
            return set_smooth(&mut self.right_pan, right);
        }
        let (target, target_value) = match name {
            "trim_db" => (&mut self.trim, db_to_gain(value)?),
            "width_percent" => (&mut self.width, value * 0.01),
            "invert_left" => (&mut self.invert_left, polarity(value)),
            "invert_right" => (&mut self.invert_right, polarity(value)),
            "mute" => (&mut self.mute, 1.0 - value),
            _ => {
                return Err(EffectError::new(format!(
                    "unknown Utility parameter {name}"
                )))
            }
        };
        set_smooth(target, target_value)
    }

    fn reset(&mut self) {
        // Utility has no recursive state. Smoothers intentionally retain their
        // current values so reset never jumps a live gain or polarity target.
    }
}

fn smooth(value: f32) -> SmoothedValue {
    SmoothedValue::new(value).expect("validated finite effect parameter")
}

fn set_smooth(value: &mut SmoothedValue, target: f32) -> Result<(), EffectError> {
    value
        .set_target(target, PARAMETER_SMOOTH_SAMPLES)
        .map_err(|error| EffectError::new(error.to_string()))
}

fn polarity(value: f32) -> f32 {
    if value == 0.0 {
        1.0
    } else {
        -1.0
    }
}

fn stereo_pan_gains(pan: f32) -> (f32, f32) {
    if pan < 0.0 {
        (1.0, (-pan * std::f32::consts::FRAC_PI_2).cos())
    } else {
        ((pan * std::f32::consts::FRAC_PI_2).cos(), 1.0)
    }
}

impl From<crate::dsp::DspError> for EffectError {
    fn from(error: crate::dsp::DspError) -> Self {
        Self::new(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_graph::EFFECT_FORMAT_VERSION;
    use crate::dsp::allocation_test::assert_no_allocations;
    use std::collections::BTreeMap;

    fn utility(parameters: BTreeMap<String, f32>, bypass: bool) -> EffectInstance {
        EffectInstance {
            id: 7,
            kind: EffectKind::Utility,
            version: EFFECT_FORMAT_VERSION,
            bypass,
            parameters,
            owned_memory_bytes: 0,
        }
    }

    #[test]
    fn slot_preserves_identity_and_rejects_mismatched_parameter_schemas() {
        let slot = EffectSlot::compile(&utility(BTreeMap::new(), false), 48_000, 128).unwrap();
        assert_eq!(slot.id(), 7);
        assert_eq!(slot.kind(), EffectKind::Utility);
        let mut effect = utility(BTreeMap::new(), false);
        effect.kind = EffectKind::Chorus;
        effect.parameters.insert("trim_db".into(), 0.0);
        assert!(EffectSlot::compile(&effect, 48_000, 128).is_err());
    }

    #[test]
    fn utility_processes_stereo_parameters_without_allocating() {
        let effect = utility(
            BTreeMap::from([
                ("trim_db".into(), -6.0206),
                ("pan".into(), 1.0),
                ("width_percent".into(), 100.0),
            ]),
            false,
        );
        let mut slot = EffectSlot::compile(&effect, 48_000, 128).unwrap();
        let meters = slot.meters();
        let mut block = [StereoFrame::new(0.5, 0.5); 128];
        assert_no_allocations(|| slot.process(&mut block));
        assert!(block
            .iter()
            .all(|frame| frame.left.abs() < 0.001 && (frame.right - 0.25).abs() < 0.001));
        assert!(meters.input.load().peak.left >= 0.5);
        assert!(meters.output.load().peak.right >= 0.249);
    }

    #[test]
    fn utility_gain_pan_width_polarity_and_mute_have_exact_stereo_laws() {
        let cases = [
            (
                BTreeMap::from([("trim_db".into(), -6.0206)]),
                StereoFrame::new(0.5, -0.25),
                StereoFrame::new(0.25, -0.125),
            ),
            (
                BTreeMap::from([("pan".into(), -1.0)]),
                StereoFrame::new(0.5, -0.25),
                StereoFrame::new(0.5, 0.0),
            ),
            (
                BTreeMap::from([("width_percent".into(), 0.0)]),
                StereoFrame::new(0.75, -0.25),
                StereoFrame::new(0.25, 0.25),
            ),
            (
                BTreeMap::from([("invert_left".into(), 1.0), ("invert_right".into(), 1.0)]),
                StereoFrame::new(0.5, -0.25),
                StereoFrame::new(-0.5, 0.25),
            ),
            (
                BTreeMap::from([("mute".into(), 1.0)]),
                StereoFrame::new(0.5, -0.25),
                StereoFrame::SILENCE,
            ),
        ];
        for (parameters, input, expected) in cases {
            let mut slot = EffectSlot::compile(&utility(parameters, false), 48_000, 1).unwrap();
            let mut frame = [input];
            slot.process(&mut frame);
            assert!((frame[0].left - expected.left).abs() < 1.0e-5);
            assert!((frame[0].right - expected.right).abs() < 1.0e-5);
        }
    }

    #[test]
    fn bypass_crossfade_is_bounded_and_reaches_exact_dry() {
        let effect = utility(BTreeMap::from([("trim_db".into(), -12.0)]), false);
        let mut slot = EffectSlot::compile(&effect, 48_000, 256).unwrap();
        slot.set_bypass(true).unwrap();
        let mut block = [StereoFrame::new(0.5, -0.5); 256];
        slot.process(&mut block);
        assert!(block.iter().all(|frame| {
            frame.left.is_finite()
                && frame.right.is_finite()
                && (0.125..=0.5).contains(&frame.left)
                && (-0.5..=-0.125).contains(&frame.right)
        }));
        let maximum_step = block
            .windows(2)
            .map(|pair| (pair[1].left - pair[0].left).abs())
            .fold(0.0_f32, f32::max);
        assert!(maximum_step < 0.002, "bypass step {maximum_step}");
        assert_eq!(block[255], StereoFrame::new(0.5, -0.5));
    }

    #[test]
    fn poison_is_metered_and_recovers_to_finite_output() {
        let mut slot = EffectSlot::compile(&utility(BTreeMap::new(), false), 48_000, 4).unwrap();
        let meters = slot.meters();
        let mut block = [
            StereoFrame::new(f32::NAN, 0.25),
            StereoFrame::new(0.5, f32::INFINITY),
            StereoFrame::new(0.25, -0.25),
            StereoFrame::SILENCE,
        ];
        slot.process(&mut block);
        assert!(block
            .iter()
            .all(|frame| frame.left.is_finite() && frame.right.is_finite()));
        assert_eq!(meters.input.load().non_finite, 2);
        assert_eq!(meters.output.load().non_finite, 0);
        slot.reset();
        assert_eq!(meters.input.load(), Default::default());
        assert_eq!(meters.output.load(), Default::default());
    }

    #[test]
    fn rapid_valid_moves_are_smoothed_and_invalid_moves_are_refused() {
        let mut slot = EffectSlot::compile(&utility(BTreeMap::new(), false), 48_000, 64).unwrap();
        for index in 0..100 {
            slot.set_parameter("trim_db", if index % 2 == 0 { -60.0 } else { 12.0 })
                .unwrap();
            slot.set_parameter("invert_left", (index % 2) as f32)
                .unwrap();
            let mut block = [StereoFrame::new(1.0, -1.0); 17];
            slot.process(&mut block);
            assert!(block
                .iter()
                .all(|frame| frame.left.is_finite() && frame.right.is_finite()));
        }
        assert!(slot.set_parameter("trim_db", f32::NAN).is_err());
        assert!(slot.set_parameter("future", 0.0).is_err());
    }

    #[test]
    fn published_runtime_control_is_lock_free_bounded_and_stale_safe() {
        let effect = utility(BTreeMap::new(), false);
        let mut slot = EffectSlot::compile(&effect, 48_000, 64).unwrap();
        let control = slot.control();
        control.publish_normalized("trim_db", u16::MAX).unwrap();
        control.publish_bypass(false).unwrap();
        let mut block = [StereoFrame::new(0.1, -0.1); 64];
        assert_no_allocations(|| slot.process(&mut block));
        assert!(block
            .iter()
            .all(|frame| frame.left.is_finite() && frame.right.is_finite()));
        drop(slot);
        assert!(control.publish_normalized("trim_db", 0).is_err());
        assert!(control.publish_normalized("missing", 0).is_err());
    }

    #[test]
    fn graph_and_drum_control_owners_clear_independently() {
        let graph = EffectSlot::compile(&utility(BTreeMap::new(), false), 48_000, 64).unwrap();
        let mut drum_effect = utility(BTreeMap::new(), false);
        drum_effect.id = 8;
        let drums = EffectSlot::compile(&drum_effect, 48_000, 64).unwrap();
        let graph_control = graph.control();
        let drum_control = drums.control();
        let hub = EffectControlHub::default();
        hub.replace_graph([(graph.id(), graph_control)]);
        hub.replace_drums([(drums.id(), drum_control)]);
        hub.clear_graph();
        assert!(hub
            .publish_normalized(
                8,
                EffectKind::Utility,
                EFFECT_FORMAT_VERSION,
                Some("mute"),
                0
            )
            .is_ok());
        assert!(hub
            .publish_normalized(
                7,
                EffectKind::Utility,
                EFFECT_FORMAT_VERSION,
                Some("mute"),
                0
            )
            .is_err());
        hub.clear_drums();
        assert!(hub
            .publish_normalized(
                8,
                EffectKind::Utility,
                EFFECT_FORMAT_VERSION,
                Some("mute"),
                0
            )
            .is_err());
    }

    #[test]
    fn normalized_eq_frequency_uses_the_editor_logarithmic_control_space() {
        let mut effect = utility(BTreeMap::new(), false);
        effect.id = 9;
        effect.kind = EffectKind::Eq;
        effect.parameters.clear();
        let slot = EffectSlot::compile(&effect, 48_000, 64).unwrap();
        let control = slot.control();
        control
            .publish_normalized("low_cut_hz", u16::MAX / 2)
            .unwrap();
        let (index, spec) = effect_schema::schema(EffectKind::Eq)
            .iter()
            .enumerate()
            .find(|(_, spec)| spec.name == "low_cut_hz")
            .unwrap();
        let published = f32::from_bits(control.values[index].load(Ordering::Acquire));
        let expected = (spec.minimum * spec.maximum).sqrt();
        assert!((published - expected).abs() < expected * 0.001);
    }
}
