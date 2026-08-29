use crate::config::RuntimeConfig;
use crate::control::{defaults, CONTROLS};
use anyhow::{bail, Context, Result};
use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::XmlVersion;
use quick_xml::{Reader, Writer};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::env;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BackendKind {
    Synthv1,
    Yoshimi,
    FluidSynth,
    MojSint,
    ShrSampler,
}

impl BackendKind {
    pub const ALL: [Self; 5] = [
        Self::MojSint,
        Self::ShrSampler,
        Self::Synthv1,
        Self::Yoshimi,
        Self::FluidSynth,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Synthv1 => "synthv1",
            Self::Yoshimi => "Yoshimi",
            Self::FluidSynth => "FluidSynth",
            Self::MojSint => "Moj Sint",
            Self::ShrSampler => "SHR Sampler",
        }
    }

    pub fn next(self, direction: i8) -> Self {
        let index = Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0);
        Self::ALL
            [(index as isize + direction as isize).rem_euclid(Self::ALL.len() as isize) as usize]
    }
}

impl fmt::Display for BackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

impl std::str::FromStr for BackendKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "synthv1" | "synth" => Ok(Self::Synthv1),
            "yoshimi" => Ok(Self::Yoshimi),
            "fluidsynth" | "fluid" => Ok(Self::FluidSynth),
            "moj sint" | "moj-sint" | "moj_sint" | "mojsint" => Ok(Self::MojSint),
            "shr sampler" | "shr-sampler" | "shr_sampler" | "sampler" => Ok(Self::ShrSampler),
            _ => bail!("unknown sound engine {value:?}"),
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PresetId {
    Synthv1 {
        path: PathBuf,
    },
    Yoshimi {
        path: PathBuf,
    },
    FluidSynth {
        soundfont: PathBuf,
        soundfont_index: u8,
        bank: u16,
        program: u8,
    },
    MojSint {
        model: MojModel,
        path: PathBuf,
    },
    ShrSampler {
        instrument_id: String,
        path: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MojModel {
    ModelD,
    SixOpPm,
    StrangeOscillator,
    SwarmMachine,
    BassMatrix,
    DualFilter,
}

impl MojModel {
    pub const ALL: [Self; 6] = [
        Self::ModelD,
        Self::SixOpPm,
        Self::StrangeOscillator,
        Self::SwarmMachine,
        Self::BassMatrix,
        Self::DualFilter,
    ];

    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::ModelD => "model_d",
            Self::SixOpPm => "six_op_pm",
            Self::StrangeOscillator => "strange_oscillator",
            Self::SwarmMachine => "swarm_machine",
            Self::BassMatrix => "bass_matrix",
            Self::DualFilter => "dual_filter",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::ModelD => "Model D",
            Self::SixOpPm => "Six-Op PM",
            Self::StrangeOscillator => "Strange Osc",
            Self::SwarmMachine => "Swarm Machine",
            Self::BassMatrix => "Bass Matrix",
            Self::DualFilter => "Dual Filter",
        }
    }

    pub const fn catalog_letter(self) -> char {
        match self {
            Self::ModelD => 'D',
            Self::SixOpPm => 'P',
            Self::StrangeOscillator => 'O',
            Self::SwarmMachine => 'S',
            Self::BassMatrix => 'B',
            Self::DualFilter => 'F',
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Preset {
    pub backend: BackendKind,
    pub name: String,
    pub category: Option<String>,
    pub id: PresetId,
}

impl Preset {
    pub fn synthv1(name: impl Into<String>, path: PathBuf) -> Self {
        Self {
            backend: BackendKind::Synthv1,
            name: name.into(),
            category: None,
            id: PresetId::Synthv1 { path },
        }
    }

    pub fn display_name(&self) -> String {
        if let PresetId::MojSint { model, .. } = &self.id {
            return compact_moj_sint_name(*model, &self.name);
        }
        if self.backend == BackendKind::FluidSynth {
            return self.name.clone();
        }
        self.category
            .as_ref()
            .map(|category| format!("[{category}] {}", self.name))
            .unwrap_or_else(|| self.name.clone())
    }

    /// Portable identity used by Project-owned software routes. It is stable
    /// across catalog ordering and deliberately excludes machine-local
    /// absolute paths.
    pub fn route_id(&self) -> String {
        match &self.id {
            PresetId::Synthv1 { .. } => self.name.clone(),
            PresetId::Yoshimi { .. } => self
                .category
                .as_ref()
                .map(|category| format!("{category}/{}", self.name))
                .unwrap_or_else(|| self.name.clone()),
            PresetId::FluidSynth {
                soundfont,
                soundfont_index,
                bank,
                program,
                ..
            } => {
                let soundfont = soundfont
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("soundfont");
                format!("sf{soundfont_index}:{soundfont}:{bank}:{program}")
            }
            PresetId::MojSint { model, .. } => {
                format!("{}/{}", model.stable_id(), self.name)
            }
            PresetId::ShrSampler { instrument_id, .. } => instrument_id.clone(),
        }
    }

    pub fn legacy_route_id(&self) -> Option<String> {
        match &self.id {
            PresetId::FluidSynth {
                soundfont,
                bank,
                program,
                ..
            } => {
                let soundfont = soundfont
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("soundfont");
                Some(format!("{soundfont}:{bank}:{program}"))
            }
            PresetId::MojSint {
                model: MojModel::ModelD,
                ..
            } => Some(self.name.clone()),
            PresetId::MojSint { .. } => None,
            _ => None,
        }
    }

    pub const fn moj_model(&self) -> Option<MojModel> {
        match &self.id {
            PresetId::MojSint { model, .. } => Some(*model),
            _ => None,
        }
    }

    /// General MIDI percussion uses bank 128, with program 0 selecting the
    /// standard kit. SoundFont discovery supplies the actual configured file,
    /// index, bank, and program; callers never need a machine-local filename.
    pub fn is_general_midi_drum_kit(&self) -> bool {
        matches!(
            &self.id,
            PresetId::FluidSynth {
                bank: 128,
                program: 0,
                ..
            }
        )
    }
}

fn compact_moj_sint_name(model: MojModel, name: &str) -> String {
    let (number, mut sound) = name.split_once(' ').map_or((None, name), |(first, rest)| {
        if !first.is_empty() && first.chars().all(|character| character.is_ascii_digit()) {
            (Some(first), rest)
        } else {
            (None, name)
        }
    });
    let (code, redundant_prefixes): (&str, &[&str]) = match model {
        MojModel::ModelD => ("M-D", &["Model D", "M-D"]),
        MojModel::SixOpPm => ("6-OP", &["Six-Op PM", "Six-Op", "6-OP"]),
        MojModel::StrangeOscillator => ("S-OSC", &["Strange Oscillator", "Strange Osc", "S-OSC"]),
        MojModel::SwarmMachine => ("SWARM", &["Swarm Machine", "Swarm", "SWARM"]),
        MojModel::BassMatrix => ("B-MAT", &["Bass Matrix", "B-MAT"]),
        MojModel::DualFilter => ("D-FLT", &["Dual Filter", "D-FLT"]),
    };
    for prefix in redundant_prefixes {
        if sound
            .get(..prefix.len())
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
            && sound
                .get(prefix.len()..)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
        {
            sound = sound[prefix.len()..].trim_start();
            break;
        }
    }
    match (number, sound.is_empty()) {
        (Some(number), false) => format!("{number} {code} {sound}"),
        (Some(number), true) => format!("{number} {code}"),
        (None, false) => format!("{code} {sound}"),
        (None, true) => code.into(),
    }
}

/// Presets shows Moj Sint as one model-grouped catalog. Each model owns one
/// stable letter and its own visible 01-based sequence, independent of old
/// global factory filename numbers.
pub fn moj_catalog_display_name(presets: &[Preset], index: usize) -> Option<String> {
    let preset = presets.get(index)?;
    let model = preset.moj_model()?;
    let ordinal = presets[..=index]
        .iter()
        .filter(|candidate| candidate.moj_model() == Some(model))
        .count();
    let compact = compact_moj_sint_name(model, &preset.name);
    let without_number = compact
        .split_once(' ')
        .map_or(compact.as_str(), |(first, rest)| {
            if first.chars().all(|character| character.is_ascii_digit()) {
                rest
            } else {
                compact.as_str()
            }
        });
    let old_code = match model {
        MojModel::ModelD => "M-D",
        MojModel::SixOpPm => "6-OP",
        MojModel::StrangeOscillator => "S-OSC",
        MojModel::SwarmMachine => "SWARM",
        MojModel::BassMatrix => "B-MAT",
        MojModel::DualFilter => "D-FLT",
    };
    let sound = without_number
        .strip_prefix(old_code)
        .unwrap_or(without_number)
        .trim_start();
    let sound = if sound.is_empty() {
        clean_numbered_name(&preset.name)
    } else {
        sound.to_owned()
    };
    Some(format!("{}{:02} {sound}", model.catalog_letter(), ordinal))
}

#[derive(Clone, Debug)]
pub struct Catalog {
    pub backend: BackendKind,
    pub presets: Vec<Preset>,
    pub unavailable: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserPresetStorage {
    pub synthv1: PathBuf,
    pub moj_sint: PathBuf,
}

impl UserPresetStorage {
    pub fn from_environment() -> Self {
        let data_home = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env::var_os("HOME").unwrap_or_else(|| ".".into()))
                    .join(".local/share")
            });
        Self {
            synthv1: env::var_os("SHSYNTH_PRESET_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| data_home.join("shsynth/presets/synthv1")),
            moj_sint: env::var_os("SHSYNTH_MOJ_PRESET_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| data_home.join("moj-sint/presets")),
        }
    }

    pub fn root_for(&self, backend: BackendKind) -> Option<&Path> {
        match backend {
            BackendKind::Synthv1 => Some(&self.synthv1),
            BackendKind::MojSint => Some(&self.moj_sint),
            BackendKind::Yoshimi | BackendKind::FluidSynth | BackendKind::ShrSampler => None,
        }
    }
}

pub fn discover_all(
    config: &RuntimeConfig,
    synthv1_dir: &Path,
    user_storage: &UserPresetStorage,
) -> Vec<Catalog> {
    let mut moj_roots = config.moj_sint.backend.preset_roots.clone();
    if !moj_roots.contains(&user_storage.moj_sint) {
        moj_roots.push(user_storage.moj_sint.clone());
    }
    vec![
        catalog(
            BackendKind::MojSint,
            command_exists(&config.moj_sint.backend.command),
            discover_moj_sint(&moj_roots),
            &config.moj_sint.backend.command,
        ),
        catalog(
            BackendKind::ShrSampler,
            command_exists(&config.shr_sampler.backend.command),
            discover_shr_sampler(&config.shr_sampler.backend.preset_roots),
            &config.shr_sampler.backend.command,
        ),
        catalog(
            BackendKind::Synthv1,
            command_exists(&config.synth_command),
            discover_synthv1_roots(&[synthv1_dir.to_path_buf(), user_storage.synthv1.clone()]),
            &config.synth_command,
        ),
        catalog(
            BackendKind::Yoshimi,
            command_exists(&config.yoshimi.backend.command),
            discover_yoshimi(
                &config.yoshimi.backend.preset_roots,
                &config.yoshimi.categories,
                config.yoshimi.presets_per_category,
            ),
            &config.yoshimi.backend.command,
        ),
        catalog(
            BackendKind::FluidSynth,
            command_exists(&config.fluidsynth.backend.command),
            discover_fluidsynth(&config.fluidsynth.soundfonts),
            &config.fluidsynth.backend.command,
        ),
    ]
}

const MAX_SHR_SAMPLER_INSTRUMENTS: usize = 512;
const MAX_SHR_SAMPLER_MANIFEST_BYTES: u64 = 1_048_576;

pub fn discover_shr_sampler(roots: &[PathBuf]) -> Result<Vec<Preset>> {
    let mut presets = Vec::new();
    let mut ids = BTreeSet::new();
    let mut pending = Vec::new();
    for root in roots {
        match fs::symlink_metadata(root) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                bail!(
                    "SHR Sampler instrument root is not a regular directory: {}",
                    root.display()
                )
            }
            Ok(_) => pending.push(root.clone()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("inspect SHR Sampler instrument root"),
        }
    }
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("read SHR Sampler root {}", directory.display()))?
        {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let path = entry.path();
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() && extension_is(&path, "shrinst") {
                if presets.len() == MAX_SHR_SAMPLER_INSTRUMENTS {
                    bail!("SHR Sampler catalog exceeds {MAX_SHR_SAMPLER_INSTRUMENTS} packages");
                }
                let manifest_path = path.join("manifest.json");
                let bytes = read_regular_bounded(
                    &manifest_path,
                    MAX_SHR_SAMPLER_MANIFEST_BYTES,
                    "SHR Sampler manifest",
                )?;
                let document: serde_json::Value = serde_json::from_slice(&bytes)
                    .with_context(|| format!("parse {}", manifest_path.display()))?;
                let object = document
                    .as_object()
                    .context("SHR Sampler manifest must be a JSON object")?;
                if object
                    .get("format_version")
                    .and_then(serde_json::Value::as_u64)
                    != Some(1)
                {
                    bail!("unsupported SHR Sampler package format");
                }
                let instrument_id = object
                    .get("instrument_id")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| valid_package_id(value))
                    .context("invalid SHR Sampler instrument_id")?
                    .to_owned();
                let name = object
                    .get("display_name")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| !value.trim().is_empty() && value.len() <= 128)
                    .context("invalid SHR Sampler display_name")?
                    .to_owned();
                if !ids.insert(instrument_id.clone()) {
                    bail!("duplicate SHR Sampler instrument ID {instrument_id:?}");
                }
                presets.push(Preset {
                    backend: BackendKind::ShrSampler,
                    name,
                    category: None,
                    id: PresetId::ShrSampler {
                        instrument_id,
                        path,
                    },
                });
            } else if file_type.is_dir() {
                pending.push(path);
            }
        }
    }
    sort_presets(&mut presets);
    Ok(presets)
}

fn valid_package_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

const MAX_MOJ_PRESETS: usize = 512;
const MAX_MOJ_PRESET_BYTES: u64 = 1_048_576;

pub fn discover_moj_sint(roots: &[PathBuf]) -> Result<Vec<Preset>> {
    let mut presets = Vec::new();
    let mut pending = roots
        .iter()
        .filter(|root| root.is_dir())
        .cloned()
        .collect::<Vec<_>>();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("read Moj Sint preset root {}", directory.display()))?
        {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let path = entry.path();
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() && extension_is(&path, "mojsint") {
                if presets.len() == MAX_MOJ_PRESETS {
                    bail!("Moj Sint catalog exceeds {MAX_MOJ_PRESETS} regular files");
                }
                let (name, model, _) = read_moj_sint(&path)?;
                presets.push(Preset {
                    backend: BackendKind::MojSint,
                    name,
                    category: Some(model.label().into()),
                    id: PresetId::MojSint { model, path },
                });
            }
        }
    }
    sort_presets(&mut presets);
    Ok(presets)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MojPresetV3 {
    schema_version: u32,
    name: String,
    voices: usize,
    output_gain: f32,
    model_d_patch: MojModelDPatch,
    macros: MojMacrosV2,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MojPresetV4 {
    schema_version: u32,
    name: String,
    voices: usize,
    output_gain: f32,
    model: MojModel,
    model_d_patch: MojModelDPatch,
    macros: MojMacrosV2,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MojPresetV5ModelD {
    schema_version: u32,
    name: String,
    voices: usize,
    output_gain: f32,
    #[serde(default = "full_moj_volume")]
    instrument_volume: f32,
    model: MojModel,
    model_d_patch: MojModelDPatch,
    macros: MojMacrosV2,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MojPresetV5SixOp {
    schema_version: u32,
    name: String,
    voices: usize,
    output_gain: f32,
    #[serde(default = "full_moj_volume")]
    instrument_volume: f32,
    model: MojModel,
    six_op_patch: MojSixOpPatch,
    macros: MojMacrosSixOp,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MojPresetV6Strange {
    schema_version: u32,
    name: String,
    voices: usize,
    output_gain: f32,
    #[serde(default = "full_moj_volume")]
    instrument_volume: f32,
    model: MojModel,
    strange_patch: MojStrangePatch,
    macros: MojMacrosStrange,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum MojModelDPatch {
    Bass,
    Lead,
    FilterArticulation,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum MojSixOpPatch {
    BellMetal,
    FracturedMetal,
    ElectricPianoMallet,
    GlassWood,
    BrassBass,
    MechanicalStab,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum MojStrangePatch {
    Unified,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum MojSwarmPatch {
    WarmPad,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum MojBassMatrixPatch {
    Transformer,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum MojDualFilterCore {
    Industrial,
    Counter,
}

fn full_moj_volume() -> f32 {
    1.0
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MojPresetV2 {
    schema_version: u32,
    name: String,
    voices: usize,
    output_gain: f32,
    macros: MojMacrosV2,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MojPresetV1 {
    schema_version: u32,
    name: String,
    voices: usize,
    output_gain: f32,
    envelope: MojLegacyEnvelope,
    macros: MojMacrosV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MojLegacyEnvelope {
    attack_seconds: f32,
    decay_seconds: f32,
    sustain_level: f32,
    release_seconds: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MojMacrosV2 {
    evolve: f32,
    shape: f32,
    color: f32,
    edge: f32,
    couple: f32,
    motion: f32,
    depth: f32,
    space: f32,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MojMacrosSixOp {
    index: f32,
    ratio: f32,
    feedback: f32,
    operator_decay: f32,
    balance: f32,
    key_scale: f32,
    velocity: f32,
    motion: f32,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MojMacrosStrange {
    #[serde(rename = "type")]
    type_: f32,
    form: f32,
    warp: f32,
    couple: f32,
    motion: f32,
    chaos: f32,
    color: f32,
    space: f32,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MojMacrosSwarm {
    mass: f32,
    detune: f32,
    spread: f32,
    shape: f32,
    bite: f32,
    motion: f32,
    color: f32,
    space: f32,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MojMacrosBassMatrix {
    body: f32,
    growl: f32,
    metal: f32,
    punch: f32,
    character: f32,
    drive: f32,
    filter: f32,
    unstable: f32,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MojDualFilterControls {
    filter_a_cutoff: f32,
    filter_a_resonance: f32,
    filter_a_envelope_depth: f32,
    filter_b_cutoff: f32,
    filter_b_resonance: f32,
    filter_b_envelope_depth: f32,
    structure: f32,
    filter_attack: f32,
    filter_decay: f32,
    filter_sustain: f32,
    filter_release: f32,
    amp_attack: f32,
    amp_decay: f32,
    amp_sustain: f32,
    amp_release: f32,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MojPresetV7Swarm {
    schema_version: u32,
    name: String,
    voices: usize,
    output_gain: f32,
    instrument_volume: f32,
    model: MojModel,
    swarm_patch: MojSwarmPatch,
    macros: MojMacrosSwarm,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MojPresetV7BassMatrix {
    schema_version: u32,
    name: String,
    voices: usize,
    output_gain: f32,
    instrument_volume: f32,
    model: MojModel,
    bass_matrix_patch: MojBassMatrixPatch,
    macros: MojMacrosBassMatrix,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MojPresetV8DualFilter {
    schema_version: u32,
    name: String,
    voices: usize,
    output_gain: f32,
    instrument_volume: f32,
    model: MojModel,
    dual_filter_core: MojDualFilterCore,
    controls: MojDualFilterControls,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MojMacrosV1 {
    evolve: f32,
    shape: f32,
    color: f32,
    edge: f32,
    couple: f32,
    motion: f32,
    depth: f32,
    width: f32,
    space: f32,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
}

impl MojMacrosV2 {
    fn values(self) -> [f32; 15] {
        [
            self.evolve,
            self.shape,
            self.color,
            self.edge,
            self.couple,
            self.motion,
            self.depth,
            self.space,
            self.attack,
            self.decay,
            self.sustain,
            self.release,
            0.5,
            0.5,
            0.5,
        ]
    }
}

impl MojMacrosSixOp {
    fn values(self) -> [f32; 15] {
        [
            self.index,
            self.ratio,
            self.feedback,
            self.operator_decay,
            self.balance,
            self.key_scale,
            self.velocity,
            self.motion,
            self.attack,
            self.decay,
            self.sustain,
            self.release,
            0.5,
            0.5,
            0.5,
        ]
    }
}

impl MojMacrosStrange {
    fn values(self) -> [f32; 15] {
        [
            self.type_,
            self.form,
            self.warp,
            self.couple,
            self.motion,
            self.chaos,
            self.color,
            self.space,
            self.attack,
            self.decay,
            self.sustain,
            self.release,
            0.5,
            0.5,
            0.5,
        ]
    }
}

impl MojMacrosSwarm {
    fn values(self) -> [f32; 15] {
        [
            self.mass,
            self.detune,
            self.spread,
            self.shape,
            self.bite,
            self.motion,
            self.color,
            self.space,
            self.attack,
            self.decay,
            self.sustain,
            self.release,
            0.5,
            0.5,
            0.5,
        ]
    }
}

impl MojMacrosBassMatrix {
    fn values(self) -> [f32; 15] {
        [
            self.body,
            self.growl,
            self.metal,
            self.punch,
            self.character,
            self.drive,
            self.filter,
            self.unstable,
            self.attack,
            self.decay,
            self.sustain,
            self.release,
            0.5,
            0.5,
            0.5,
        ]
    }
}

impl MojDualFilterControls {
    fn values(self) -> [f32; 15] {
        [
            self.filter_a_cutoff,
            self.filter_a_resonance,
            self.filter_a_envelope_depth,
            self.filter_b_cutoff,
            self.filter_b_resonance,
            self.filter_b_envelope_depth,
            self.structure,
            self.filter_attack,
            self.filter_decay,
            self.filter_sustain,
            self.filter_release,
            self.amp_attack,
            self.amp_decay,
            self.amp_sustain,
            self.amp_release,
        ]
    }
}

#[derive(Clone, Copy, Debug)]
enum MojPatch {
    ModelD(MojModelDPatch),
    SixOpPm(MojSixOpPatch),
    StrangeOscillator(MojStrangePatch),
    SwarmMachine(MojSwarmPatch),
    BassMatrix(MojBassMatrixPatch),
    DualFilter(MojDualFilterCore),
}

#[derive(Debug)]
struct MojDocument {
    name: String,
    model: MojModel,
    voices: usize,
    output_gain: f32,
    instrument_volume: f32,
    patch: MojPatch,
    values: [f32; 15],
}

fn read_moj_document(path: &Path) -> Result<MojDocument> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("open Moj Sint preset {}", path.display()))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_MOJ_PRESET_BYTES {
        bail!(
            "Moj Sint preset must be a regular file no larger than 1 MiB: {}",
            path.display()
        );
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_MOJ_PRESET_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MOJ_PRESET_BYTES {
        bail!("Moj Sint preset exceeds 1 MiB: {}", path.display());
    }
    let source = String::from_utf8(bytes).context("Moj Sint preset is not UTF-8")?;
    let value: toml::Value =
        toml::from_str(&source).with_context(|| format!("parse {}", path.display()))?;
    let version = value
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        .context("Moj Sint preset has no numeric schema_version")?;
    let (name, model, voices, output_gain, instrument_volume, patch, values) = match version {
        7 | 8 => {
            let model = value
                .get("model")
                .and_then(toml::Value::as_str)
                .context("current Moj Sint preset has no model")?;
            match model {
                "model_d" => {
                    let document: MojPresetV5ModelD = toml::from_str(&source)?;
                    if document.schema_version != version as u32
                        || document.model != MojModel::ModelD
                    {
                        bail!("invalid current Model D identity");
                    }
                    (
                        document.name,
                        document.model,
                        document.voices,
                        document.output_gain,
                        document.instrument_volume,
                        MojPatch::ModelD(document.model_d_patch),
                        document.macros.values(),
                    )
                }
                "six_op_pm" => {
                    let document: MojPresetV5SixOp = toml::from_str(&source)?;
                    if document.schema_version != version as u32
                        || document.model != MojModel::SixOpPm
                    {
                        bail!("invalid current Six-Op PM identity");
                    }
                    (
                        document.name,
                        document.model,
                        document.voices,
                        document.output_gain,
                        document.instrument_volume,
                        MojPatch::SixOpPm(document.six_op_patch),
                        document.macros.values(),
                    )
                }
                "strange_oscillator" => {
                    let document: MojPresetV6Strange = toml::from_str(&source)?;
                    if document.schema_version != version as u32
                        || document.model != MojModel::StrangeOscillator
                    {
                        bail!("invalid current Strange Oscillator identity");
                    }
                    (
                        document.name,
                        document.model,
                        document.voices,
                        document.output_gain,
                        document.instrument_volume,
                        MojPatch::StrangeOscillator(document.strange_patch),
                        document.macros.values(),
                    )
                }
                "swarm_machine" => {
                    let document: MojPresetV7Swarm = toml::from_str(&source)?;
                    if document.schema_version != version as u32
                        || document.model != MojModel::SwarmMachine
                    {
                        bail!("invalid current Swarm Machine identity");
                    }
                    (
                        document.name,
                        document.model,
                        document.voices,
                        document.output_gain,
                        document.instrument_volume,
                        MojPatch::SwarmMachine(document.swarm_patch),
                        document.macros.values(),
                    )
                }
                "bass_matrix" => {
                    let document: MojPresetV7BassMatrix = toml::from_str(&source)?;
                    if document.schema_version != version as u32
                        || document.model != MojModel::BassMatrix
                    {
                        bail!("invalid current Bass Matrix identity");
                    }
                    (
                        document.name,
                        document.model,
                        document.voices,
                        document.output_gain,
                        document.instrument_volume,
                        MojPatch::BassMatrix(document.bass_matrix_patch),
                        document.macros.values(),
                    )
                }
                "dual_filter" if version == 8 => {
                    let document: MojPresetV8DualFilter = toml::from_str(&source)?;
                    if document.schema_version != 8 || document.model != MojModel::DualFilter {
                        bail!("invalid schema-8 Dual Filter identity");
                    }
                    (
                        document.name,
                        document.model,
                        document.voices,
                        document.output_gain,
                        document.instrument_volume,
                        MojPatch::DualFilter(document.dual_filter_core),
                        document.controls.values(),
                    )
                }
                _ => bail!("unknown current Moj Sint model"),
            }
        }
        6 => {
            let model = value
                .get("model")
                .and_then(toml::Value::as_str)
                .context("schema-6 Moj Sint preset has no model")?;
            match model {
                "model_d" => {
                    let document: MojPresetV5ModelD = toml::from_str(&source)?;
                    if document.schema_version != 6 || document.model != MojModel::ModelD {
                        bail!("invalid schema-6 Model D identity");
                    }
                    (
                        document.name,
                        document.model,
                        document.voices,
                        document.output_gain,
                        document.instrument_volume,
                        MojPatch::ModelD(document.model_d_patch),
                        document.macros.values(),
                    )
                }
                "six_op_pm" => {
                    let document: MojPresetV5SixOp = toml::from_str(&source)?;
                    if document.schema_version != 6 || document.model != MojModel::SixOpPm {
                        bail!("invalid schema-6 Six-Op PM identity");
                    }
                    (
                        document.name,
                        document.model,
                        document.voices,
                        document.output_gain,
                        document.instrument_volume,
                        MojPatch::SixOpPm(document.six_op_patch),
                        document.macros.values(),
                    )
                }
                "strange_oscillator" => {
                    let document: MojPresetV6Strange = toml::from_str(&source)?;
                    if document.schema_version != 6 || document.model != MojModel::StrangeOscillator
                    {
                        bail!("invalid schema-6 Strange Oscillator identity");
                    }
                    (
                        document.name,
                        document.model,
                        document.voices,
                        document.output_gain,
                        document.instrument_volume,
                        MojPatch::StrangeOscillator(document.strange_patch),
                        document.macros.values(),
                    )
                }
                _ => bail!("unknown schema-6 Moj Sint model"),
            }
        }
        5 => {
            let model = value
                .get("model")
                .and_then(toml::Value::as_str)
                .context("schema-5 Moj Sint preset has no model")?;
            match model {
                "model_d" => {
                    let document: MojPresetV5ModelD = toml::from_str(&source)?;
                    if document.schema_version != 5 || document.model != MojModel::ModelD {
                        bail!("invalid schema-5 Model D identity");
                    }
                    let _validated_patch = document.model_d_patch;
                    (
                        document.name,
                        document.model,
                        document.voices,
                        document.output_gain,
                        1.0,
                        MojPatch::ModelD(document.model_d_patch),
                        document.macros.values(),
                    )
                }
                "six_op_pm" => {
                    let document: MojPresetV5SixOp = toml::from_str(&source)?;
                    if document.schema_version != 5 || document.model != MojModel::SixOpPm {
                        bail!("invalid schema-5 Six-Op PM identity");
                    }
                    let _validated_patch = document.six_op_patch;
                    (
                        document.name,
                        document.model,
                        document.voices,
                        document.output_gain,
                        1.0,
                        MojPatch::SixOpPm(document.six_op_patch),
                        document.macros.values(),
                    )
                }
                _ => bail!("unknown schema-5 Moj Sint model"),
            }
        }
        4 => {
            let document: MojPresetV4 = toml::from_str(&source)?;
            if document.schema_version != 4 || document.model != MojModel::ModelD {
                bail!("invalid schema-4 Moj Sint model identity");
            }
            let _validated_patch = document.model_d_patch;
            (
                document.name,
                document.model,
                document.voices,
                document.output_gain,
                1.0,
                MojPatch::ModelD(document.model_d_patch),
                document.macros.values(),
            )
        }
        3 => {
            let document: MojPresetV3 = toml::from_str(&source)?;
            debug_assert_eq!(document.schema_version, 3);
            let _validated_patch = document.model_d_patch;
            (
                document.name,
                MojModel::ModelD,
                document.voices,
                document.output_gain,
                1.0,
                MojPatch::ModelD(document.model_d_patch),
                document.macros.values(),
            )
        }
        2 => {
            let document: MojPresetV2 = toml::from_str(&source)?;
            debug_assert_eq!(document.schema_version, 2);
            (
                document.name,
                MojModel::ModelD,
                document.voices,
                document.output_gain,
                1.0,
                MojPatch::ModelD(MojModelDPatch::Bass),
                document.macros.values(),
            )
        }
        1 => {
            let document: MojPresetV1 = toml::from_str(&source)?;
            if document.schema_version != 1
                || !document.envelope.attack_seconds.is_finite()
                || document.envelope.attack_seconds <= 0.0
                || !document.envelope.decay_seconds.is_finite()
                || document.envelope.decay_seconds <= 0.0
                || !document.envelope.release_seconds.is_finite()
                || document.envelope.release_seconds <= 0.0
                || !document.envelope.sustain_level.is_finite()
                || !(0.0..=1.0).contains(&document.envelope.sustain_level)
            {
                bail!("invalid version-1 Moj Sint envelope");
            }
            let _legacy = (
                document.envelope.attack_seconds,
                document.envelope.decay_seconds,
                document.envelope.sustain_level,
                document.envelope.release_seconds,
                document.macros.width,
            );
            (
                document.name,
                MojModel::ModelD,
                document.voices,
                document.output_gain,
                1.0,
                MojPatch::ModelD(MojModelDPatch::Bass),
                MojMacrosV2 {
                    evolve: document.macros.evolve,
                    shape: document.macros.shape,
                    color: document.macros.color,
                    edge: document.macros.edge,
                    couple: document.macros.couple,
                    motion: document.macros.motion,
                    depth: document.macros.depth,
                    space: document.macros.space,
                    attack: document.macros.attack,
                    decay: document.macros.decay,
                    sustain: document.macros.sustain,
                    release: document.macros.release,
                }
                .values(),
            )
        }
        unsupported => bail!("unsupported Moj Sint preset schema {unsupported}"),
    };
    let _ = value;
    if name.trim().is_empty()
        || !(1..=64).contains(&voices)
        || !output_gain.is_finite()
        || !(0.0..=1.0).contains(&output_gain)
        || !instrument_volume.is_finite()
        || !(0.0..=1.0).contains(&instrument_volume)
        || values
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        bail!("invalid bounded Moj Sint preset {}", path.display());
    }
    Ok(MojDocument {
        name,
        model,
        voices,
        output_gain,
        instrument_volume,
        patch,
        values,
    })
}

fn read_moj_sint(path: &Path) -> Result<(String, MojModel, HashMap<u8, f32>)> {
    let document = read_moj_document(path)?;
    let mut values: HashMap<u8, f32> = crate::control::moj_controls(document.model)
        .iter()
        .enumerate()
        .map(|(index, control)| {
            let value = if control.cc == 7 {
                document.instrument_volume
            } else {
                document.values[index]
            };
            (control.cc, value)
        })
        .collect();
    if let MojPatch::DualFilter(core) = document.patch {
        values.insert(
            crate::control::MOJ_CORE_STATE_CC,
            match core {
                MojDualFilterCore::Industrial => 0.0,
                MojDualFilterCore::Counter => 1.0,
            },
        );
    }
    Ok((document.name, document.model, values))
}

fn catalog(
    backend: BackendKind,
    executable_exists: bool,
    discovered: Result<Vec<Preset>>,
    command: &str,
) -> Catalog {
    let (presets, unavailable) = match discovered {
        Ok(presets) if !executable_exists => (
            presets,
            Some(format!(
                "{} executable not found: {command}",
                backend.label()
            )),
        ),
        Ok(presets) if presets.is_empty() => (
            presets,
            Some(format!("no configured {} sounds found", backend.label())),
        ),
        Ok(presets) => (presets, None),
        Err(error) => (Vec::new(), Some(format!("{error:#}"))),
    };
    Catalog {
        backend,
        presets,
        unavailable,
    }
}

fn command_exists(program: &str) -> bool {
    crate::fsutil::command_exists(program)
}

fn discover_synthv1_roots(roots: &[PathBuf]) -> Result<Vec<Preset>> {
    let mut presets = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();
    for dir in roots {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error).with_context(|| format!("read {}", dir.display())),
        };
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            if extension_is(&path, "synthv1") && seen_paths.insert(path.clone()) {
                let name = file_stem(&path);
                presets.push(Preset::synthv1(name, path));
            }
        }
    }
    sort_presets(&mut presets);
    Ok(presets)
}

pub fn discover_yoshimi(
    roots: &[PathBuf],
    categories: &[String],
    per_category: usize,
) -> Result<Vec<Preset>> {
    let mut grouped: BTreeMap<String, Vec<Preset>> = BTreeMap::new();
    for root in roots.iter().filter(|root| root.is_dir()) {
        let mut files = Vec::new();
        recursive_files(root, "xiz", &mut files)?;
        for path in files {
            let searchable = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_ascii_lowercase();
            let Some(category) = categories
                .iter()
                .find(|category| category_matches(category, &searchable))
                .cloned()
            else {
                continue;
            };
            grouped.entry(category.clone()).or_default().push(Preset {
                backend: BackendKind::Yoshimi,
                name: clean_numbered_name(&file_stem(&path)),
                category: Some(title_case(&category)),
                id: PresetId::Yoshimi { path },
            });
        }
    }
    let mut presets = Vec::new();
    for (_, mut group) in grouped {
        sort_presets(&mut group);
        group.truncate(per_category);
        presets.extend(group);
    }
    sort_presets(&mut presets);
    Ok(presets)
}

fn category_matches(category: &str, text: &str) -> bool {
    match category {
        "bass" => ["bass", "sub", "acid"]
            .iter()
            .any(|word| text.contains(word)),
        "lead" => ["lead", "solo", "saw"]
            .iter()
            .any(|word| text.contains(word)),
        "pad" => ["pad", "string", "choir"]
            .iter()
            .any(|word| text.contains(word)),
        "pluck" => ["pluck", "harp", "guitar"]
            .iter()
            .any(|word| text.contains(word)),
        "bell" => ["bell", "tine", "mallet"]
            .iter()
            .any(|word| text.contains(word)),
        "organ" => text.contains("organ"),
        "drone" => ["drone", "atmos", "ambient"]
            .iter()
            .any(|word| text.contains(word)),
        "keys" => ["piano", "rhodes", "keys"]
            .iter()
            .any(|word| text.contains(word)),
        other => text.contains(other),
    }
}

pub fn discover_fluidsynth(soundfonts: &[PathBuf]) -> Result<Vec<Preset>> {
    let mut presets = Vec::new();
    let mut valid_fonts = 0usize;
    let mut failures = Vec::new();
    for path in soundfonts.iter().filter(|path| path.is_file()) {
        if valid_fonts > u8::MAX as usize {
            break;
        }
        let font_name = file_stem(path);
        let programs = match soundfont_presets(path) {
            Ok(programs) => programs,
            Err(error) => {
                failures.push(format!("{error:#}"));
                continue;
            }
        };
        let index = valid_fonts as u8;
        valid_fonts += 1;
        for program in programs {
            presets.push(Preset {
                backend: BackendKind::FluidSynth,
                name: program.name,
                category: Some(format!(
                    "{font_name} {:03}:{:03}",
                    program.bank, program.program
                )),
                id: PresetId::FluidSynth {
                    soundfont: path.clone(),
                    soundfont_index: index,
                    bank: program.bank,
                    program: program.program,
                },
            });
        }
    }
    if valid_fonts == 0 && !failures.is_empty() {
        bail!("no valid configured SoundFonts: {}", failures.join("; "));
    }
    presets.sort_by(|a, b| match (&a.id, &b.id) {
        (
            PresetId::FluidSynth {
                soundfont_index: ai,
                bank: ab,
                program: ap,
                ..
            },
            PresetId::FluidSynth {
                soundfont_index: bi,
                bank: bb,
                program: bp,
                ..
            },
        ) => (ai, ab, ap).cmp(&(bi, bb, bp)),
        _ => a.name.cmp(&b.name),
    });
    Ok(presets)
}

#[derive(Debug, Eq, PartialEq)]
struct SoundFontProgram {
    name: String,
    bank: u16,
    program: u8,
}

fn soundfont_presets(path: &Path) -> Result<Vec<SoundFontProgram>> {
    const MAX_PRESET_TABLE_BYTES: u64 = 16 * 1024 * 1024;
    let mut file =
        File::open(path).with_context(|| format!("read SoundFont {}", path.display()))?;
    let file_len = file.metadata()?.len();
    let mut header = [0u8; 12];
    file.read_exact(&mut header)
        .with_context(|| format!("read SoundFont header {}", path.display()))?;
    if &header[..4] != b"RIFF" || &header[8..12] != b"sfbk" {
        bail!("{} is not an SF2/SF3 SoundFont", path.display());
    }
    let riff_end = u64::from(le_u32(&header[4..8]))
        .checked_add(8)
        .context("SoundFont RIFF length overflow")?;
    if riff_end < 12 || riff_end > file_len {
        bail!("{} has a truncated RIFF container", path.display());
    }
    let mut phdr = None::<Vec<u8>>;
    let mut offset = 12u64;
    while offset + 8 <= riff_end {
        let (id, size) = read_chunk_header(&mut file, offset)?;
        let start = offset + 8;
        let end = start
            .checked_add(size)
            .context("SoundFont chunk length overflow")?;
        if end > riff_end {
            bail!("{} has a truncated RIFF chunk", path.display());
        }
        if &id == b"LIST" && size >= 4 {
            let mut inner = start + 4;
            while inner + 8 <= end {
                let (inner_id, inner_size) = read_chunk_header(&mut file, inner)?;
                let data = inner + 8;
                let data_end = data
                    .checked_add(inner_size)
                    .context("SoundFont subchunk length overflow")?;
                if data_end > end {
                    bail!("{} has a truncated RIFF subchunk", path.display());
                }
                if &inner_id == b"phdr" {
                    if inner_size > MAX_PRESET_TABLE_BYTES {
                        bail!("{} has an oversized preset table", path.display());
                    }
                    let mut table = vec![0; inner_size as usize];
                    file.seek(SeekFrom::Start(data))?;
                    file.read_exact(&mut table)?;
                    phdr = Some(table);
                    break;
                }
                inner = padded_chunk_end(data_end, inner_size)?;
            }
        }
        if phdr.is_some() {
            break;
        }
        offset = padded_chunk_end(end, size)?;
    }
    let phdr =
        phdr.with_context(|| format!("{} has no SoundFont preset headers", path.display()))?;
    if phdr.len() < 38 || phdr.len() % 38 != 0 {
        bail!("{} has a malformed SoundFont preset table", path.display());
    }
    let count = phdr.len() / 38;
    let mut programs = Vec::new();
    for record in phdr.chunks_exact(38).take(count.saturating_sub(1)) {
        let name = nul_string(&record[..20]);
        let program = u16::from_le_bytes([record[20], record[21]]);
        let bank = u16::from_le_bytes([record[22], record[23]]);
        if program <= 127 {
            programs.push(SoundFontProgram {
                name,
                bank,
                program: program as u8,
            });
        }
    }
    Ok(programs)
}

pub(crate) fn soundfont_offsets(soundfonts: &[PathBuf]) -> Result<Vec<(PathBuf, u16)>> {
    let mut fonts = Vec::new();
    let mut next_offset = 0u32;
    for path in soundfonts.iter().filter(|path| path.is_file()) {
        let Ok(programs) = soundfont_presets(path) else {
            continue;
        };
        let max_bank = programs.iter().map(|preset| preset.bank).max().unwrap_or(0);
        if next_offset + u32::from(max_bank) > 16_383 {
            bail!("configured SoundFont banks exceed the 14-bit MIDI bank range");
        }
        fonts.push((path.clone(), next_offset as u16));
        next_offset += u32::from(max_bank) + 1;
    }
    Ok(fonts)
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().unwrap())
}

fn read_chunk_header(file: &mut File, offset: u64) -> Result<([u8; 4], u64)> {
    let mut header = [0u8; 8];
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut header)?;
    Ok((
        header[..4].try_into().unwrap(),
        u64::from(le_u32(&header[4..])),
    ))
}

fn padded_chunk_end(end: u64, size: u64) -> Result<u64> {
    end.checked_add(size & 1)
        .context("SoundFont chunk padding overflow")
}

fn nul_string(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_owned()
}

fn recursive_files(root: &Path, extension: &str, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("read {}", root.display()))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            recursive_files(&path, extension, files)?;
        } else if (file_type.is_file() || (file_type.is_symlink() && path.is_file()))
            && extension_is(&path, extension)
        {
            files.push(path);
        }
    }
    Ok(())
}

fn extension_is(path: &Path, wanted: &str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(wanted))
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn clean_numbered_name(name: &str) -> String {
    name.trim_start_matches(|character: char| character.is_ascii_digit())
        .trim_start_matches(['-', '_', ' '])
        .replace(['_', '-'], " ")
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

pub(crate) fn sort_presets(presets: &mut [Preset]) {
    presets.sort_by_key(|preset| {
        (
            preset.moj_model(),
            preset
                .category
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase(),
            preset.name.to_ascii_lowercase(),
            preset.id.clone(),
        )
    });
}

pub fn values(preset: &Preset) -> Result<HashMap<u8, f32>> {
    if let PresetId::MojSint { path, .. } = &preset.id {
        return read_moj_sint(path).map(|(_, _, values)| values);
    }
    if matches!(
        preset.backend,
        BackendKind::Yoshimi | BackendKind::FluidSynth | BackendKind::ShrSampler
    ) {
        return Ok(HashMap::from([(crate::control::INSTRUMENT_VOLUME_CC, 1.0)]));
    }
    let PresetId::Synthv1 { path } = &preset.id else {
        return Ok(HashMap::new());
    };
    let mut out = defaults();
    let mut reader =
        Reader::from_file(path).with_context(|| format!("parse {}", path.display()))?;
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut wanted = None;
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) if e.name().as_ref() == b"param" => {
                let mut name = None;
                for attribute in e.attributes() {
                    let attribute = attribute?;
                    if attribute.key.as_ref() == b"name" {
                        name = Some(
                            attribute
                                .normalized_value(XmlVersion::Implicit1_0)?
                                .into_owned(),
                        );
                    }
                }
                wanted = name
                    .as_deref()
                    .and_then(|name| CONTROLS.iter().find(|c| c.xml_name == name).map(|c| c.cc));
            }
            Event::Text(e) if wanted.is_some() => {
                let cc = wanted.take().context("mapped preset parameter missing")?;
                let decoded = e.xml_content(XmlVersion::Implicit1_0)?;
                let value = quick_xml::escape::unescape(&decoded)?
                    .parse::<f32>()
                    .with_context(|| format!("mapped preset CC {cc} is not numeric"))?;
                let control = crate::control::by_cc(cc).context("unknown mapped preset CC")?;
                if !value.is_finite() || !(control.min..=control.max).contains(&value) {
                    bail!(
                        "preset parameter {} must be {}..={}",
                        control.xml_name,
                        control.min,
                        control.max
                    );
                }
                out.insert(cc, value);
            }
            Event::End(e) if e.name().as_ref() == b"param" => wanted = None,
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

pub fn moj_model(path: &Path) -> Result<MojModel> {
    read_moj_sint(path).map(|(_, model, _)| model)
}

const MAX_SYNTHV1_PRESET_BYTES: u64 = 4 * 1_048_576;

pub fn user_preset_can_overwrite(storage: &UserPresetStorage, preset: &Preset) -> bool {
    user_owned_preset_path(storage, preset).is_ok()
}

pub fn next_user_preset_name(
    storage: &UserPresetStorage,
    source: &Preset,
    catalog: &[Preset],
) -> Result<String> {
    let directory = user_destination_directory(storage, source)?;
    let extension = user_preset_extension(source.backend)?;
    for number in 1..=999_999_u32 {
        let name = format!("User {number:03}");
        let route_used = catalog.iter().any(|preset| {
            preset.backend == source.backend
                && preset.moj_model() == source.moj_model()
                && preset.name == name
        });
        let path_used = fs::symlink_metadata(directory.join(format!("{name}.{extension}"))).is_ok();
        if !route_used && !path_used {
            return Ok(name);
        }
    }
    bail!("user preset numbering is exhausted")
}

pub fn save_new_user_preset(
    storage: &UserPresetStorage,
    source: &Preset,
    current_values: &HashMap<u8, f32>,
    catalog: &[Preset],
) -> Result<Preset> {
    if source.backend == BackendKind::MojSint && catalog.len() >= MAX_MOJ_PRESETS {
        bail!("Moj Sint private catalog is full")
    }
    let directory = user_destination_directory(storage, source)?;
    let root = storage
        .root_for(source.backend)
        .context("this preset backend has no private storage")?;
    prepare_private_directory(root, &directory)?;
    let extension = user_preset_extension(source.backend)?;
    for _ in 0..1_024 {
        let name = next_user_preset_name(storage, source, catalog)?;
        let path = directory.join(format!("{name}.{extension}"));
        let encoded = serialize_user_preset(source, &name, current_values)?;
        match crate::fsutil::atomic_write_noreplace(&path, &encoded) {
            Ok(()) => return saved_preset_from_path(source.backend, source.moj_model(), path),
            Err(_error) if fs::symlink_metadata(&path).is_ok() => continue,
            Err(error) => return Err(error).context("publish new private preset"),
        }
    }
    bail!("could not allocate a collision-free user preset")
}

pub fn overwrite_user_preset(
    storage: &UserPresetStorage,
    source: &Preset,
    current_values: &HashMap<u8, f32>,
) -> Result<Preset> {
    let path = user_owned_preset_path(storage, source)?;
    let encoded = serialize_user_preset(source, &source.name, current_values)?;
    // Serialization and strict validation finish before the atomic replacement,
    // so the prior valid file remains intact on every earlier failure.
    crate::fsutil::atomic_write(&path, &encoded).context("replace private preset")?;
    saved_preset_from_path(source.backend, source.moj_model(), path)
}

fn user_preset_extension(backend: BackendKind) -> Result<&'static str> {
    match backend {
        BackendKind::Synthv1 => Ok("synthv1"),
        BackendKind::MojSint => Ok("mojsint"),
        BackendKind::Yoshimi | BackendKind::FluidSynth | BackendKind::ShrSampler => {
            bail!("{} presets are not editable", backend.label())
        }
    }
}

fn user_destination_directory(storage: &UserPresetStorage, source: &Preset) -> Result<PathBuf> {
    let root = storage
        .root_for(source.backend)
        .context("this preset backend has no private storage")?;
    validate_private_root(root)?;
    let directory = match source.moj_model() {
        Some(model) => root.join(model.stable_id()),
        None => root.to_path_buf(),
    };
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            bail!("private preset destination is not a regular directory")
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("inspect private preset destination"),
    }
    Ok(directory)
}

fn validate_private_root(root: &Path) -> Result<()> {
    if !root.is_absolute()
        || root
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        bail!("private preset root must be an absolute normalized path");
    }
    let mut candidate = PathBuf::new();
    for component in root.components() {
        candidate.push(component.as_os_str());
        match fs::symlink_metadata(&candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!("private preset root path must not contain symlinks")
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error).context("inspect private preset root"),
        }
    }
    if let Some(worktree) = root
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
    {
        let relative = root.strip_prefix(worktree)?;
        if relative.components().next().map(Component::as_os_str) != Some("user".as_ref()) {
            bail!("refusing to write presets into a public source checkout");
        }
    }
    Ok(())
}

fn prepare_private_directory(root: &Path, directory: &Path) -> Result<()> {
    validate_private_root(root)?;
    fs::create_dir_all(directory)
        .with_context(|| format!("create private preset directory {}", directory.display()))?;
    for path in [root, directory] {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            bail!("private preset destination is not a regular directory");
        }
    }
    Ok(())
}

fn user_owned_preset_path(storage: &UserPresetStorage, preset: &Preset) -> Result<PathBuf> {
    if !is_numbered_user_name(&preset.name) {
        bail!("only private numbered user presets may be overwritten")
    }
    let root = storage
        .root_for(preset.backend)
        .context("this preset backend has no private storage")?;
    validate_private_root(root)?;
    let path = match &preset.id {
        PresetId::Synthv1 { path } | PresetId::MojSint { path, .. } => path,
        PresetId::Yoshimi { .. } | PresetId::FluidSynth { .. } | PresetId::ShrSampler { .. } => {
            bail!("this preset backend is not editable")
        }
    };
    let metadata = fs::symlink_metadata(path).context("inspect private preset")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("private preset must be a regular non-symlink file");
    }
    let canonical_root = root.canonicalize().context("resolve private preset root")?;
    let canonical_path = path.canonicalize().context("resolve private preset")?;
    let relative = path
        .strip_prefix(root)
        .context("preset is outside private user storage")?;
    let mut candidate = root.to_path_buf();
    for component in relative.components() {
        candidate.push(component.as_os_str());
        if fs::symlink_metadata(&candidate).is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            bail!("private preset path must not contain symlinks")
        }
    }
    if !canonical_path.starts_with(&canonical_root)
        || canonical_path.extension().and_then(|value| value.to_str())
            != Some(user_preset_extension(preset.backend)?)
        || path.file_stem().and_then(|value| value.to_str()) != Some(preset.name.as_str())
    {
        bail!("preset is outside private user storage")
    }
    Ok(path.clone())
}

fn is_numbered_user_name(name: &str) -> bool {
    name.strip_prefix("User ").is_some_and(|number| {
        number.len() >= 3
            && number.bytes().all(|byte| byte.is_ascii_digit())
            && number.bytes().any(|byte| byte != b'0')
    })
}

fn saved_preset_from_path(
    backend: BackendKind,
    expected_model: Option<MojModel>,
    path: PathBuf,
) -> Result<Preset> {
    match backend {
        BackendKind::Synthv1 => {
            let preset = Preset::synthv1(file_stem(&path), path);
            values(&preset)?;
            Ok(preset)
        }
        BackendKind::MojSint => {
            let (name, model, _) = read_moj_sint(&path)?;
            if Some(model) != expected_model {
                bail!("saved Moj Sint model identity changed")
            }
            Ok(Preset {
                backend,
                name,
                category: Some(model.label().into()),
                id: PresetId::MojSint { model, path },
            })
        }
        BackendKind::Yoshimi | BackendKind::FluidSynth | BackendKind::ShrSampler => {
            bail!("{} presets are not editable", backend.label())
        }
    }
}

fn serialize_user_preset(
    source: &Preset,
    name: &str,
    current_values: &HashMap<u8, f32>,
) -> Result<Vec<u8>> {
    match &source.id {
        PresetId::Synthv1 { path } => serialize_synthv1(path, name, current_values),
        PresetId::MojSint { model, path } => serialize_moj_sint(path, *model, name, current_values),
        PresetId::Yoshimi { .. } | PresetId::FluidSynth { .. } | PresetId::ShrSampler { .. } => {
            bail!("{} presets are not editable", source.backend.label())
        }
    }
}

fn serialize_synthv1(
    path: &Path,
    name: &str,
    current_values: &HashMap<u8, f32>,
) -> Result<Vec<u8>> {
    let source = read_regular_bounded(path, MAX_SYNTHV1_PRESET_BYTES, "synthv1 preset")?;
    let mut replacement = HashMap::new();
    for control in CONTROLS {
        let value = current_values
            .get(&control.cc)
            .copied()
            .with_context(|| format!("missing mapped synthv1 CC {}", control.cc))?;
        if !value.is_finite() || !(control.min..=control.max).contains(&value) {
            bail!("mapped synthv1 CC {} is outside its safe range", control.cc)
        }
        replacement.insert(control.xml_name, value);
    }

    let mut reader = Reader::from_reader(source.as_slice());
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(source.len()));
    let mut buffer = Vec::new();
    let mut active_parameter = None;
    let mut found = std::collections::HashSet::new();
    let mut written = std::collections::HashSet::new();
    let mut saw_preset = false;
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(element) => {
                let is_preset = element.name().as_ref() == b"preset";
                let is_parameter = element.name().as_ref() == b"param";
                if is_parameter {
                    let mut parameter_name = None;
                    for attribute in element.attributes() {
                        let attribute = attribute?;
                        if attribute.key.as_ref() == b"name" {
                            if parameter_name.is_some() {
                                bail!("synthv1 parameter has duplicate name attributes")
                            }
                            parameter_name = Some(
                                attribute
                                    .normalized_value(XmlVersion::Implicit1_0)?
                                    .into_owned(),
                            );
                        }
                    }
                    active_parameter = parameter_name
                        .as_deref()
                        .and_then(|value| replacement.get_key_value(value))
                        .map(|(key, value)| (*key, *value));
                    if let Some((key, _)) = active_parameter {
                        if !found.insert(key) {
                            bail!("synthv1 preset repeats mapped parameter {key}")
                        }
                    }
                }
                let mut element = element.into_owned();
                if is_preset {
                    if saw_preset {
                        bail!("synthv1 document has multiple preset roots")
                    }
                    saw_preset = true;
                    element = replace_xml_name_attribute(element, name)?;
                }
                writer.write_event(Event::Start(element))?;
            }
            Event::Text(_text) if active_parameter.is_some() => {
                let (key, value) = active_parameter.take().expect("checked mapped parameter");
                written.insert(key);
                writer.write_event(Event::Text(BytesText::new(&value.to_string())))?;
            }
            Event::End(element) => {
                if element.name().as_ref() == b"param" {
                    active_parameter = None;
                }
                writer.write_event(Event::End(element.into_owned()))?;
            }
            Event::Eof => break,
            event => writer.write_event(event.into_owned())?,
        }
        buffer.clear();
    }
    if !saw_preset || found.len() != CONTROLS.len() || written.len() != CONTROLS.len() {
        bail!("synthv1 preset is missing required mapped parameters")
    }
    let encoded = writer.into_inner();
    validate_xml_document(&encoded)?;
    Ok(encoded)
}

fn replace_xml_name_attribute(
    mut element: BytesStart<'static>,
    name: &str,
) -> Result<BytesStart<'static>> {
    let attributes = element
        .attributes()
        .map(|attribute| {
            let attribute = attribute?;
            Ok((
                attribute.key.as_ref().to_vec(),
                attribute.value.into_owned(),
            ))
        })
        .collect::<std::result::Result<Vec<_>, quick_xml::events::attributes::AttrError>>()?;
    element.clear_attributes();
    let escaped_name = quick_xml::escape::escape(name);
    let mut saw_name = false;
    for (key, value) in &attributes {
        if key == b"name" {
            if saw_name {
                bail!("synthv1 preset has duplicate name attributes")
            }
            saw_name = true;
            element.push_attribute((key.as_slice(), escaped_name.as_bytes()));
        } else {
            element.push_attribute((key.as_slice(), value.as_slice()));
        }
    }
    if !saw_name {
        element.push_attribute(("name", escaped_name.as_ref()));
    }
    Ok(element)
}

fn validate_xml_document(source: &[u8]) -> Result<()> {
    let mut reader = Reader::from_reader(source);
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Eof => return Ok(()),
            _ => buffer.clear(),
        }
    }
}

fn serialize_moj_sint(
    path: &Path,
    expected_model: MojModel,
    name: &str,
    current_values: &HashMap<u8, f32>,
) -> Result<Vec<u8>> {
    let document = read_moj_document(path)?;
    if document.model != expected_model {
        bail!("Moj Sint route model does not match its preset")
    }
    let mut values = document.values;
    let instrument_volume = current_values
        .get(&7)
        .copied()
        .unwrap_or(document.instrument_volume);
    if !instrument_volume.is_finite() || !(0.0..=1.0).contains(&instrument_volume) {
        bail!("mapped instrument volume is outside 0..=1")
    }
    for (index, control) in crate::control::moj_controls(expected_model)
        .iter()
        .enumerate()
    {
        if control.cc == 7 {
            continue;
        }
        let value = current_values
            .get(&control.cc)
            .copied()
            .with_context(|| format!("missing mapped Moj Sint CC {}", control.cc))?;
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            bail!("mapped Moj Sint CC {} is outside 0..=1", control.cc)
        }
        values[index] = value;
    }
    let encoded = match (expected_model, document.patch) {
        (MojModel::ModelD, MojPatch::ModelD(model_d_patch)) => {
            toml::to_string_pretty(&MojPresetV5ModelD {
                schema_version: 8,
                name: name.into(),
                voices: document.voices,
                output_gain: document.output_gain,
                instrument_volume,
                model: MojModel::ModelD,
                model_d_patch,
                macros: MojMacrosV2::from_values(values),
            })?
        }
        (MojModel::SixOpPm, MojPatch::SixOpPm(six_op_patch)) => {
            toml::to_string_pretty(&MojPresetV5SixOp {
                schema_version: 8,
                name: name.into(),
                voices: document.voices,
                output_gain: document.output_gain,
                instrument_volume,
                model: MojModel::SixOpPm,
                six_op_patch,
                macros: MojMacrosSixOp::from_values(values),
            })?
        }
        (MojModel::StrangeOscillator, MojPatch::StrangeOscillator(strange_patch)) => {
            toml::to_string_pretty(&MojPresetV6Strange {
                schema_version: 8,
                name: name.into(),
                voices: document.voices,
                output_gain: document.output_gain,
                instrument_volume,
                model: MojModel::StrangeOscillator,
                strange_patch,
                macros: MojMacrosStrange::from_values(values),
            })?
        }
        (MojModel::SwarmMachine, MojPatch::SwarmMachine(swarm_patch)) => {
            toml::to_string_pretty(&MojPresetV7Swarm {
                schema_version: 8,
                name: name.into(),
                voices: document.voices,
                output_gain: document.output_gain,
                instrument_volume,
                model: MojModel::SwarmMachine,
                swarm_patch,
                macros: MojMacrosSwarm::from_values(values),
            })?
        }
        (MojModel::BassMatrix, MojPatch::BassMatrix(bass_matrix_patch)) => {
            toml::to_string_pretty(&MojPresetV7BassMatrix {
                schema_version: 8,
                name: name.into(),
                voices: document.voices,
                output_gain: document.output_gain,
                instrument_volume,
                model: MojModel::BassMatrix,
                bass_matrix_patch,
                macros: MojMacrosBassMatrix::from_values(values),
            })?
        }
        (MojModel::DualFilter, MojPatch::DualFilter(_)) => {
            let dual_filter_core = if current_values
                .get(&crate::control::MOJ_CORE_STATE_CC)
                .copied()
                .unwrap_or(0.0)
                >= 0.5
            {
                MojDualFilterCore::Counter
            } else {
                MojDualFilterCore::Industrial
            };
            toml::to_string_pretty(&MojPresetV8DualFilter {
                schema_version: 8,
                name: name.into(),
                voices: document.voices,
                output_gain: document.output_gain,
                instrument_volume,
                model: MojModel::DualFilter,
                dual_filter_core,
                controls: MojDualFilterControls::from_values(values),
            })?
        }
        _ => bail!("Moj Sint model and patch identity do not match"),
    };
    // Round-trip through the same strict schema before publication.
    validate_moj_source(&encoded, expected_model)?;
    Ok(encoded.into_bytes())
}

impl MojMacrosV2 {
    fn from_values(values: [f32; 15]) -> Self {
        Self {
            evolve: values[0],
            shape: values[1],
            color: values[2],
            edge: values[3],
            couple: values[4],
            motion: values[5],
            depth: values[6],
            space: values[7],
            attack: values[8],
            decay: values[9],
            sustain: values[10],
            release: values[11],
        }
    }
}

impl MojMacrosSixOp {
    fn from_values(values: [f32; 15]) -> Self {
        Self {
            index: values[0],
            ratio: values[1],
            feedback: values[2],
            operator_decay: values[3],
            balance: values[4],
            key_scale: values[5],
            velocity: values[6],
            motion: values[7],
            attack: values[8],
            decay: values[9],
            sustain: values[10],
            release: values[11],
        }
    }
}

impl MojMacrosStrange {
    fn from_values(values: [f32; 15]) -> Self {
        Self {
            type_: values[0],
            form: values[1],
            warp: values[2],
            couple: values[3],
            motion: values[4],
            chaos: values[5],
            color: values[6],
            space: values[7],
            attack: values[8],
            decay: values[9],
            sustain: values[10],
            release: values[11],
        }
    }
}

impl MojMacrosSwarm {
    fn from_values(values: [f32; 15]) -> Self {
        Self {
            mass: values[0],
            detune: values[1],
            spread: values[2],
            shape: values[3],
            bite: values[4],
            motion: values[5],
            color: values[6],
            space: values[7],
            attack: values[8],
            decay: values[9],
            sustain: values[10],
            release: values[11],
        }
    }
}

impl MojMacrosBassMatrix {
    fn from_values(values: [f32; 15]) -> Self {
        Self {
            body: values[0],
            growl: values[1],
            metal: values[2],
            punch: values[3],
            character: values[4],
            drive: values[5],
            filter: values[6],
            unstable: values[7],
            attack: values[8],
            decay: values[9],
            sustain: values[10],
            release: values[11],
        }
    }
}

impl MojDualFilterControls {
    fn from_values(values: [f32; 15]) -> Self {
        Self {
            filter_a_cutoff: values[0],
            filter_a_resonance: values[1],
            filter_a_envelope_depth: values[2],
            filter_b_cutoff: values[3],
            filter_b_resonance: values[4],
            filter_b_envelope_depth: values[5],
            structure: values[6],
            filter_attack: values[7],
            filter_decay: values[8],
            filter_sustain: values[9],
            filter_release: values[10],
            amp_attack: values[11],
            amp_decay: values[12],
            amp_sustain: values[13],
            amp_release: values[14],
        }
    }
}

fn validate_moj_source(source: &str, expected_model: MojModel) -> Result<()> {
    let value: toml::Value = toml::from_str(source)?;
    if value
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        != Some(8)
    {
        bail!("saved Moj Sint preset is not schema 8")
    }
    match expected_model {
        MojModel::ModelD => {
            let document: MojPresetV5ModelD = toml::from_str(source)?;
            if document.model != MojModel::ModelD {
                bail!("saved Moj Sint Model D identity is invalid")
            }
        }
        MojModel::SixOpPm => {
            let document: MojPresetV5SixOp = toml::from_str(source)?;
            if document.model != MojModel::SixOpPm {
                bail!("saved Moj Sint Six-Op identity is invalid")
            }
        }
        MojModel::StrangeOscillator => {
            let document: MojPresetV6Strange = toml::from_str(source)?;
            if document.model != MojModel::StrangeOscillator {
                bail!("saved Moj Sint Strange Oscillator identity is invalid")
            }
        }
        MojModel::SwarmMachine => {
            let document: MojPresetV7Swarm = toml::from_str(source)?;
            if document.model != MojModel::SwarmMachine {
                bail!("saved Moj Sint Swarm identity is invalid")
            }
        }
        MojModel::BassMatrix => {
            let document: MojPresetV7BassMatrix = toml::from_str(source)?;
            if document.model != MojModel::BassMatrix {
                bail!("saved Moj Sint Bass Matrix identity is invalid")
            }
        }
        MojModel::DualFilter => {
            let document: MojPresetV8DualFilter = toml::from_str(source)?;
            if document.model != MojModel::DualFilter {
                bail!("saved Moj Sint Dual Filter identity is invalid")
            }
        }
    }
    Ok(())
}

fn read_regular_bounded(path: &Path, maximum: u64, label: &str) -> Result<Vec<u8>> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("open {label}"))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > maximum {
        bail!("{label} must be a regular file within its size limit")
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(maximum + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        bail!("{label} exceeds its size limit")
    }
    Ok(bytes)
}

pub fn resolve<'a>(presets: &'a [Preset], arg: &str) -> Option<&'a Preset> {
    if let Some(number) = arg
        .strip_prefix("preset_")
        .and_then(|value| value.parse::<usize>().ok())
    {
        return number.checked_sub(1).and_then(|index| presets.get(index));
    }
    presets.iter().find(|preset| {
        preset.name.eq_ignore_ascii_case(arg)
            || format!("{}:{}", preset.backend.label(), preset.name).eq_ignore_ascii_case(arg)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_legacy_xml_by_name_not_obsolete_index() {
        let path = std::env::temp_dir().join(format!(
            "shsynth-legacy-preset-{}.synthv1",
            std::process::id()
        ));
        fs::write(
            &path,
            r#"<preset><params>
                <param index="999" name="DCF1_CUTOFF">0.19</param>
                <param index="0" name="DEL1_WET">1.0</param>
            </params></preset>"#,
        )
        .unwrap();
        let preset = Preset::synthv1("Legacy fixture", path.clone());
        let values = values(&preset).unwrap();
        assert!((values[&74] - 0.19).abs() < 0.0001);
        assert!((values[&18] - 1.0).abs() < 0.0001);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mapped_preset_values_must_be_finite_and_in_range() {
        let path = std::env::temp_dir().join(format!(
            "shsynth-invalid-value-{}.synthv1",
            std::process::id()
        ));
        let preset = Preset::synthv1("Invalid fixture", path.clone());
        for value in ["NaN", "1.5", "not-a-number"] {
            fs::write(
                &path,
                format!(
                    "<preset><params><param name=\"DCF1_CUTOFF\">{value}</param></params></preset>"
                ),
            )
            .unwrap();
            assert!(values(&preset).is_err(), "accepted {value}");
        }
        fs::write(
            &path,
            r#"<preset><params><param name="DCF1_CUTOFF" name="DEL1_WET">0.5</param></params></preset>"#,
        )
        .unwrap();
        assert!(values(&preset).is_err(), "accepted duplicate attributes");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn engine_cycle_wraps_in_both_directions() {
        assert_eq!(BackendKind::ALL[0], BackendKind::MojSint);
        assert_eq!(BackendKind::MojSint.next(-1), BackendKind::FluidSynth);
        assert_eq!(BackendKind::MojSint.next(1), BackendKind::ShrSampler);
        assert_eq!(BackendKind::ShrSampler.next(1), BackendKind::Synthv1);
        assert_eq!(BackendKind::Synthv1.next(1), BackendKind::Yoshimi);
        assert_eq!(BackendKind::Yoshimi.next(1), BackendKind::FluidSynth);
        assert_eq!(BackendKind::FluidSynth.next(1), BackendKind::MojSint);
    }

    #[test]
    fn shr_sampler_discovery_is_bounded_strict_and_uses_stable_package_identity() {
        let base =
            std::env::temp_dir().join(format!("shsynth-sampler-catalog-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let package = base.join("nested/shr-clear-tone.shrinst");
        fs::create_dir_all(&package).unwrap();
        fs::write(
            package.join("manifest.json"),
            r#"{"format_version":1,"instrument_id":"shr-clear-tone","display_name":"SHR Clear Tone"}"#,
        )
        .unwrap();
        let discovered = discover_shr_sampler(std::slice::from_ref(&base)).unwrap();
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].backend, BackendKind::ShrSampler);
        assert_eq!(discovered[0].name, "SHR Clear Tone");
        assert_eq!(discovered[0].route_id(), "shr-clear-tone");
        assert_eq!(
            discovered[0].id,
            PresetId::ShrSampler {
                instrument_id: "shr-clear-tone".into(),
                path: package.clone(),
            }
        );

        let duplicate = base.join("duplicate.shrinst");
        fs::create_dir(&duplicate).unwrap();
        fs::copy(
            package.join("manifest.json"),
            duplicate.join("manifest.json"),
        )
        .unwrap();
        assert!(discover_shr_sampler(std::slice::from_ref(&base)).is_err());
        fs::remove_dir_all(duplicate).unwrap();
        fs::write(package.join("manifest.json"), b"not json").unwrap();
        assert!(discover_shr_sampler(std::slice::from_ref(&base)).is_err());
        let linked_root = base.with_extension("linked-root");
        let _ = fs::remove_file(&linked_root);
        std::os::unix::fs::symlink(&base, &linked_root).unwrap();
        assert!(discover_shr_sampler(std::slice::from_ref(&linked_root)).is_err());
        fs::remove_file(linked_root).unwrap();
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn moj_sint_display_uses_short_model_codes_without_repeating_the_model() {
        let preset = |model: MojModel, name: &str| Preset {
            backend: BackendKind::MojSint,
            name: name.into(),
            category: Some(model.label().into()),
            id: PresetId::MojSint {
                model,
                path: PathBuf::from("sound.mojsint"),
            },
        };

        assert_eq!(
            preset(MojModel::ModelD, "01 Full Bass").display_name(),
            "01 M-D Full Bass"
        );
        assert_eq!(
            preset(MojModel::SixOpPm, "08 Six-Op Bell Metal").display_name(),
            "08 6-OP Bell Metal"
        );
        assert_eq!(
            preset(MojModel::SixOpPm, "Six-Op PM User 001").display_name(),
            "6-OP User 001"
        );
    }

    #[test]
    fn moj_catalog_is_model_grouped_and_numbered_inside_each_model() {
        let preset = |model: MojModel, name: &str| Preset {
            backend: BackendKind::MojSint,
            name: name.into(),
            category: Some(model.label().into()),
            id: PresetId::MojSint {
                model,
                path: PathBuf::from(format!("{name}.mojsint")),
            },
        };
        let mut presets = vec![
            preset(MojModel::DualFilter, "18 Dual Filter Serial Bass"),
            preset(MojModel::BassMatrix, "16 Bass Matrix"),
            preset(MojModel::SixOpPm, "09 Six-Op Fractured Metal"),
            preset(MojModel::ModelD, "02 Full Lead"),
            preset(MojModel::SwarmMachine, "15 Swarm Warm Pad"),
            preset(MojModel::StrangeOscillator, "14 Strange Oscillator"),
            preset(MojModel::DualFilter, "17 Dual Filter Industrial Lead"),
            preset(MojModel::SixOpPm, "08 Six-Op Bell Metal"),
            preset(MojModel::ModelD, "01 Full Bass"),
        ];

        sort_presets(&mut presets);

        assert_eq!(
            presets
                .iter()
                .enumerate()
                .map(|(index, _)| moj_catalog_display_name(&presets, index).unwrap())
                .collect::<Vec<_>>(),
            [
                "D01 Full Bass",
                "D02 Full Lead",
                "P01 Bell Metal",
                "P02 Fractured Metal",
                "O01 Strange Oscillator",
                "S01 Warm Pad",
                "B01 Bass Matrix",
                "F01 Industrial Lead",
                "F02 Serial Bass",
            ]
        );
    }

    #[test]
    fn read_only_managed_instruments_start_with_visible_unity_volume() {
        let presets = [
            Preset {
                backend: BackendKind::Yoshimi,
                name: "Yoshimi".into(),
                category: None,
                id: PresetId::Yoshimi {
                    path: "sound.xiz".into(),
                },
            },
            Preset {
                backend: BackendKind::FluidSynth,
                name: "Fluid".into(),
                category: None,
                id: PresetId::FluidSynth {
                    soundfont: "sound.sf2".into(),
                    soundfont_index: 0,
                    bank: 0,
                    program: 0,
                },
            },
            Preset {
                backend: BackendKind::ShrSampler,
                name: "Sampler".into(),
                category: None,
                id: PresetId::ShrSampler {
                    instrument_id: "sample".into(),
                    path: "sample.shrinst".into(),
                },
            },
        ];
        for preset in presets {
            assert_eq!(
                values(&preset).unwrap(),
                HashMap::from([(crate::control::INSTRUMENT_VOLUME_CC, 1.0)])
            );
        }
    }

    #[test]
    fn fluidsynth_display_hides_font_bank_metadata() {
        let preset = Preset {
            backend: BackendKind::FluidSynth,
            name: "Warm Pad".into(),
            category: Some("TimGM6mb 002:009".into()),
            id: PresetId::FluidSynth {
                soundfont: "/sounds/tim.sf2".into(),
                soundfont_index: 0,
                bank: 2,
                program: 9,
            },
        };
        assert_eq!(preset.display_name(), "Warm Pad");
        assert_eq!(preset.route_id(), "sf0:tim.sf2:2:9");
        assert_eq!(preset.legacy_route_id().as_deref(), Some("tim.sf2:2:9"));
    }

    #[test]
    fn fluidsynth_route_identity_distinguishes_configured_soundfonts() {
        let preset = |index| Preset {
            backend: BackendKind::FluidSynth,
            name: "Same Program".into(),
            category: None,
            id: PresetId::FluidSynth {
                soundfont: format!("/fonts/{index}/same.sf2").into(),
                soundfont_index: index,
                bank: 0,
                program: 9,
            },
        };
        assert_ne!(preset(0).route_id(), preset(1).route_id());
    }

    #[test]
    fn yoshimi_discovery_is_recursive_curated_and_bounded() {
        let base = std::env::temp_dir().join(format!("shsynth-yoshimi-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("bank")).unwrap();
        for name in [
            "0001-Fat_Bass.xiz",
            "0002-Soft Bass.xiz",
            "0003-Random Flute.xiz",
        ] {
            fs::write(base.join("bank").join(name), "x").unwrap();
        }
        let presets = discover_yoshimi(std::slice::from_ref(&base), &["bass".into()], 1).unwrap();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].backend, BackendKind::Yoshimi);
        assert_eq!(presets[0].category.as_deref(), Some("Bass"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn yoshimi_category_ignores_parent_directory_names_and_symlink_loops() {
        let base = std::env::temp_dir().join(format!("shsynth-bass-parent-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("bank")).unwrap();
        fs::write(base.join("bank/0001-Bright_Lead.xiz"), "x").unwrap();
        std::os::unix::fs::symlink(&base, base.join("bank/loop")).unwrap();

        let presets = discover_yoshimi(
            std::slice::from_ref(&base),
            &["bass".into(), "lead".into()],
            8,
        )
        .unwrap();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].category.as_deref(), Some("Lead"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn moj_sint_discovery_is_regular_bounded_strict_and_has_twelve_values() {
        let base = std::env::temp_dir().join(format!("shsynth-moj-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let source = r#"
schema_version = 4
name = "01 Full Bass"
voices = 8
output_gain = 0.2
model = "model_d"
model_d_patch = "bass"
[macros]
evolve = 1.0
shape = 0.5
color = 0.4
edge = 1.0
couple = 1.0
motion = 0.4
depth = 1.0
space = 0.45
attack = 0.2
decay = 0.6
sustain = 0.7
release = 0.6
"#;
        let names = [
            "01 Full Bass",
            "02 Full Lead",
            "03 Full Filter Articulation",
            "04 Matched Idealized",
            "05 Matched Linear Mixer",
            "06 Matched Linear Ladder",
            "07 Matched No Drift or Feedback",
        ];
        for (index, name) in names.iter().enumerate() {
            let patch = match index {
                1 => "lead",
                2 => "filter_articulation",
                _ => "bass",
            };
            fs::write(
                base.join(format!("{index:02}.mojsint")),
                source.replace("01 Full Bass", name).replace(
                    "model_d_patch = \"bass\"",
                    &format!("model_d_patch = \"{patch}\""),
                ),
            )
            .unwrap();
        }
        fs::write(base.join("ignored.txt"), source).unwrap();
        std::os::unix::fs::symlink(base.join("00.mojsint"), base.join("linked.mojsint")).unwrap();
        let presets = discover_moj_sint(std::slice::from_ref(&base)).unwrap();
        assert_eq!(presets.len(), 7);
        assert_eq!(presets[0].backend, BackendKind::MojSint);
        assert_eq!(presets[0].moj_model(), Some(MojModel::ModelD));
        assert_eq!(presets[0].category.as_deref(), Some("Model D"));
        assert_eq!(presets[0].route_id(), "model_d/01 Full Bass");
        assert_eq!(
            presets
                .iter()
                .map(|preset| preset.name.as_str())
                .collect::<Vec<_>>(),
            names
        );
        assert_eq!(values(&presets[0]).unwrap().len(), 12);
        let version_two = base.join("legacy-v2.mojsint");
        fs::write(
            &version_two,
            source
                .replace("schema_version = 4", "schema_version = 2")
                .replace("model = \"model_d\"\n", "")
                .replace("model_d_patch = \"bass\"\n", ""),
        )
        .unwrap();
        let (_, model, values) = read_moj_sint(&version_two).unwrap();
        assert_eq!(model, MojModel::ModelD);
        assert_eq!(values.len(), 12);
        fs::remove_file(version_two).unwrap();
        let version_three = base.join("legacy-v3.mojsint");
        fs::write(
            &version_three,
            source
                .replace("schema_version = 4", "schema_version = 3")
                .replace("model = \"model_d\"\n", ""),
        )
        .unwrap();
        assert_eq!(read_moj_sint(&version_three).unwrap().1, MojModel::ModelD);
        fs::remove_file(version_three).unwrap();
        fs::write(
            base.join("bad.mojsint"),
            source.replace("model_d_patch = \"bass\"", "model_d_patch = \"unknown\""),
        )
        .unwrap();
        assert!(discover_moj_sint(std::slice::from_ref(&base)).is_err());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn moj_sint_schema_five_discovers_six_op_model_and_strict_macros() {
        let path =
            std::env::temp_dir().join(format!("shsynth-six-op-{}.mojsint", std::process::id()));
        let source = r#"
schema_version = 5
name = "08 Six-Op Bell Metal"
voices = 4
output_gain = 0.8
model = "six_op_pm"
six_op_patch = "bell_metal"
[macros]
index = 0.5
ratio = 0.5
feedback = 0.5
operator_decay = 0.5
balance = 0.5
key_scale = 0.5
velocity = 0.5
motion = 0.5
attack = 0.05
decay = 0.35
sustain = 0.8
release = 0.25
"#;
        fs::write(&path, source).unwrap();
        let (name, model, values) = read_moj_sint(&path).unwrap();
        assert_eq!(name, "08 Six-Op Bell Metal");
        assert_eq!(model, MojModel::SixOpPm);
        assert_eq!(values.len(), 12);
        assert_eq!(values.get(&20), Some(&0.5));

        fs::write(&path, format!("{source}\nmodel_d_patch = \"bass\"\n")).unwrap();
        assert!(read_moj_sint(&path).is_err());
        let _ = fs::remove_file(path);
    }

    fn test_storage(base: &Path) -> UserPresetStorage {
        UserPresetStorage {
            synthv1: base.join("synthv1"),
            moj_sint: base.join("moj-sint"),
        }
    }

    fn moj_source(model: MojModel) -> String {
        match model {
            MojModel::ModelD => r#"
schema_version = 4
name = "Factory Bass"
voices = 8
output_gain = 0.2
model = "model_d"
model_d_patch = "bass"
[macros]
evolve = 0.1
shape = 0.2
color = 0.3
edge = 0.4
couple = 0.5
motion = 0.6
depth = 0.7
space = 0.8
attack = 0.2
decay = 0.3
sustain = 0.7
release = 0.4
"#
            .into(),
            MojModel::SixOpPm => r#"
schema_version = 5
name = "Factory Bell"
voices = 4
output_gain = 0.8
model = "six_op_pm"
six_op_patch = "bell_metal"
[macros]
index = 0.1
ratio = 0.2
feedback = 0.3
operator_decay = 0.4
balance = 0.5
key_scale = 0.6
velocity = 0.7
motion = 0.8
attack = 0.2
decay = 0.3
sustain = 0.7
release = 0.4
"#
            .into(),
            MojModel::StrangeOscillator => r#"
schema_version = 6
name = "Factory Strange"
voices = 4
output_gain = 0.35
model = "strange_oscillator"
strange_patch = "unified"
[macros]
type = 0.14285715
form = 0.2
warp = 0.3
couple = 0.4
motion = 0.5
chaos = 0.6
color = 0.7
space = 0.8
attack = 0.2
decay = 0.3
sustain = 0.7
release = 0.4
"#
            .into(),
            MojModel::SwarmMachine => r#"
schema_version = 7
name = "Factory Swarm"
voices = 4
output_gain = 0.24
instrument_volume = 1.0
model = "swarm_machine"
swarm_patch = "warm_pad"
[macros]
mass = 0.1
detune = 0.2
spread = 0.3
shape = 0.4
bite = 0.5
motion = 0.6
color = 0.7
space = 0.8
attack = 0.2
decay = 0.3
sustain = 0.7
release = 0.4
"#
            .into(),
            MojModel::BassMatrix => r#"
schema_version = 7
name = "Factory Bass Matrix"
voices = 4
output_gain = 0.46
instrument_volume = 1.0
model = "bass_matrix"
bass_matrix_patch = "transformer"
[macros]
body = 0.1
growl = 0.2
metal = 0.3
punch = 0.4
character = 0.5
drive = 0.6
filter = 0.7
unstable = 0.8
attack = 0.2
decay = 0.3
sustain = 0.7
release = 0.4
"#
            .into(),
            MojModel::DualFilter => r#"
schema_version = 8
name = "Factory Dual Filter"
voices = 4
output_gain = 0.18
instrument_volume = 1.0
model = "dual_filter"
dual_filter_core = "industrial"
[controls]
filter_a_cutoff = 0.1
filter_a_resonance = 0.2
filter_a_envelope_depth = 0.3
filter_b_cutoff = 0.4
filter_b_resonance = 0.5
filter_b_envelope_depth = 0.6
structure = 0.7
filter_attack = 0.8
filter_decay = 0.2
filter_sustain = 0.3
filter_release = 0.7
amp_attack = 0.4
amp_decay = 0.5
amp_sustain = 0.6
amp_release = 0.7
"#
            .into(),
        }
    }

    #[test]
    fn all_moj_models_save_as_strict_schema_eight_and_remain_model_scoped() {
        let base =
            std::env::temp_dir().join(format!("shsynth-user-moj-save-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let storage = test_storage(&base.join("private"));

        for model in MojModel::ALL {
            let path = base.join(format!("factory-{}.mojsint", model.stable_id()));
            fs::write(&path, moj_source(model)).unwrap();
            let (name, parsed_model, mut current) = read_moj_sint(&path).unwrap();
            assert_eq!(parsed_model, model);
            current.insert(20, 0.91);
            current.insert(7, 0.37);
            if model == MojModel::DualFilter {
                current.insert(crate::control::MOJ_CORE_STATE_CC, 1.0);
            }
            let source = Preset {
                backend: BackendKind::MojSint,
                name,
                category: Some(model.label().into()),
                id: PresetId::MojSint { model, path },
            };

            let saved = save_new_user_preset(&storage, &source, &current, &[]).unwrap();
            let PresetId::MojSint { path, .. } = &saved.id else {
                panic!("expected Moj Sint user preset");
            };
            assert_eq!(saved.name, "User 001");
            assert_eq!(
                path.parent().unwrap(),
                storage.moj_sint.join(model.stable_id())
            );
            let encoded = fs::read_to_string(path).unwrap();
            assert!(encoded.contains("schema_version = 8"));
            assert!(encoded.contains("instrument_volume = 0.37"));
            assert!(encoded.contains(&format!("model = {:?}", model.stable_id())));
            match model {
                MojModel::ModelD => {
                    assert!(encoded.contains("model_d_patch = \"bass\""));
                    assert!(encoded.contains("evolve = 0.91"));
                    assert!(!encoded.contains("six_op_patch"));
                }
                MojModel::SixOpPm => {
                    assert!(encoded.contains("six_op_patch = \"bell_metal\""));
                    assert!(encoded.contains("index = 0.91"));
                    assert!(!encoded.contains("model_d_patch"));
                    assert!(!encoded.contains("evolve ="));
                }
                MojModel::StrangeOscillator => {
                    assert!(encoded.contains("strange_patch = \"unified\""));
                    assert!(encoded.contains("type = 0.91"));
                    assert!(!encoded.contains("model_d_patch"));
                    assert!(!encoded.contains("six_op_patch"));
                }
                MojModel::SwarmMachine => {
                    assert!(encoded.contains("swarm_patch = \"warm_pad\""));
                    assert!(encoded.contains("mass = 0.91"));
                    assert!(encoded.contains("bite = 0.5"));
                }
                MojModel::BassMatrix => {
                    assert!(encoded.contains("bass_matrix_patch = \"transformer\""));
                    assert!(encoded.contains("body = 0.91"));
                    assert!(encoded.contains("character = 0.5"));
                }
                MojModel::DualFilter => {
                    assert!(encoded.contains("dual_filter_core = \"counter\""));
                    assert!(encoded.contains("filter_a_cutoff = 0.91"));
                    assert!(encoded.contains("amp_release = 0.7"));
                    assert_eq!(
                        read_moj_sint(path).unwrap().2[&crate::control::MOJ_CORE_STATE_CC],
                        1.0
                    );
                }
            }
            assert_eq!(read_moj_sint(path).unwrap().1, model);
        }
        let discovered = discover_moj_sint(std::slice::from_ref(&storage.moj_sint)).unwrap();
        assert_eq!(discovered.len(), MojModel::ALL.len());
        assert!(discovered.iter().any(|preset| {
            preset.name == "User 001" && preset.moj_model() == Some(MojModel::ModelD)
        }));
        assert!(discovered.iter().any(|preset| {
            preset.name == "User 001" && preset.moj_model() == Some(MojModel::SixOpPm)
        }));
        assert!(discovered.iter().any(|preset| {
            preset.name == "User 001" && preset.moj_model() == Some(MojModel::StrangeOscillator)
        }));
        assert!(discovered.iter().any(|preset| preset.name == "User 001"
            && preset.moj_model() == Some(MojModel::SwarmMachine)));
        assert!(discovered
            .iter()
            .any(|preset| preset.name == "User 001"
                && preset.moj_model() == Some(MojModel::DualFilter)));
        assert!(discovered
            .iter()
            .any(|preset| preset.name == "User 001"
                && preset.moj_model() == Some(MojModel::BassMatrix)));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn synthv1_user_save_preserves_complete_document_and_updates_mapped_values() {
        let base =
            std::env::temp_dir().join(format!("shsynth-user-synth-save-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let storage = test_storage(&base.join("private"));
        let source_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("presets/synthv1/Velvet Tines.synthv1");
        let source = Preset::synthv1("Velvet Tines", source_path.clone());
        let mut current = values(&source).unwrap();
        current.insert(74, 0.12345);

        let saved = save_new_user_preset(&storage, &source, &current, &[]).unwrap();
        let PresetId::Synthv1 { path } = &saved.id else {
            panic!("expected synthv1 user preset");
        };
        assert_eq!(saved.name, "User 001");
        assert_eq!(schema(path), schema(&source_path));
        assert!((values(&saved).unwrap()[&74] - 0.12345).abs() < 0.00001);
        let encoded = fs::read_to_string(path).unwrap();
        assert!(encoded.contains("name=\"User 001\""));
        assert_eq!(
            discover_synthv1_roots(std::slice::from_ref(&storage.synthv1))
                .unwrap()
                .iter()
                .map(|preset| preset.name.as_str())
                .collect::<Vec<_>>(),
            ["User 001"]
        );
        let alternate_root = base.join("alternate-synthv1");
        fs::create_dir_all(&alternate_root).unwrap();
        fs::copy(path, alternate_root.join("User 001.synthv1")).unwrap();
        let forward =
            discover_synthv1_roots(&[storage.synthv1.clone(), alternate_root.clone()]).unwrap();
        let reverse = discover_synthv1_roots(&[alternate_root, storage.synthv1.clone()]).unwrap();
        assert_eq!(
            forward
                .iter()
                .map(|preset| preset.id.clone())
                .collect::<Vec<_>>(),
            reverse
                .iter()
                .map(|preset| preset.id.clone())
                .collect::<Vec<_>>()
        );
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn user_save_autonumbers_collisions_and_failed_overwrite_keeps_prior_file() {
        let base =
            std::env::temp_dir().join(format!("shsynth-user-collision-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let storage = test_storage(&base.join("private"));
        let source_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("presets/synthv1/Velvet Tines.synthv1");
        let source = Preset::synthv1("Velvet Tines", source_path);
        let current = values(&source).unwrap();
        let first = save_new_user_preset(&storage, &source, &current, &[]).unwrap();
        let second =
            save_new_user_preset(&storage, &source, &current, std::slice::from_ref(&first))
                .unwrap();
        assert_eq!(
            (first.name.as_str(), second.name.as_str()),
            ("User 001", "User 002")
        );
        let PresetId::Synthv1 { path } = &first.id else {
            panic!("expected synthv1 user preset");
        };
        let before = fs::read(path).unwrap();
        let mut incomplete = current;
        incomplete.remove(&74);
        assert!(overwrite_user_preset(&storage, &first, &incomplete).is_err());
        assert_eq!(fs::read(path).unwrap(), before);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn factory_public_and_symlink_presets_are_never_overwrite_targets() {
        let base =
            std::env::temp_dir().join(format!("shsynth-user-boundary-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let storage = test_storage(&base.join("private"));
        fs::create_dir_all(&storage.synthv1).unwrap();
        let factory_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("presets/synthv1/Velvet Tines.synthv1");
        let factory = Preset::synthv1("Velvet Tines", factory_path.clone());
        assert!(!user_preset_can_overwrite(&storage, &factory));
        let seeded_factory_path = storage.synthv1.join("Velvet Tines.synthv1");
        fs::copy(&factory_path, &seeded_factory_path).unwrap();
        let seeded_factory = Preset::synthv1("Velvet Tines", seeded_factory_path);
        assert!(!user_preset_can_overwrite(&storage, &seeded_factory));

        let outside_numbered = Preset::synthv1("User 900", factory_path.clone());
        assert!(!user_preset_can_overwrite(&storage, &outside_numbered));
        assert!(
            overwrite_user_preset(&storage, &outside_numbered, &values(&factory).unwrap()).is_err()
        );

        let linked_path = storage.synthv1.join("User 007.synthv1");
        std::os::unix::fs::symlink(&factory_path, &linked_path).unwrap();
        let linked = Preset::synthv1("User 007", linked_path);
        assert!(!user_preset_can_overwrite(&storage, &linked));
        assert!(overwrite_user_preset(&storage, &linked, &values(&factory).unwrap()).is_err());
        assert!(save_new_user_preset(&storage, &linked, &values(&factory).unwrap(), &[]).is_err());

        let malformed_path = base.join("malformed.synthv1");
        fs::write(&malformed_path, "<preset name=\"bad\"><params/></preset>").unwrap();
        let malformed = Preset::synthv1("malformed", malformed_path);
        assert!(
            save_new_user_preset(&storage, &malformed, &values(&factory).unwrap(), &[]).is_err()
        );

        let oversized_path = base.join("oversized.synthv1");
        let oversized = fs::File::create(&oversized_path).unwrap();
        oversized.set_len(MAX_SYNTHV1_PRESET_BYTES + 1).unwrap();
        let oversized = Preset::synthv1("oversized", oversized_path);
        assert!(
            save_new_user_preset(&storage, &oversized, &values(&factory).unwrap(), &[]).is_err()
        );
        assert!(!storage.synthv1.join("User 001.synthv1").exists());

        let public_storage = UserPresetStorage {
            synthv1: Path::new(env!("CARGO_MANIFEST_DIR")).join("presets/synthv1"),
            moj_sint: storage.moj_sint.clone(),
        };
        assert!(next_user_preset_name(&public_storage, &factory, &[]).is_err());

        let linked_root = base.join("linked-root");
        fs::create_dir_all(base.join("real-root")).unwrap();
        std::os::unix::fs::symlink(base.join("real-root"), &linked_root).unwrap();
        let linked_storage = UserPresetStorage {
            synthv1: linked_root,
            moj_sint: storage.moj_sint.clone(),
        };
        assert!(next_user_preset_name(&linked_storage, &factory, &[]).is_err());

        let linked_parent = base.join("linked-parent");
        std::os::unix::fs::symlink(base.join("real-root"), &linked_parent).unwrap();
        let linked_parent_storage = UserPresetStorage {
            synthv1: linked_parent.join("presets"),
            moj_sint: storage.moj_sint.clone(),
        };
        assert!(next_user_preset_name(&linked_parent_storage, &factory, &[]).is_err());

        let relative_storage = UserPresetStorage {
            synthv1: PathBuf::from("user/presets/synthv1"),
            moj_sint: storage.moj_sint.clone(),
        };
        assert!(next_user_preset_name(&relative_storage, &factory, &[]).is_err());

        fs::create_dir_all(&storage.moj_sint).unwrap();
        let outside_moj = base.join("outside-moj");
        fs::create_dir_all(&outside_moj).unwrap();
        std::os::unix::fs::symlink(
            &outside_moj,
            storage.moj_sint.join(MojModel::ModelD.stable_id()),
        )
        .unwrap();
        let moj_path = base.join("factory.mojsint");
        fs::write(&moj_path, moj_source(MojModel::ModelD)).unwrap();
        let (_, _, moj_values) = read_moj_sint(&moj_path).unwrap();
        let moj = Preset {
            backend: BackendKind::MojSint,
            name: "Factory Bass".into(),
            category: Some(MojModel::ModelD.label().into()),
            id: PresetId::MojSint {
                model: MojModel::ModelD,
                path: moj_path,
            },
        };
        assert!(next_user_preset_name(&storage, &moj, &[]).is_err());
        assert!(save_new_user_preset(&storage, &moj, &moj_values, &[]).is_err());
        assert_eq!(fs::read_dir(&outside_moj).unwrap().count(), 0);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn executable_discovery_checks_permission_bits() {
        use std::os::unix::fs::PermissionsExt;

        let path = std::env::temp_dir().join(format!("shsynth-command-{}", std::process::id()));
        fs::write(&path, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!command_exists(path.to_str().unwrap()));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(command_exists(path.to_str().unwrap()));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn parses_soundfont_bank_and_program_headers() {
        let path = std::env::temp_dir().join(format!("shsynth-{}.sf2", std::process::id()));
        let record = |name: &str, program: u16, bank: u16| {
            let mut out = vec![0; 38];
            out[..name.len()].copy_from_slice(name.as_bytes());
            out[20..22].copy_from_slice(&program.to_le_bytes());
            out[22..24].copy_from_slice(&bank.to_le_bytes());
            out
        };
        let mut phdr = record("Warm Pad", 9, 2);
        phdr.extend(record("EOP", 0, 0));
        let mut pdta = b"pdta".to_vec();
        pdta.extend_from_slice(b"phdr");
        pdta.extend_from_slice(&(phdr.len() as u32).to_le_bytes());
        pdta.extend(phdr);
        let mut riff = b"RIFF".to_vec();
        riff.extend_from_slice(&((4 + 8 + pdta.len()) as u32).to_le_bytes());
        riff.extend_from_slice(b"sfbkLIST");
        riff.extend_from_slice(&(pdta.len() as u32).to_le_bytes());
        riff.extend(pdta);
        fs::write(&path, riff).unwrap();
        assert_eq!(
            soundfont_presets(&path).unwrap(),
            [SoundFontProgram {
                name: "Warm Pad".into(),
                bank: 2,
                program: 9
            }]
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn cleared_presets_use_complete_current_schema() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("presets/synthv1");
        let manifest = fs::read_to_string(root.join("cleared-presets.txt")).unwrap();
        let expected = manifest
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        let actual = fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                name.ends_with(".synthv1").then_some(name)
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected, "cleared preset manifest is stale");
        assert_eq!(expected.len(), 21);

        let template = schema(&root.join("Velvet Tines.synthv1"));
        assert_eq!(template.len(), 145);
        for filename in expected {
            let name = filename.trim_end_matches(".synthv1");
            let path = root.join(&filename);
            assert_eq!(schema(&path), template, "schema mismatch in {filename}");
            assert_eq!(values(&Preset::synthv1(name, path)).unwrap().len(), 12);
        }
    }

    fn schema(path: &Path) -> Vec<(u16, String)> {
        let mut reader = Reader::from_file(path).unwrap();
        let mut buffer = Vec::new();
        let mut schema = Vec::new();
        loop {
            match reader.read_event_into(&mut buffer).unwrap() {
                Event::Start(element) if element.name().as_ref() == b"param" => {
                    let mut index = None;
                    let mut name = None;
                    for attribute in element.attributes().flatten() {
                        if attribute.key.as_ref() == b"index" {
                            index = std::str::from_utf8(&attribute.value)
                                .ok()
                                .and_then(|value| value.parse().ok());
                        } else if attribute.key.as_ref() == b"name" {
                            name = std::str::from_utf8(&attribute.value)
                                .ok()
                                .map(str::to_owned);
                        }
                    }
                    schema.push((index.unwrap(), name.unwrap()));
                }
                Event::Eof => break,
                _ => {}
            }
            buffer.clear();
        }
        schema.sort();
        schema
    }
}
