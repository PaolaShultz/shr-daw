//! Backend-specific mapped control profiles. Existing Moj Sint models share
//! a 4×4 surface, but never synthv1 parameter indices or XML semantics.
//! Full model tables also retain preset-owned controls outside the surface.

use std::collections::HashMap;

/// Four equal parameter columns occupy the native 40-cell display. Static
/// labels must fit whole; preset/schema IDs are not display labels.
pub const SYNTH_PARAMETER_LABEL_CELLS: usize = 9;

pub const VOLUME_CC: u8 = 93;
/// Standard MIDI channel-volume controller used by managed instruments whose
/// native parameter map is not synthv1's DCA map.
pub const INSTRUMENT_VOLUME_CC: u8 = 7;
pub const LEGACY_SYNTH_CONTROL_COUNT: usize = 12;
/// Physical slot 13 is volume; the last three slots are AUX. The master
/// encoder supplies slot 1, leaving fifteen learned rotaries.
pub const SYNTH_SURFACE_CONTROL_COUNT: usize = 13;
pub const SYNTH_VOLUME_SLOT: usize = 12;
pub const AUX_SEND_CONTROL_COUNT: usize = 3;
pub const PERFORMANCE_SURFACE_CONTROL_COUNT: usize =
    LEGACY_SYNTH_CONTROL_COUNT + AUX_SEND_CONTROL_COUNT;
/// The learned performance surface has exactly fifteen rotaries after the
/// master encoder, which edits the first synth parameter on synth screens.
pub const MAPPED_CONTROL_CAPACITY: usize = PERFORMANCE_SURFACE_CONTROL_COUNT;
pub const MOJ_CORE_TOGGLE_CC: u8 = 35;
pub const MOJ_CORE_STATE_CC: u8 = 36;

/// synthv1 0.9.29 indices/ranges, verified against src/synthv1_param.cpp.
#[derive(Clone, Copy, Debug)]
pub struct Control {
    pub cc: u8,
    pub index: u16,
    pub name: &'static str,
    pub xml_name: &'static str,
    pub min: f32,
    pub max: f32,
}

pub const CONTROLS: [Control; LEGACY_SYNTH_CONTROL_COUNT] = [
    Control {
        cc: 74,
        index: 17,
        name: "Flt cut",
        xml_name: "DCF1_CUTOFF",
        min: 0.0,
        max: 1.0,
    },
    Control {
        cc: 71,
        index: 18,
        name: "Flt res",
        xml_name: "DCF1_RESO",
        min: 0.0,
        max: 1.0,
    },
    Control {
        cc: 76,
        index: 21,
        name: "Flt env",
        xml_name: "DCF1_ENVELOPE",
        min: -1.0,
        max: 1.0,
    },
    Control {
        cc: 77,
        index: 30,
        name: "LFO rate",
        xml_name: "LFO1_RATE",
        min: 0.0,
        max: 1.0,
    },
    Control {
        cc: VOLUME_CC,
        index: 44,
        name: "Volume",
        xml_name: "DCA1_VOLUME",
        min: 0.0,
        max: 1.0,
    },
    Control {
        cc: 18,
        index: 132,
        name: "Dly amt",
        xml_name: "DEL1_WET",
        min: 0.0,
        max: 1.0,
    },
    Control {
        cc: 19,
        index: 133,
        name: "Dly time",
        xml_name: "DEL1_DELAY",
        min: 0.0,
        max: 1.0,
    },
    Control {
        cc: 16,
        index: 134,
        name: "Dly fb",
        xml_name: "DEL1_FEEDB",
        min: 0.0,
        max: 1.0,
    },
    Control {
        cc: 82,
        index: 45,
        name: "Atk",
        xml_name: "DCA1_ATTACK",
        min: 0.0,
        max: 1.0,
    },
    Control {
        cc: 83,
        index: 46,
        name: "Dec",
        xml_name: "DCA1_DECAY",
        min: 0.0,
        max: 1.0,
    },
    Control {
        cc: 85,
        index: 47,
        name: "Sus",
        xml_name: "DCA1_SUSTAIN",
        min: 0.0,
        max: 1.0,
    },
    Control {
        cc: 17,
        index: 48,
        name: "Rel",
        xml_name: "DCA1_RELEASE",
        min: 0.0,
        max: 1.0,
    },
];

#[derive(Clone, Copy, Debug)]
pub struct MojControl {
    pub cc: u8,
    pub name: &'static str,
    pub macro_id: &'static str,
}

pub const MOJ_MODEL_D_CONTROLS: [MojControl; 13] = [
    MojControl {
        cc: 20,
        name: "Character",
        macro_id: "evolve",
    },
    MojControl {
        cc: 21,
        name: "Osc Mix",
        macro_id: "shape",
    },
    MojControl {
        cc: 22,
        name: "Cutoff",
        macro_id: "color",
    },
    MojControl {
        cc: 23,
        name: "Drive",
        macro_id: "edge",
    },
    MojControl {
        cc: 7,
        name: "Volume",
        macro_id: "instrument_volume",
    },
    MojControl {
        cc: 25,
        name: "F Env",
        macro_id: "motion",
    },
    MojControl {
        cc: 26,
        name: "Ladder",
        macro_id: "depth",
    },
    MojControl {
        cc: 27,
        name: "Resonance",
        macro_id: "space",
    },
    MojControl {
        cc: 28,
        name: "Attack",
        macro_id: "attack",
    },
    MojControl {
        cc: 29,
        name: "Decay",
        macro_id: "decay",
    },
    MojControl {
        cc: 30,
        name: "Sustain",
        macro_id: "sustain",
    },
    MojControl {
        cc: 31,
        name: "Release",
        macro_id: "release",
    },
    MojControl {
        cc: 24,
        name: "Couple",
        macro_id: "couple",
    },
];

pub const MOJ_SIX_OP_PM_CONTROLS: [MojControl; 13] = [
    MojControl {
        cc: 20,
        name: "Index",
        macro_id: "index",
    },
    MojControl {
        cc: 21,
        name: "Ratio",
        macro_id: "ratio",
    },
    MojControl {
        cc: 22,
        name: "Feedback",
        macro_id: "feedback",
    },
    MojControl {
        cc: 23,
        name: "Op Decay",
        macro_id: "operator_decay",
    },
    MojControl {
        cc: 7,
        name: "Volume",
        macro_id: "instrument_volume",
    },
    MojControl {
        cc: 25,
        name: "KeyScale",
        macro_id: "key_scale",
    },
    MojControl {
        cc: 26,
        name: "Velocity",
        macro_id: "velocity",
    },
    MojControl {
        cc: 27,
        name: "Motion",
        macro_id: "motion",
    },
    MojControl {
        cc: 28,
        name: "Attack",
        macro_id: "attack",
    },
    MojControl {
        cc: 29,
        name: "Decay",
        macro_id: "decay",
    },
    MojControl {
        cc: 30,
        name: "Sustain",
        macro_id: "sustain",
    },
    MojControl {
        cc: 31,
        name: "Release",
        macro_id: "release",
    },
    MojControl {
        cc: 24,
        name: "Balance",
        macro_id: "balance",
    },
];

pub const MOJ_STRANGE_CONTROLS: [MojControl; 13] = [
    MojControl {
        cc: 20,
        name: "Type",
        macro_id: "type",
    },
    MojControl {
        cc: 21,
        name: "Form",
        macro_id: "form",
    },
    MojControl {
        cc: 22,
        name: "Warp",
        macro_id: "warp",
    },
    MojControl {
        cc: 23,
        name: "Couple",
        macro_id: "couple",
    },
    MojControl {
        cc: 7,
        name: "Volume",
        macro_id: "instrument_volume",
    },
    MojControl {
        cc: 25,
        name: "Chaos",
        macro_id: "chaos",
    },
    MojControl {
        cc: 26,
        name: "Color",
        macro_id: "color",
    },
    MojControl {
        cc: 27,
        name: "Space",
        macro_id: "space",
    },
    MojControl {
        cc: 28,
        name: "Attack",
        macro_id: "attack",
    },
    MojControl {
        cc: 29,
        name: "Decay",
        macro_id: "decay",
    },
    MojControl {
        cc: 30,
        name: "Sustain",
        macro_id: "sustain",
    },
    MojControl {
        cc: 31,
        name: "Release",
        macro_id: "release",
    },
    MojControl {
        cc: 24,
        name: "Motion",
        macro_id: "motion",
    },
];

pub const MOJ_SWARM_CONTROLS: [MojControl; 13] = [
    MojControl {
        cc: 20,
        name: "Mass",
        macro_id: "mass",
    },
    MojControl {
        cc: 21,
        name: "Detune",
        macro_id: "detune",
    },
    MojControl {
        cc: 22,
        name: "Spread",
        macro_id: "spread",
    },
    MojControl {
        cc: 23,
        name: "Shape",
        macro_id: "shape",
    },
    MojControl {
        cc: 7,
        name: "Volume",
        macro_id: "instrument_volume",
    },
    MojControl {
        cc: 25,
        name: "Motion",
        macro_id: "motion",
    },
    MojControl {
        cc: 26,
        name: "Color",
        macro_id: "color",
    },
    MojControl {
        cc: 27,
        name: "Space",
        macro_id: "space",
    },
    MojControl {
        cc: 28,
        name: "Attack",
        macro_id: "attack",
    },
    MojControl {
        cc: 29,
        name: "Decay",
        macro_id: "decay",
    },
    MojControl {
        cc: 30,
        name: "Sustain",
        macro_id: "sustain",
    },
    MojControl {
        cc: 31,
        name: "Release",
        macro_id: "release",
    },
    MojControl {
        cc: 24,
        name: "Bite",
        macro_id: "bite",
    },
];

pub const MOJ_BASS_MATRIX_CONTROLS: [MojControl; 12] = [
    MojControl {
        cc: 20,
        name: "Body",
        macro_id: "body",
    },
    MojControl {
        cc: 21,
        name: "Growl",
        macro_id: "growl",
    },
    MojControl {
        cc: 22,
        name: "Metal",
        macro_id: "metal",
    },
    MojControl {
        cc: 23,
        name: "Punch",
        macro_id: "punch",
    },
    MojControl {
        cc: 7,
        name: "Volume",
        macro_id: "instrument_volume",
    },
    MojControl {
        cc: 25,
        name: "Drive",
        macro_id: "drive",
    },
    MojControl {
        cc: 26,
        name: "Filter",
        macro_id: "filter",
    },
    MojControl {
        cc: 27,
        name: "Unstable",
        macro_id: "unstable",
    },
    MojControl {
        cc: 28,
        name: "Attack",
        macro_id: "attack",
    },
    MojControl {
        cc: 29,
        name: "Decay",
        macro_id: "decay",
    },
    MojControl {
        cc: 30,
        name: "Sustain",
        macro_id: "sustain",
    },
    MojControl {
        cc: 31,
        name: "Release",
        macro_id: "release",
    },
];

pub const MOJ_PRESSURE_CONTROLS: [MojControl; 13] = [
    MojControl {
        cc: 20,
        name: "Source",
        macro_id: "source",
    },
    MojControl {
        cc: 21,
        name: "Shape",
        macro_id: "shape",
    },
    MojControl {
        cc: 22,
        name: "Cutoff",
        macro_id: "cutoff",
    },
    MojControl {
        cc: 23,
        name: "Res",
        macro_id: "resonance",
    },
    MojControl {
        cc: 24,
        name: "Sweep",
        macro_id: "sweep",
    },
    MojControl {
        cc: 25,
        name: "F Decay",
        macro_id: "filter_decay",
    },
    MojControl {
        cc: 26,
        name: "Pressure",
        macro_id: "pressure",
    },
    MojControl {
        cc: 27,
        name: "Bite",
        macro_id: "bite",
    },
    MojControl {
        cc: 28,
        name: "Attack",
        macro_id: "attack",
    },
    MojControl {
        cc: 29,
        name: "Decay",
        macro_id: "decay",
    },
    MojControl {
        cc: 30,
        name: "Sustain",
        macro_id: "sustain",
    },
    MojControl {
        cc: 31,
        name: "Release",
        macro_id: "release",
    },
    MojControl {
        cc: 7,
        name: "Volume",
        macro_id: "instrument_volume",
    },
];

pub const MOJ_DUAL_FILTER_CONTROLS: [MojControl; 16] = [
    MojControl {
        cc: 20,
        name: "A Cutoff",
        macro_id: "filter_a_cutoff",
    },
    MojControl {
        cc: 21,
        name: "A Res",
        macro_id: "filter_a_resonance",
    },
    MojControl {
        cc: 22,
        name: "A Env",
        macro_id: "filter_a_envelope_depth",
    },
    MojControl {
        cc: 23,
        name: "B Cutoff",
        macro_id: "filter_b_cutoff",
    },
    MojControl {
        cc: 24,
        name: "B Res",
        macro_id: "filter_b_resonance",
    },
    MojControl {
        cc: 25,
        name: "B Env",
        macro_id: "filter_b_envelope_depth",
    },
    MojControl {
        cc: 26,
        name: "Struct",
        macro_id: "structure",
    },
    MojControl {
        cc: 27,
        name: "F Attack",
        macro_id: "filter_attack",
    },
    MojControl {
        cc: 28,
        name: "F Decay",
        macro_id: "filter_decay",
    },
    MojControl {
        cc: 29,
        name: "F Sus",
        macro_id: "filter_sustain",
    },
    MojControl {
        cc: 30,
        name: "F Rel",
        macro_id: "filter_release",
    },
    MojControl {
        cc: 31,
        name: "A Attack",
        macro_id: "amp_attack",
    },
    MojControl {
        cc: 32,
        name: "A Decay",
        macro_id: "amp_decay",
    },
    MojControl {
        cc: 33,
        name: "A Sus",
        macro_id: "amp_sustain",
    },
    MojControl {
        cc: 34,
        name: "A Rel",
        macro_id: "amp_release",
    },
    MojControl {
        cc: 7,
        name: "Volume",
        macro_id: "instrument_volume",
    },
];

// The first five Moj Sint models share twelve physical positions. Position
// five is their universal instrument-volume CC 7. Dual Filter supplies its own
// 15-control state table; its MAIN/AMP pages share twelve physical positions.
// Meanings come from the selected synthesis model, not controller.conf.
pub const MOJ_CONTROLS: [MojControl; 13] = MOJ_MODEL_D_CONTROLS;

pub const MOJ_OPEN303_CONTROLS: [MojControl; 12] = [
    MojControl {
        cc: 20,
        name: "Wave",
        macro_id: "waveform",
    },
    MojControl {
        cc: 21,
        name: "Cutoff",
        macro_id: "cutoff",
    },
    MojControl {
        cc: 22,
        name: "Res",
        macro_id: "resonance",
    },
    MojControl {
        cc: 23,
        name: "Env Mod",
        macro_id: "env_mod",
    },
    MojControl {
        cc: 7,
        name: "Volume",
        macro_id: "instrument_volume",
    },
    MojControl {
        cc: 25,
        name: "F Decay",
        macro_id: "filter_decay",
    },
    MojControl {
        cc: 26,
        name: "Accent",
        macro_id: "accent",
    },
    MojControl {
        cc: 27,
        name: "Slide",
        macro_id: "slide",
    },
    MojControl {
        cc: 28,
        name: "F Attack",
        macro_id: "normal_attack",
    },
    MojControl {
        cc: 29,
        name: "Ac Atk",
        macro_id: "accent_attack",
    },
    MojControl {
        cc: 30,
        name: "Ac Decay",
        macro_id: "accent_decay",
    },
    MojControl {
        cc: 31,
        name: "Amp Dec",
        macro_id: "amp_decay",
    },
];

pub const fn moj_controls(model: crate::preset::MojModel) -> &'static [MojControl] {
    match model {
        crate::preset::MojModel::ModelD => &MOJ_MODEL_D_CONTROLS,
        crate::preset::MojModel::SixOpPm => &MOJ_SIX_OP_PM_CONTROLS,
        crate::preset::MojModel::StrangeOscillator => &MOJ_STRANGE_CONTROLS,
        crate::preset::MojModel::SwarmMachine => &MOJ_SWARM_CONTROLS,
        crate::preset::MojModel::BassMatrix => &MOJ_BASS_MATRIX_CONTROLS,
        crate::preset::MojModel::DualFilter => &MOJ_DUAL_FILTER_CONTROLS,
        crate::preset::MojModel::PressureChain => &MOJ_PRESSURE_CONTROLS,
        crate::preset::MojModel::Open303 => &MOJ_OPEN303_CONTROLS,
    }
}

/// Surface order is independent of native preset order. Short profiles leave
/// slot 8 empty so envelopes still occupy row 3 and volume starts row 4.
pub const SYNTHV1_SURFACE: [Control; 12] = [
    CONTROLS[0],
    CONTROLS[1],
    CONTROLS[2],
    CONTROLS[3],
    CONTROLS[5],
    CONTROLS[6],
    CONTROLS[7],
    CONTROLS[8],
    CONTROLS[9],
    CONTROLS[10],
    CONTROLS[11],
    CONTROLS[4],
];

pub const fn synth_surface_slot(index: usize, count: usize) -> usize {
    if count == 12 && index >= 7 {
        index + 1
    } else {
        index
    }
}

pub fn synth_surface_index(slot: usize, count: usize) -> Option<usize> {
    if slot >= SYNTH_SURFACE_CONTROL_COUNT || (count == 12 && slot == 7) {
        None
    } else {
        Some(if count == 12 && slot > 7 {
            slot - 1
        } else {
            slot
        })
    }
}

const MODEL_D_SURFACE: [MojControl; 13] = [
    MOJ_MODEL_D_CONTROLS[0],
    MOJ_MODEL_D_CONTROLS[1],
    MOJ_MODEL_D_CONTROLS[2],
    MOJ_MODEL_D_CONTROLS[3],
    MOJ_MODEL_D_CONTROLS[12],
    MOJ_MODEL_D_CONTROLS[5],
    MOJ_MODEL_D_CONTROLS[6],
    MOJ_MODEL_D_CONTROLS[7],
    MOJ_MODEL_D_CONTROLS[8],
    MOJ_MODEL_D_CONTROLS[9],
    MOJ_MODEL_D_CONTROLS[10],
    MOJ_MODEL_D_CONTROLS[11],
    MOJ_MODEL_D_CONTROLS[4],
];
const SIX_OP_PM_SURFACE: [MojControl; 13] = [
    MOJ_SIX_OP_PM_CONTROLS[0],
    MOJ_SIX_OP_PM_CONTROLS[1],
    MOJ_SIX_OP_PM_CONTROLS[2],
    MOJ_SIX_OP_PM_CONTROLS[3],
    MOJ_SIX_OP_PM_CONTROLS[12],
    MOJ_SIX_OP_PM_CONTROLS[5],
    MOJ_SIX_OP_PM_CONTROLS[6],
    MOJ_SIX_OP_PM_CONTROLS[7],
    MOJ_SIX_OP_PM_CONTROLS[8],
    MOJ_SIX_OP_PM_CONTROLS[9],
    MOJ_SIX_OP_PM_CONTROLS[10],
    MOJ_SIX_OP_PM_CONTROLS[11],
    MOJ_SIX_OP_PM_CONTROLS[4],
];
const STRANGE_SURFACE: [MojControl; 13] = [
    MOJ_STRANGE_CONTROLS[0],
    MOJ_STRANGE_CONTROLS[1],
    MOJ_STRANGE_CONTROLS[2],
    MOJ_STRANGE_CONTROLS[3],
    MOJ_STRANGE_CONTROLS[12],
    MOJ_STRANGE_CONTROLS[5],
    MOJ_STRANGE_CONTROLS[6],
    MOJ_STRANGE_CONTROLS[7],
    MOJ_STRANGE_CONTROLS[8],
    MOJ_STRANGE_CONTROLS[9],
    MOJ_STRANGE_CONTROLS[10],
    MOJ_STRANGE_CONTROLS[11],
    MOJ_STRANGE_CONTROLS[4],
];
const SWARM_SURFACE: [MojControl; 13] = [
    MOJ_SWARM_CONTROLS[0],
    MOJ_SWARM_CONTROLS[1],
    MOJ_SWARM_CONTROLS[2],
    MOJ_SWARM_CONTROLS[3],
    MOJ_SWARM_CONTROLS[12],
    MOJ_SWARM_CONTROLS[5],
    MOJ_SWARM_CONTROLS[6],
    MOJ_SWARM_CONTROLS[7],
    MOJ_SWARM_CONTROLS[8],
    MOJ_SWARM_CONTROLS[9],
    MOJ_SWARM_CONTROLS[10],
    MOJ_SWARM_CONTROLS[11],
    MOJ_SWARM_CONTROLS[4],
];
const BASS_MATRIX_SURFACE: [MojControl; 12] = [
    MOJ_BASS_MATRIX_CONTROLS[0],
    MOJ_BASS_MATRIX_CONTROLS[1],
    MOJ_BASS_MATRIX_CONTROLS[2],
    MOJ_BASS_MATRIX_CONTROLS[3],
    MOJ_BASS_MATRIX_CONTROLS[5],
    MOJ_BASS_MATRIX_CONTROLS[6],
    MOJ_BASS_MATRIX_CONTROLS[7],
    MOJ_BASS_MATRIX_CONTROLS[8],
    MOJ_BASS_MATRIX_CONTROLS[9],
    MOJ_BASS_MATRIX_CONTROLS[10],
    MOJ_BASS_MATRIX_CONTROLS[11],
    MOJ_BASS_MATRIX_CONTROLS[4],
];
const PRESSURE_SURFACE: [MojControl; 13] = [
    MOJ_PRESSURE_CONTROLS[0],
    MOJ_PRESSURE_CONTROLS[1],
    MOJ_PRESSURE_CONTROLS[2],
    MOJ_PRESSURE_CONTROLS[3],
    MOJ_PRESSURE_CONTROLS[4],
    MOJ_PRESSURE_CONTROLS[5],
    MOJ_PRESSURE_CONTROLS[6],
    MOJ_PRESSURE_CONTROLS[7],
    MOJ_PRESSURE_CONTROLS[8],
    MOJ_PRESSURE_CONTROLS[9],
    MOJ_PRESSURE_CONTROLS[10],
    MOJ_PRESSURE_CONTROLS[11],
    MOJ_PRESSURE_CONTROLS[12],
];
const DUAL_FILTER_SURFACE: [MojControl; 13] = [
    MOJ_DUAL_FILTER_CONTROLS[0],
    MOJ_DUAL_FILTER_CONTROLS[1],
    MOJ_DUAL_FILTER_CONTROLS[2],
    MOJ_DUAL_FILTER_CONTROLS[3],
    MOJ_DUAL_FILTER_CONTROLS[4],
    MOJ_DUAL_FILTER_CONTROLS[5],
    MOJ_DUAL_FILTER_CONTROLS[6],
    MOJ_DUAL_FILTER_CONTROLS[8],
    MOJ_DUAL_FILTER_CONTROLS[11],
    MOJ_DUAL_FILTER_CONTROLS[12],
    MOJ_DUAL_FILTER_CONTROLS[13],
    MOJ_DUAL_FILTER_CONTROLS[14],
    MOJ_DUAL_FILTER_CONTROLS[15],
];
const OPEN303_SURFACE: [MojControl; 12] = [
    MOJ_OPEN303_CONTROLS[0],
    MOJ_OPEN303_CONTROLS[1],
    MOJ_OPEN303_CONTROLS[2],
    MOJ_OPEN303_CONTROLS[3],
    MOJ_OPEN303_CONTROLS[6],
    MOJ_OPEN303_CONTROLS[7],
    MOJ_OPEN303_CONTROLS[9],
    MOJ_OPEN303_CONTROLS[8],
    MOJ_OPEN303_CONTROLS[5],
    MOJ_OPEN303_CONTROLS[10],
    MOJ_OPEN303_CONTROLS[11],
    MOJ_OPEN303_CONTROLS[4],
];

/// The obsolete amp-page flag is accepted for existing routing callers; every
/// model now exposes its envelope on the same physical row.
pub fn moj_surface_controls(
    model: crate::preset::MojModel,
    _amp_page: bool,
) -> &'static [MojControl] {
    match model {
        crate::preset::MojModel::ModelD => &MODEL_D_SURFACE,
        crate::preset::MojModel::SixOpPm => &SIX_OP_PM_SURFACE,
        crate::preset::MojModel::StrangeOscillator => &STRANGE_SURFACE,
        crate::preset::MojModel::SwarmMachine => &SWARM_SURFACE,
        crate::preset::MojModel::BassMatrix => &BASS_MATRIX_SURFACE,
        crate::preset::MojModel::DualFilter => &DUAL_FILTER_SURFACE,
        crate::preset::MojModel::PressureChain => &PRESSURE_SURFACE,
        crate::preset::MojModel::Open303 => &OPEN303_SURFACE,
    }
}

pub fn moj_by_cc(cc: u8) -> Option<MojControl> {
    MOJ_DUAL_FILTER_CONTROLS
        .iter()
        .copied()
        .find(|control| control.cc == cc)
}

pub fn defaults() -> HashMap<u8, f32> {
    CONTROLS.iter().map(|c| (c.cc, c.min)).collect()
}

pub fn value_from_cc(control: Control, raw: u8) -> f32 {
    control.min + (raw as f32 / 127.0) * (control.max - control.min)
}

pub fn value_to_cc(control: Control, value: f32) -> u8 {
    (normalize(control, value) * 127.0).round() as u8
}

pub fn normalize(control: Control, value: f32) -> f32 {
    ((value - control.min) / (control.max - control.min)).clamp(0.0, 1.0)
}

pub fn parameter_color(value: f32, original: f32) -> ratatui::style::Color {
    let difference = value - original;
    if difference < -0.03 {
        ratatui::style::Color::Green
    } else if difference > 0.03 {
        ratatui::style::Color::Red
    } else {
        ratatui::style::Color::LightYellow
    }
}

pub fn by_cc(cc: u8) -> Option<Control> {
    CONTROLS.iter().copied().find(|c| c.cc == cc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_synth_parameter_labels_fit_the_native_cell_budget() {
        assert_eq!(SYNTH_PARAMETER_LABEL_CELLS, 40 / 4 - 1);
        let check = |name: &str| {
            assert!(!name.is_empty());
            assert!(!name.chars().any(char::is_control), "{name:?}");
            assert!(
                crate::ui_text::width(name) <= SYNTH_PARAMETER_LABEL_CELLS,
                "parameter label {name:?} exceeds the nine-cell label budget"
            );
        };
        for control in CONTROLS {
            check(control.name);
        }
        for model in crate::preset::MojModel::ALL {
            for control in moj_controls(model) {
                check(control.name);
            }
        }
        for name in ["Volume", "Aux 1", "Aux 2", "Aux 3"] {
            check(name);
        }
    }

    #[test]
    fn bipolar_envelope_range_is_exact() {
        let c = by_cc(76).unwrap();
        assert!((value_from_cc(c, 0) + 1.0).abs() < f32::EPSILON);
        assert!((value_from_cc(c, 127) - 1.0).abs() < f32::EPSILON);
        assert_eq!(value_to_cc(c, -1.0), 0);
        assert_eq!(value_to_cc(c, 0.0), 64);
        assert_eq!(value_to_cc(c, 1.0), 127);
    }

    #[test]
    fn surface_keeps_volume_aux_and_envelope_rows_without_losing_preset_state() {
        for model in crate::preset::MojModel::ALL {
            let full = moj_controls(model);
            let surface = moj_surface_controls(model, false);
            let slots: Vec<_> = surface
                .iter()
                .enumerate()
                .map(|(index, c)| {
                    let slot = synth_surface_slot(index, surface.len());
                    assert_eq!(synth_surface_index(slot, surface.len()), Some(index));
                    assert!(full.iter().any(|saved| saved.cc == c.cc));
                    (slot, c.cc)
                })
                .collect();
            assert_eq!(slots.last(), Some(&(12, 7)));
            assert_eq!(surface.as_ptr(), moj_surface_controls(model, true).as_ptr());
            assert!(!full
                .iter()
                .enumerate()
                .any(|(i, c)| full[i + 1..].iter().any(|other| other.cc == c.cc)));
            for slot in 8..12 {
                assert!(slots.iter().any(|(s, _)| *s == slot));
            }
        }
        let dual = moj_surface_controls(crate::preset::MojModel::DualFilter, false);
        assert_eq!(
            dual[8..12].iter().map(|c| c.cc).collect::<Vec<_>>(),
            [31, 32, 33, 34]
        );
        for cc in [27, 29, 30] {
            assert!(moj_controls(crate::preset::MojModel::DualFilter)
                .iter()
                .any(|c| c.cc == cc));
            assert!(!dual.iter().any(|c| c.cc == cc));
        }
    }

    #[test]
    fn mapping_has_unique_ccs_and_indices() {
        assert_eq!(CONTROLS.len(), LEGACY_SYNTH_CONTROL_COUNT);
        assert_eq!(
            CONTROLS.len() + AUX_SEND_CONTROL_COUNT,
            MAPPED_CONTROL_CAPACITY
        );
        for (i, a) in CONTROLS.iter().enumerate() {
            for b in &CONTROLS[i + 1..] {
                assert_ne!(a.cc, b.cc);
                assert_ne!(a.index, b.index);
            }
        }
    }

    #[test]
    fn normalization_and_relative_parameter_colors_include_bipolar_range() {
        let c = by_cc(76).unwrap();
        assert_eq!(normalize(c, -1.0), 0.0);
        assert_eq!(normalize(c, 0.0), 0.5);
        assert_eq!(normalize(c, 1.0), 1.0);
        assert_eq!(parameter_color(0.46, 0.5), ratatui::style::Color::Green);
        assert_eq!(
            parameter_color(0.471, 0.5),
            ratatui::style::Color::LightYellow
        );
        assert_eq!(
            parameter_color(0.529, 0.5),
            ratatui::style::Color::LightYellow
        );
        assert_eq!(parameter_color(0.54, 0.5), ratatui::style::Color::Red);
    }

    #[test]
    fn previously_displaced_native_controls_return_to_the_surface() {
        for (model, name) in [
            (crate::preset::MojModel::ModelD, "couple"),
            (crate::preset::MojModel::SixOpPm, "balance"),
            (crate::preset::MojModel::StrangeOscillator, "motion"),
            (crate::preset::MojModel::SwarmMachine, "bite"),
        ] {
            let restored = moj_surface_controls(model, false)
                .iter()
                .find(|c| c.cc == 24)
                .unwrap();
            assert_eq!(restored.macro_id, name);
        }
        assert_eq!(synth_surface_index(7, SYNTHV1_SURFACE.len()), None);
        assert_eq!(
            synth_surface_slot(11, SYNTHV1_SURFACE.len()),
            SYNTH_VOLUME_SLOT
        );
    }
}
