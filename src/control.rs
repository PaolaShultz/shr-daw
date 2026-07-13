//! The synthv1-only mapped control profile. Yoshimi and FluidSynth deliberately
//! do not inherit these plugin parameter indices or reset semantics.

use std::collections::HashMap;

pub const VOLUME_CC: u8 = 93;

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

pub const CONTROLS: [Control; 12] = [
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
}
