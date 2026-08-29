//! Discoverable, data-driven input-controller profiles.

use crate::pads::{ControllerButton, ControllerLayout, PadAction, PadConfig, ROTARY_COUNT};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const UPDATE_URL: &str =
    "https://raw.githubusercontent.com/PaolaShultz/shr-daw/main/controller-profiles/catalog.json";

#[derive(Clone, Debug, Deserialize)]
pub struct ControllerProfile {
    pub id: String,
    pub name: String,
    pub match_names: Vec<String>,
    pub layout: u8,
    /// Physical rotary number 2..=16 -> incoming controller CC. Rotary 1 is
    /// configured separately as the relative master encoder.
    #[serde(default)]
    pub rotaries: HashMap<u8, u8>,
    /// Legacy v8 positional POT slot -> incoming controller CC.
    #[serde(default)]
    pub pots: HashMap<u8, u8>,
    /// Legacy incoming CC -> synthv1 CC. Read only and normalized on apply.
    #[serde(default)]
    pub controls: HashMap<u8, u8>,
    #[serde(default)]
    pub encoder_relative_cc: Option<u8>,
    #[serde(default)]
    pub encoder_relative_reverse: bool,
    #[serde(default)]
    pub encoder_modified_relative_cc: Option<u8>,
    #[serde(default)]
    pub encoder_modified_relative_reverse: bool,
    #[serde(default)]
    pub encoder_press_cc: Option<u8>,
    #[serde(default)]
    pub encoder_press_note: Option<u8>,
    /// Optional 1-based channel qualifier for the encoder press message.
    #[serde(default)]
    pub encoder_press_channel: Option<u8>,
    #[serde(default)]
    pub synth_press_cc: Option<u8>,
    #[serde(default)]
    pub synth_press_note: Option<u8>,
    #[serde(default)]
    pub synth_press_channel: Option<u8>,
    #[serde(default)]
    pub secondary_encoder_press_cc: Option<u8>,
    #[serde(default)]
    pub secondary_encoder_press_note: Option<u8>,
    /// Optional 1-based channel qualifier for the rotary 9 press message.
    #[serde(default)]
    pub secondary_encoder_press_channel: Option<u8>,
    #[serde(default)]
    pub encoder_modifier_cc: Option<u8>,
    /// Optional 1-based channel qualifier for the held encoder modifier.
    #[serde(default)]
    pub encoder_modifier_channel: Option<u8>,
    #[serde(default)]
    pub shifted_encoder_compatibility: Vec<ShiftedEncoderCompatibility>,
    #[serde(default)]
    pub lock_cc: Option<u8>,
    #[serde(default)]
    pub note_buttons: HashMap<u8, String>,
    /// Optional 1-based channel qualifiers keyed by command note.
    #[serde(default)]
    pub note_button_channels: HashMap<u8, u8>,
    #[serde(default)]
    pub cc_buttons: HashMap<u8, String>,
    /// Optional 1-based channel qualifiers keyed by command CC.
    #[serde(default)]
    pub cc_button_channels: HashMap<u8, u8>,
    /// One-based physical PAD position -> incoming note/CC.
    #[serde(default)]
    pub note_pads: HashMap<u8, u8>,
    #[serde(default)]
    pub note_pad_channels: HashMap<u8, u8>,
    #[serde(default)]
    pub cc_pads: HashMap<u8, u8>,
    #[serde(default)]
    pub cc_pad_channels: HashMap<u8, u8>,
    #[serde(default)]
    pub source: String,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ShiftedEncoderCompatibility {
    pub encoder_relative_cc: u8,
    #[serde(default)]
    pub encoder_relative_reverse: bool,
    pub encoder_modified_relative_cc: u8,
    #[serde(default)]
    pub encoder_modified_relative_reverse: bool,
    pub encoder_modifier_cc: u8,
    #[serde(default = "default_midi_channel")]
    pub encoder_modifier_channel: u8,
}

const fn default_midi_channel() -> u8 {
    1
}

impl ControllerProfile {
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            bail!("controller profile needs a non-empty id and name");
        }
        if !self
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            bail!("controller profile id must use only letters, numbers, '-' or '_'");
        }
        if self
            .match_names
            .iter()
            .all(|name| normalize(name).is_empty())
        {
            bail!("controller profile {} needs a match name", self.id);
        }
        if !matches!(self.layout, 4 | 5 | 8) {
            bail!("controller profile {} layout must be 4, 5, or 8", self.id);
        }
        let positional_sources = usize::from(!self.rotaries.is_empty())
            + usize::from(!self.pots.is_empty())
            + usize::from(!self.controls.is_empty());
        if positional_sources > 1 {
            bail!(
                "controller profile {} mixes rotary and legacy POT mappings",
                self.id
            );
        }
        let mut pot_ccs = HashSet::new();
        for (&rotary, &incoming) in &self.rotaries {
            if !(2..=ROTARY_COUNT).contains(&rotary) {
                bail!(
                    "controller profile {} rotary number must be 2..{ROTARY_COUNT}",
                    self.id
                );
            }
            crate::pads::ensure_midi_number(incoming, "controller profile rotary CC")?;
            if !pot_ccs.insert(incoming) {
                bail!("controller profile {} reuses rotary CC {incoming}", self.id);
            }
        }
        for (&position, &incoming) in &self.pots {
            if !(1..=15).contains(&position) {
                bail!("controller profile {} POT position must be 1..15", self.id);
            }
            crate::pads::ensure_midi_number(incoming, "controller profile POT CC")?;
            if !pot_ccs.insert(incoming) {
                bail!("controller profile {} reuses POT CC {incoming}", self.id);
            }
        }
        for (&incoming, &target) in &self.controls {
            crate::pads::ensure_midi_number(incoming, "controller profile CC")?;
            if crate::control::by_cc(target).is_none() {
                bail!(
                    "controller profile {} has unknown target CC {target}",
                    self.id
                );
            }
        }
        for action in self.note_buttons.values().chain(self.cc_buttons.values()) {
            action.parse::<PadAction>().with_context(|| {
                format!("controller profile {} has invalid action {action}", self.id)
            })?;
        }
        if (!self.note_pads.is_empty() || !self.cc_pads.is_empty())
            && (!self.note_buttons.is_empty() || !self.cc_buttons.is_empty())
        {
            bail!(
                "controller profile {} mixes positional and legacy PAD mappings",
                self.id
            );
        }
        let mut pad_positions = HashSet::new();
        for (&position, &note) in &self.note_pads {
            if !(1..=self.layout).contains(&position) || !pad_positions.insert(position) {
                bail!(
                    "controller profile {} has an invalid or duplicate PAD position",
                    self.id
                );
            }
            crate::pads::ensure_midi_number(note, "controller profile PAD note")?;
        }
        for (&position, &cc) in &self.cc_pads {
            if !(1..=self.layout).contains(&position) || !pad_positions.insert(position) {
                bail!(
                    "controller profile {} has an invalid or duplicate PAD position",
                    self.id
                );
            }
            crate::pads::ensure_midi_number(cc, "controller profile PAD CC")?;
        }
        if self.note_pad_channels.iter().any(|(position, channel)| {
            !self.note_pads.contains_key(position) || !(1..=16).contains(channel)
        }) || self.cc_pad_channels.iter().any(|(position, channel)| {
            !self.cc_pads.contains_key(position) || !(1..=16).contains(channel)
        }) {
            bail!(
                "controller profile {} has an invalid PAD channel qualifier",
                self.id
            );
        }
        for &note in self.note_buttons.keys() {
            crate::pads::ensure_midi_number(note, "controller profile button note")?;
        }
        for &cc in self.cc_buttons.keys() {
            crate::pads::ensure_midi_number(cc, "controller profile button CC")?;
        }
        if self.note_button_channels.iter().any(|(note, channel)| {
            !self.note_buttons.contains_key(note) || !(1..=16).contains(channel)
        }) {
            bail!(
                "controller profile {} has an invalid note-button channel qualifier",
                self.id
            );
        }
        if self
            .cc_button_channels
            .iter()
            .any(|(cc, channel)| !self.cc_buttons.contains_key(cc) || !(1..=16).contains(channel))
        {
            bail!(
                "controller profile {} has an invalid CC-button channel qualifier",
                self.id
            );
        }
        for (number, description) in [
            (self.encoder_relative_cc, "controller profile encoder CC"),
            (
                self.encoder_modified_relative_cc,
                "controller profile modified encoder CC",
            ),
            (self.encoder_press_cc, "controller profile encoder press CC"),
            (
                self.encoder_press_note,
                "controller profile encoder press note",
            ),
            (self.synth_press_cc, "controller profile synth press CC"),
            (self.synth_press_note, "controller profile synth press note"),
            (
                self.secondary_encoder_press_cc,
                "controller profile secondary encoder press CC",
            ),
            (
                self.secondary_encoder_press_note,
                "controller profile secondary encoder press note",
            ),
            (
                self.encoder_modifier_cc,
                "controller profile encoder modifier CC",
            ),
            (self.lock_cc, "controller profile lock CC"),
        ] {
            if let Some(number) = number {
                crate::pads::ensure_midi_number(number, description)?;
            }
        }
        let mut used_cc = self
            .rotaries
            .values()
            .copied()
            .chain(self.pots.values().copied())
            .chain(self.controls.keys().copied())
            .collect::<HashSet<_>>();
        for cc in self
            .cc_pads
            .values()
            .copied()
            .chain(self.cc_buttons.keys().copied())
            .chain(
                [
                    self.encoder_relative_cc,
                    self.encoder_modified_relative_cc,
                    self.encoder_press_cc,
                    self.synth_press_cc,
                    self.secondary_encoder_press_cc,
                    self.encoder_modifier_cc,
                    self.lock_cc,
                ]
                .into_iter()
                .flatten(),
            )
        {
            if !used_cc.insert(cc) {
                bail!("controller profile {} reuses CC {cc}", self.id);
            }
        }
        if self.encoder_press_cc.is_some() && self.encoder_press_note.is_some() {
            bail!(
                "controller profile {} encoder press must use either a CC or a note",
                self.id
            );
        }
        if self.synth_press_cc.is_some() && self.synth_press_note.is_some() {
            bail!(
                "controller profile {} synth press must use either a CC or a note",
                self.id
            );
        }
        if self.secondary_encoder_press_cc.is_some() && self.secondary_encoder_press_note.is_some()
        {
            bail!(
                "controller profile {} secondary encoder press must use either a CC or a note",
                self.id
            );
        }
        if self.encoder_modified_relative_cc.is_some() && self.encoder_modifier_cc.is_none() {
            bail!(
                "controller profile {} modified encoder CC requires an encoder modifier",
                self.id
            );
        }
        if self.encoder_modified_relative_reverse && self.encoder_modified_relative_cc.is_none() {
            bail!(
                "controller profile {} modified encoder reverse requires its CC",
                self.id
            );
        }
        if self
            .encoder_press_channel
            .is_some_and(|channel| !(1..=16).contains(&channel))
            || (self.encoder_press_channel.is_some()
                && self.encoder_press_cc.is_none()
                && self.encoder_press_note.is_none())
        {
            bail!(
                "controller profile {} has an invalid encoder press channel",
                self.id
            );
        }
        if self
            .synth_press_channel
            .is_some_and(|channel| !(1..=16).contains(&channel))
            || (self.synth_press_channel.is_some()
                && self.synth_press_cc.is_none()
                && self.synth_press_note.is_none())
        {
            bail!(
                "controller profile {} has an invalid synth press channel",
                self.id
            );
        }
        if self
            .secondary_encoder_press_channel
            .is_some_and(|channel| !(1..=16).contains(&channel))
            || (self.secondary_encoder_press_channel.is_some()
                && self.secondary_encoder_press_cc.is_none()
                && self.secondary_encoder_press_note.is_none())
        {
            bail!(
                "controller profile {} has an invalid secondary encoder press channel",
                self.id
            );
        }
        if self
            .encoder_modifier_channel
            .is_some_and(|channel| !(1..=16).contains(&channel))
            || (self.encoder_modifier_channel.is_some() && self.encoder_modifier_cc.is_none())
        {
            bail!(
                "controller profile {} has an invalid encoder modifier channel",
                self.id
            );
        }
        for compatibility in &self.shifted_encoder_compatibility {
            for (number, description) in [
                (
                    compatibility.encoder_relative_cc,
                    "controller compatibility encoder CC",
                ),
                (
                    compatibility.encoder_modified_relative_cc,
                    "controller compatibility modified encoder CC",
                ),
                (
                    compatibility.encoder_modifier_cc,
                    "controller compatibility encoder modifier CC",
                ),
            ] {
                crate::pads::ensure_midi_number(number, description)?;
            }
            if !(1..=16).contains(&compatibility.encoder_modifier_channel) {
                bail!(
                    "controller profile {} has an invalid compatibility modifier channel",
                    self.id
                );
            }
            if compatibility.encoder_relative_cc == compatibility.encoder_modified_relative_cc
                || compatibility.encoder_relative_cc == compatibility.encoder_modifier_cc
                || compatibility.encoder_modified_relative_cc == compatibility.encoder_modifier_cc
            {
                bail!(
                    "controller profile {} reuses a compatibility encoder CC",
                    self.id
                );
            }
        }
        let used_notes = self
            .note_pads
            .values()
            .copied()
            .chain(self.note_buttons.keys().copied())
            .collect::<HashSet<_>>();
        if used_notes.len() != self.note_pads.len() + self.note_buttons.len() {
            bail!("controller profile {} reuses a PAD note", self.id);
        }
        if self
            .encoder_press_note
            .is_some_and(|note| used_notes.contains(&note))
        {
            bail!(
                "controller profile {} reuses encoder press note as a PAD",
                self.id
            );
        }
        if self
            .synth_press_note
            .is_some_and(|note| used_notes.contains(&note) || self.encoder_press_note == Some(note))
        {
            bail!("controller profile {} reuses synth press note", self.id);
        }
        if self
            .secondary_encoder_press_note
            .is_some_and(|note| used_notes.contains(&note))
            || (self.secondary_encoder_press_note.is_some()
                && self.secondary_encoder_press_note == self.encoder_press_note)
        {
            bail!(
                "controller profile {} reuses secondary encoder press note",
                self.id
            );
        }
        Ok(())
    }

    pub fn matches(&self, port_name: &str) -> bool {
        let port = normalize(port_name);
        self.match_names
            .iter()
            .map(|name| normalize(name))
            .any(|name| !name.is_empty() && port.contains(&name))
    }

    pub fn apply(&self, config: &mut PadConfig, input_name: &str) -> Result<()> {
        self.validate()?;
        config.input_match = Some(input_name.to_owned());
        config.profile = Some(self.id.clone());
        config.layout = match self.layout {
            8 => ControllerLayout::Eight,
            5 => ControllerLayout::Five,
            4 => ControllerLayout::Four,
            _ => unreachable!(),
        };
        config.controls = if !self.rotaries.is_empty() {
            self.rotaries
                .iter()
                .map(|(&rotary, &incoming)| (incoming, rotary - 1))
                .collect()
        } else if self.pots.is_empty() {
            self.controls
                .iter()
                .filter_map(|(&incoming, &target)| {
                    let position = crate::control::CONTROLS
                        .iter()
                        .position(|control| control.cc == target)?
                        as u8
                        + 1;
                    Some((incoming, position))
                })
                .collect()
        } else {
            self.pots
                .iter()
                .map(|(&position, &incoming)| (incoming, position))
                .collect()
        };
        config.encoder_relative_cc = self.encoder_relative_cc;
        config.encoder_relative_reverse = self.encoder_relative_reverse;
        config.encoder_modified_relative_cc = self.encoder_modified_relative_cc;
        config.encoder_modified_relative_reverse = self.encoder_modified_relative_reverse;
        config.encoder_press_cc = self.encoder_press_cc;
        config.encoder_press_note = self.encoder_press_note;
        config.encoder_press_channel = self.encoder_press_channel.map(|channel| channel - 1);
        config.synth_press_cc = self.synth_press_cc.or(self.secondary_encoder_press_cc);
        config.synth_press_note = self.synth_press_note.or(self.secondary_encoder_press_note);
        config.synth_press_channel = self
            .synth_press_channel
            .or(self.secondary_encoder_press_channel)
            .map(|channel| channel - 1);
        config.secondary_encoder_press_cc = self.secondary_encoder_press_cc.or(self.synth_press_cc);
        config.secondary_encoder_press_note =
            self.secondary_encoder_press_note.or(self.synth_press_note);
        config.secondary_encoder_press_channel = self
            .secondary_encoder_press_channel
            .or(self.synth_press_channel)
            .map(|channel| channel - 1);
        config.encoder_modifier = self.encoder_modifier_cc.map(|cc| ControllerButton::Cc {
            channel: self.encoder_modifier_channel.unwrap_or(1) - 1,
            cc,
        });
        config.page_cycle_modifier = None;
        config.page_cycle_trigger = None;
        config.lock_cc = self.lock_cc;
        if self.note_pads.is_empty() && self.cc_pads.is_empty() {
            config.pads = self
                .note_buttons
                .iter()
                .map(|(&number, action)| {
                    let action: PadAction = action.parse()?;
                    Ok((number, action.normalized(config.layout)))
                })
                .collect::<Result<_>>()?;
            config.pad_channels = self
                .note_button_channels
                .iter()
                .map(|(&number, &channel)| (number, channel - 1))
                .collect();
            config.cc_buttons = self
                .cc_buttons
                .iter()
                .map(|(&number, action)| {
                    let action: PadAction = action.parse()?;
                    Ok((number, action.normalized(config.layout)))
                })
                .collect::<Result<_>>()?;
            config.cc_button_channels = self
                .cc_button_channels
                .iter()
                .map(|(&number, &channel)| (number, channel - 1))
                .collect();
        } else {
            config.pads = self
                .note_pads
                .iter()
                .map(|(&position, &note)| {
                    (
                        note,
                        PadAction::physical(position).expect("validated PAD position"),
                    )
                })
                .collect();
            config.pad_channels = self
                .note_pad_channels
                .iter()
                .filter_map(|(&position, &channel)| {
                    self.note_pads
                        .get(&position)
                        .map(|&note| (note, channel - 1))
                })
                .collect();
            config.cc_buttons = self
                .cc_pads
                .iter()
                .map(|(&position, &cc)| {
                    (
                        cc,
                        PadAction::physical(position).expect("validated PAD position"),
                    )
                })
                .collect();
            config.cc_button_channels = self
                .cc_pad_channels
                .iter()
                .filter_map(|(&position, &channel)| {
                    self.cc_pads.get(&position).map(|&cc| (cc, channel - 1))
                })
                .collect();
        }
        config.validate()
    }
}

/// Adds only the reviewed shifted-encoder packet to an otherwise learned
/// mapping when the connected device, ordinary encoder, and Shift button all
/// agree with that reviewed profile. This keeps existing user mappings intact
/// and changes only the in-memory router configuration.
pub fn augment_shifted_encoder_for_connected(
    current: &mut PadConfig,
    connected_name: &str,
    catalog: &Catalog,
) -> bool {
    if current.encoder_modified_relative_cc.is_some() {
        return false;
    }
    let Some(profile) = catalog.matching(connected_name) else {
        return false;
    };
    let primary = profile
        .encoder_relative_cc
        .zip(profile.encoder_modified_relative_cc)
        .zip(profile.encoder_modifier_cc)
        .map(
            |((encoder_relative_cc, encoder_modified_relative_cc), encoder_modifier_cc)| {
                ShiftedEncoderCompatibility {
                    encoder_relative_cc,
                    encoder_relative_reverse: profile.encoder_relative_reverse,
                    encoder_modified_relative_cc,
                    encoder_modified_relative_reverse: profile.encoder_modified_relative_reverse,
                    encoder_modifier_cc,
                    encoder_modifier_channel: profile.encoder_modifier_channel.unwrap_or(1),
                }
            },
        );
    let Some(variant) = primary
        .into_iter()
        .chain(profile.shifted_encoder_compatibility.iter().copied())
        .find(|variant| {
            current.encoder_relative_cc == Some(variant.encoder_relative_cc)
                && current.encoder_relative_reverse == variant.encoder_relative_reverse
                && current.encoder_modifier
                    == Some(ControllerButton::Cc {
                        channel: variant.encoder_modifier_channel - 1,
                        cc: variant.encoder_modifier_cc,
                    })
        })
    else {
        return false;
    };
    current.encoder_modified_relative_cc = Some(variant.encoder_modified_relative_cc);
    current.encoder_modified_relative_reverse = variant.encoder_modified_relative_reverse;
    if current.validate().is_ok() {
        true
    } else {
        current.encoder_modified_relative_cc = None;
        current.encoder_modified_relative_reverse = false;
        false
    }
}

#[derive(Default)]
pub struct Catalog {
    profiles: Vec<ControllerProfile>,
}

impl Catalog {
    pub fn discover() -> Self {
        Self::discover_in(roots())
    }

    /// Loads only profiles shipped with this binary/checkout. Runtime
    /// compatibility repairs must not be masked by an older downloaded
    /// catalog, while ordinary profile discovery still honors user roots.
    pub fn discover_bundled() -> Self {
        Self::discover_in(bundled_roots())
    }

    fn discover_in(roots: Vec<PathBuf>) -> Self {
        let mut profiles = Vec::new();
        let mut ids = HashSet::new();
        for root in roots {
            let path = root.join("catalog.json");
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(found) = serde_json::from_str::<Vec<ControllerProfile>>(&text) else {
                continue;
            };
            for profile in found {
                if profile.validate().is_ok() && ids.insert(profile.id.clone()) {
                    profiles.push(profile);
                }
            }
        }
        Self { profiles }
    }

    pub fn matching(&self, port_name: &str) -> Option<&ControllerProfile> {
        let normalized_port = normalize(port_name);
        let matches = self
            .profiles
            .iter()
            .filter(|profile| profile.matches(port_name))
            .map(|profile| {
                let specificity = profile
                    .match_names
                    .iter()
                    .map(|name| normalize(name))
                    .filter(|name| normalized_port.contains(name))
                    .map(|name| name.len())
                    .max()
                    .unwrap_or(0);
                (specificity, profile)
            })
            .collect::<Vec<_>>();
        let best = matches.iter().map(|(specificity, _)| *specificity).max()?;
        let mut best_matches = matches
            .into_iter()
            .filter(|(specificity, _)| *specificity == best);
        let (_, profile) = best_matches.next()?;
        best_matches.next().is_none().then_some(profile)
    }

    pub fn profiles(&self) -> &[ControllerProfile] {
        &self.profiles
    }
}

/// Resolves the selected controller when it remains connected. If every saved
/// selection is offline, one exact connected endpoint with a unique reviewed
/// profile is adopted as a replacement. The replacement always starts from
/// that profile rather than inheriting stale learned messages. Unknown or
/// multiple reviewed replacements remain unresolved.
pub fn expected_for_connected(
    current: &PadConfig,
    runtime_matches: &[String],
    connected_names: &[String],
    catalog: &Catalog,
) -> Result<Option<(PadConfig, String)>> {
    // The private controller selection owns the current device. Runtime MIDI
    // inputs are legacy fallbacks only when that selection is absent; mixing
    // both would let a stale old-device name block automatic model switching.
    let current_matches = current
        .input_match
        .iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();
    let wanted = if current_matches.is_empty() {
        runtime_matches
            .iter()
            .filter(|value| !value.trim().is_empty())
            .collect::<Vec<_>>()
    } else {
        current_matches
    };
    let mut resolved = Vec::new();
    for wanted in &wanted {
        let wanted_lower = wanted.to_ascii_lowercase();
        let matches = connected_names
            .iter()
            .filter(|name| name.to_ascii_lowercase().contains(&wanted_lower))
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [name] if !resolved.contains(name) => resolved.push(*name),
            [] => {}
            [_] => {}
            _ => bail!("configured controller input is ambiguous: {wanted}"),
        }
    }
    let connected = match resolved.as_slice() {
        [name] => *name,
        [] if wanted.is_empty() => return Ok(None),
        [] => {
            let reviewed = connected_names
                .iter()
                .filter_map(|name| catalog.matching(name).map(|profile| (name, profile)))
                .collect::<Vec<_>>();
            match reviewed.as_slice() {
                [(name, _)] => *name,
                [] | [_, _, ..] => return Ok(None),
            }
        }
        _ => bail!("configured controller inputs resolve to different devices"),
    };
    let Some(profile) = catalog.matching(connected) else {
        return Ok(None);
    };
    let replacing_offline_selection = resolved.is_empty();
    if !replacing_offline_selection
        && (current.profile.as_deref() == Some("learned")
            || current.profile.as_deref() == Some(profile.id.as_str()))
    {
        return Ok(None);
    }
    if !replacing_offline_selection
        && current
            .profile
            .as_deref()
            .is_some_and(|id| id != profile.id)
    {
        bail!("connected controller conflicts with saved profile marker");
    }
    let stable = crate::controller_learn::stable_input_match(connected);
    let mut expected = PadConfig::unmapped(stable.clone());
    profile.apply(&mut expected, &stable)?;
    Ok(Some((expected, profile.name.clone())))
}

#[cfg(test)]
pub fn validate_catalog(path: &Path) -> Result<usize> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    validate_catalog_bytes(&bytes).with_context(|| format!("parse {}", path.display()))
}

pub fn validate_catalog_bytes(bytes: &[u8]) -> Result<usize> {
    let profiles: Vec<ControllerProfile> = serde_json::from_slice(bytes)?;
    let mut ids = HashSet::new();
    for profile in &profiles {
        profile.validate()?;
        if !ids.insert(&profile.id) {
            bail!("duplicate controller profile id {}", profile.id);
        }
    }
    Ok(profiles.len())
}

pub fn user_catalog_path() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_else(|| ".".into())).join(".local/share")
        })
        .join("shsynth/controller-profiles/catalog.json")
}

/// Private learned mappings are keyed by the reviewed hardware model so an
/// automatic controller replacement never destroys another device's setup.
pub fn private_mapping_path(state: &Path, profile_id: &str) -> Result<PathBuf> {
    if profile_id.is_empty()
        || !profile_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        bail!("invalid controller profile id for private mapping");
    }
    Ok(state
        .join("controller-mappings")
        .join(format!("{profile_id}.conf")))
}

pub fn load_private_mapping(
    state: &Path,
    profile_id: &str,
    input_name: &str,
) -> Result<Option<PadConfig>> {
    let path = private_mapping_path(state, profile_id)?;
    if !path.is_file() {
        return Ok(None);
    }
    let mut config = PadConfig::load(&path)?;
    if config.profile.as_deref() != Some(profile_id) {
        bail!(
            "private controller mapping {} has the wrong profile marker",
            path.display()
        );
    }
    config.input_match = Some(input_name.to_owned());
    config.validate()?;
    Ok(Some(config))
}

fn roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(path) = env::var_os("SHSYNTH_CONTROLLER_PROFILE_DIR") {
        roots.push(PathBuf::from(path));
    }
    if let Some(parent) = user_catalog_path().parent() {
        roots.push(parent.to_path_buf());
    }
    roots.extend(bundled_roots());
    roots
}

fn bundled_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            roots.push(parent.join("../share/shsynth/controller-profiles"));
        }
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("controller-profiles"));
    roots
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_is_valid_and_matches_punctuation_insensitively() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("controller-profiles/catalog.json");
        assert!(validate_catalog(&path).unwrap() >= 1);
        let catalog = Catalog::discover_bundled();
        let profile = catalog.matching("20:0 Arturia MiniLab3 MIDI 1").unwrap();
        assert_eq!(profile.id, "arturia-minilab-3");
        assert!(profile.rotaries.is_empty());
        assert!(profile.pots.is_empty());
        assert!(profile.controls.is_empty());
        assert_eq!(profile.note_pads.len(), 8);
        assert!(profile.note_buttons.is_empty());
        let mut config = PadConfig::default();
        profile.apply(&mut config, "MiniLab3 MIDI").unwrap();
        assert!(config.controls.is_empty());
        assert_eq!(config.pads.len(), 8);
        assert_eq!(config.pad_channels.len(), 8);
        assert!(config.pad_channels.values().all(|channel| *channel == 9));
        assert_eq!(config.encoder_relative_cc, Some(114));
        assert_eq!(config.encoder_modified_relative_cc, Some(112));
        assert_eq!(config.encoder_press_cc, Some(115));
        assert_eq!(config.encoder_press_channel, Some(0));
        assert_eq!(
            config.encoder_modifier,
            Some(ControllerButton::Cc { channel: 0, cc: 9 })
        );
        assert_eq!(config.lock_cc, None);
        for (offset, action) in [
            PadAction::Pad1,
            PadAction::Pad2,
            PadAction::Pad3,
            PadAction::Pad4,
            PadAction::Pad5,
            PadAction::Pad6,
            PadAction::Pad7,
            PadAction::Pad8,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(config.pads.get(&(36 + offset as u8)), Some(&action));
        }
        assert_eq!(config.lock_action(&[0xb0, 9, 127]), (false, false));

        let profile = catalog
            .matching("Arturia MiniLab mkII:Arturia MiniLab mkII MIDI 1 20:0")
            .unwrap();
        assert_eq!(profile.id, "arturia-minilab-mkii");
        assert!(profile.rotaries.is_empty());
        assert!(profile.pots.is_empty());
        assert!(profile.note_pads.is_empty());
        let mut config = PadConfig::default();
        profile
            .apply(
                &mut config,
                "Arturia MiniLab mkII:Arturia MiniLab mkII MIDI 1",
            )
            .unwrap();
        assert_eq!(config.layout, ControllerLayout::Eight);
        assert!(config.controls.is_empty());
        assert!(config.pads.is_empty());
        assert!(config.encoder_relative_cc.is_none());
        assert!(config.encoder_press_cc.is_none());
    }

    #[test]
    fn matching_reviewed_profile_repairs_only_missing_shift_turn_in_memory() {
        let catalog = Catalog::discover_bundled();
        let mut learned = PadConfig {
            input_match: Some("Minilab3 MIDI".into()),
            profile: Some("learned".into()),
            encoder_relative_cc: Some(114),
            encoder_modifier: Some(ControllerButton::Cc { channel: 0, cc: 9 }),
            pads: HashMap::from([(99, PadAction::Item1)]),
            ..PadConfig::default()
        };
        let original_pads = learned.pads.clone();

        assert!(augment_shifted_encoder_for_connected(
            &mut learned,
            "20:0 Arturia MiniLab3 MIDI 1",
            &catalog,
        ));
        assert_eq!(learned.encoder_modified_relative_cc, Some(112));
        assert_eq!(learned.pads, original_pads);
        assert_eq!(learned.profile.as_deref(), Some("learned"));

        assert!(!augment_shifted_encoder_for_connected(
            &mut learned,
            "20:0 Arturia MiniLab3 MIDI 1",
            &catalog,
        ));

        let mut custom = PadConfig {
            encoder_relative_cc: Some(114),
            encoder_modifier: Some(ControllerButton::Cc { channel: 0, cc: 8 }),
            ..PadConfig::default()
        };
        assert!(!augment_shifted_encoder_for_connected(
            &mut custom,
            "20:0 Arturia MiniLab3 MIDI 1",
            &catalog,
        ));
        assert_eq!(custom.encoder_modified_relative_cc, None);

        let mut legacy = PadConfig {
            encoder_relative_cc: Some(114),
            encoder_modifier: Some(ControllerButton::Cc { channel: 0, cc: 27 }),
            ..PadConfig::default()
        };
        assert!(augment_shifted_encoder_for_connected(
            &mut legacy,
            "20:0 Arturia MiniLab3 MIDI 1",
            &catalog,
        ));
        assert_eq!(legacy.encoder_modified_relative_cc, Some(29));
    }

    fn minimal_profile() -> ControllerProfile {
        ControllerProfile {
            id: "test-controller".into(),
            name: "Test Controller".into(),
            match_names: vec!["test controller".into()],
            layout: 4,
            rotaries: HashMap::new(),
            pots: HashMap::new(),
            controls: HashMap::new(),
            encoder_relative_cc: None,
            encoder_relative_reverse: false,
            encoder_modified_relative_cc: None,
            encoder_modified_relative_reverse: false,
            encoder_press_cc: None,
            encoder_press_note: None,
            encoder_press_channel: None,
            synth_press_cc: None,
            synth_press_note: None,
            synth_press_channel: None,
            secondary_encoder_press_cc: None,
            secondary_encoder_press_note: None,
            secondary_encoder_press_channel: None,
            encoder_modifier_cc: None,
            encoder_modifier_channel: None,
            shifted_encoder_compatibility: Vec::new(),
            lock_cc: None,
            note_buttons: HashMap::new(),
            note_button_channels: HashMap::new(),
            cc_buttons: HashMap::new(),
            cc_button_channels: HashMap::new(),
            note_pads: HashMap::new(),
            note_pad_channels: HashMap::new(),
            cc_pads: HashMap::new(),
            cc_pad_channels: HashMap::new(),
            source: "hardware verification".into(),
        }
    }

    #[test]
    fn catalog_rejects_out_of_range_and_conflicting_physical_messages() {
        let mut profile = minimal_profile();
        profile.controls.insert(128, 74);
        assert!(profile.validate().is_err());

        profile = minimal_profile();
        profile.encoder_press_note = Some(36);
        profile.note_buttons.insert(36, "item-1".into());
        assert!(profile.validate().is_err());

        profile = minimal_profile();
        profile.encoder_press_cc = Some(118);
        profile.encoder_press_note = Some(36);
        assert!(profile.validate().is_err());

        profile = minimal_profile();
        profile.note_button_channels.insert(36, 10);
        assert!(profile.validate().is_err());

        profile = minimal_profile();
        profile.note_buttons.insert(36, "item-1".into());
        profile.note_button_channels.insert(36, 17);
        assert!(profile.validate().is_err());

        profile = minimal_profile();
        profile.cc_buttons.insert(44, "item-1".into());
        profile.cc_button_channels.insert(44, 0);
        assert!(profile.validate().is_err());
    }

    #[test]
    fn qualified_profile_application_and_controller_save_retain_channels() {
        let mut profile = minimal_profile();
        profile.layout = 8;
        profile.note_buttons.insert(36, "page-1".into());
        profile.note_button_channels.insert(36, 10);
        profile.cc_buttons.insert(44, "item-1".into());
        profile.cc_button_channels.insert(44, 3);
        let mut config = PadConfig::default();
        profile.apply(&mut config, "Test Controller MIDI").unwrap();
        assert_eq!(config.pad_channels, HashMap::from([(36, 9)]));
        assert_eq!(config.cc_button_channels, HashMap::from([(44, 2)]));

        let path = std::env::temp_dir().join(format!(
            "shsynth-profile-channel-roundtrip-{}.conf",
            std::process::id()
        ));
        config.save(&path).unwrap();
        let loaded = PadConfig::load(&path).unwrap();
        assert_eq!(loaded.pad_channels, config.pad_channels);
        assert_eq!(loaded.cc_button_channels, config.cc_button_channels);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn downloaded_catalog_bytes_are_fully_validated() {
        let profile = r#"{
            "id":"test-controller",
            "name":"Test Controller",
            "match_names":["test controller"],
            "layout":4
        }"#;
        let valid = format!("[{profile}]");
        assert_eq!(validate_catalog_bytes(valid.as_bytes()).unwrap(), 1);

        let duplicate = format!("[{profile},{profile}]");
        assert!(validate_catalog_bytes(duplicate.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("duplicate controller profile id"));
        assert!(validate_catalog_bytes(b"not json").is_err());
    }

    #[test]
    fn equally_specific_controller_profiles_do_not_auto_select() {
        let mut first = minimal_profile();
        first.id = "first".into();
        let mut second = minimal_profile();
        second.id = "second".into();
        let catalog = Catalog {
            profiles: vec![first, second],
        };

        assert!(catalog.matching("Test Controller MIDI").is_none());
    }

    #[test]
    fn known_connected_controller_rebuilds_stale_state_from_reviewed_profile() {
        let catalog = Catalog::discover();
        let stale = PadConfig {
            input_match: Some("Minilab3:Minilab3 MIDI".into()),
            lock_cc: Some(27),
            ..PadConfig::default()
        };
        let connected = vec!["Minilab3:Minilab3 MIDI 28:0".into()];
        let (expected, name) = expected_for_connected(&stale, &[], &connected, &catalog)
            .unwrap()
            .unwrap();
        assert_eq!(name, "Arturia MiniLab 3");
        assert_eq!(expected.lock_cc, None);
        assert!(expected.controls.is_empty());
        assert_eq!(expected.pad_channels.len(), 8);
        assert_ne!(expected, stale);
    }

    #[test]
    fn missing_user_config_resolves_configured_controller_default() {
        let missing = std::env::temp_dir().join(format!(
            "shsynth-missing-controller-profile-{}.conf",
            std::process::id()
        ));
        let _ = fs::remove_file(&missing);
        let current = PadConfig::load(&missing).unwrap();
        let catalog = Catalog::discover();
        let runtime = vec!["Minilab3:Minilab3 MIDI".into()];
        let connected = vec!["Minilab3:Minilab3 MIDI 28:0".into()];

        let (expected, name) = expected_for_connected(&current, &runtime, &connected, &catalog)
            .unwrap()
            .unwrap();

        assert_eq!(name, "Arturia MiniLab 3");
        assert_eq!(expected.profile.as_deref(), Some("arturia-minilab-3"));
        assert_eq!(
            expected.input_match.as_deref(),
            Some("Minilab3:Minilab3 MIDI")
        );
        assert_eq!(expected.encoder_relative_cc, Some(114));
        assert_eq!(expected.encoder_press_cc, Some(115));
        assert_eq!(expected.encoder_press_channel, Some(0));
        assert!(expected.controls.is_empty());
        assert_eq!(expected.pads.len(), 8);
        assert!(!missing.exists());
    }

    #[test]
    fn offline_selection_adopts_only_one_reviewed_replacement_without_stale_mappings() {
        let catalog = Catalog::discover();
        let mut stale = PadConfig::unmapped("Minilab3:Minilab3 MIDI");
        stale.profile = Some("learned".into());
        stale.encoder_relative_cc = Some(114);
        stale.encoder_press_cc = Some(115);
        stale.controls.insert(74, 1);
        stale.pads.insert(36, PadAction::Pad1);
        let connected = vec![
            "Midi Through:Midi Through Port-0 14:0".into(),
            "AudioBox USB 96:AudioBox USB 96 MIDI 1 16:0".into(),
            "Arturia MiniLab mkII:Arturia MiniLab mkII MIDI 1 20:0".into(),
        ];
        let (expected, name) = expected_for_connected(&stale, &[], &connected, &catalog)
            .unwrap()
            .unwrap();
        assert_eq!(name, "Arturia MiniLab mkII");
        assert_eq!(expected.profile.as_deref(), Some("arturia-minilab-mkii"));
        assert_eq!(
            expected.input_match.as_deref(),
            Some("Arturia MiniLab mkII:Arturia MiniLab mkII MIDI 1")
        );
        assert!(expected.controls.is_empty());
        assert!(expected.pads.is_empty());
        assert!(expected.encoder_relative_cc.is_none());
        assert!(expected.encoder_press_cc.is_none());

        let ambiguous = vec![
            "Minilab3:Minilab3 MIDI 28:0".into(),
            "Arturia MiniLab mkII:Arturia MiniLab mkII MIDI 1 20:0".into(),
        ];
        assert!(expected_for_connected(&stale, &[], &ambiguous, &catalog)
            .unwrap()
            .is_none());

        let unknown = vec!["Unknown Controller:Unknown MIDI 24:0".into()];
        assert!(expected_for_connected(&stale, &[], &unknown, &catalog)
            .unwrap()
            .is_none());

        let current = PadConfig::default();
        let runtime = vec!["Minilab3".into(), "Other".into()];
        let connected = vec!["Minilab3 MIDI".into(), "Other MIDI".into()];
        assert!(expected_for_connected(&current, &runtime, &connected, &catalog).is_err());

        let mut learned = PadConfig::unmapped("Minilab3");
        learned.profile = Some("learned".into());
        assert!(
            expected_for_connected(&learned, &[], &["Minilab3 MIDI".into()], &catalog)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn connected_model_keeps_its_complete_model_owned_mapping() {
        let catalog = Catalog::discover_bundled();
        let mut current = PadConfig::unmapped("Arturia MiniLab mkII:Arturia MiniLab mkII MIDI 1");
        current.profile = Some("arturia-minilab-mkii".into());
        current.encoder_relative_cc = Some(28);
        current.encoder_press_cc = Some(118);
        current.secondary_encoder_press_cc = Some(117);
        let connected = vec!["Arturia MiniLab mkII:Arturia MiniLab mkII MIDI 1 20:0".into()];

        assert!(expected_for_connected(&current, &[], &connected, &catalog)
            .unwrap()
            .is_none());
    }

    #[test]
    fn private_model_mapping_restores_with_the_current_stable_input() {
        let base = std::env::temp_dir().join(format!(
            "shsynth-private-controller-model-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let path = private_mapping_path(&base, "arturia-minilab-mkii").unwrap();
        let learned = PadConfig {
            input_match: Some("old numeric endpoint 24:0".into()),
            profile: Some("arturia-minilab-mkii".into()),
            encoder_relative_cc: Some(28),
            encoder_press_cc: Some(118),
            secondary_encoder_press_cc: Some(117),
            ..PadConfig::default()
        };
        learned.save(&path).unwrap();

        let restored = load_private_mapping(
            &base,
            "arturia-minilab-mkii",
            "Arturia MiniLab mkII:Arturia MiniLab mkII MIDI 1",
        )
        .unwrap()
        .unwrap();

        assert_eq!(restored.encoder_relative_cc, Some(28));
        assert_eq!(restored.encoder_press_cc, Some(118));
        assert_eq!(
            restored.input_match.as_deref(),
            Some("Arturia MiniLab mkII:Arturia MiniLab mkII MIDI 1")
        );
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn stale_legacy_runtime_name_cannot_block_switching_back_to_old_model() {
        let catalog = Catalog::discover_bundled();
        let mut current = PadConfig::unmapped("Arturia MiniLab mkII:Arturia MiniLab mkII MIDI 1");
        current.profile = Some("arturia-minilab-mkii".into());
        let runtime = vec!["Minilab3:Minilab3 MIDI".into()];
        let connected = vec!["Minilab3:Minilab3 MIDI 28:0".into()];

        let (replacement, _) = expected_for_connected(&current, &runtime, &connected, &catalog)
            .unwrap()
            .unwrap();

        assert_eq!(replacement.profile.as_deref(), Some("arturia-minilab-3"));
        assert_eq!(replacement.encoder_relative_cc, Some(114));
    }
}
