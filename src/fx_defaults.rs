//! Private startup FX snapshot, independent of saved Projects and runtime bypass.
use crate::audio_graph::{EffectKind, InsertRack, ProjectAuxRouting};
use crate::master_strip::MasterStripSettings;
use crate::sequencer::Song;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FxDefaults {
    version: u32,
    insert_rack: InsertRack,
    drum_rack: InsertRack,
    aux_routing: ProjectAuxRouting,
    master_strip: MasterStripSettings,
}

impl FxDefaults {
    pub(crate) fn load(path: &Path, fresh: &Song) -> Result<Self> {
        match std::fs::metadata(path) {
            Ok(meta) => {
                if meta.len() > 256 * 1024 {
                    bail!("FX defaults exceed size limit");
                }
                let defaults: Self =
                    serde_json::from_slice(&std::fs::read(path)?).context("read FX defaults")?;
                defaults.validate()?;
                Ok(defaults)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut defaults = Self {
                    version: 1,
                    insert_rack: fresh.insert_rack.clone(),
                    drum_rack: fresh.drum_rack.clone(),
                    aux_routing: fresh.aux_routing.clone(),
                    master_strip: fresh.master_strip.clone(),
                };
                let id = defaults.aux_routing.next_effect_id(&defaults.insert_rack)?;
                defaults
                    .aux_routing
                    .master_rack
                    .add_with_id(EffectKind::Compressor, id)?;
                let effect = defaults.aux_routing.master_rack.effect_mut(id).unwrap();
                for (name, value) in [
                    ("threshold_db", -12.0),
                    ("knee_db", 2.5),
                    ("makeup_db", 1.5),
                    ("attack_ms", 1.0),
                    ("release_ms", 100.0),
                    ("ratio", 2.0),
                ] {
                    effect.parameters.insert(name.into(), value);
                }
                defaults.validate()?;
                Ok(defaults)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!("unsupported FX defaults version");
        }
        self.aux_routing.validate(&self.insert_rack)?;
        crate::audio_graph::validate_drum_rack(&self.drum_rack)?;
        self.master_strip.validate().map_err(anyhow::Error::msg)?;
        Ok(())
    }

    pub(crate) fn apply(&self, song: &mut Song) {
        song.insert_rack = self.insert_rack.clone();
        song.drum_rack = self.drum_rack.clone();
        song.aux_routing = self.aux_routing.clone();
        song.master_strip = self.master_strip.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trip_preserves_values_and_rejects_invalid_defaults() {
        let original = Song::new(&crate::config::RuntimeConfig::default().external_midi);
        let mut defaults =
            FxDefaults::load(Path::new("/nonexistent/shr-fx-defaults.json"), &original).unwrap();
        let id = defaults.insert_rack.add(EffectKind::Delay).unwrap();
        let effect = defaults.insert_rack.effect_mut(id).unwrap();
        effect.parameters.insert("time_ms".into(), 375.0);
        effect.bypass = true;
        // IDs belong to one graph namespace, including master effects.
        defaults.aux_routing.master_rack.effects[0].id = id + 1;
        defaults.aux_routing.master_rack.order[0] = id + 1;
        defaults.validate().unwrap();
        let bytes = serde_json::to_vec(&defaults).unwrap();
        let mut restored: FxDefaults = serde_json::from_slice(&bytes).unwrap();
        let mut song = original.clone();
        restored.apply(&mut song);
        assert_eq!(song.insert_rack, defaults.insert_rack);
        assert_eq!(song.drum_rack, defaults.drum_rack);
        assert_eq!(song.aux_routing, defaults.aux_routing);
        assert_eq!(song.patterns, original.patterns);
        restored.version = 2;
        assert!(restored.validate().is_err());
        restored.version = 1;
        restored.aux_routing.master_rack.effects[0]
            .parameters
            .insert("ratio".into(), 0.0);
        assert!(restored.validate().is_err());
    }

    #[test]
    fn fresh_defaults_add_requested_compressor_without_eq() {
        let mut song = Song::new(&crate::config::RuntimeConfig::default().external_midi);
        let defaults =
            FxDefaults::load(Path::new("/nonexistent/shr-fx-defaults.json"), &song).unwrap();
        defaults.apply(&mut song);
        let effects = &song.aux_routing.master_rack.effects;
        assert_eq!(effects.len(), 1);
        let compressor = &effects[0];
        assert_eq!(compressor.kind, EffectKind::Compressor);
        assert!(
            !compressor.bypass,
            "the independent master bypass owns startup bypass"
        );
        for (name, value) in [
            ("threshold_db", -12.0),
            ("knee_db", 2.5),
            ("makeup_db", 1.5),
            ("attack_ms", 1.0),
            ("release_ms", 100.0),
            ("ratio", 2.0),
        ] {
            assert_eq!(compressor.parameters[name], value);
        }
    }
}
