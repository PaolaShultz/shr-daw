//! Stable, named parameter contracts for persisted audio effects.
//!
//! UI input may be clamped before it reaches this module. Persisted values are
//! deliberately stricter: unknown names, non-finite values, out-of-range
//! values, and invalid discrete choices reject the complete graph.

use crate::audio_graph::{EffectInstance, EffectKind};
use std::collections::BTreeMap;
use std::fmt;

pub const COMPRESSOR_GAIN_TABLE_STEPS: usize = 2_048;
pub const MODULATION_TABLE_STEPS: usize = 1_024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParameterType {
    Continuous,
    Integer,
    Toggle,
    Choices(&'static [i16]),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParameterSpec {
    pub name: &'static str,
    pub unit: &'static str,
    pub default: f32,
    pub minimum: f32,
    pub maximum: f32,
    pub value_type: ParameterType,
}

/// One physical knob position in the musician-facing 2×4 effect layout.
/// The persisted schema remains independent so older Projects retain every
/// supported DSP value even when a secondary setup parameter is not assigned
/// to the eight performance knobs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlSpec {
    pub parameter: &'static str,
    pub label: &'static str,
}

impl ControlSpec {
    const fn new(parameter: &'static str, label: &'static str) -> Self {
        Self { parameter, label }
    }
}

impl ParameterSpec {
    pub const fn continuous(
        name: &'static str,
        unit: &'static str,
        default: f32,
        minimum: f32,
        maximum: f32,
    ) -> Self {
        Self {
            name,
            unit,
            default,
            minimum,
            maximum,
            value_type: ParameterType::Continuous,
        }
    }

    pub const fn integer(
        name: &'static str,
        unit: &'static str,
        default: f32,
        minimum: f32,
        maximum: f32,
    ) -> Self {
        Self {
            name,
            unit,
            default,
            minimum,
            maximum,
            value_type: ParameterType::Integer,
        }
    }

    pub const fn toggle(name: &'static str, default: bool) -> Self {
        Self {
            name,
            unit: "on/off",
            default: if default { 1.0 } else { 0.0 },
            minimum: 0.0,
            maximum: 1.0,
            value_type: ParameterType::Toggle,
        }
    }

    pub const fn choices(
        name: &'static str,
        unit: &'static str,
        default: i16,
        choices: &'static [i16],
    ) -> Self {
        Self {
            name,
            unit,
            default: default as f32,
            minimum: 0.0,
            maximum: 0.0,
            value_type: ParameterType::Choices(choices),
        }
    }

    pub fn accepts(self, value: f32) -> bool {
        if !value.is_finite() {
            return false;
        }
        match self.value_type {
            ParameterType::Continuous => (self.minimum..=self.maximum).contains(&value),
            ParameterType::Integer => {
                (self.minimum..=self.maximum).contains(&value) && value.fract() == 0.0
            }
            ParameterType::Toggle => value == 0.0 || value == 1.0,
            ParameterType::Choices(choices) => {
                value.fract() == 0.0 && choices.contains(&(value as i16))
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaError(String);

impl SchemaError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for SchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SchemaError {}

const UTILITY: &[ParameterSpec] = &[
    ParameterSpec::continuous("trim_db", "dB", 0.0, -60.0, 12.0),
    ParameterSpec::continuous("pan", "L/R", 0.0, -1.0, 1.0),
    ParameterSpec::continuous("width_percent", "%", 100.0, 0.0, 200.0),
    ParameterSpec::toggle("invert_left", false),
    ParameterSpec::toggle("invert_right", false),
    ParameterSpec::toggle("mute", false),
];

const EQ: &[ParameterSpec] = &[
    ParameterSpec::toggle("low_cut_enabled", false),
    ParameterSpec::continuous("low_cut_hz", "Hz", 80.0, 20.0, 500.0),
    ParameterSpec::continuous("low_shelf_hz", "Hz", 120.0, 40.0, 800.0),
    ParameterSpec::continuous("low_shelf_db", "dB", 0.0, -18.0, 18.0),
    ParameterSpec::continuous("low_mid_hz", "Hz", 500.0, 80.0, 3_000.0),
    ParameterSpec::continuous("low_mid_db", "dB", 0.0, -18.0, 18.0),
    ParameterSpec::continuous("high_mid_hz", "Hz", 3_000.0, 400.0, 12_000.0),
    ParameterSpec::continuous("high_mid_db", "dB", 0.0, -18.0, 18.0),
    ParameterSpec::continuous("high_shelf_hz", "Hz", 8_000.0, 1_500.0, 20_000.0),
    ParameterSpec::continuous("high_shelf_db", "dB", 0.0, -18.0, 18.0),
    ParameterSpec::continuous("output_trim_db", "dB", 0.0, -18.0, 12.0),
];

const COMPRESSOR: &[ParameterSpec] = &[
    ParameterSpec::continuous("threshold_db", "dBFS", -18.0, -48.0, 0.0),
    ParameterSpec::continuous("ratio", ":1", 4.0, 1.0, 20.0),
    ParameterSpec::continuous("knee_db", "dB", 6.0, 0.0, 12.0),
    ParameterSpec::continuous("attack_ms", "ms", 10.0, 0.1, 100.0),
    ParameterSpec::continuous("release_ms", "ms", 150.0, 20.0, 1_500.0),
    ParameterSpec::continuous("makeup_db", "dB", 0.0, -12.0, 18.0),
    ParameterSpec::continuous("mix_percent", "%", 100.0, 0.0, 100.0),
    ParameterSpec::continuous("sidechain_highpass_hz", "Hz", 20.0, 20.0, 250.0),
];

const DISTORTION: &[ParameterSpec] = &[
    // 0 soft cubic, 1 hard clip, 2 asymmetric diode-like.
    ParameterSpec::integer("mode", "mode", 0.0, 0.0, 2.0),
    ParameterSpec::continuous("drive_db", "dB", 6.0, 0.0, 30.0),
    ParameterSpec::continuous("bias", "", 0.0, -0.5, 0.5),
    ParameterSpec::continuous("tone_hz", "Hz", 12_000.0, 800.0, 18_000.0),
    ParameterSpec::continuous("output_db", "dB", -6.0, -24.0, 0.0),
    ParameterSpec::continuous("mix_percent", "%", 100.0, 0.0, 100.0),
];

const DELAY: &[ParameterSpec] = &[
    ParameterSpec::integer("mode", "mode", 0.0, 0.0, 2.0),
    ParameterSpec::toggle("tempo_sync", false),
    ParameterSpec::continuous("tempo_bpm", "BPM", 120.0, 20.0, 300.0),
    ParameterSpec::integer("division", "division", 4.0, 0.0, 7.0),
    ParameterSpec::continuous("time_ms", "ms", 375.0, 1.0, 2_000.0),
    ParameterSpec::continuous("feedback_percent", "%", 30.0, 0.0, 92.0),
    ParameterSpec::continuous("stereo_ratio", "", 1.0, 0.5, 2.0),
    ParameterSpec::continuous("tone_hz", "Hz", 8_000.0, 500.0, 18_000.0),
    ParameterSpec::continuous("wet_percent", "%", 25.0, 0.0, 100.0),
    ParameterSpec::continuous("dry_percent", "%", 100.0, 0.0, 100.0),
    ParameterSpec::toggle("tail_on_bypass", false),
];

const REVERB: &[ParameterSpec] = &[
    ParameterSpec::integer("type", "type", 0.0, 0.0, 2.0),
    ParameterSpec::continuous("predelay_ms", "ms", 20.0, 0.0, 200.0),
    ParameterSpec::continuous("decay_seconds", "s", 1.5, 0.2, 8.0),
    ParameterSpec::continuous("size_percent", "%", 50.0, 0.0, 100.0),
    ParameterSpec::continuous("damping_percent", "%", 50.0, 0.0, 100.0),
    ParameterSpec::continuous("input_low_cut_hz", "Hz", 80.0, 20.0, 500.0),
    ParameterSpec::continuous("width_percent", "%", 100.0, 0.0, 100.0),
    ParameterSpec::continuous("wet_percent", "%", 25.0, 0.0, 100.0),
    ParameterSpec::continuous("dry_percent", "%", 100.0, 0.0, 100.0),
];

const CHORUS: &[ParameterSpec] = &[
    ParameterSpec::continuous("base_delay_ms", "ms", 15.0, 5.0, 30.0),
    ParameterSpec::continuous("rate_hz", "Hz", 0.5, 0.05, 5.0),
    ParameterSpec::continuous("depth_percent", "%", 35.0, 0.0, 100.0),
    ParameterSpec::continuous("stereo_phase_degrees", "deg", 90.0, 0.0, 180.0),
    ParameterSpec::continuous("feedback_percent", "%", 0.0, 0.0, 35.0),
    ParameterSpec::continuous("mix_percent", "%", 35.0, 0.0, 100.0),
    ParameterSpec::continuous("dry_percent", "%", 100.0, 0.0, 100.0),
];

const FLANGER: &[ParameterSpec] = &[
    ParameterSpec::continuous("base_delay_ms", "ms", 2.0, 0.2, 8.0),
    ParameterSpec::continuous("rate_hz", "Hz", 0.25, 0.03, 5.0),
    ParameterSpec::continuous("depth_percent", "%", 50.0, 0.0, 100.0),
    ParameterSpec::continuous("feedback_percent", "%", 25.0, -80.0, 80.0),
    ParameterSpec::continuous("stereo_phase_degrees", "deg", 90.0, 0.0, 180.0),
    ParameterSpec::continuous("mix_percent", "%", 50.0, 0.0, 100.0),
    ParameterSpec::continuous("dry_percent", "%", 100.0, 0.0, 100.0),
];

const PHASER: &[ParameterSpec] = &[
    ParameterSpec::choices("stages", "stages", 4, &[4, 6]),
    ParameterSpec::continuous("rate_hz", "Hz", 0.25, 0.03, 5.0),
    ParameterSpec::continuous("center_hz", "Hz", 1_000.0, 100.0, 5_000.0),
    ParameterSpec::continuous("range_octaves", "oct", 3.0, 0.5, 6.0),
    ParameterSpec::continuous("feedback_percent", "%", 0.0, -75.0, 75.0),
    ParameterSpec::continuous("stereo_phase_degrees", "deg", 90.0, 0.0, 180.0),
    ParameterSpec::continuous("mix_percent", "%", 50.0, 0.0, 100.0),
    ParameterSpec::continuous("dry_percent", "%", 100.0, 0.0, 100.0),
];

const TREMOLO_PAN: &[ParameterSpec] = &[
    ParameterSpec::integer("mode", "mode", 0.0, 0.0, 1.0),
    ParameterSpec::continuous("rate_hz", "Hz", 4.0, 0.05, 15.0),
    ParameterSpec::continuous("depth_percent", "%", 50.0, 0.0, 100.0),
    ParameterSpec::integer("shape", "shape", 0.0, 0.0, 2.0),
    ParameterSpec::continuous("stereo_phase_degrees", "deg", 180.0, 0.0, 180.0),
    ParameterSpec::continuous("output_trim_db", "dB", 0.0, -18.0, 12.0),
];

const FILTER: &[ParameterSpec] = &[
    // 0 low-pass, 1 band-pass, 2 high-pass.
    ParameterSpec::integer("mode", "mode", 0.0, 0.0, 2.0),
    ParameterSpec::continuous("cutoff_hz", "Hz", 1_000.0, 20.0, 20_000.0),
    ParameterSpec::continuous("resonance", "%", 20.0, 0.0, 90.0),
    ParameterSpec::continuous("drive_db", "dB", 0.0, 0.0, 12.0),
    ParameterSpec::continuous("mix_percent", "%", 100.0, 0.0, 100.0),
];

const GATE: &[ParameterSpec] = &[
    ParameterSpec::continuous("threshold_db", "dBFS", -48.0, -80.0, 0.0),
    ParameterSpec::continuous("hysteresis_db", "dB", 6.0, 0.0, 24.0),
    ParameterSpec::continuous("range_db", "dB", -60.0, -80.0, 0.0),
    ParameterSpec::continuous("attack_ms", "ms", 2.0, 0.1, 100.0),
    ParameterSpec::continuous("hold_ms", "ms", 40.0, 0.0, 500.0),
    ParameterSpec::continuous("release_ms", "ms", 150.0, 5.0, 2_000.0),
];

const CRUSHER: &[ParameterSpec] = &[
    ParameterSpec::integer("bit_depth", "bit", 12.0, 4.0, 16.0),
    ParameterSpec::integer("hold_factor", "x", 1.0, 1.0, 32.0),
    ParameterSpec::toggle("dither", false),
    ParameterSpec::continuous("mix_percent", "%", 100.0, 0.0, 100.0),
];

const UTILITY_CONTROLS: &[ControlSpec] = &[
    ControlSpec::new("trim_db", "TRIM"),
    ControlSpec::new("pan", "PAN"),
    ControlSpec::new("width_percent", "WIDTH"),
    ControlSpec::new("mute", "MUTE"),
    ControlSpec::new("invert_left", "INVERT L"),
    ControlSpec::new("invert_right", "INVERT R"),
];
const EQ_CONTROLS: &[ControlSpec] = &[
    ControlSpec::new("low_shelf_hz", "LOW FREQ"),
    ControlSpec::new("low_mid_hz", "LO MID F"),
    ControlSpec::new("high_mid_hz", "HI MID F"),
    ControlSpec::new("high_shelf_hz", "HIGH FREQ"),
    ControlSpec::new("low_shelf_db", "LOW GAIN"),
    ControlSpec::new("low_mid_db", "LO MID G"),
    ControlSpec::new("high_mid_db", "HI MID G"),
    ControlSpec::new("high_shelf_db", "HIGH GAIN"),
];
const COMPRESSOR_CONTROLS: &[ControlSpec] = &[
    ControlSpec::new("threshold_db", "THRESH"),
    ControlSpec::new("ratio", "RATIO"),
    ControlSpec::new("attack_ms", "ATTACK"),
    ControlSpec::new("release_ms", "RELEASE"),
    ControlSpec::new("knee_db", "KNEE"),
    ControlSpec::new("makeup_db", "MAKEUP"),
    ControlSpec::new("mix_percent", "MIX"),
    ControlSpec::new("sidechain_highpass_hz", "SC HPF"),
];
const DISTORTION_CONTROLS: &[ControlSpec] = &[
    ControlSpec::new("mode", "MODE"),
    ControlSpec::new("drive_db", "DRIVE"),
    ControlSpec::new("bias", "BIAS"),
    ControlSpec::new("tone_hz", "TONE"),
    ControlSpec::new("output_db", "OUTPUT"),
    ControlSpec::new("mix_percent", "MIX"),
];
const DELAY_CONTROLS: &[ControlSpec] = &[
    ControlSpec::new("time_ms", "TIME"),
    ControlSpec::new("feedback_percent", "FEEDBACK"),
    ControlSpec::new("tone_hz", "TONE"),
    ControlSpec::new("stereo_ratio", "STEREO"),
    ControlSpec::new("tempo_sync", "SYNC"),
    ControlSpec::new("division", "DIVISION"),
    ControlSpec::new("wet_percent", "WET"),
    ControlSpec::new("dry_percent", "DRY"),
];
const REVERB_CONTROLS: &[ControlSpec] = &[
    ControlSpec::new("type", "TYPE"),
    ControlSpec::new("predelay_ms", "PREDELAY"),
    ControlSpec::new("decay_seconds", "DECAY"),
    ControlSpec::new("size_percent", "SIZE"),
    ControlSpec::new("damping_percent", "DAMPING"),
    ControlSpec::new("input_low_cut_hz", "LOW CUT"),
    ControlSpec::new("wet_percent", "WET"),
    ControlSpec::new("dry_percent", "DRY"),
];
const CHORUS_CONTROLS: &[ControlSpec] = &[
    ControlSpec::new("base_delay_ms", "TIME"),
    ControlSpec::new("rate_hz", "RATE"),
    ControlSpec::new("depth_percent", "DEPTH"),
    ControlSpec::new("stereo_phase_degrees", "PHASE"),
    ControlSpec::new("feedback_percent", "FEEDBACK"),
    ControlSpec::new("mix_percent", "MIX"),
    ControlSpec::new("dry_percent", "DRY"),
];
const FLANGER_CONTROLS: &[ControlSpec] = CHORUS_CONTROLS;
const PHASER_CONTROLS: &[ControlSpec] = &[
    ControlSpec::new("rate_hz", "RATE"),
    ControlSpec::new("center_hz", "CENTER"),
    ControlSpec::new("range_octaves", "RANGE"),
    ControlSpec::new("feedback_percent", "FEEDBACK"),
    ControlSpec::new("stages", "STAGES"),
    ControlSpec::new("stereo_phase_degrees", "PHASE"),
    ControlSpec::new("mix_percent", "MIX"),
    ControlSpec::new("dry_percent", "DRY"),
];
const TREMOLO_PAN_CONTROLS: &[ControlSpec] = &[
    ControlSpec::new("rate_hz", "RATE"),
    ControlSpec::new("depth_percent", "DEPTH"),
    ControlSpec::new("shape", "SHAPE"),
    ControlSpec::new("stereo_phase_degrees", "PHASE"),
    ControlSpec::new("mode", "MODE"),
    ControlSpec::new("output_trim_db", "OUTPUT"),
];
const FILTER_CONTROLS: &[ControlSpec] = &[
    ControlSpec::new("cutoff_hz", "CUTOFF"),
    ControlSpec::new("resonance", "RESONANCE"),
    ControlSpec::new("drive_db", "DRIVE"),
    ControlSpec::new("mix_percent", "MIX"),
    ControlSpec::new("mode", "MODE"),
];
const GATE_CONTROLS: &[ControlSpec] = &[
    ControlSpec::new("threshold_db", "THRESH"),
    ControlSpec::new("hysteresis_db", "HYST"),
    ControlSpec::new("range_db", "RANGE"),
    ControlSpec::new("attack_ms", "ATTACK"),
    ControlSpec::new("hold_ms", "HOLD"),
    ControlSpec::new("release_ms", "RELEASE"),
];
const CRUSHER_CONTROLS: &[ControlSpec] = &[
    ControlSpec::new("bit_depth", "BITS"),
    ControlSpec::new("hold_factor", "HOLD"),
    ControlSpec::new("dither", "DITHER"),
    ControlSpec::new("mix_percent", "MIX"),
];

pub const fn schema(kind: EffectKind) -> &'static [ParameterSpec] {
    match kind {
        EffectKind::Utility => UTILITY,
        EffectKind::Eq => EQ,
        EffectKind::Compressor => COMPRESSOR,
        EffectKind::Distortion => DISTORTION,
        EffectKind::Delay => DELAY,
        EffectKind::Reverb => REVERB,
        EffectKind::Chorus => CHORUS,
        EffectKind::Flanger => FLANGER,
        EffectKind::Phaser => PHASER,
        EffectKind::TremoloPan => TREMOLO_PAN,
        EffectKind::Filter => FILTER,
        EffectKind::Gate => GATE,
        EffectKind::Crusher => CRUSHER,
    }
}

pub const fn controls(kind: EffectKind) -> &'static [ControlSpec] {
    match kind {
        EffectKind::Utility => UTILITY_CONTROLS,
        EffectKind::Eq => EQ_CONTROLS,
        EffectKind::Compressor => COMPRESSOR_CONTROLS,
        EffectKind::Distortion => DISTORTION_CONTROLS,
        EffectKind::Delay => DELAY_CONTROLS,
        EffectKind::Reverb => REVERB_CONTROLS,
        EffectKind::Chorus => CHORUS_CONTROLS,
        EffectKind::Flanger => FLANGER_CONTROLS,
        EffectKind::Phaser => PHASER_CONTROLS,
        EffectKind::TremoloPan => TREMOLO_PAN_CONTROLS,
        EffectKind::Filter => FILTER_CONTROLS,
        EffectKind::Gate => GATE_CONTROLS,
        EffectKind::Crusher => CRUSHER_CONTROLS,
    }
}

pub fn controlled_parameter(kind: EffectKind, index: usize) -> Option<ParameterSpec> {
    let control = controls(kind).get(index)?;
    schema(kind)
        .iter()
        .find(|parameter| parameter.name == control.parameter)
        .copied()
}

pub fn defaults(kind: EffectKind) -> BTreeMap<String, f32> {
    schema(kind)
        .iter()
        .map(|spec| (spec.name.to_owned(), spec.default))
        .collect()
}

/// Deliberate musician-facing abbreviations. Persistence continues to use the
/// full schema `name`; these labels are stable UI metadata, never heuristics.
pub fn abbreviation(name: &str) -> &'static str {
    match name {
        "trim_db" => "TRIM",
        "pan" => "PAN",
        "width_percent" => "WIDTH",
        "invert_left" => "INV-L",
        "invert_right" => "INV-R",
        "mute" => "MUTE",
        "low_cut_enabled" => "L-CUT",
        "low_cut_hz" => "LC-HZ",
        "low_shelf_hz" => "LS-HZ",
        "low_shelf_db" => "LS-DB",
        "low_mid_hz" => "LM-HZ",
        "low_mid_db" => "LM-DB",
        "high_mid_hz" => "HM-HZ",
        "high_mid_db" => "HM-DB",
        "high_shelf_hz" => "HS-HZ",
        "high_shelf_db" => "HS-DB",
        "output_trim_db" | "output_db" => "OUT",
        "threshold_db" => "THR",
        "ratio" => "RAT",
        "knee_db" => "KNEE",
        "attack_ms" => "ATK",
        "release_ms" => "REL",
        "makeup_db" => "MAKE",
        "mix_percent" => "MIX",
        "sidechain_highpass_hz" => "SC-HP",
        "mode" => "MODE",
        "drive_db" => "DRIVE",
        "bias" => "BIAS",
        "tone_hz" => "TONE",
        "tempo_sync" => "SYNC",
        "tempo_bpm" => "BPM",
        "division" => "DIV",
        "time_ms" | "base_delay_ms" => "TIME",
        "feedback_percent" => "FDBK",
        "stereo_ratio" => "STEREO",
        "wet_percent" => "WET",
        "dry_percent" => "DRY",
        "tail_on_bypass" => "TAIL",
        "type" => "TYPE",
        "predelay_ms" => "PRE",
        "decay_seconds" => "DECAY",
        "size_percent" => "SIZE",
        "damping_percent" => "DAMP",
        "input_low_cut_hz" => "L-CUT",
        "rate_hz" => "RATE",
        "depth_percent" => "DEPTH",
        "stereo_phase_degrees" => "PHASE",
        "stages" => "STAGES",
        "center_hz" => "CENTER",
        "range_octaves" => "RANGE",
        "shape" => "SHAPE",
        "cutoff_hz" => "CUTOFF",
        "resonance" => "RES",
        "hysteresis_db" => "HYST",
        "range_db" => "RANGE",
        "hold_ms" => "HOLD",
        "bit_depth" => "BITS",
        "hold_factor" => "HOLD",
        "dither" => "DITH",
        _ => "?",
    }
}

pub fn format_value(kind: EffectKind, spec: ParameterSpec, value: f32) -> String {
    if spec.value_type == ParameterType::Toggle {
        return if value >= 0.5 { "ON" } else { "OFF" }.into();
    }
    let indexed = value.round() as usize;
    let named = match (kind, spec.name, indexed) {
        (EffectKind::Distortion, "mode", 0) => Some("SOFT"),
        (EffectKind::Distortion, "mode", 1) => Some("HARD"),
        (EffectKind::Distortion, "mode", 2) => Some("DIODE"),
        (EffectKind::Delay, "mode", 0) => Some("STEREO"),
        (EffectKind::Delay, "mode", 1) => Some("PING"),
        (EffectKind::Delay, "mode", 2) => Some("MONO-ST"),
        (EffectKind::Delay, "division", 0) => Some("1/16"),
        (EffectKind::Delay, "division", 1) => Some("1/8"),
        (EffectKind::Delay, "division", 2) => Some("1/4"),
        (EffectKind::Delay, "division", 3) => Some("1/2"),
        (EffectKind::Delay, "division", 4) => Some("1/1"),
        (EffectKind::Delay, "division", 5) => Some("2/1"),
        (EffectKind::Delay, "division", 6) => Some("4/1"),
        (EffectKind::Delay, "division", 7) => Some("8/1"),
        (EffectKind::Reverb, "type", 0) => Some("SHORT"),
        (EffectKind::Reverb, "type", 1) => Some("MED"),
        (EffectKind::Reverb, "type", 2) => Some("LONG"),
        (EffectKind::TremoloPan, "mode", 0) => Some("TREM"),
        (EffectKind::TremoloPan, "mode", 1) => Some("PAN"),
        (EffectKind::TremoloPan, "shape", 0) => Some("SINE"),
        (EffectKind::TremoloPan, "shape", 1) => Some("TRI"),
        (EffectKind::TremoloPan, "shape", 2) => Some("SQUARE"),
        (EffectKind::Filter, "mode", 0) => Some("LP"),
        (EffectKind::Filter, "mode", 1) => Some("BP"),
        (EffectKind::Filter, "mode", 2) => Some("HP"),
        _ => None,
    };
    if let Some(label) = named {
        return label.into();
    }
    match spec.unit {
        "dB" | "dBFS" => format!("{value:+.1} {}", spec.unit),
        ":1" => format!("{value:.1}:1"),
        "%" => format!("{value:.0}%"),
        "Hz" if value >= 1_000.0 => format!("{:.1} kHz", value / 1_000.0),
        "Hz" if value >= 10.0 => format!("{value:.0} Hz"),
        "Hz" => format!("{value:.2} Hz"),
        "ms" if value >= 10.0 => format!("{value:.0} ms"),
        "ms" => format!("{value:.1} ms"),
        "s" => format!("{value:.1} s"),
        "deg" => format!("{value:.0}°"),
        "oct" => format!("{value:.1} oct"),
        "BPM" => format!("{value:.0} BPM"),
        "stages" => format!("{value:.0} STG"),
        "bit" => format!("{value:.0} bit"),
        "x" => format!("{value:.0}x"),
        "" if spec.value_type == ParameterType::Integer => format!("{value:.0}"),
        "" => format!("{value:.2}"),
        unit if spec.value_type == ParameterType::Integer => format!("{value:.0} {unit}"),
        unit => format!("{value:.2} {unit}"),
    }
}

/// Minimum heap storage allocated by one Phase 2 runtime slot. This is derived
/// from kind and callback capacity; persisted memory claims cannot reduce it.
pub fn minimum_runtime_memory_bytes(
    kind: EffectKind,
    sample_rate: u32,
    maximum_frames: usize,
) -> usize {
    let metering = 2usize
        .saturating_mul(maximum_frames)
        .saturating_mul(std::mem::size_of::<crate::dsp::StereoFrame>());
    let processor = match kind {
        EffectKind::Compressor => {
            (COMPRESSOR_GAIN_TABLE_STEPS + 1).saturating_mul(std::mem::size_of::<f32>())
        }
        EffectKind::Delay => 2usize
            .saturating_mul((sample_rate as usize).saturating_mul(2).saturating_add(2))
            .saturating_mul(std::mem::size_of::<f32>()),
        EffectKind::Chorus => 2usize
            .saturating_mul(((sample_rate as usize).saturating_mul(45) / 1_000).saturating_add(3))
            .saturating_mul(std::mem::size_of::<f32>()),
        EffectKind::Flanger => 2usize
            .saturating_mul(((sample_rate as usize).saturating_mul(16) / 1_000).saturating_add(3))
            .saturating_mul(std::mem::size_of::<f32>()),
        EffectKind::Phaser => {
            (MODULATION_TABLE_STEPS + 1).saturating_mul(std::mem::size_of::<f32>())
        }
        EffectKind::TremoloPan => (MODULATION_TABLE_STEPS + 1)
            .saturating_mul(std::mem::size_of::<crate::dsp::StereoFrame>()),
        EffectKind::Reverb => {
            let predelay = 2usize.saturating_mul(
                ((sample_rate as usize).saturating_mul(200) / 1_000).saturating_add(3),
            );
            let fdn = 4usize.saturating_mul(
                ((sample_rate as usize).saturating_mul(100) / 1_000).saturating_add(3),
            );
            // Four bounded input diffusers use less than 8 ms each.
            let diffusion = 4usize.saturating_mul(
                ((sample_rate as usize).saturating_mul(8) / 1_000).saturating_add(3),
            );
            predelay
                .saturating_add(fdn)
                .saturating_add(diffusion)
                .saturating_mul(std::mem::size_of::<f32>())
        }
        _ => 0,
    };
    if crate::audio_graph::is_insert_effect(kind) {
        metering.saturating_add(processor)
    } else {
        0
    }
}

pub fn parameter(effect: &EffectInstance, name: &str) -> Result<f32, SchemaError> {
    let spec = schema(effect.kind)
        .iter()
        .find(|spec| spec.name == name)
        .ok_or_else(|| SchemaError::new(format!("unknown {:?} parameter {name}", effect.kind)))?;
    let value = effect.parameters.get(name).copied().unwrap_or(spec.default);
    if !spec.accepts(value) {
        return Err(SchemaError::new(format!(
            "invalid {:?} parameter {name}",
            effect.kind
        )));
    }
    Ok(value)
}

pub fn validate(effect: &EffectInstance) -> Result<(), SchemaError> {
    let specs = schema(effect.kind);
    for name in effect.parameters.keys() {
        if !specs.iter().any(|spec| spec.name == name) {
            return Err(SchemaError::new(format!(
                "unknown {:?} parameter {name}",
                effect.kind
            )));
        }
    }
    for spec in specs {
        parameter(effect, spec.name)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_graph::EFFECT_FORMAT_VERSION;
    use std::collections::BTreeSet;

    fn instance(kind: EffectKind) -> EffectInstance {
        EffectInstance {
            id: 1,
            kind,
            version: EFFECT_FORMAT_VERSION,
            bypass: false,
            parameters: BTreeMap::new(),
            owned_memory_bytes: 0,
        }
    }

    #[test]
    fn every_kind_has_unique_valid_named_defaults() {
        for kind in [
            EffectKind::Utility,
            EffectKind::Eq,
            EffectKind::Compressor,
            EffectKind::Distortion,
            EffectKind::Delay,
            EffectKind::Reverb,
            EffectKind::Chorus,
            EffectKind::Flanger,
            EffectKind::Phaser,
            EffectKind::TremoloPan,
            EffectKind::Filter,
            EffectKind::Gate,
            EffectKind::Crusher,
        ] {
            let effect = instance(kind);
            validate(&effect).unwrap();
            let defaults = defaults(kind);
            assert_eq!(defaults.len(), schema(kind).len());
            assert!(schema(kind).iter().all(|spec| {
                !spec.name.is_empty()
                    && abbreviation(spec.name) != "?"
                    && spec.accepts(spec.default)
            }));
            let abbreviations = schema(kind)
                .iter()
                .map(|spec| abbreviation(spec.name))
                .collect::<BTreeSet<_>>();
            assert_eq!(abbreviations.len(), schema(kind).len(), "{kind:?}");
            assert!(abbreviations.iter().all(|label| label.chars().count() <= 6));
        }
    }

    #[test]
    fn compact_values_are_type_aware() {
        let compressor = schema(EffectKind::Compressor);
        assert_eq!(
            format_value(EffectKind::Compressor, compressor[0], -18.0),
            "-18.0 dBFS"
        );
        assert_eq!(
            format_value(EffectKind::Compressor, compressor[1], 4.0),
            "4.0:1"
        );
        assert_eq!(
            format_value(EffectKind::Compressor, compressor[6], 75.0),
            "75%"
        );
        let delay = schema(EffectKind::Delay);
        assert_eq!(format_value(EffectKind::Delay, delay[1], 1.0), "ON");
        assert_eq!(format_value(EffectKind::Delay, delay[3], 4.0), "1/1");
        assert_eq!(format_value(EffectKind::Delay, delay[4], 375.0), "375 ms");
        assert_eq!(
            format_value(EffectKind::Delay, delay[7], 8_000.0),
            "8.0 kHz"
        );
        let crusher = schema(EffectKind::Crusher);
        assert_eq!(
            format_value(EffectKind::Crusher, crusher[0], 12.0),
            "12 bit"
        );
    }

    #[test]
    fn every_effect_has_a_valid_two_by_four_performance_layout() {
        for kind in [
            EffectKind::Utility,
            EffectKind::Eq,
            EffectKind::Compressor,
            EffectKind::Distortion,
            EffectKind::Delay,
            EffectKind::Reverb,
            EffectKind::Chorus,
            EffectKind::Flanger,
            EffectKind::Phaser,
            EffectKind::TremoloPan,
            EffectKind::Filter,
            EffectKind::Gate,
            EffectKind::Crusher,
        ] {
            let layout = controls(kind);
            assert!(!layout.is_empty(), "{kind:?}");
            assert!(layout.len() <= 8, "{kind:?} has too many controls");
            let names = layout
                .iter()
                .map(|control| control.parameter)
                .collect::<BTreeSet<_>>();
            assert_eq!(names.len(), layout.len(), "{kind:?}");
            assert!(layout.iter().enumerate().all(|(index, control)| {
                control.label.chars().count() <= 9
                    && controlled_parameter(kind, index)
                        .is_some_and(|parameter| parameter.name == control.parameter)
            }));
        }

        assert_eq!(
            controls(EffectKind::Eq)
                .iter()
                .map(|control| control.parameter)
                .collect::<Vec<_>>(),
            [
                "low_shelf_hz",
                "low_mid_hz",
                "high_mid_hz",
                "high_shelf_hz",
                "low_shelf_db",
                "low_mid_db",
                "high_mid_db",
                "high_shelf_db",
            ]
        );
    }

    #[test]
    fn persisted_values_are_strict_but_missing_values_use_defaults() {
        let mut effect = instance(EffectKind::Crusher);
        assert_eq!(parameter(&effect, "bit_depth").unwrap(), 12.0);
        effect.parameters.insert("bit_depth".into(), 8.5);
        assert!(validate(&effect)
            .unwrap_err()
            .to_string()
            .contains("bit_depth"));
        effect.parameters.insert("bit_depth".into(), 8.0);
        effect.parameters.insert("dither".into(), 0.5);
        assert!(validate(&effect)
            .unwrap_err()
            .to_string()
            .contains("dither"));
        effect.parameters.insert("dither".into(), 1.0);
        effect.parameters.insert("future_control".into(), 0.0);
        assert!(validate(&effect)
            .unwrap_err()
            .to_string()
            .contains("future_control"));
    }

    #[test]
    fn non_finite_and_out_of_range_values_are_rejected() {
        let mut effect = instance(EffectKind::Compressor);
        for bad in [f32::NAN, f32::INFINITY, -49.0, 1.0] {
            effect.parameters.insert("threshold_db".into(), bad);
            assert!(validate(&effect).is_err(), "accepted {bad}");
        }
        effect.parameters.insert("threshold_db".into(), -24.0);
        validate(&effect).unwrap();

        let mut phaser = instance(EffectKind::Phaser);
        phaser.parameters.insert("stages".into(), 5.0);
        assert!(validate(&phaser).is_err());
        phaser.parameters.insert("stages".into(), 6.0);
        validate(&phaser).unwrap();
    }

    #[test]
    fn runtime_memory_is_derived_even_when_persisted_claim_is_zero() {
        let frames = 128;
        let meters = 2 * frames * std::mem::size_of::<crate::dsp::StereoFrame>();
        assert_eq!(
            minimum_runtime_memory_bytes(EffectKind::Eq, 48_000, frames),
            meters
        );
        assert_eq!(
            minimum_runtime_memory_bytes(EffectKind::Compressor, 48_000, frames),
            meters + (COMPRESSOR_GAIN_TABLE_STEPS + 1) * std::mem::size_of::<f32>()
        );
        assert_eq!(
            minimum_runtime_memory_bytes(EffectKind::Delay, 48_000, frames),
            meters + 2 * (96_000 + 2) * std::mem::size_of::<f32>()
        );
    }
}
