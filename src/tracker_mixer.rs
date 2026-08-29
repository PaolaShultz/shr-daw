//! FT2 live mixer ownership and adaptive relative-rotary banking.
//!
//! The UI is intentionally a view over the existing final-bus owner controls.
//! This module never stores a second gain value.

use crate::config::RuntimeConfig;
use crate::final_bus::{BusSource, SOURCE_GAIN_MAX_DB, SOURCE_GAIN_MIN_DB};
use crate::sequencer::{Page, PageTarget, Pattern};

pub const MAX_MIXER_STRIPS: usize = 12;
pub const ACTIVE_ROTARY_CAPACITY: usize = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StripKind {
    Page(usize),
    LoopMix,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MixerStrip {
    pub kind: StripKind,
    pub name: String,
    pub owner: Option<BusSource>,
    pub unavailable: Option<&'static str>,
}

impl MixerStrip {
    pub fn display_number(&self) -> String {
        match self.kind {
            StripKind::Page(index) => format!("{:02}", index + 1),
            StripKind::LoopMix => "LP".into(),
        }
    }
}

pub fn strips(pattern: &Pattern, config: &RuntimeConfig) -> Vec<MixerStrip> {
    let mut strips = pattern
        .pages
        .iter()
        .take(MAX_MIXER_STRIPS)
        .enumerate()
        .map(|(index, page)| page_strip(index, page, config))
        .collect::<Vec<_>>();
    if strips.len() < MAX_MIXER_STRIPS && pattern.audio_loops.iter().any(Option::is_some) {
        strips.push(MixerStrip {
            kind: StripKind::LoopMix,
            name: "Loop Mix".into(),
            owner: Some(BusSource::Loop),
            unavailable: None,
        });
    }
    strips
}

fn page_strip(index: usize, page: &Page, config: &RuntimeConfig) -> MixerStrip {
    let (owner, unavailable) = match &page.target {
        PageTarget::ActiveInstrument | PageTarget::Synthv1(_) | PageTarget::Software(_) => {
            (Some(BusSource::Synth), None)
        }
        PageTarget::InternalDrums(_) => (Some(BusSource::Drums), None),
        PageTarget::Default if !config.external_midi.enabled => (Some(BusSource::Synth), None),
        PageTarget::Default | PageTarget::ConfiguredExternal | PageTarget::Midi(_) => {
            if configured_input(config) {
                (Some(BusSource::Input), None)
            } else {
                (None, Some("NO RETURN"))
            }
        }
    };
    MixerStrip {
        kind: StripKind::Page(index),
        name: page.name.clone(),
        owner,
        unavailable,
    }
}

fn configured_input(config: &RuntimeConfig) -> bool {
    config
        .audio_graph
        .input
        .as_ref()
        .or_else(|| config.capture.inputs.first())
        .is_some_and(|input| {
            !input.left_port.trim().is_empty()
                && !input.right_port.trim().is_empty()
                && input.left_port != input.right_port
        })
}

#[derive(Clone, Debug)]
pub struct MixerState {
    bank: usize,
    rotary_positions: Vec<usize>,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            bank: 0,
            rotary_positions: Vec::new(),
        }
    }
}

impl MixerState {
    pub fn configure_rotaries(&mut self, positions: impl IntoIterator<Item = usize>) {
        let mut positions = positions
            .into_iter()
            .filter(|position| *position < ACTIVE_ROTARY_CAPACITY)
            .collect::<Vec<_>>();
        positions.sort_unstable();
        positions.dedup();
        self.rotary_positions = positions;
        self.bank = 0;
    }

    pub fn rotary_count(&self) -> usize {
        self.rotary_positions.len()
    }

    pub fn bank(&self) -> usize {
        self.bank
    }

    pub fn bank_count(&self, strip_count: usize) -> usize {
        if self.rotary_positions.is_empty() || strip_count == 0 {
            1
        } else {
            strip_count.div_ceil(self.rotary_positions.len())
        }
    }

    pub fn move_bank(&mut self, direction: i8, strip_count: usize) -> bool {
        let count = self.bank_count(strip_count);
        let next = if direction < 0 {
            self.bank.checked_sub(1).unwrap_or(count - 1)
        } else {
            (self.bank + 1) % count
        };
        let changed = next != self.bank;
        self.bank = next;
        changed
    }

    pub fn clamp_bank(&mut self, strip_count: usize) {
        self.bank = self
            .bank
            .min(self.bank_count(strip_count).saturating_sub(1));
    }

    pub fn assigned_strip(&self, physical_position: usize, strip_count: usize) -> Option<usize> {
        let rank = self
            .rotary_positions
            .iter()
            .position(|position| *position == physical_position)?;
        let strip = self.bank * self.rotary_positions.len() + rank;
        (strip < strip_count).then_some(strip)
    }

    pub fn assigned_rotary(&self, strip_index: usize, strip_count: usize) -> Option<usize> {
        self.rotary_positions
            .iter()
            .copied()
            .find(|position| self.assigned_strip(*position, strip_count) == Some(strip_index))
    }

    pub fn rotary_number_for_strip(&self, strip_index: usize, strip_count: usize) -> Option<usize> {
        self.assigned_rotary(strip_index, strip_count)
            .map(|position| position + 2)
    }
}

pub fn normalized_to_gain_db(normalized: f32) -> f32 {
    let gain =
        SOURCE_GAIN_MIN_DB + normalized.clamp(0.0, 1.0) * (SOURCE_GAIN_MAX_DB - SOURCE_GAIN_MIN_DB);
    (gain * 2.0).round() / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StereoInputConfig;
    use crate::sequencer::{Page, PageTarget, Song};

    fn test_strips(owners: &[Option<BusSource>]) -> Vec<MixerStrip> {
        owners
            .iter()
            .enumerate()
            .map(|(index, owner)| MixerStrip {
                kind: StripKind::Page(index),
                name: format!("Page {}", index + 1),
                owner: *owner,
                unavailable: owner.is_none().then_some("NO RETURN"),
            })
            .collect()
    }

    #[test]
    fn adaptive_banks_use_only_configured_rotaries() {
        let strips = test_strips(&[Some(BusSource::Synth); 12]);
        let mut state = MixerState::default();
        state.configure_rotaries([0, 2, 4, 6]);
        assert_eq!(state.rotary_count(), 4);
        assert_eq!(state.bank_count(strips.len()), 3);
        assert_eq!(state.assigned_strip(2, strips.len()), Some(1));
        assert!(state.move_bank(1, strips.len()));
        assert_eq!(state.assigned_strip(2, strips.len()), Some(5));
        assert!(state.move_bank(-1, strips.len()));
        assert_eq!(state.assigned_strip(6, strips.len()), Some(3));
    }

    #[test]
    fn twelve_active_rotaries_map_directly_without_banking() {
        let strips = test_strips(&[Some(BusSource::Synth); 12]);
        let mut state = MixerState::default();
        state.configure_rotaries(0..12);
        assert_eq!(state.bank_count(strips.len()), 1);
        for index in 0..12 {
            assert_eq!(state.assigned_strip(index, strips.len()), Some(index));
        }
    }

    #[test]
    fn external_pages_are_unavailable_without_a_configured_audio_return() {
        let mut config = RuntimeConfig::default();
        config.audio_graph.input = None;
        config.capture.inputs.clear();
        let mut song = Song::new(&config.external_midi);
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.pages[0].target = PageTarget::ConfiguredExternal;
        let unavailable = strips(pattern, &config);
        assert_eq!(unavailable[0].owner, None);
        assert_eq!(unavailable[0].unavailable, Some("NO RETURN"));

        config.audio_graph.input = Some(StereoInputConfig {
            name: "Hardware return".into(),
            left_port: "system:capture_1".into(),
            right_port: "system:capture_2".into(),
        });
        let configured = strips(pattern, &config);
        assert_eq!(configured[0].owner, Some(BusSource::Input));
        assert_eq!(configured[0].unavailable, None);
    }

    #[test]
    fn software_drums_and_loop_mix_resolve_to_existing_final_bus_owners() {
        let config = RuntimeConfig::default();
        let mut song = Song::new(&config.external_midi);
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.pages = vec![
            Page::new_portable("Synth", false),
            Page::new("Drums", 9, true, 0),
        ];
        pattern.pages[0].target = PageTarget::ActiveInstrument;
        pattern.pages[1].target = PageTarget::InternalDrums("kit".into());
        pattern.audio_loops[0] = Some(crate::sequencer::LoopSettings::new(
            "loop.wav".into(),
            12_000,
            crate::sequencer::BpmInterpretation::Normal,
            0,
            4,
            0,
        ));
        let strips = strips(pattern, &config);
        assert_eq!(strips[0].owner, Some(BusSource::Synth));
        assert_eq!(strips[1].owner, Some(BusSource::Drums));
        assert_eq!(strips[2].owner, Some(BusSource::Loop));
    }
}
