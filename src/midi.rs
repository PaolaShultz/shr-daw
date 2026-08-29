use crate::control::{
    by_cc, moj_controls, normalize, value_from_cc, AUX_SEND_CONTROL_COUNT, CONTROLS,
    INSTRUMENT_VOLUME_CC, LEGACY_SYNTH_CONTROL_COUNT, MOJ_CORE_TOGGLE_CC,
};
use crate::pads::{EncoderAction, PadAction, PadConfig};
use crate::preset::{BackendKind, MojModel};
use std::collections::HashMap;

const PICKUP_TOLERANCE: f32 = 1.0 / 127.0 + f32::EPSILON;

#[derive(Clone, Copy, Debug)]
struct PickupControl {
    target: f32,
    previous: Option<f32>,
    caught: bool,
}

/// Prevents a physical control from changing a newly loaded preset until the
/// control reaches or crosses that preset's value.
#[derive(Debug, Default)]
pub struct Pickup {
    controls: HashMap<u8, PickupControl>,
}

impl Pickup {
    pub fn arm(&mut self, values: &HashMap<u8, f32>) {
        self.controls = values
            .iter()
            .filter_map(|(&cc, &value)| {
                by_cc(cc)
                    .map(|control| normalize(control, value))
                    .or_else(|| crate::control::moj_by_cc(cc).map(|_| value.clamp(0.0, 1.0)))
                    .or_else(|| (cc == INSTRUMENT_VOLUME_CC).then(|| value.clamp(0.0, 1.0)))
                    .map(|target| {
                        (
                            cc,
                            PickupControl {
                                target,
                                previous: None,
                                caught: false,
                            },
                        )
                    })
            })
            .collect();
    }

    pub fn accept(&mut self, cc: u8, value: f32) -> bool {
        let Some(state) = self.controls.get_mut(&cc) else {
            return true;
        };
        if state.caught {
            return true;
        }
        let current = by_cc(cc)
            .map(|control| normalize(control, value))
            .unwrap_or_else(|| value.clamp(0.0, 1.0));
        let close = (current - state.target).abs() <= PICKUP_TOLERANCE;
        let crossed = state
            .previous
            .map(|previous| (previous - state.target) * (current - state.target) <= 0.0)
            .unwrap_or(false);
        state.previous = Some(current);
        state.caught = close || crossed;
        state.caught
    }
}

#[derive(Debug, PartialEq)]
pub struct Routed<'a> {
    pub consumed: bool,
    pub pad: Option<PadAction>,
    pub encoder: Option<EncoderAction>,
    pub encoder_modified: bool,
    pub synth_action: Option<bool>,
    pub value: Option<(u8, f32)>,
    /// A same-screen Project control owned by a physical rotary position, not
    /// by the active synth's MIDI CC namespace.
    pub surface: Option<(usize, f32)>,
    pub translated: Option<[u8; 3]>,
    pub forward: Option<&'a [u8]>,
}

#[cfg(test)]
pub fn route<'a>(pads: &PadConfig, backend: BackendKind, message: &'a [u8]) -> Routed<'a> {
    route_with_pad_lock(pads, backend, message, false)
}

#[cfg(test)]
pub fn route_with_pad_lock<'a>(
    pads: &PadConfig,
    backend: BackendKind,
    message: &'a [u8],
    pad_locked: bool,
) -> Routed<'a> {
    route_with_pad_lock_and_modifier(pads, backend, None, message, pad_locked, false)
}

#[cfg(test)]
pub fn route_with_pad_lock_and_modifier<'a>(
    pads: &PadConfig,
    backend: BackendKind,
    moj_model: Option<MojModel>,
    message: &'a [u8],
    pad_locked: bool,
    encoder_modifier_down: bool,
) -> Routed<'a> {
    route_with_pad_lock_modifier_and_state(
        pads,
        backend,
        moj_model,
        message,
        pad_locked,
        encoder_modifier_down,
    )
}

pub fn route_with_pad_lock_modifier_and_state<'a>(
    pads: &PadConfig,
    backend: BackendKind,
    moj_model: Option<MojModel>,
    message: &'a [u8],
    pad_locked: bool,
    encoder_modifier_down: bool,
) -> Routed<'a> {
    let synth_action = pads.synth_press_action(message);
    let (lock_consumed, _) = pads.lock_action(message);
    let (mut pad_consumed, mut pad) = if pad_locked {
        (false, None)
    } else {
        pads.route(message)
    };
    if !pad_locked && !pad_consumed {
        if let Some((action, pressed)) = pads.action_state(message) {
            pad_consumed = true;
            pad = pressed.then_some(action);
        }
    }
    let (cc_encoder_consumed, mut encoder, mut encoder_modified) =
        pads.encoder_action_with_modifier_and_state(message, encoder_modifier_down);
    let (note_encoder_consumed, note_encoder) = pads.encoder_note_action(message);
    if encoder.is_none() {
        encoder_modified = encoder_modifier_down && note_encoder.is_some();
    }
    encoder = encoder.or(note_encoder);
    let encoder_consumed = cc_encoder_consumed || note_encoder_consumed;
    let secondary_click_consumed = pads.secondary_encoder_press_consumed(message);
    let command_consumed = lock_consumed
        || pad_consumed
        || encoder_consumed
        || secondary_click_consumed
        || synth_action.is_some();
    let mapped_position = (!command_consumed && message.len() >= 3 && message[0] & 0xf0 == 0xb0)
        .then(|| pads.rotary_position(message[1]))
        .flatten();
    let mapped_control_count = if backend == BackendKind::MojSint {
        moj_controls(moj_model.unwrap_or(MojModel::ModelD)).len()
    } else {
        CONTROLS.len()
    };
    let legacy_aux_surface = match backend {
        BackendKind::Synthv1 => true,
        BackendKind::MojSint => moj_model.unwrap_or(MojModel::ModelD) != MojModel::DualFilter,
        BackendKind::Yoshimi | BackendKind::FluidSynth | BackendKind::ShrSampler => false,
    };
    let surface = legacy_aux_surface
        .then_some(mapped_position)
        .flatten()
        .filter(|index| {
            (LEGACY_SYNTH_CONTROL_COUNT..LEGACY_SYNTH_CONTROL_COUNT + AUX_SEND_CONTROL_COUNT)
                .contains(index)
        })
        .map(|index| (index, f32::from(message[2].min(127)) / 127.0));
    let reserved_rotary =
        mapped_position.is_some_and(|index| index >= mapped_control_count && surface.is_none());
    let consumed = command_consumed || surface.is_some() || reserved_rotary;
    let volume_position = CONTROLS
        .iter()
        .position(|control| control.cc == crate::control::VOLUME_CC);
    let mapped_standard_volume = mapped_position == volume_position;
    let value = if backend == BackendKind::Synthv1
        && !command_consumed
        && message.len() >= 3
        && message[0] & 0xf0 == 0xb0
    {
        pads.rotary_position(message[1])
            .and_then(|position| CONTROLS.get(position).copied())
            .map(|c| (c.cc, value_from_cc(c, message[2])))
    } else if backend == BackendKind::MojSint {
        let controls = moj_controls(moj_model.unwrap_or(MojModel::ModelD));
        mapped_position
            .and_then(|index| controls.get(index))
            .map(|control| (control.cc, f32::from(message[2].min(127)) / 127.0))
    } else {
        mapped_standard_volume
            .then(|| (INSTRUMENT_VOLUME_CC, f32::from(message[2].min(127)) / 127.0))
    };
    let translated = if backend == BackendKind::MojSint {
        if synth_action == Some(true) {
            Some([0xb0 | (message[0] & 0x0f), MOJ_CORE_TOGGLE_CC, 127])
        } else {
            let controls = moj_controls(moj_model.unwrap_or(MojModel::ModelD));
            mapped_position.and_then(|index| {
                controls
                    .get(index)
                    .map(|control| [message[0], control.cc, message[2].min(127)])
            })
        }
    } else {
        (backend != BackendKind::Synthv1
            && !consumed
            && message.len() >= 3
            && message[0] & 0xf0 == 0xb0
            && mapped_standard_volume)
            .then(|| [message[0], INSTRUMENT_VOLUME_CC, message[2]])
    };
    Routed {
        consumed,
        pad,
        encoder,
        encoder_modified,
        synth_action,
        value,
        surface,
        translated,
        forward: (!consumed && translated.is_none()).then_some(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_on_and_off_are_consumed_but_notes_pass() {
        let pads = PadConfig {
            pads: HashMap::from([(36, PadAction::Rec)]),
            ..PadConfig::default()
        };
        assert_eq!(
            route(&pads, BackendKind::Synthv1, &[0x90, 36, 99]).pad,
            Some(PadAction::Rec)
        );
        assert!(route(&pads, BackendKind::Synthv1, &[0x80, 36, 0]).consumed);
        assert!(route(&pads, BackendKind::Synthv1, &[0x90, 60, 99])
            .forward
            .is_some());
    }

    #[test]
    fn minilab_command_notes_are_channel_qualified_and_pressure_safe() {
        let pads = PadConfig {
            pads: (36..=43).map(|note| (note, PadAction::Item1)).collect(),
            pad_channels: (36..=43).map(|note| (note, 9)).collect(),
            ..PadConfig::default()
        };
        for note in 36..=43 {
            for channel in 0..16 {
                for message in [
                    [0x90 | channel, note, 100],
                    [0x80 | channel, note, 0],
                    [0x90 | channel, note, 0],
                    [0xa0 | channel, note, 64],
                ] {
                    let routed = route(&pads, BackendKind::Synthv1, &message);
                    assert_eq!(routed.consumed, channel == 9);
                    if channel == 9 {
                        assert!(routed.forward.is_none());
                    } else {
                        assert_eq!(routed.forward, Some(&message[..]));
                    }
                }
            }
        }
    }

    #[test]
    fn daws_shift_cc_is_musical_data_when_no_lock_is_configured() {
        let pads = PadConfig::default();
        for message in [[0xb0, 27, 127], [0xb0, 27, 0]] {
            let routed = route(&pads, BackendKind::Synthv1, &message);
            assert!(!routed.consumed);
            assert_eq!(routed.forward, Some(&message[..]));
        }
    }

    #[test]
    fn encoder_commands_do_not_reach_the_synth() {
        let pads = PadConfig {
            encoder_relative_cc: Some(28),
            encoder_press_cc: Some(118),
            ..PadConfig::default()
        };
        let turn = route(&pads, BackendKind::Synthv1, &[0xb0, 28, 61]);
        assert_eq!(turn.encoder, Some(EncoderAction::Up));
        assert!(turn.consumed);
        assert!(turn.forward.is_none());
        let release = route(&pads, BackendKind::Synthv1, &[0xb0, 118, 0]);
        assert!(release.consumed);
        assert!(release.encoder.is_none());
        assert!(release.forward.is_none());
    }

    #[test]
    fn rotary_nine_click_and_legacy_aux_turns_are_consumed_without_synth_actions() {
        let pads = PadConfig {
            controls: HashMap::from([(86, 13), (87, 15)]),
            secondary_encoder_press_cc: Some(119),
            secondary_encoder_press_channel: Some(0),
            ..PadConfig::default()
        };
        for message in [[0xb0, 119, 127], [0xb0, 119, 0]] {
            let routed = route(&pads, BackendKind::Synthv1, &message);
            assert!(routed.consumed);
            assert_eq!(routed.encoder, None);
            assert_eq!(routed.value, None);
            assert_eq!(routed.surface, None);
            assert_eq!(routed.translated, None);
            assert!(routed.forward.is_none());
        }
        for (message, position) in [([0xb0, 86, 64], 12), ([0xb0, 87, 64], 14)] {
            let routed = route(&pads, BackendKind::Synthv1, &message);
            assert!(routed.consumed);
            assert_eq!(routed.encoder, None);
            assert_eq!(routed.value, None);
            assert_eq!(routed.surface, Some((position, 64.0 / 127.0)));
            assert_eq!(routed.translated, None);
            assert!(routed.forward.is_none());
        }
        let wrong_channel = [0xb1, 119, 127];
        assert_eq!(
            route(&pads, BackendKind::Synthv1, &wrong_channel).forward,
            Some(&wrong_channel[..])
        );
    }

    #[test]
    fn configured_shifted_encoder_cc_is_consumed_and_classified_only_while_held() {
        let pads = PadConfig {
            encoder_relative_cc: Some(114),
            encoder_modified_relative_cc: Some(29),
            encoder_modifier: Some(crate::pads::ControllerButton::Cc { channel: 0, cc: 27 }),
            ..PadConfig::default()
        };
        let (consumed, mut modifier_down) = pads.encoder_modifier_action(&[0xb0, 27, 127]);
        assert!(consumed);
        assert!(modifier_down);

        let left = route_with_pad_lock_and_modifier(
            &pads,
            BackendKind::Synthv1,
            None,
            &[0xb0, 29, 63],
            false,
            modifier_down,
        );
        assert_eq!(left.encoder, Some(EncoderAction::Up));
        assert!(left.encoder_modified);
        assert!(left.consumed);
        assert!(left.forward.is_none());

        let right = route_with_pad_lock_and_modifier(
            &pads,
            BackendKind::Synthv1,
            None,
            &[0xb0, 29, 65],
            false,
            modifier_down,
        );
        assert_eq!(right.encoder, Some(EncoderAction::Down));
        assert!(right.encoder_modified);
        assert!(right.forward.is_none());

        let (consumed, released) = pads.encoder_modifier_action(&[0xb0, 27, 0]);
        assert!(consumed);
        modifier_down = released;
        let ordinary = route_with_pad_lock_and_modifier(
            &pads,
            BackendKind::Synthv1,
            None,
            &[0xb0, 114, 65],
            false,
            modifier_down,
        );
        assert_eq!(ordinary.encoder, Some(EncoderAction::Down));
        assert!(!ordinary.encoder_modified);

        for musical in [[0x90, 60, 100], [0x80, 60, 0]] {
            let routed = route_with_pad_lock_and_modifier(
                &pads,
                BackendKind::Synthv1,
                None,
                &musical,
                false,
                modifier_down,
            );
            assert!(!routed.consumed);
            assert_eq!(routed.forward, Some(&musical[..]));
        }
    }

    #[test]
    fn cc_command_buttons_and_note_encoder_press_are_consumed() {
        let pads = PadConfig {
            cc_buttons: HashMap::from([(44, PadAction::Item1)]),
            encoder_press_note: Some(99),
            ..PadConfig::default()
        };
        let button = route(&pads, BackendKind::Synthv1, &[0xb0, 44, 127]);
        assert_eq!(button.pad, Some(PadAction::Item1));
        assert!(button.consumed);
        let encoder = route(&pads, BackendKind::Synthv1, &[0x90, 99, 100]);
        assert_eq!(encoder.encoder, Some(EncoderAction::Select));
        assert!(encoder.consumed);
    }

    #[test]
    fn navigation_never_reaches_recording_tracker_or_external_thru() {
        let pads = PadConfig {
            pads: HashMap::from([(36, PadAction::Play)]),
            encoder_relative_cc: Some(28),
            ..PadConfig::default()
        };
        for message in [[0x90, 36, 100], [0x80, 36, 0], [0xb0, 28, 65]] {
            let routed = route(&pads, BackendKind::Synthv1, &message);
            assert!(routed.consumed);
            assert!(routed.forward.is_none());
        }
        assert_eq!(
            route(&pads, BackendKind::Synthv1, &[0x90, 60, 100]).forward,
            Some(&[0x90, 60, 100][..])
        );
    }

    #[test]
    fn pad_lock_consumes_shift_but_releases_command_notes_as_music() {
        let pads = PadConfig {
            pads: HashMap::from([(36, PadAction::Play)]),
            lock_cc: Some(27),
            ..PadConfig::default()
        };
        let shift = route_with_pad_lock(&pads, BackendKind::Synthv1, &[0xb0, 27, 127], false);
        assert!(shift.consumed);
        assert!(shift.forward.is_none());
        for message in [[0x90, 36, 100], [0x80, 36, 0]] {
            let routed = route_with_pad_lock(&pads, BackendKind::Synthv1, &message, true);
            assert!(!routed.consumed);
            assert_eq!(routed.pad, None);
            assert_eq!(routed.forward, Some(&message[..]));
        }
    }

    #[test]
    fn synthv1_mapping_is_not_imposed_on_other_backends() {
        let pads = PadConfig {
            controls: HashMap::from([(86, 1)]),
            ..PadConfig::default()
        };
        let synthv1 = route(&pads, BackendKind::Synthv1, &[0xb0, 86, 64]);
        assert_eq!(synthv1.value.map(|value| value.0), Some(74));
        let fluid = route(&pads, BackendKind::FluidSynth, &[0xb0, 86, 64]);
        assert_eq!(fluid.value, None);
        assert_eq!(fluid.forward, Some(&[0xb0, 86, 64][..]));
    }

    #[test]
    fn physical_volume_becomes_channel_volume_on_optional_backends() {
        let pads = PadConfig {
            controls: HashMap::from([(110, 5)]),
            ..PadConfig::default()
        };
        for backend in [
            BackendKind::Yoshimi,
            BackendKind::FluidSynth,
            BackendKind::ShrSampler,
        ] {
            let routed = route(&pads, backend, &[0xb2, 110, 99]);
            assert_eq!(routed.translated, Some([0xb2, 7, 99]));
            assert_eq!(routed.value, Some((INSTRUMENT_VOLUME_CC, 99.0 / 127.0)));
            assert!(routed.forward.is_none());
        }
        let synthv1 = route(&pads, BackendKind::Synthv1, &[0xb2, 110, 99]);
        assert_eq!(synthv1.translated, None);
        assert_eq!(
            synthv1.value.map(|value| value.0),
            Some(crate::control::VOLUME_CC)
        );
        assert_eq!(synthv1.forward, Some(&[0xb2, 110, 99][..]));
    }

    #[test]
    fn pickup_blocks_until_target_is_reached_or_crossed() {
        let mut pickup = Pickup::default();
        pickup.arm(&HashMap::from([(74, 0.5)]));
        assert!(!pickup.accept(74, 0.1));
        assert!(!pickup.accept(74, 0.4));
        assert!(pickup.accept(74, 0.6));
        assert!(pickup.accept(74, 0.2));
    }

    #[test]
    fn pickup_rearms_after_a_preset_reset() {
        let mut pickup = Pickup::default();
        pickup.arm(&HashMap::from([(76, 0.0)]));
        assert!(pickup.accept(76, 0.0));
        assert!(pickup.accept(76, 1.0));
        pickup.arm(&HashMap::from([(76, -0.5)]));
        assert!(!pickup.accept(76, 1.0));
        assert!(pickup.accept(76, -0.5));
    }

    #[test]
    fn moj_sint_uses_position_matched_ccs_and_normalized_pickup() {
        let pads = PadConfig {
            controls: HashMap::from([(86, 1), (87, 9)]),
            ..PadConfig::default()
        };
        let color = route(&pads, BackendKind::MojSint, &[0xb0, 86, 64]);
        assert_eq!(color.value, Some((20, 64.0 / 127.0)));
        assert_eq!(color.translated, Some([0xb0, 20, 64]));
        let attack = route(&pads, BackendKind::MojSint, &[0xb0, 87, 32]);
        assert_eq!(attack.value, Some((28, 32.0 / 127.0)));
        assert_eq!(attack.translated, Some([0xb0, 28, 32]));

        let volume_pads = PadConfig {
            controls: HashMap::from([(93, 5)]),
            ..PadConfig::default()
        };
        let volume = route(&volume_pads, BackendKind::MojSint, &[0xb0, 93, 99]);
        assert_eq!(volume.value, Some((7, 99.0 / 127.0)));
        assert_eq!(volume.translated, Some([0xb0, 7, 99]));

        let mut pickup = Pickup::default();
        pickup.arm(&HashMap::from([(20, 0.75)]));
        assert!(!pickup.accept(20, 0.2));
        assert!(pickup.accept(20, 0.8));
    }

    #[test]
    fn dual_filter_routes_all_fifteen_positions_and_a_press_only_core_click() {
        let pads = PadConfig {
            controls: HashMap::from([(99, 15)]),
            secondary_encoder_press_cc: Some(100),
            secondary_encoder_press_channel: Some(0),
            ..PadConfig::default()
        };
        let control = route_with_pad_lock_and_modifier(
            &pads,
            BackendKind::MojSint,
            Some(MojModel::DualFilter),
            &[0xb0, 99, 64],
            false,
            false,
        );
        assert_eq!(control.value, Some((34, 64.0 / 127.0)));
        assert_eq!(control.surface, None);
        assert_eq!(control.translated, Some([0xb0, 34, 64]));

        let press = route_with_pad_lock_and_modifier(
            &pads,
            BackendKind::MojSint,
            Some(MojModel::DualFilter),
            &[0xb0, 100, 127],
            false,
            false,
        );
        assert_eq!(press.synth_action, Some(true));
        assert_eq!(press.translated, Some([0xb0, MOJ_CORE_TOGGLE_CC, 127]));
        let release = route_with_pad_lock_and_modifier(
            &pads,
            BackendKind::MojSint,
            Some(MojModel::DualFilter),
            &[0xb0, 100, 0],
            false,
            false,
        );
        assert_eq!(release.synth_action, Some(false));
        assert_eq!(release.translated, None);
        assert!(release.forward.is_none());
    }
}
