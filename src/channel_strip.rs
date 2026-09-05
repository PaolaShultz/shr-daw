//! Project instrument bindings, independent of tracker lane and display order.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub enabled: bool,
    /// Half-decibel steps, bounded to +/-6 dB.
    pub bass: i8,
    pub treble: i8,
    /// Zero is OFF; 1..100 is a conventional soft-knee compressor amount.
    pub comp: u8,
}

impl Settings {
    pub fn validate(self) -> Result<(), String> {
        if !(-12..=12).contains(&self.bass) || !(-12..=12).contains(&self.treble) || self.comp > 100
        {
            return Err("channel strip parameter out of range".into());
        }
        Ok(())
    }
    pub fn active(self) -> bool {
        self.enabled && (self.bass != 0 || self.treble != 0 || self.comp != 0)
    }
    fn pack(self) -> u32 {
        u32::from(self.enabled)
            | ((self.bass as u8 as u32) << 8)
            | ((self.treble as u8 as u32) << 16)
            | ((self.comp as u32) << 24)
    }
    fn unpack(bits: u32) -> Self {
        Self {
            enabled: bits & 1 != 0,
            bass: (bits >> 8) as i8,
            treble: (bits >> 16) as i8,
            comp: (bits >> 24) as u8,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    /// Backend namespace plus its portable catalog/package identity. Never a lane.
    pub backend: String,
    pub instrument: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Instrument {
    /// Project-local identity retained when an owned preset is saved under a new name.
    pub id: u32,
    pub binding: Binding,
    pub settings: Settings,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Channels {
    pub instruments: Vec<Instrument>,
}

impl Channels {
    pub fn validate(&self) -> Result<(), String> {
        if self.instruments.len() > 128 {
            return Err("more than 128 channel strips".into());
        }
        let mut ids = BTreeSet::new();
        let mut bindings = BTreeSet::new();
        for instrument in &self.instruments {
            if instrument.id == 0
                || !ids.insert(instrument.id)
                || !bindings.insert(&instrument.binding)
            {
                return Err("duplicate or invalid channel identity".into());
            }
            let binding = &instrument.binding;
            if !["synthv1", "yoshimi", "moj-sint", "shr-sampler", "shr-drums"]
                .contains(&binding.backend.as_str())
            {
                return Err("unsupported channel backend (independent audio required)".into());
            }
            if binding.instrument.is_empty()
                || binding.instrument.len() > 256
                || binding.instrument.chars().any(char::is_control)
            {
                return Err("invalid channel instrument identity".into());
            }
            instrument.settings.validate()?;
        }
        Ok(())
    }
    pub fn settings(&self, binding: &Binding) -> Settings {
        self.instruments
            .iter()
            .find(|i| &i.binding == binding)
            .map(|i| i.settings)
            .unwrap_or_default()
    }
    pub fn set(&mut self, binding: Binding, settings: Settings) -> Result<(), String> {
        let mut next = self.clone();
        if let Some(instrument) = next.instruments.iter_mut().find(|i| i.binding == binding) {
            instrument.settings = settings;
        } else {
            let id = next
                .instruments
                .iter()
                .map(|i| i.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("channel identity exhausted")?;
            next.instruments.push(Instrument {
                id,
                binding,
                settings,
            });
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
}

/// One coherent parameter snapshot; callback never waits for a writer.
#[derive(Default)]
pub struct Controls {
    settings: AtomicU64,
    pub peak: AtomicU32,
    pub reduction: AtomicU32,
}
impl Controls {
    pub fn publish(&self, settings: Settings) -> Result<(), String> {
        settings.validate()?;
        let generation = self.settings.load(Ordering::Relaxed) & 0xffff_ffff_0000_0000;
        self.settings
            .store(generation | u64::from(settings.pack()), Ordering::Release);
        Ok(())
    }
    pub fn publish_reset(&self, settings: Settings) {
        let generation = ((self.settings.load(Ordering::Relaxed) >> 32) as u32).wrapping_add(1);
        self.settings.store(
            (u64::from(generation) << 32) | u64::from(settings.pack()),
            Ordering::Release,
        );
    }
    pub fn snapshot(&self) -> (u32, Settings) {
        let bits = self.settings.load(Ordering::Acquire);
        ((bits >> 32) as u32, Settings::unpack(bits as u32))
    }
    pub fn meter(&self) -> (f32, f32) {
        (
            f32::from_bits(self.peak.load(Ordering::Relaxed)),
            f32::from_bits(self.reduction.load(Ordering::Relaxed)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channel_over_limit_is_rejected_without_truncation() {
        let mut channels = Channels::default();
        for id in 1..=128 {
            channels
                .set(
                    Binding {
                        backend: "shr-sampler".into(),
                        instrument: format!("package.{id}"),
                    },
                    Settings::default(),
                )
                .unwrap();
        }
        let before = channels.clone();
        assert!(channels
            .set(
                Binding {
                    backend: "shr-sampler".into(),
                    instrument: "excess".into()
                },
                Settings::default()
            )
            .is_err());
        assert_eq!(channels, before);
    }

    #[test]
    fn channel_identity_and_validation_are_strict() {
        let binding = Binding {
            backend: "shr-sampler".into(),
            instrument: "factory.piano".into(),
        };
        let mut channels = Channels::default();
        let settings = Settings {
            enabled: true,
            comp: 50,
            ..Settings::default()
        };
        channels.set(binding.clone(), settings).unwrap();
        let id = channels.instruments[0].id;
        channels
            .set(
                binding.clone(),
                Settings {
                    bass: 3,
                    ..settings
                },
            )
            .unwrap();
        assert_eq!(channels.instruments.len(), 1);
        assert_eq!(channels.instruments[0].id, id);
        let original = channels.clone();
        assert!(channels
            .set(
                binding.clone(),
                Settings {
                    comp: 101,
                    ..settings
                }
            )
            .is_err());
        assert_eq!(channels, original);
        let mut malformed = original.clone();
        malformed.instruments.push(malformed.instruments[0].clone());
        assert!(malformed.validate().is_err());
        assert!(serde_json::from_str::<Settings>(
            r#"{"enabled":true,"bass":0,"treble":0,"comp":0,"automatic":true}"#
        )
        .is_err());
        for backend in ["synthv1", "yoshimi", "moj-sint", "shr-sampler", "shr-drums"] {
            channels
                .set(
                    Binding {
                        backend: backend.into(),
                        instrument: "same".into(),
                    },
                    settings,
                )
                .unwrap();
        }
        assert!(channels
            .set(
                Binding {
                    backend: "fluidsynth".into(),
                    instrument: "mixed".into()
                },
                settings
            )
            .is_err());
    }
}
