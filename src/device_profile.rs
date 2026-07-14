//! Data-driven names and MIDI selection metadata for external instruments.
//! Profiles describe devices; tracker routing remains device-independent.

use crate::config::{BankSelectMode, ExternalMidiConfig};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const PROFILE_SCHEMA: u8 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct DeviceProfile {
    pub schema_version: u8,
    pub id: String,
    pub manufacturer: String,
    pub model: String,
    #[serde(default)]
    pub port_matches: Vec<String>,
    pub program_change: ProgramChange,
    #[serde(default)]
    pub banks: Vec<ProgramBank>,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ProgramChange {
    /// Whether the device uses CC0/CC32 before Program Change.
    pub bank_select: BankSelect,
    /// Explanation shown in documentation and retained with the profile data.
    #[serde(default)]
    pub note: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BankSelect {
    Off,
    Cc0,
    Cc0Cc32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ProgramBank {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub bank_msb: Option<u8>,
    #[serde(default)]
    pub bank_lsb: Option<u8>,
    pub program_offset: u8,
    #[serde(default)]
    pub writable: bool,
    /// Device-native labels, in Program Change order.
    pub slots: Vec<String>,
    /// Optional names parallel to `slots`; missing/null entries remain writable
    /// or unknown slots instead of being assigned invented names.
    #[serde(default)]
    pub names: Vec<Option<String>>,
}

impl DeviceProfile {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("read MIDI device profile {}", path.display()))?;
        let profile: Self = serde_json::from_str(&text)
            .with_context(|| format!("parse MIDI device profile {}", path.display()))?;
        profile.validate()?;
        Ok(profile)
    }

    fn validate(&self) -> Result<()> {
        if self.schema_version != PROFILE_SCHEMA {
            bail!(
                "profile {} uses unsupported schema {}",
                self.id,
                self.schema_version
            );
        }
        if self.id.trim().is_empty()
            || self.manufacturer.trim().is_empty()
            || self.model.trim().is_empty()
        {
            bail!("profile identity fields cannot be empty");
        }
        if self.program_change.note.trim().is_empty() || self.sources.is_empty() {
            bail!("profile {} needs MIDI-selection notes and sources", self.id);
        }
        let mut selections = BTreeSet::new();
        for bank in &self.banks {
            if bank.slots.is_empty() || bank.names.len() > bank.slots.len() {
                bail!(
                    "profile {} bank {} has invalid slot/name lengths",
                    self.id,
                    bank.id
                );
            }
            for index in 0..bank.slots.len() {
                let program = usize::from(bank.program_offset) + index;
                if program > 127 {
                    bail!(
                        "profile {} bank {} exceeds MIDI program 127",
                        self.id,
                        bank.id
                    );
                }
                let key = (bank.bank_msb, bank.bank_lsb, program as u8);
                if !selections.insert(key) {
                    bail!("profile {} contains a duplicate MIDI selection", self.id);
                }
            }
        }
        Ok(())
    }

    pub fn label(&self) -> String {
        format!("{} {}", self.manufacturer, self.model)
    }

    pub fn apply_midi_selection(&self, config: &mut ExternalMidiConfig) {
        config.program_changes = true;
        config.bank_select = match self.program_change.bank_select {
            BankSelect::Off => BankSelectMode::Off,
            BankSelect::Cc0 => BankSelectMode::Cc0,
            BankSelect::Cc0Cc32 => BankSelectMode::Cc0Cc32,
        };
    }

    pub fn program_label(&self, bank_msb: u8, bank_lsb: u8, program: u8) -> Option<String> {
        self.banks.iter().find_map(|bank| {
            if bank.bank_msb.is_some_and(|value| value != bank_msb)
                || bank.bank_lsb.is_some_and(|value| value != bank_lsb)
                || program < bank.program_offset
            {
                return None;
            }
            let index = usize::from(program - bank.program_offset);
            let slot = bank.slots.get(index)?;
            let name = bank.names.get(index).and_then(Option::as_deref);
            Some(match name {
                Some(name) => format!("{slot} {name}"),
                None => format!("{slot} {}", bank.name),
            })
        })
    }

    fn matches_port(&self, port: &str) -> bool {
        let port = port.to_ascii_lowercase();
        self.port_matches
            .iter()
            .any(|needle| !needle.is_empty() && port.contains(&needle.to_ascii_lowercase()))
    }
}

#[derive(Clone, Debug, Default)]
pub struct Registry {
    profiles: BTreeMap<String, DeviceProfile>,
}

impl Registry {
    pub fn discover() -> Self {
        let mut registry = Self::default();
        for root in profile_roots() {
            let mut paths = fs::read_dir(root)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(std::result::Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension == "json")
                })
                .collect::<Vec<_>>();
            paths.sort();
            for path in paths {
                if let Ok(profile) = DeviceProfile::load(&path) {
                    // Earlier roots are user/configured overrides.
                    registry
                        .profiles
                        .entry(profile.id.clone())
                        .or_insert(profile);
                }
            }
        }
        registry
    }

    pub fn by_id(&self, id: &str) -> Option<&DeviceProfile> {
        self.profiles.get(id)
    }

    pub fn matching_port(&self, port: &str) -> Option<&DeviceProfile> {
        self.profiles
            .values()
            .find(|profile| profile.matches_port(port))
    }
}

fn profile_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(value) = env::var_os("SHSYNTH_DEVICE_PROFILE_DIR") {
        roots.extend(env::split_paths(&value));
    }
    if let Some(data) = env::var_os("XDG_DATA_HOME") {
        roots.push(PathBuf::from(data).join("shsynth/midi-devices"));
    }
    if let Ok(executable) = env::current_exe() {
        if let Some(parent) = executable.parent() {
            roots.push(parent.join("../share/shsynth/midi-devices"));
        }
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("midi-devices"));
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_d50_profile_has_original_and_card_memory_groups() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("midi-devices/roland-d-50.json");
        let profile = DeviceProfile::load(&path).unwrap();
        assert_eq!(
            profile.program_label(0, 0, 0).as_deref(),
            Some("I-11 Fantasia")
        );
        assert_eq!(
            profile.program_label(0, 0, 63).as_deref(),
            Some("I-88 PCM E-Piano")
        );
        assert_eq!(
            profile.program_label(0, 0, 64).as_deref(),
            Some("C-11 Card")
        );
        assert_eq!(
            profile.program_label(0, 0, 127).as_deref(),
            Some("C-88 Card")
        );
        assert_eq!(profile.program_change.bank_select, BankSelect::Off);
    }

    #[test]
    fn registry_finds_bundled_profile_by_id_and_port_name() {
        let profiles = Registry::discover();
        assert!(profiles.by_id("roland-d-50").is_some());
        assert_eq!(
            profiles
                .matching_port("USB MIDI: Roland D-50")
                .map(|p| p.id.as_str()),
            Some("roland-d-50")
        );
    }

    #[test]
    fn selected_profile_applies_its_generic_program_selection_mode() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("midi-devices/roland-d-50.json");
        let profile = DeviceProfile::load(&path).unwrap();
        let mut config = crate::config::RuntimeConfig::default().external_midi;
        config.program_changes = false;
        config.bank_select = BankSelectMode::Cc0Cc32;
        profile.apply_midi_selection(&mut config);
        assert!(config.program_changes);
        assert_eq!(config.bank_select, BankSelectMode::Off);
    }
}
