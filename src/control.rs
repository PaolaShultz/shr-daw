//! Backend-specific mapped control profiles. Moj Sint shares the twelve
//! physical positions and pickup behavior, but never synthv1 parameter indices
//! or XML semantics.

use std::collections::HashMap;

pub const VOLUME_CC: u8 = 93;
/// Standard MIDI channel-volume controller used by managed instruments whose
/// native parameter map is not synthv1's DCA map.
pub const INSTRUMENT_VOLUME_CC: u8 = 7;
/// Controller/menu architecture reserves four banks of four controls.  The
/// synthv1 0.9.29 profile intentionally populates only the verified 12.
pub const MAPPED_CONTROL_CAPACITY: usize = 16;

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

pub const CONTROLS: [Control; MAPPED_CONTROL_CAPACITY - 4] = [
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

pub const MOJ_MODEL_D_CONTROLS: [MojControl; 12] = [
    MojControl {
        cc: 20,
        name: "Evolve",
        macro_id: "evolve",
    },
    MojControl {
        cc: 21,
        name: "Shape",
        macro_id: "shape",
    },
    MojControl {
        cc: 22,
        name: "Color",
        macro_id: "color",
    },
    MojControl {
        cc: 23,
        name: "Edge",
        macro_id: "edge",
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
        name: "Depth",
        macro_id: "depth",
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
];

pub const MOJ_SIX_OP_PM_CONTROLS: [MojControl; 12] = [
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
        name: "Key Scale",
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
];

pub const MOJ_STRANGE_CONTROLS: [MojControl; 12] = [
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
];

pub const MOJ_SWARM_CONTROLS: [MojControl; 12] = [
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

// Moj Sint shares twelve physical positions. Position five is the universal
// instrument-volume CC 7; the remaining displayed meanings come from the
// selected synthesis model, not controller.conf.
pub const MOJ_CONTROLS: [MojControl; 12] = MOJ_MODEL_D_CONTROLS;

pub const fn moj_controls(model: crate::preset::MojModel) -> &'static [MojControl; 12] {
    match model {
        crate::preset::MojModel::ModelD => &MOJ_MODEL_D_CONTROLS,
        crate::preset::MojModel::SixOpPm => &MOJ_SIX_OP_PM_CONTROLS,
        crate::preset::MojModel::StrangeOscillator => &MOJ_STRANGE_CONTROLS,
        crate::preset::MojModel::SwarmMachine => &MOJ_SWARM_CONTROLS,
        crate::preset::MojModel::BassMatrix => &MOJ_BASS_MATRIX_CONTROLS,
    }
}

pub fn moj_by_cc(cc: u8) -> Option<MojControl> {
    MOJ_MODEL_D_CONTROLS
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
    fn bipolar_envelope_range_is_exact() {
        let c = by_cc(76).unwrap();
        assert!((value_from_cc(c, 0) + 1.0).abs() < f32::EPSILON);
        assert!((value_from_cc(c, 127) - 1.0).abs() < f32::EPSILON);
        assert_eq!(value_to_cc(c, -1.0), 0);
        assert_eq!(value_to_cc(c, 0.0), 64);
        assert_eq!(value_to_cc(c, 1.0), 127);
    }

    #[test]
    fn mapping_has_unique_ccs_and_indices() {
        assert!(CONTROLS.len() <= MAPPED_CONTROL_CAPACITY);
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
    fn moj_models_share_positions_but_expose_model_specific_controls() {
        let model_d = moj_controls(crate::preset::MojModel::ModelD);
        let six_op = moj_controls(crate::preset::MojModel::SixOpPm);
        assert_eq!(
            model_d.map(|control| control.cc),
            six_op.map(|control| control.cc)
        );
        assert_eq!(
            six_op.map(|control| control.macro_id),
            [
                "index",
                "ratio",
                "feedback",
                "operator_decay",
                "instrument_volume",
                "key_scale",
                "velocity",
                "motion",
                "attack",
                "decay",
                "sustain",
                "release",
            ]
        );
        for model in crate::preset::MojModel::ALL {
            let controls = moj_controls(model);
            assert_eq!(controls[4].cc, 7);
            assert_eq!(controls[4].name, "Volume");
            assert_eq!(controls[4].macro_id, "instrument_volume");
        }
    }
}
