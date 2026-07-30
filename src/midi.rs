use crate::control::{by_cc, normalize, value_from_cc, CONTROLS, MOJ_CONTROLS};
use crate::pads::{EncoderAction, PadAction, PadConfig};
use crate::preset::BackendKind;
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
    pub value: Option<(u8, f32)>,
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
    route_with_pad_lock_and_modifier(pads, backend, message, pad_locked, false)
}

pub fn route_with_pad_lock_and_modifier<'a>(
    pads: &PadConfig,
    backend: BackendKind,
    message: &'a [u8],
    pad_locked: bool,
    encoder_modifier_down: bool,
) -> Routed<'a> {
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
        pads.encoder_action_with_modifier(message, encoder_modifier_down);
    let (note_encoder_consumed, note_encoder) = pads.encoder_note_action(message);
    if encoder.is_none() {
        encoder_modified = encoder_modifier_down && note_encoder.is_some();
    }
    encoder = encoder.or(note_encoder);
    let encoder_consumed = cc_encoder_consumed || note_encoder_consumed;
    let consumed = lock_consumed || pad_consumed || encoder_consumed;
    let mapped_position = (!consumed && message.len() >= 3 && message[0] & 0xf0 == 0xb0)
        .then(|| pads.target_cc(message[1]))
        .flatten()
        .and_then(|target| CONTROLS.iter().position(|control| control.cc == target));
    let value = if backend == BackendKind::Synthv1
        && !consumed
        && message.len() >= 3
        && message[0] & 0xf0 == 0xb0
    {
        pads.target_cc(message[1])
            .and_then(by_cc)
            .map(|c| (c.cc, value_from_cc(c, message[2])))
    } else if backend == BackendKind::MojSint {
        mapped_position.map(|index| {
            (
                MOJ_CONTROLS[index].cc,
                f32::from(message[2].min(127)) / 127.0,
            )
        })
    } else {
        None
    };
    let translated = if backend == BackendKind::MojSint {
        mapped_position.map(|index| [message[0], MOJ_CONTROLS[index].cc, message[2].min(127)])
    } else {
        (backend != BackendKind::Synthv1
            && !consumed
            && message.len() >= 3
            && message[0] & 0xf0 == 0xb0
            && pads.target_cc(message[1]) == Some(crate::control::VOLUME_CC))
        .then(|| [message[0], 7, message[2]])
    };
    Routed {
        consumed,
        pad,
        encoder,
        encoder_modified,
        value,
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
            controls: HashMap::from([(86, 74)]),
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
            controls: HashMap::from([(110, crate::control::VOLUME_CC)]),
            ..PadConfig::default()
        };
        for backend in [BackendKind::Yoshimi, BackendKind::FluidSynth] {
            let routed = route(&pads, backend, &[0xb2, 110, 99]);
            assert_eq!(routed.translated, Some([0xb2, 7, 99]));
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
            controls: HashMap::from([(86, 74), (87, 82)]),
            ..PadConfig::default()
        };
        let color = route(&pads, BackendKind::MojSint, &[0xb0, 86, 64]);
        assert_eq!(color.value, Some((20, 64.0 / 127.0)));
        assert_eq!(color.translated, Some([0xb0, 20, 64]));
        let attack = route(&pads, BackendKind::MojSint, &[0xb0, 87, 32]);
        assert_eq!(attack.value, Some((28, 32.0 / 127.0)));
        assert_eq!(attack.translated, Some([0xb0, 28, 32]));

        let mut pickup = Pickup::default();
        pickup.arm(&HashMap::from([(20, 0.75)]));
        assert!(!pickup.accept(20, 0.2));
        assert!(pickup.accept(20, 0.8));
    }
}
