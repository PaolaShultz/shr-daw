//! Multi-destination FT2-style sequencing. Song editing/storage and event
//! planning remain independent from the owned software-synth lifecycle.
use crate::audio_graph::{default_drum_rack, validate_drum_rack, InsertRack, ProjectAuxRouting};
use crate::config::{BankSelectMode, ExternalMidiConfig};
use crate::device_profile::Registry as DeviceProfiles;
use crate::master_strip::MasterStripSettings;
use crate::preset::BackendKind;
use crate::scale::{Scale, ScaleKind};
use crate::tempo::Bpm;
use anyhow::{anyhow, bail, Context, Result};
use midir::{MidiOutput, MidiOutputConnection};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const SONG_VERSION: u8 = 17;
pub const LANES_PER_PAGE: usize = 4;
pub const LOOP_SLOT_COUNT: usize = 4;
pub const AUTOMATION_TICKS_PER_ROW: u32 = 1_680;
pub const TIMING_UNITS_PER_ROW: i8 = 96;
pub const MAX_CELL_NUDGE: i8 = TIMING_UNITS_PER_ROW / 2;
const MAX_PROJECT_BYTES: usize = 16 * 1024 * 1024;
const MAX_PROJECT_PATTERNS: usize = 256;
const MAX_ARRANGEMENT_STEPS: usize = 4096;
pub(crate) const MAX_PROJECT_CELLS: usize = 1_048_576;
const MAX_SETUP_MESSAGES_PER_PAGE: usize = 256;
pub const MAX_AUTOMATION_LANES_PER_PATTERN: usize = 128;
pub const MAX_AUTOMATION_POINTS_PER_LANE: usize = 4_096;
pub const MAX_PROJECT_AUTOMATION_POINTS: usize = 262_144;
#[cfg(test)]
const DEFAULT_GESTURE_SETTLE: Duration = Duration::from_millis(45);

pub fn musician_channel(channel: u8) -> u16 {
    u16::from(channel) + 1
}

pub fn musician_program(program: u8) -> u16 {
    u16::from(program) + 1
}

#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    pub name: String,
    pub project_key: Scale,
    pub drum_kit: String,
    pub drum_tuning: shr_drums::KitTuning,
    pub drum_rack: InsertRack,
    pub steps_per_beat: u8,
    pub gate_percent: u8,
    pub insert_rack: InsertRack,
    pub aux_routing: ProjectAuxRouting,
    pub master_strip: MasterStripSettings,
    pub order: Vec<u16>,
    pub patterns: BTreeMap<u16, Pattern>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BpmInterpretation {
    Half,
    #[default]
    Normal,
    Double,
}

impl BpmInterpretation {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Half => "1/2x",
            Self::Normal => "1x",
            Self::Double => "2x",
        }
    }

    pub fn apply(self, bpm: f64) -> f64 {
        match self {
            Self::Half => bpm / 2.0,
            Self::Normal => bpm,
            Self::Double => bpm * 2.0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoopSettings {
    /// Filename only; imported files always live in the private loop store.
    pub file: String,
    /// Hundredths of a BPM. WAV files are not assumed to contain BPM metadata.
    pub source_bpm_x100: u32,
    pub interpretation: BpmInterpretation,
    pub start_beat: u32,
    pub length_beats: u32,
    /// Placement offset in song beats. Positive values move the loop later.
    pub offset_beats: i32,
    /// Linear level in thousandths. 1000 is unity; 1500 is the bounded maximum.
    pub level_x1000: u16,
    /// Bipolar DJ-filter position in thousandths. Zero is neutral.
    pub filter_x1000: i16,
}

impl LoopSettings {
    pub fn new(
        file: String,
        source_bpm_x100: u32,
        interpretation: BpmInterpretation,
        start_beat: u32,
        length_beats: u32,
        offset_beats: i32,
    ) -> Self {
        Self {
            file,
            source_bpm_x100,
            interpretation,
            start_beat,
            length_beats,
            offset_beats,
            level_x1000: 1000,
            filter_x1000: 0,
        }
    }

    pub fn source_bpm(&self) -> f64 {
        f64::from(self.source_bpm_x100) / 100.0
    }

    pub fn interpreted_bpm(&self) -> f64 {
        self.interpretation.apply(self.source_bpm())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Page {
    pub name: String,
    pub enabled: bool,
    /// MIDI channel and master instrument are independent for each of the
    /// page's four visible tracker columns. The destination remains common.
    pub columns: [ColumnSetup; LANES_PER_PAGE],
    pub velocity: u8,
    pub percussion: bool,
    /// Whether Edit/Record entry automatically writes release cells. This is
    /// off for one-shot percussion by default, while explicit OFF/CUT cells
    /// and transport cleanup always remain effective.
    pub note_off_enabled: bool,
    /// Controls only where future edit/record note events are stored. Playback
    /// always follows the four ordinary tracker lanes.
    pub entry_mode: NoteEntryMode,
    /// Zero-based One-column destination. It remains stored while another
    /// entry mode is selected.
    pub entry_anchor: u8,
    /// Per-kit exceptions to the General MIDI drum classification used only by
    /// Drum-auto placement.
    pub drum_class_overrides: BTreeMap<u8, DrumNoteClass>,
    pub target: PageTarget,
    /// Optional convenience metadata for labels and bank protocol. Raw MIDI
    /// routing remains complete when this is `None`.
    pub device_profile: Option<String>,
    /// Reserved for a later small per-page MIDI setup sequence. It is stored
    /// and routed, but deliberately has no editor yet.
    pub setup: Vec<Vec<u8>>,
    pub lanes: Vec<Lane>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NoteEntryMode {
    #[default]
    Manual,
    OneColumn,
    DrumAuto,
}

impl NoteEntryMode {
    pub const fn compact_label(self, anchor: u8) -> &'static str {
        match (self, anchor) {
            (Self::Manual, _) => "MANUAL",
            (Self::OneColumn, 0) => "ONE C1",
            (Self::OneColumn, 1) => "ONE C2",
            (Self::OneColumn, 2) => "ONE C3",
            (Self::OneColumn, _) => "ONE C4",
            (Self::DrumAuto, _) => "DRUM AUTO",
        }
    }
}

const fn legacy_entry_mode(percussion: bool) -> NoteEntryMode {
    if percussion {
        NoteEntryMode::DrumAuto
    } else {
        NoteEntryMode::Manual
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DrumRole {
    Core,
    LongTail,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrumNoteClass {
    pub role: DrumRole,
    pub choke_group: Option<u8>,
}

impl DrumNoteClass {
    pub const fn new(role: DrumRole, choke_group: Option<u8>) -> Self {
        Self { role, choke_group }
    }

    /// General MIDI provides a useful placement default without making MIDI
    /// channel 10 itself a percussion classifier. Unknown notes deliberately
    /// fall back to ordinary short percussion.
    pub const fn general_midi(note: u8) -> Self {
        match note {
            35..=40 => Self::new(DrumRole::Core, None),
            42 | 44 => Self::new(DrumRole::Other, Some(1)),
            46 => Self::new(DrumRole::LongTail, Some(1)),
            49 | 51 | 52 | 53 | 55 | 57 | 59 => Self::new(DrumRole::LongTail, None),
            _ => Self::new(DrumRole::Other, None),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ColumnSetup {
    pub channel: u8,
    pub bank_msb: u8,
    pub bank_lsb: u8,
    pub program: u8,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PageTarget {
    /// Portable/unassigned route. The saved Project contains no device name or
    /// MIDI channel; runtime uses this machine's active configured defaults.
    Default,
    /// The one software instrument currently owned and monitored by SHR-DAW.
    /// This legacy route is upgraded in memory when an older Project loads.
    ActiveInstrument,
    /// A synthv1 preset owned by the Pattern rather than by the standalone
    /// Software Synth workspace. The portable identifier is the discovered
    /// preset name, never a machine-local absolute path.
    Synthv1(String),
    /// Explicit software route. Engine and instrument identities travel
    /// together so catalog order or the standalone current engine cannot
    /// retarget a Pattern.
    Software(SoftwareRoute),
    /// In-process SHR Drums kit. This is not a managed synth backend.
    InternalDrums(String),
    /// An exact ALSA MIDI output port name selected by the user.
    Midi(String),
    /// The configured `external_midi.output` route.
    ConfiguredExternal,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SoftwareRoute {
    pub engine: BackendKind,
    pub instrument: String,
}

impl SoftwareRoute {
    pub fn synthv1(instrument: impl Into<String>) -> Self {
        Self {
            engine: BackendKind::Synthv1,
            instrument: instrument.into(),
        }
    }
}

impl PageTarget {
    pub fn label(&self) -> &str {
        match self {
            Self::Default => "AUTO · machine default",
            Self::ActiveInstrument => "SHR-DAW instrument",
            Self::Synthv1(name) => name,
            Self::Software(route) => &route.instrument,
            Self::InternalDrums(kit) => kit,
            Self::Midi(name) => name,
            Self::ConfiguredExternal => "Configured MIDI output",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lane {
    pub name: String,
    pub enabled: bool,
    pub playback: LanePlayback,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LanePlayback {
    /// Zero follows the current Pattern length; non-zero values are explicit
    /// source-row cycle lengths.
    pub cycle_rows: u16,
    pub rate: LaneRate,
    pub direction: LaneDirection,
}

impl LanePlayback {
    pub fn effective_rows(self, pattern_rows: usize) -> usize {
        if self.cycle_rows == 0 {
            pattern_rows
        } else {
            usize::from(self.cycle_rows).min(pattern_rows)
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LaneRate {
    Quarter,
    Half,
    #[default]
    Normal,
    Double,
    Quadruple,
}

impl LaneRate {
    pub const ALL: [Self; 5] = [
        Self::Quarter,
        Self::Half,
        Self::Normal,
        Self::Double,
        Self::Quadruple,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Quarter => "1/4X",
            Self::Half => "1/2X",
            Self::Normal => "1X",
            Self::Double => "2X",
            Self::Quadruple => "4X",
        }
    }

    const fn ratio(self) -> (usize, usize) {
        match self {
            Self::Quarter => (1, 4),
            Self::Half => (1, 2),
            Self::Normal => (1, 1),
            Self::Double => (2, 1),
            Self::Quadruple => (4, 1),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LaneDirection {
    #[default]
    Forward,
    Reverse,
    Pendulum,
    Variation,
}

impl LaneDirection {
    pub const ALL: [Self; 4] = [
        Self::Forward,
        Self::Reverse,
        Self::Pendulum,
        Self::Variation,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Forward => "FORWARD",
            Self::Reverse => "REVERSE",
            Self::Pendulum => "PENDULUM",
            Self::Variation => "VARIATION",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pattern {
    pub tempo: Bpm,
    pub meter: u8,
    pub swing_division: SwingDivision,
    pub swing_percent: u8,
    pub audio_loops: [Option<LoopSettings>; LOOP_SLOT_COUNT],
    pub automation: Vec<AutomationLane>,
    pub pages: Vec<Page>,
    pub rows: Vec<Vec<Cell>>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SwingDivision {
    Eighth,
    #[default]
    Sixteenth,
}

impl SwingDivision {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Eighth => "EIGHTH",
            Self::Sixteenth => "SIXTEENTH",
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationCurve {
    Linear,
    Step,
}

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AutomationTarget {
    Instrument {
        page: u8,
        engine: String,
        control: String,
    },
    MidiCc {
        page: u8,
        channel: u8,
        controller: u8,
    },
    Effect {
        rack: EffectRackTarget,
        effect_id: crate::audio_graph::EffectId,
        effect_kind: crate::audio_graph::EffectKind,
        effect_version: u32,
        parameter: String,
    },
    EffectBypass {
        rack: EffectRackTarget,
        effect_id: crate::audio_graph::EffectId,
        effect_kind: crate::audio_graph::EffectKind,
        effect_version: u32,
    },
}

#[derive(
    Clone, Copy, Debug, serde::Deserialize, Eq, Ord, PartialEq, PartialOrd, serde::Serialize,
)]
#[serde(tag = "scope", content = "id", rename_all = "snake_case")]
pub enum EffectRackTarget {
    Source,
    Master,
    Aux(u8),
    Drums,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationPoint {
    pub tick: u32,
    /// Normalized 0..=65535. MIDI CC targets map this deterministically to
    /// 0..=127; effect and mapped-control targets resolve through their schema.
    pub value: u16,
}

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationLane {
    pub id: u32,
    pub target: AutomationTarget,
    pub curve: AutomationCurve,
    pub points: Vec<AutomationPoint>,
}

impl AutomationLane {
    pub fn value_at(&self, tick: u32, pattern_ticks: u32) -> Option<u16> {
        let first = *self.points.first()?;
        if self.points.len() == 1 || pattern_ticks == 0 {
            return Some(first.value);
        }
        let index = self.points.partition_point(|point| point.tick <= tick);
        let (a, b, position, span) = if index == 0 {
            let a = *self.points.last()?;
            (
                a,
                first,
                tick.saturating_add(pattern_ticks - a.tick),
                pattern_ticks - a.tick + first.tick,
            )
        } else if index == self.points.len() {
            let a = self.points[index - 1];
            (a, first, tick - a.tick, pattern_ticks - a.tick + first.tick)
        } else {
            let a = self.points[index - 1];
            let b = self.points[index];
            (a, b, tick - a.tick, b.tick - a.tick)
        };
        if self.curve == AutomationCurve::Step || span == 0 {
            return Some(a.value);
        }
        let start = i64::from(a.value);
        let delta = i64::from(b.value) - start;
        let numerator = delta * i64::from(position);
        let half = i64::from(span) / 2;
        let rounded_delta = if numerator < 0 {
            (numerator - half) / i64::from(span)
        } else {
            (numerator + half) / i64::from(span)
        };
        let rounded = start + rounded_delta;
        Some(rounded.clamp(0, i64::from(u16::MAX)) as u16)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatternHalf {
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatternDouble {
    Copy,
    Empty,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatternResize {
    pub pattern: Pattern,
    pub discarded_cells: usize,
    pub copied_cells: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cell {
    pub note: Note,
    pub velocity: Option<u8>,
    pub program: Option<u8>,
    /// Percentage of one row used as this note's gate. `None` inherits the
    /// song gate.
    pub gate: Option<u8>,
    pub command: Command,
    /// Signed timing offset in 1/96-row units. It is applied after Pattern
    /// swing and independently of the legacy one-command field.
    pub nudge: i8,
    /// Independent deterministic trigger probability. One hundred preserves
    /// legacy behavior; zero is deliberately not representable.
    pub probability: u8,
    /// Optional loop-aware trigger condition, evaluated before probability.
    pub condition: StepCondition,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            note: Note::Empty,
            velocity: None,
            program: None,
            gate: None,
            command: Command::None,
            nudge: 0,
            probability: 100,
            condition: StepCondition::Always,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StepCondition {
    #[default]
    Always,
    First,
    /// Last pass in a bounded cycle of `length` passes.
    Last(u8),
    /// Fire on pass `hit` within a bounded `cycle` (the displayed A:B form).
    Ratio {
        hit: u8,
        cycle: u8,
    },
    /// Depend on the preceding note trigger in the same lane and playback pass.
    Previous,
    /// Fire only while the performance Fill latch is armed.
    Fill,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Note {
    #[default]
    Empty,
    On(u8),
    Off,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Command {
    #[default]
    None,
    Cut(u8),
    Delay(u8),
    Retrigger(u8),
    Tempo(Bpm),
}

impl Command {
    /// Stable one-column FT2 marker. A cell has exactly one command.
    pub const fn marker(self) -> char {
        match self {
            Self::None => ' ',
            Self::Cut(_) => 'C',
            Self::Delay(_) => 'D',
            Self::Retrigger(_) => 'R',
            Self::Tempo(_) => 'T',
        }
    }
}

impl Cell {
    pub(crate) fn validate(self) -> Result<()> {
        if self.velocity.is_some_and(|value| value > 127)
            || self.program.is_some_and(|value| value > 127)
        {
            bail!("cell MIDI value out of range");
        }
        if self.gate.is_some_and(|gate| !(1..=100).contains(&gate)) {
            bail!("cell gate must be 1..=100 percent");
        }
        if matches!(self.note, Note::On(128..=u8::MAX)) {
            bail!("cell note out of MIDI range");
        }
        match self.command {
            Command::None => {}
            Command::Cut(tick) | Command::Delay(tick) if tick <= 15 => {}
            Command::Retrigger(count) if (1..=8).contains(&count) => {}
            Command::Tempo(_) => {}
            Command::Cut(_) | Command::Delay(_) => bail!("command tick must be 0..=15"),
            Command::Retrigger(_) => bail!("retrigger count must be 1..=8"),
        }
        if !(-MAX_CELL_NUDGE..=MAX_CELL_NUDGE).contains(&self.nudge) {
            bail!("cell timing must be -{MAX_CELL_NUDGE}..={MAX_CELL_NUDGE}");
        }
        if !(1..=100).contains(&self.probability) {
            bail!("cell probability must be 1..=100 percent");
        }
        match self.condition {
            StepCondition::Always
            | StepCondition::First
            | StepCondition::Previous
            | StepCondition::Fill => {}
            StepCondition::Last(length) if (2..=16).contains(&length) => {}
            StepCondition::Ratio { hit, cycle }
                if (2..=16).contains(&cycle) && (1..=cycle).contains(&hit) => {}
            StepCondition::Last(_) => bail!("LAST condition cycle must be 2..=16"),
            StepCondition::Ratio { .. } => {
                bail!("A:B condition needs 1<=A<=B and B in 2..=16")
            }
        }
        if self.condition != StepCondition::Always && !matches!(self.note, Note::On(_)) {
            bail!("step conditions require a note trigger");
        }
        if self.probability != 100 && !matches!(self.note, Note::On(_)) {
            bail!("step probability requires a note trigger");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GestureCommit {
    pub notes: Vec<(u8, u8)>,
    pub overflowed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct GestureCapture {
    held: BTreeMap<u8, u16>,
    collected: BTreeMap<u8, u8>,
    released_at: Option<Instant>,
    overflowed: bool,
}

impl GestureCapture {
    pub fn observe(&mut self, now: Instant, message: &[u8]) {
        if message.len() < 3 || message[1] > 127 || message[2] > 127 {
            return;
        }
        let kind = message[0] & 0xf0;
        let note = message[1];
        if kind == 0x90 && message[2] > 0 {
            *self.held.entry(note).or_default() += 1;
            if self.collected.len() < LANES_PER_PAGE || self.collected.contains_key(&note) {
                self.collected.entry(note).or_insert(message[2]);
            } else {
                self.overflowed = true;
            }
            self.released_at = None;
        } else if kind == 0x80 || (kind == 0x90 && message[2] == 0) {
            if let Some(count) = self.held.get_mut(&note) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.held.remove(&note);
                }
            }
            if self.held.is_empty() && !self.collected.is_empty() {
                self.released_at = Some(now);
            }
        }
    }

    pub fn finish(&mut self, now: Instant, settle: Duration) -> Option<GestureCommit> {
        let ready = self.held.is_empty()
            && self
                .released_at
                .is_some_and(|released| now.saturating_duration_since(released) >= settle);
        ready.then(|| {
            let commit = GestureCommit {
                notes: std::mem::take(&mut self.collected).into_iter().collect(),
                overflowed: std::mem::take(&mut self.overflowed),
            };
            self.released_at = None;
            commit
        })
    }

    pub fn cancel(&mut self) {
        self.held.clear();
        self.collected.clear();
        self.released_at = None;
        self.overflowed = false;
    }

    pub fn is_active(&self) -> bool {
        !self.collected.is_empty()
    }

    pub fn is_released(&self) -> bool {
        self.held.is_empty() && self.released_at.is_some()
    }
}

impl Song {
    pub fn new(config: &ExternalMidiConfig) -> Self {
        Self::new_with_pages(config, default_pages(config))
    }

    pub fn new_with_pages(config: &ExternalMidiConfig, pages: Vec<Page>) -> Self {
        let mut patterns = BTreeMap::new();
        patterns.insert(
            0,
            Pattern::new(config.default_pattern_rows, config.default_tempo, 4, pages),
        );
        let drum_kit = "electronic-house".to_owned();
        Self {
            name: "untitled".into(),
            project_key: Scale::default(),
            drum_rack: default_drum_rack(&drum_kit, 1).expect("factory drum effects are valid"),
            drum_kit,
            drum_tuning: shr_drums::KitTuning::default(),
            steps_per_beat: config.steps_per_beat,
            gate_percent: config.gate_percent,
            insert_rack: InsertRack::default(),
            aux_routing: ProjectAuxRouting::default(),
            master_strip: MasterStripSettings::default(),
            order: vec![0],
            patterns,
        }
    }

    pub fn validate(&self) -> Result<()> {
        validate_label(&self.name, "project name", 64)?;
        if self.project_key.root > 11 {
            bail!("Project key tonic must be 0..=11");
        }
        validate_label(&self.drum_kit, "Project drum kit", 64)?;
        for (piece, tuning) in &self.drum_tuning.pieces {
            validate_label(piece, "Project drum tuning piece", 64)?;
            if tuning.target_pitch_class.is_some_and(|pitch| pitch.0 > 11)
                || !(-2_400..=2_400).contains(&tuning.cents_adjustment)
            {
                bail!("Project drum tuning is out of range");
            }
        }
        validate_drum_rack(&self.drum_rack).map_err(|error| anyhow!(error.to_string()))?;
        if !(1..=16).contains(&self.steps_per_beat) || !(1..=100).contains(&self.gate_percent) {
            bail!("project steps/gate out of range");
        }
        if self.order.is_empty() || self.order.len() > MAX_ARRANGEMENT_STEPS {
            bail!("project needs 1..={MAX_ARRANGEMENT_STEPS} arrangement steps");
        }
        if self.patterns.is_empty() || self.patterns.len() > MAX_PROJECT_PATTERNS {
            bail!("project needs 1..={MAX_PROJECT_PATTERNS} patterns");
        }
        self.insert_rack
            .validate()
            .map_err(|error| anyhow!(error.to_string()))?;
        self.aux_routing
            .validate(&self.insert_rack)
            .map_err(|error| anyhow!(error.to_string()))?;
        self.master_strip
            .validate()
            .map_err(|error| anyhow!(error))?;
        if self
            .order
            .iter()
            .any(|number| !self.patterns.contains_key(number))
        {
            bail!("order references a missing pattern");
        }
        for pattern in self.patterns.values() {
            pattern.validate()?;
        }
        let total_cells = self.total_cell_count()?;
        if total_cells > MAX_PROJECT_CELLS {
            bail!("project exceeds {MAX_PROJECT_CELLS} cells");
        }
        let total_automation_points =
            self.patterns.values().try_fold(0usize, |total, pattern| {
                pattern.automation.iter().try_fold(total, |total, lane| {
                    total
                        .checked_add(lane.points.len())
                        .context("Project automation point count overflow")
                })
            })?;
        if total_automation_points > MAX_PROJECT_AUTOMATION_POINTS {
            bail!("Project exceeds {MAX_PROJECT_AUTOMATION_POINTS} automation points");
        }
        for pattern in self.patterns.values() {
            for lane in &pattern.automation {
                validate_effect_automation_target(self, lane)?;
            }
        }
        Ok(())
    }

    /// Remove automation that can no longer resolve after an effect is
    /// removed or its schema is replaced. Returns `(lanes, points)` removed.
    pub fn remove_effect_automation(
        &mut self,
        rack: EffectRackTarget,
        effect_id: crate::audio_graph::EffectId,
    ) -> (usize, usize) {
        let mut lanes = 0;
        let mut points = 0;
        for pattern in self.patterns.values_mut() {
            pattern.automation.retain(|lane| {
                let matches = match &lane.target {
                    AutomationTarget::Effect {
                        rack: target_rack,
                        effect_id: target_id,
                        ..
                    }
                    | AutomationTarget::EffectBypass {
                        rack: target_rack,
                        effect_id: target_id,
                        ..
                    } => *target_rack == rack && *target_id == effect_id,
                    _ => false,
                };
                if matches {
                    lanes += 1;
                    points += lane.points.len();
                }
                !matches
            });
        }
        (lanes, points)
    }

    fn total_cell_count(&self) -> Result<usize> {
        self.patterns.values().try_fold(0usize, |total, pattern| {
            let pattern_cells = pattern
                .rows
                .len()
                .checked_mul(pattern.total_lanes())
                .context("project cell count overflow")?;
            total
                .checked_add(pattern_cells)
                .context("project cell count overflow")
        })
    }

    pub fn append_pattern(&mut self, pattern: Pattern) -> Result<u16> {
        if self.patterns.len() >= MAX_PROJECT_PATTERNS {
            bail!("project already has {MAX_PROJECT_PATTERNS} patterns");
        }
        if self.order.len() >= MAX_ARRANGEMENT_STEPS {
            bail!("arrangement already has {MAX_ARRANGEMENT_STEPS} steps");
        }
        pattern.validate()?;
        let added_cells = pattern
            .rows
            .len()
            .checked_mul(pattern.total_lanes())
            .context("project cell count overflow")?;
        let projected = self
            .total_cell_count()?
            .checked_add(added_cells)
            .context("project cell count overflow")?;
        if projected > MAX_PROJECT_CELLS {
            bail!("project would exceed {MAX_PROJECT_CELLS} cells");
        }
        let number = self
            .patterns
            .keys()
            .next_back()
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .context("pattern number space is exhausted")?;
        self.patterns.insert(number, pattern);
        self.order.push(number);
        Ok(number)
    }

    pub fn replace_pattern(&mut self, number: u16, pattern: Pattern) -> Result<()> {
        pattern.validate()?;
        let old = self.patterns.get(&number).context("pattern missing")?;
        let old_cells = old
            .rows
            .len()
            .checked_mul(old.total_lanes())
            .context("project cell count overflow")?;
        let new_cells = pattern
            .rows
            .len()
            .checked_mul(pattern.total_lanes())
            .context("project cell count overflow")?;
        let projected = self
            .total_cell_count()?
            .checked_sub(old_cells)
            .and_then(|total| total.checked_add(new_cells))
            .context("project cell count overflow")?;
        if projected > MAX_PROJECT_CELLS {
            bail!("project would exceed {MAX_PROJECT_CELLS} cells");
        }
        self.patterns.insert(number, pattern);
        Ok(())
    }

    pub fn insert_arrangement_step(&mut self, index: usize, pattern: u16) -> Result<usize> {
        if self.order.len() >= MAX_ARRANGEMENT_STEPS {
            bail!("arrangement already has {MAX_ARRANGEMENT_STEPS} steps");
        }
        if !self.patterns.contains_key(&pattern) {
            bail!("arrangement pattern is missing");
        }
        if index > self.order.len() {
            bail!("arrangement insertion is out of range");
        }
        self.order.insert(index, pattern);
        Ok(index)
    }

    pub fn pattern_reference_count(&self, number: u16) -> usize {
        self.order
            .iter()
            .filter(|candidate| **candidate == number)
            .count()
    }

    /// Delete only an arrangement-orphaned pattern. No order step is ever
    /// rewritten as a side effect, and errors leave the song untouched.
    pub fn delete_unused_pattern(&mut self, number: u16) -> Result<()> {
        let references = self.pattern_reference_count(number);
        if references != 0 {
            bail!("pattern {number} is referenced by {references} arrangement step(s)");
        }
        if self.patterns.len() <= 1 {
            bail!("a Project must keep at least one pattern");
        }
        if !self.patterns.contains_key(&number) {
            bail!("pattern {number} does not exist");
        }
        self.patterns.remove(&number);
        Ok(())
    }
}

fn default_pages(_config: &ExternalMidiConfig) -> Vec<Page> {
    vec![
        Page::new_portable("MELODY", false),
        Page::new_portable("DRUMS", true),
    ]
}

/// The musician-facing factory routing for a genuinely new Pattern. MIDI
/// bytes remain zero-based internally even though the UI shows channels 1--16
/// and programs 1--128.
pub fn factory_routing_pages(first_synthv1: &str, gm_drums_route: SoftwareRoute) -> Vec<Page> {
    let mut synth = Page::new("Software Synth", 0, false, 0);
    synth.target = PageTarget::Software(SoftwareRoute::synthv1(first_synthv1));
    let midi = Page::new("MIDI", 0, false, 0);
    let mut drums = Page::new("Drums", 9, true, 0);
    drums.target = PageTarget::Software(gm_drums_route);
    vec![synth, midi, drums]
}

pub fn pattern_has_note_events(pattern: &Pattern) -> bool {
    pattern
        .rows
        .iter()
        .flatten()
        .any(|cell| cell.note != Note::Empty)
}

/// True only while a Project still has the exact unsaved structure created
/// from the current FT2 routing defaults. The display name is deliberately
/// ignored because naming a new Project does not make it musically non-empty.
pub fn matches_new_empty_default_project(
    song: &Song,
    config: &ExternalMidiConfig,
    routing_defaults: &[Page],
) -> bool {
    let mut expected = Song::new_with_pages(config, routing_defaults.to_vec());
    expected.name = song.name.clone();
    song == &expected
}

pub fn upgrade_legacy_synth_routes(song: &mut Song, first_synthv1: &str) -> usize {
    let mut changed = 0;
    for page in song
        .patterns
        .values_mut()
        .flat_map(|pattern| pattern.pages.iter_mut())
    {
        let replacement = match &page.target {
            PageTarget::ActiveInstrument => Some(first_synthv1.to_owned()),
            PageTarget::Synthv1(name) => Some(name.clone()),
            _ => None,
        };
        if let Some(name) = replacement {
            page.target = PageTarget::Software(SoftwareRoute::synthv1(name));
            changed += 1;
        }
    }
    changed
}

pub fn routing_defaults_path() -> PathBuf {
    songs_dir()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("ft2-routing-defaults.shsong")
}

pub fn save_routing_defaults(path: &Path, pages: &[Page]) -> Result<()> {
    let pattern = Pattern::new(1, Bpm::DEFAULT, 4, pages.to_vec());
    let song = Song {
        name: "FT2 routing defaults".into(),
        project_key: Scale::default(),
        drum_kit: "electronic-house".into(),
        drum_tuning: shr_drums::KitTuning::default(),
        drum_rack: default_drum_rack("electronic-house", 1)
            .expect("factory drum effects are valid"),
        steps_per_beat: 4,
        gate_percent: 80,
        insert_rack: InsertRack::default(),
        aux_routing: ProjectAuxRouting::default(),
        master_strip: MasterStripSettings::default(),
        order: vec![0],
        patterns: BTreeMap::from([(0, pattern)]),
    };
    crate::fsutil::atomic_write(path, encode(&song)?.as_bytes())
}

pub fn load_routing_defaults(path: &Path, fallback: &[Page]) -> Result<Vec<Page>> {
    if !path.exists() {
        return Ok(fallback.to_vec());
    }
    let song = decode(&fs::read_to_string(path)?)?;
    if song.patterns.len() != 1 || song.order != [0] {
        bail!("FT2 routing defaults must contain exactly one Pattern");
    }
    let pattern = song
        .patterns
        .get(&0)
        .context("FT2 routing defaults missing Pattern 0")?;
    if pattern_has_note_events(pattern) {
        bail!("FT2 routing defaults cannot contain note events");
    }
    Ok(pattern.pages.clone())
}

impl Page {
    pub fn new(name: &str, channel: u8, percussion: bool, program: u8) -> Self {
        Self {
            name: name.into(),
            enabled: true,
            columns: [ColumnSetup {
                channel,
                bank_msb: 0,
                bank_lsb: 0,
                program,
            }; LANES_PER_PAGE],
            velocity: 96,
            percussion,
            note_off_enabled: !percussion,
            entry_mode: legacy_entry_mode(percussion),
            entry_anchor: 0,
            drum_class_overrides: BTreeMap::new(),
            target: PageTarget::ConfiguredExternal,
            device_profile: None,
            setup: Vec::new(),
            lanes: (1..=LANES_PER_PAGE)
                .map(|lane| Lane {
                    name: format!("L{lane}"),
                    enabled: true,
                    playback: LanePlayback::default(),
                })
                .collect(),
        }
    }

    pub fn new_portable(name: &str, percussion: bool) -> Self {
        let mut page = Self::new(name, 0, percussion, 0);
        page.target = PageTarget::Default;
        page
    }

    pub fn column(&self, lane: usize) -> &ColumnSetup {
        &self.columns[lane.min(LANES_PER_PAGE - 1)]
    }

    pub fn column_mut(&mut self, lane: usize) -> &mut ColumnSetup {
        &mut self.columns[lane.min(LANES_PER_PAGE - 1)]
    }

    pub fn runtime_channel(&self, lane: usize, config: &ExternalMidiConfig) -> u8 {
        if self.target != PageTarget::Default {
            return self.column(lane).channel;
        }
        if self.percussion {
            config.percussion_channel.unwrap_or(config.melody_channel)
        } else {
            config
                .channels
                .get(lane % config.channels.len().max(1))
                .copied()
                .unwrap_or(config.melody_channel)
        }
    }

    pub fn drum_class(&self, note: u8) -> DrumNoteClass {
        self.drum_class_overrides
            .get(&note)
            .copied()
            .unwrap_or_else(|| DrumNoteClass::general_midi(note))
    }
}

impl Song {
    #[cfg(test)]
    pub fn add_page(&mut self, target: PageTarget, channel: u8) -> Result<usize> {
        let pattern = self.order.first().copied().context("missing pattern")?;
        self.add_page_to_pattern(pattern, target, channel)
    }

    pub fn add_page_to_pattern(
        &mut self,
        pattern_number: u16,
        target: PageTarget,
        channel: u8,
    ) -> Result<usize> {
        if channel > 15 {
            bail!("MIDI channel out of range");
        }
        let pattern = self
            .patterns
            .get(&pattern_number)
            .context("pattern missing")?;
        if pattern.pages.len() >= 64 {
            bail!("pattern already has 64 pages");
        }
        let projected = self
            .total_cell_count()?
            .checked_add(
                pattern
                    .rows
                    .len()
                    .checked_mul(LANES_PER_PAGE)
                    .context("project cell count overflow")?,
            )
            .context("project cell count overflow")?;
        if projected > MAX_PROJECT_CELLS {
            bail!("project would exceed {MAX_PROJECT_CELLS} cells");
        }
        let pattern = self
            .patterns
            .get_mut(&pattern_number)
            .context("pattern missing")?;
        let number = pattern.pages.len() + 1;
        let page = if target == PageTarget::Default {
            Page::new_portable(&format!("PAGE {number}"), false)
        } else {
            let mut page = Page::new(&format!("PAGE {number}"), channel, false, 0);
            page.target = target;
            page
        };
        pattern.pages.push(page);
        for row in &mut pattern.rows {
            row.extend(std::iter::repeat_n(Cell::default(), LANES_PER_PAGE));
        }
        let index = pattern.pages.len() - 1;
        Ok(index)
    }

    #[cfg(test)]
    pub fn total_lanes(&self) -> usize {
        self.order
            .first()
            .and_then(|number| self.patterns.get(number))
            .map_or(0, Pattern::total_lanes)
    }
}

impl Pattern {
    pub fn new(rows: usize, tempo: Bpm, meter: u8, pages: Vec<Page>) -> Self {
        let mut pattern = Self {
            tempo,
            meter,
            swing_division: SwingDivision::default(),
            swing_percent: 50,
            audio_loops: std::array::from_fn(|_| None),
            automation: Vec::new(),
            pages,
            rows: Vec::new(),
        };
        let _ = pattern.resize_rows(rows);
        pattern
    }

    #[cfg(test)]
    pub fn from_config(config: &ExternalMidiConfig, rows: usize, meter: u8) -> Self {
        Self::new(rows, config.default_tempo, meter, default_pages(config))
    }

    pub fn from_routing(
        config: &ExternalMidiConfig,
        rows: usize,
        meter: u8,
        pages: &[Page],
    ) -> Self {
        Self::new(rows, config.default_tempo, meter, pages.to_vec())
    }

    pub fn empty_like_setup(rows: usize, setup: &Pattern) -> Self {
        let mut pattern = Self::new(rows, setup.tempo, setup.meter, setup.pages.clone());
        pattern.swing_division = setup.swing_division;
        pattern.swing_percent = setup.swing_percent;
        pattern
    }

    /// Change only the tracker length. Pattern-owned Loop Mix material and
    /// page/routing setup remain attached.
    pub fn resize_rows(&mut self, rows: usize) -> Result<()> {
        if !(1..=256).contains(&rows) {
            bail!("pattern must have 1..=256 rows");
        }
        self.rows
            .resize(rows, vec![Cell::default(); self.total_lanes()]);
        self.clamp_boundary_nudges();
        let end = u32::try_from(rows)
            .unwrap_or(u32::MAX)
            .saturating_mul(AUTOMATION_TICKS_PER_ROW);
        for lane in &mut self.automation {
            lane.points.retain(|point| point.tick < end);
        }
        self.automation.retain(|lane| !lane.points.is_empty());
        Ok(())
    }

    fn clamp_boundary_nudges(&mut self) {
        if let Some(first) = self.rows.first_mut() {
            for cell in first {
                cell.nudge = cell.nudge.max(0);
            }
        }
        let rows = u16::try_from(self.rows.len()).unwrap_or(u16::MAX);
        for lane in self.pages.iter_mut().flat_map(|page| &mut page.lanes) {
            if lane.playback.cycle_rows > rows {
                lane.playback.cycle_rows = rows;
            }
        }
    }

    #[cfg(test)]
    pub fn empty(rows: usize, tracks: usize) -> Self {
        let pages = (0..tracks.div_ceil(LANES_PER_PAGE))
            .map(|index| Page::new(&format!("PAGE {}", index + 1), 0, false, 0))
            .collect::<Vec<_>>();
        Self::new(rows, Bpm::DEFAULT, 4, pages)
    }

    pub fn total_lanes(&self) -> usize {
        self.pages.len() * LANES_PER_PAGE
    }

    pub fn non_default_cells_in_rows(&self, rows: std::ops::Range<usize>) -> Result<usize> {
        if rows.start > rows.end || rows.end > self.rows.len() {
            bail!("pattern row range is out of bounds");
        }
        Ok(self.rows[rows]
            .iter()
            .flatten()
            .filter(|cell| **cell != Cell::default())
            .count())
    }

    pub fn halve_rows(&self, keep: PatternHalf) -> Result<PatternResize> {
        let old_rows = self.rows.len();
        if old_rows < 2 || !old_rows.is_multiple_of(2) {
            bail!("HALF needs an even Pattern of at least two rows");
        }
        let half = old_rows / 2;
        let (kept, discarded) = match keep {
            PatternHalf::Top => (0..half, half..old_rows),
            PatternHalf::Bottom => (half..old_rows, 0..half),
        };
        let discarded_cells = self.non_default_cells_in_rows(discarded)?;
        let mut pattern = self.clone();
        pattern.rows = self.rows[kept].to_vec();
        pattern.clamp_boundary_nudges();
        let shift = if keep == PatternHalf::Bottom {
            u32::try_from(half).unwrap_or_default() * AUTOMATION_TICKS_PER_ROW
        } else {
            0
        };
        let end = u32::try_from(half).unwrap_or_default() * AUTOMATION_TICKS_PER_ROW;
        for lane in &mut pattern.automation {
            lane.points = lane
                .points
                .iter()
                .filter_map(|point| {
                    point
                        .tick
                        .checked_sub(shift)
                        .filter(|tick| *tick < end)
                        .map(|tick| AutomationPoint {
                            tick,
                            value: point.value,
                        })
                })
                .collect();
        }
        pattern.automation.retain(|lane| !lane.points.is_empty());
        pattern.validate()?;
        Ok(PatternResize {
            pattern,
            discarded_cells,
            copied_cells: 0,
        })
    }

    pub fn remove_row(&self, row: usize) -> Result<PatternResize> {
        if self.rows.len() <= 1 {
            bail!("ROW- needs at least two Pattern rows");
        }
        let discarded_cells = self.non_default_cells_in_rows(row..row.saturating_add(1))?;
        let mut pattern = self.clone();
        pattern.rows.remove(row);
        pattern.clamp_boundary_nudges();
        let removed = u32::try_from(row).unwrap_or_default() * AUTOMATION_TICKS_PER_ROW;
        let after = removed + AUTOMATION_TICKS_PER_ROW;
        for lane in &mut pattern.automation {
            lane.points.retain_mut(|point| {
                if point.tick >= after {
                    point.tick -= AUTOMATION_TICKS_PER_ROW;
                    true
                } else {
                    point.tick < removed
                }
            });
        }
        pattern.automation.retain(|lane| !lane.points.is_empty());
        pattern.validate()?;
        Ok(PatternResize {
            pattern,
            discarded_cells,
            copied_cells: 0,
        })
    }

    pub fn insert_row_after(&self, row: usize) -> Result<PatternResize> {
        if self.rows.len() >= 256 {
            bail!("ROW+ cannot exceed 256 Pattern rows");
        }
        if row >= self.rows.len() {
            bail!("ROW+ cursor row is out of bounds");
        }
        let mut pattern = self.clone();
        pattern
            .rows
            .insert(row + 1, vec![Cell::default(); self.total_lanes()]);
        let insertion = u32::try_from(row + 1).unwrap_or_default() * AUTOMATION_TICKS_PER_ROW;
        for lane in &mut pattern.automation {
            for point in &mut lane.points {
                if point.tick >= insertion {
                    point.tick += AUTOMATION_TICKS_PER_ROW;
                }
            }
        }
        pattern.validate()?;
        Ok(PatternResize {
            pattern,
            discarded_cells: 0,
            copied_cells: 0,
        })
    }

    pub fn double_rows(&self, mode: PatternDouble) -> Result<PatternResize> {
        let old_rows = self.rows.len();
        if old_rows > 128 {
            bail!("DOUBLE cannot exceed 256 Pattern rows");
        }
        let copied_cells = if mode == PatternDouble::Copy {
            self.non_default_cells_in_rows(0..old_rows)?
        } else {
            0
        };
        let mut pattern = self.clone();
        match mode {
            PatternDouble::Copy => {
                pattern.rows.extend(self.rows.iter().cloned());
                let offset = u32::try_from(old_rows).unwrap_or_default() * AUTOMATION_TICKS_PER_ROW;
                for lane in &mut pattern.automation {
                    let copied = lane
                        .points
                        .iter()
                        .filter(|point| point.tick < offset)
                        .map(|point| AutomationPoint {
                            tick: point.tick + offset,
                            value: point.value,
                        })
                        .collect::<Vec<_>>();
                    lane.points.extend(copied);
                }
            }
            PatternDouble::Empty => pattern.rows.extend(
                std::iter::repeat_with(|| vec![Cell::default(); self.total_lanes()]).take(old_rows),
            ),
        }
        pattern.validate()?;
        Ok(PatternResize {
            pattern,
            discarded_cells: 0,
            copied_cells,
        })
    }

    /// Transpose note-ons on melodic pages as one atomic edit. Percussion
    /// pages and note-off/empty cells are deliberately unchanged.
    pub fn transpose_melodic(&mut self, semitones: i8) -> Result<usize> {
        let melodic_lanes = self
            .pages
            .iter()
            .enumerate()
            .filter(|(_, page)| !page.percussion)
            .flat_map(|(page, _)| {
                let start = page * LANES_PER_PAGE;
                start..start + LANES_PER_PAGE
            })
            .collect::<Vec<_>>();
        let mut changed = 0;
        for lane in &melodic_lanes {
            for cell in &self.rows {
                if let Note::On(note) = cell[*lane].note {
                    let shifted = i16::from(note) + i16::from(semitones);
                    if !(0..=127).contains(&shifted) {
                        bail!("transpose would move MIDI note {note} outside 0..=127");
                    }
                    changed += 1;
                }
            }
        }
        for lane in melodic_lanes {
            for row in &mut self.rows {
                if let Note::On(note) = row[lane].note {
                    row[lane].note = Note::On((i16::from(note) + i16::from(semitones)) as u8);
                }
            }
        }
        Ok(changed)
    }

    fn validate(&self) -> Result<()> {
        if !matches!(self.meter, 3 | 4) {
            bail!("pattern tempo/meter out of range");
        }
        if !(50..=75).contains(&self.swing_percent) {
            bail!("Pattern swing must be 50..=75 percent");
        }
        if self.pages.is_empty() || self.pages.len() > 64 {
            bail!("pattern needs 1..=64 pages");
        }
        for audio_loop in self.audio_loops.iter().flatten() {
            validate_loop_settings(audio_loop)?;
        }
        if self.automation.len() > MAX_AUTOMATION_LANES_PER_PATTERN {
            bail!("Pattern exceeds {MAX_AUTOMATION_LANES_PER_PATTERN} automation lanes");
        }
        let pattern_ticks = u32::try_from(self.rows.len())
            .ok()
            .and_then(|rows| rows.checked_mul(AUTOMATION_TICKS_PER_ROW))
            .context("Pattern automation position overflow")?;
        let mut lane_ids = BTreeSet::new();
        for lane in &self.automation {
            if lane.id == 0 || !lane_ids.insert(lane.id) {
                bail!("automation lane IDs must be unique and non-zero");
            }
            validate_automation_lane(lane, self, pattern_ticks)?;
        }
        if self
            .pages
            .iter()
            .any(|page| page.lanes.len() != LANES_PER_PAGE)
        {
            bail!("each pattern page needs exactly four lanes");
        }
        for (row_index, row) in self.rows.iter().enumerate() {
            for cell in row {
                cell.validate()?;
                if row_index == 0 && cell.nudge < 0 {
                    bail!("first Pattern row cannot move before Pattern start");
                }
            }
        }
        let mut channel_programs = BTreeMap::new();
        for page in &self.pages {
            validate_label(&page.name, "pattern page name", 64)?;
            for lane in &page.lanes {
                validate_label(&lane.name, "pattern lane name", 64)?;
                if usize::from(lane.playback.cycle_rows) > self.rows.len() {
                    bail!("lane cycle length exceeds Pattern rows");
                }
            }
            if page.velocity > 127
                || usize::from(page.entry_anchor) >= LANES_PER_PAGE
                || page.columns.iter().any(|column| {
                    column.channel > 15
                        || column.bank_msb > 127
                        || column.bank_lsb > 127
                        || column.program > 127
                })
            {
                bail!("pattern page MIDI value out of range");
            }
            if page.drum_class_overrides.iter().any(|(note, class)| {
                *note > 127
                    || class
                        .choke_group
                        .is_some_and(|group| !(1..=127).contains(&group))
            }) {
                bail!("pattern page drum classification out of range");
            }
            if page.target == PageTarget::Default
                && (page
                    .columns
                    .iter()
                    .any(|column| *column != ColumnSetup::default())
                    || !page.setup.is_empty())
            {
                bail!("portable page routing must keep channel, bank, program, and setup blank");
            }
            if page.enabled {
                for (lane, column) in page.columns.iter().enumerate() {
                    if !page.lanes[lane].enabled {
                        continue;
                    }
                    // Portable pages defer setup to the loading machine.
                    // Software routes take their master instrument from the
                    // route itself, so the stored external-MIDI program fields
                    // do not constrain pages that share that route/channel.
                    if matches!(
                        page.target,
                        PageTarget::Default
                            | PageTarget::ActiveInstrument
                            | PageTarget::Synthv1(_)
                            | PageTarget::Software(_)
                            | PageTarget::InternalDrums(_)
                    ) {
                        continue;
                    }
                    let key = (page.target.clone(), column.channel);
                    let selection = (column.bank_msb, column.bank_lsb, column.program);
                    if let Some(old) = channel_programs
                        .insert(key.clone(), selection)
                        .filter(|old| *old != selection)
                    {
                        bail!(
                            "conflicting master instruments share {} channel {}: {}/{}/{} versus {}/{}/{}",
                            key.0.label(),
                            key.1 + 1,
                            old.0,
                            old.1,
                            old.2,
                            selection.0,
                            selection.1,
                            selection.2
                        );
                    }
                }
            }
            match &page.target {
                PageTarget::Synthv1(name) => {
                    validate_label(name, "pattern page synthv1 preset", 255)?
                }
                PageTarget::Software(route) => {
                    validate_label(route.engine.label(), "pattern page software engine", 32)?;
                    validate_label(&route.instrument, "pattern page software instrument", 255)?;
                }
                PageTarget::InternalDrums(kit) => {
                    validate_label(kit, "pattern page SHR Drums kit", 64)?
                }
                PageTarget::Midi(name) => validate_label(name, "pattern page MIDI target", 256)?,
                _ => {}
            }
            if let Some(profile) = &page.device_profile {
                validate_label(profile, "pattern page device profile", 128)?;
            }
            if page.setup.len() > MAX_SETUP_MESSAGES_PER_PAGE
                || page
                    .setup
                    .iter()
                    .any(|message| message.is_empty() || message.len() > 256)
            {
                bail!(
                    "a page may contain at most {MAX_SETUP_MESSAGES_PER_PAGE} setup messages of 1..=256 bytes"
                );
            }
            if matches!(
                page.target,
                PageTarget::ActiveInstrument
                    | PageTarget::Synthv1(_)
                    | PageTarget::Software(_)
                    | PageTarget::InternalDrums(_)
            ) && page.setup.iter().any(|message| match message.as_slice() {
                [status, ..] if status & 0xf0 == 0xc0 => true,
                [status, controller, ..]
                    if status & 0xf0 == 0xb0 && matches!(controller, 0 | 32) =>
                {
                    true
                }
                _ => false,
            }) {
                bail!("software page setup cannot replace its route-owned bank/program selection");
            }
        }
        if self.rows.is_empty() || self.rows.len() > 256 {
            bail!("pattern must have 1..=256 rows");
        }
        if self.rows.iter().any(|row| row.len() != self.total_lanes()) {
            bail!("pattern track count mismatch");
        }
        for cell in self.rows.iter().flatten() {
            cell.validate()?;
        }
        Ok(())
    }
}

fn validate_automation_lane(
    lane: &AutomationLane,
    pattern: &Pattern,
    pattern_ticks: u32,
) -> Result<()> {
    if lane.points.len() > MAX_AUTOMATION_POINTS_PER_LANE {
        bail!("automation lane exceeds {MAX_AUTOMATION_POINTS_PER_LANE} points");
    }
    if lane.points.iter().any(|point| point.tick >= pattern_ticks)
        || lane
            .points
            .windows(2)
            .any(|pair| pair[0].tick >= pair[1].tick)
    {
        bail!("automation points must be strictly ordered inside the Pattern");
    }
    match &lane.target {
        AutomationTarget::Instrument {
            page,
            engine,
            control,
        } => {
            let page = pattern
                .pages
                .get(usize::from(*page))
                .context("instrument automation page is missing")?;
            let route_engine = match &page.target {
                PageTarget::Software(route) => route.engine,
                PageTarget::Synthv1(_) | PageTarget::ActiveInstrument => BackendKind::Synthv1,
                _ => bail!("instrument automation needs an SHR-owned instrument page"),
            };
            let target_engine: BackendKind = engine.parse().context("unknown automation engine")?;
            if route_engine != target_engine {
                bail!("instrument automation engine does not match its page");
            }
            validate_label(control, "automation control", 64)?;
            let supported = match target_engine {
                BackendKind::Synthv1 => crate::control::CONTROLS
                    .iter()
                    .any(|candidate| candidate.xml_name == control),
                BackendKind::MojSint => crate::control::MOJ_MODEL_D_CONTROLS
                    .iter()
                    .chain(crate::control::MOJ_SIX_OP_PM_CONTROLS.iter())
                    .chain(crate::control::MOJ_STRANGE_CONTROLS.iter())
                    .chain(crate::control::MOJ_SWARM_CONTROLS.iter())
                    .chain(crate::control::MOJ_BASS_MATRIX_CONTROLS.iter())
                    .any(|candidate| candidate.macro_id == control),
                BackendKind::Yoshimi | BackendKind::FluidSynth | BackendKind::ShrSampler => {
                    control == "instrument_volume"
                }
            };
            if !supported {
                bail!("instrument automation control is not supported by its engine");
            }
            if lane.curve != AutomationCurve::Linear {
                bail!("mapped instrument controls use linear automation");
            }
        }
        AutomationTarget::MidiCc {
            page,
            channel,
            controller,
        } => {
            let page = pattern
                .pages
                .get(usize::from(*page))
                .context("MIDI automation page is missing")?;
            if *channel > 15 || *controller > 127 {
                bail!("MIDI automation channel/CC is out of range");
            }
            if matches!(
                page.target,
                PageTarget::ActiveInstrument
                    | PageTarget::Synthv1(_)
                    | PageTarget::Software(_)
                    | PageTarget::InternalDrums(_)
            ) {
                bail!("ordinary MIDI CC automation needs an external page");
            }
            if lane.curve != AutomationCurve::Linear {
                bail!("MIDI CC automation uses linear interpolation");
            }
        }
        AutomationTarget::Effect { parameter, .. } => {
            validate_label(parameter, "effect automation parameter", 64)?;
        }
        AutomationTarget::EffectBypass { .. } => {
            if lane.curve != AutomationCurve::Step {
                bail!("effect bypass automation must step");
            }
            if lane
                .points
                .iter()
                .any(|point| !matches!(point.value, 0 | u16::MAX))
            {
                bail!("effect bypass automation values must be off or on");
            }
        }
    }
    Ok(())
}

fn effect_for_target<'a>(
    song: &'a Song,
    rack: EffectRackTarget,
    id: crate::audio_graph::EffectId,
) -> Option<&'a crate::audio_graph::EffectInstance> {
    match rack {
        EffectRackTarget::Source => song.insert_rack.effect(id),
        EffectRackTarget::Master => song.aux_routing.master_rack.effect(id),
        EffectRackTarget::Aux(aux_id) => song
            .aux_routing
            .buses
            .iter()
            .find(|bus| bus.id == aux_id)?
            .rack
            .effect(id),
        EffectRackTarget::Drums => song.drum_rack.effect(id),
    }
}

fn validate_effect_automation_target(song: &Song, lane: &AutomationLane) -> Result<()> {
    let (rack, id, kind, version, parameter) = match &lane.target {
        AutomationTarget::Effect {
            rack,
            effect_id,
            effect_kind,
            effect_version,
            parameter,
        } => (
            *rack,
            *effect_id,
            *effect_kind,
            *effect_version,
            Some(parameter.as_str()),
        ),
        AutomationTarget::EffectBypass {
            rack,
            effect_id,
            effect_kind,
            effect_version,
        } => (*rack, *effect_id, *effect_kind, *effect_version, None),
        _ => return Ok(()),
    };
    let effect = effect_for_target(song, rack, id).context("automation effect target is stale")?;
    if effect.kind != kind || effect.version != version {
        bail!("automation effect target schema is incompatible");
    }
    if let Some(parameter) = parameter {
        let spec = crate::effect_schema::schema(kind)
            .iter()
            .find(|spec| spec.name == parameter)
            .context("automation effect parameter is missing")?;
        let expected = if matches!(
            spec.value_type,
            crate::effect_schema::ParameterType::Continuous
        ) {
            AutomationCurve::Linear
        } else {
            AutomationCurve::Step
        };
        if lane.curve != expected {
            bail!("automation curve does not match the effect parameter type");
        }
    }
    Ok(())
}

fn previous_drum_lane(
    pattern: &Pattern,
    row_index: usize,
    page_index: usize,
    matches: impl Fn(u8) -> bool,
) -> Option<usize> {
    let page_start = page_index.checked_mul(LANES_PER_PAGE)?;
    pattern.rows.iter().take(row_index).rev().find_map(|row| {
        (0..LANES_PER_PAGE).find(|lane| {
            row.get(page_start + lane)
                .is_some_and(|cell| matches!(cell.note, Note::On(note) if matches(note)))
        })
    })
}

fn lane_drum_state_at(
    pattern: &Pattern,
    row_index: usize,
    page_index: usize,
    lane: usize,
) -> Option<(u8, DrumNoteClass)> {
    let page = pattern.pages.get(page_index)?;
    let lane_index = page_index.checked_mul(LANES_PER_PAGE)?.checked_add(lane)?;
    let mut active = None;
    for (source_row, row) in pattern.rows.iter().enumerate().take(row_index) {
        let cell = row.get(lane_index)?;
        match cell.note {
            Note::On(note) => {
                let class = page.drum_class(note);
                active = Some((note, class));
                let explicit_later_release = cell.gate == Some(100)
                    && pattern
                        .rows
                        .iter()
                        .skip(source_row + 1)
                        .any(|later| !matches!(later[lane_index].note, Note::Empty));
                if matches!(cell.command, Command::Cut(_))
                    || (class.role != DrumRole::LongTail && !explicit_later_release)
                {
                    active = None;
                }
            }
            Note::Off => active = None,
            Note::Empty => {
                if matches!(cell.command, Command::Cut(_)) {
                    active = None;
                }
            }
        }
    }
    active
}

fn related_drum_voice(old_note: u8, old: DrumNoteClass, note: u8, new: DrumNoteClass) -> bool {
    old_note == note
        || old
            .choke_group
            .zip(new.choke_group)
            .is_some_and(|(old_group, new_group)| old_group == new_group)
}

fn drum_auto_release_cell(cell: &Cell) -> bool {
    let mut release = *cell;
    release.note = Note::Empty;
    cell.note == Note::Off && release == Cell::default()
}

/// Allocate one simultaneous Drum-auto group without mutating the Pattern.
/// `live_active` supplements stored gate/tail state during realtime capture.
/// The caller must write the whole returned group or none of it.
pub fn drum_auto_lanes(
    pattern: &Pattern,
    row_index: usize,
    page_index: usize,
    notes: &[u8],
    live_active: &[(usize, u8)],
) -> Option<Vec<usize>> {
    let page = pattern.pages.get(page_index)?;
    let row = pattern.rows.get(row_index)?;
    let page_start = page_index.checked_mul(LANES_PER_PAGE)?;
    let mut active = (0..LANES_PER_PAGE)
        .map(|lane| lane_drum_state_at(pattern, row_index, page_index, lane))
        .collect::<Vec<_>>();
    for &(lane, note) in live_active {
        if lane < LANES_PER_PAGE {
            active[lane] = Some((note, page.drum_class(note)));
        }
    }
    let mut claimed = [false; LANES_PER_PAGE];
    let mut assignments = Vec::with_capacity(notes.len());

    for &note in notes {
        let class = page.drum_class(note);
        let exact_history = previous_drum_lane(pattern, row_index, page_index, |old| old == note);
        let core_history = (class.role == DrumRole::Core).then(|| {
            previous_drum_lane(pattern, row_index, page_index, |old| {
                page.drum_class(old).role == DrumRole::Core
            })
        });
        let mut candidates = Vec::with_capacity(LANES_PER_PAGE + 4);
        for (lane, active_voice) in active.iter().enumerate() {
            if active_voice.is_some_and(|(old_note, old_class)| {
                related_drum_voice(old_note, old_class, note, class)
            }) {
                candidates.push(lane);
            }
        }
        if let Some(lane) = exact_history {
            candidates.push(lane);
        }
        if let Some(lane) = core_history.flatten() {
            candidates.push(lane);
        }
        candidates.extend(match class.role {
            DrumRole::Core => [0, 1, 2, 3],
            DrumRole::LongTail => [2, 3, 1, 0],
            DrumRole::Other if class.choke_group.is_some() => [1, 2, 3, 0],
            DrumRole::Other => [2, 3, 1, 0],
        });
        candidates.extend(0..LANES_PER_PAGE);
        candidates.dedup();

        let lane = candidates.into_iter().find(|lane| {
            if *lane >= LANES_PER_PAGE || claimed[*lane] {
                return false;
            }
            let Some(cell) = row.get(page_start + *lane) else {
                return false;
            };
            if *cell != Cell::default()
                && !(drum_auto_release_cell(cell)
                    && active[*lane].is_some_and(|(old_note, old_class)| {
                        related_drum_voice(old_note, old_class, note, class)
                            || (old_class.role == DrumRole::Core && class.role == DrumRole::Core)
                    }))
            {
                return false;
            }
            active[*lane].is_none_or(|(old_note, old_class)| {
                old_class.role != DrumRole::LongTail
                    || related_drum_voice(old_note, old_class, note, class)
            })
        })?;
        claimed[lane] = true;
        assignments.push(lane);
    }
    Some(assignments)
}

fn validate_label(value: &str, description: &str, max_chars: usize) -> Result<()> {
    if value.is_empty() || value.chars().count() > max_chars || value.chars().any(char::is_control)
    {
        bail!("{description} must contain 1..={max_chars} printable characters");
    }
    Ok(())
}

fn validate_loop_settings(audio_loop: &LoopSettings) -> Result<()> {
    if validate_label(&audio_loop.file, "private loop filename", 255).is_err()
        || Path::new(&audio_loop.file)
            .file_name()
            .and_then(|name| name.to_str())
            != Some(audio_loop.file.as_str())
        || !(2_000..=30_000).contains(&audio_loop.source_bpm_x100)
        || audio_loop.length_beats == 0
        || !(-16_384..=16_384).contains(&audio_loop.offset_beats)
        || audio_loop.level_x1000 > 1_500
        || !(-1_000..=1_000).contains(&audio_loop.filter_x1000)
    {
        bail!("invalid private loop settings");
    }
    Ok(())
}

pub fn songs_dir() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_else(|| ".".into())).join(".local/share")
        })
        .join("shsynth/songs")
}

pub fn safe_name(input: &str) -> String {
    let name = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if name.is_empty() {
        "untitled".into()
    } else {
        name.chars().take(64).collect()
    }
}

pub fn list(base: &Path) -> Vec<String> {
    let mut names = fs::read_dir(base)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                return None;
            }
            let path = entry.path();
            if !path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("shsong"))
            {
                return None;
            }
            let name = path.file_stem()?.to_str()?.to_owned();
            (safe_name(&name) == name).then_some(name)
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// Versioned line format. Unsupported or newer versions are refused for load
/// and overwrite. Explicit deletion is independent of file contents.
pub fn encode(song: &Song) -> Result<String> {
    song.validate()?;
    let mut out = format!(
        "SHSYNTH-SONG {SONG_VERSION}\nname={}\nproject_key={}|{}\ndrum_kit={}\ndrum_tuning={}\ndrum_rack={}\nsteps={}\ngate={}\norder={}\n",
        escape(&song.name),
        song.project_key.root,
        match song.project_key.kind {
            ScaleKind::Major => "major",
            ScaleKind::NaturalMinor => "minor",
        },
        escape(&song.drum_kit),
        escape(&serde_json::to_string(&song.drum_tuning)?),
        escape(&serde_json::to_string(&song.drum_rack)?),
        song.steps_per_beat,
        song.gate_percent,
        song.order
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    out.push_str(&format!(
        "insert_rack={}\n",
        escape(&serde_json::to_string(&song.insert_rack)?)
    ));
    out.push_str(&format!(
        "aux_routing={}\n",
        escape(&serde_json::to_string(&song.aux_routing)?)
    ));
    out.push_str(&format!(
        "master_strip={}\n",
        escape(&serde_json::to_string(&song.master_strip)?)
    ));
    for (number, pattern) in &song.patterns {
        out.push_str(&format!(
            "pattern={number}|{}|{}|{}|{}|{}\n",
            pattern.rows.len(),
            pattern.tempo.hundredths(),
            pattern.meter,
            swing_division_text(pattern.swing_division),
            pattern.swing_percent
        ));
        for (slot, audio_loop) in pattern.audio_loops.iter().enumerate() {
            let Some(audio_loop) = audio_loop else {
                continue;
            };
            let interpretation = match audio_loop.interpretation {
                BpmInterpretation::Half => "half",
                BpmInterpretation::Normal => "normal",
                BpmInterpretation::Double => "double",
            };
            out.push_str(&format!(
                "pattern_loop={number}|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
                slot + 1,
                escape(&audio_loop.file),
                audio_loop.source_bpm_x100,
                interpretation,
                audio_loop.start_beat,
                audio_loop.length_beats,
                audio_loop.offset_beats,
                audio_loop.level_x1000,
                audio_loop.filter_x1000
            ));
        }
        for lane in &pattern.automation {
            out.push_str(&format!(
                "pattern_automation={number}|{}\n",
                escape(&serde_json::to_string(lane)?)
            ));
        }
        for (page_index, page) in pattern.pages.iter().enumerate() {
            out.push_str(&format!(
                "pattern_page={number}|{page_index}|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
                escape(&page.name),
                u8::from(page.enabled),
                page.velocity,
                u8::from(page.percussion),
                target_text(&page.target),
                page.device_profile
                    .as_deref()
                    .map(escape)
                    .unwrap_or_else(|| "-".into()),
                entry_mode_text(page.entry_mode),
                page.entry_anchor + 1,
                u8::from(page.note_off_enabled)
            ));
            for (note, class) in &page.drum_class_overrides {
                out.push_str(&format!(
                    "pattern_drum_class={number}|{page_index}|{note}|{}|{}\n",
                    drum_role_text(class.role),
                    class
                        .choke_group
                        .map_or_else(|| "-".into(), |group| group.to_string())
                ));
            }
            for (column_index, column) in page.columns.iter().enumerate() {
                let (channel, bank_msb, bank_lsb, program) = if page.target == PageTarget::Default {
                    (
                        "default".to_owned(),
                        "default".to_owned(),
                        "default".to_owned(),
                        "default".to_owned(),
                    )
                } else {
                    (
                        (column.channel + 1).to_string(),
                        column.bank_msb.to_string(),
                        column.bank_lsb.to_string(),
                        column.program.to_string(),
                    )
                };
                out.push_str(&format!(
                    "pattern_column={number}|{page_index}|{column_index}|{channel}|{bank_msb}|{bank_lsb}|{program}\n"
                ));
            }
            for (lane_index, lane) in page.lanes.iter().enumerate() {
                out.push_str(&format!(
                    "pattern_lane={number}|{page_index}|{lane_index}|{}|{}|{}|{}|{}\n",
                    escape(&lane.name),
                    u8::from(lane.enabled),
                    lane.playback.cycle_rows,
                    lane_rate_text(lane.playback.rate),
                    lane_direction_text(lane.playback.direction)
                ));
            }
            for message in &page.setup {
                out.push_str(&format!(
                    "pattern_setup={number}|{page_index}|{}\n",
                    message
                        .iter()
                        .map(|byte| format!("{byte:02X}"))
                        .collect::<Vec<_>>()
                        .join(":")
                ));
            }
        }
        for (row_index, row) in pattern.rows.iter().enumerate() {
            for (track_index, cell) in row
                .iter()
                .enumerate()
                .filter(|(_, c)| **c != Cell::default())
            {
                out.push_str(&format!(
                    "cell={number}|{row_index}|{track_index}|{}|{}|{}|{}|{}|{}|{}|{}\n",
                    note_text(cell.note),
                    cell.velocity.map_or("-".into(), |v| v.to_string()),
                    cell.program.map_or("-".into(), |v| v.to_string()),
                    cell.gate.map_or("-".into(), |v| v.to_string()),
                    command_text(cell.command),
                    cell.nudge,
                    cell.probability,
                    condition_text(cell.condition)
                ));
            }
        }
    }
    if out.len() > MAX_PROJECT_BYTES {
        bail!("song file exceeds {MAX_PROJECT_BYTES} bytes");
    }
    Ok(out)
}

pub fn decode(text: &str) -> Result<Song> {
    if text.len() > MAX_PROJECT_BYTES {
        bail!("song file exceeds {MAX_PROJECT_BYTES} bytes");
    }
    let mut lines = text.lines();
    let header = lines.next().context("empty song")?;
    let version = header
        .strip_prefix("SHSYNTH-SONG ")
        .context("not an SHR-DAW song")?
        .parse::<u8>()?;
    if version > SONG_VERSION {
        bail!("unsupported song version {version}; file was not changed");
    }
    let mut name = None;
    let mut project_key = None;
    let mut drum_kit = None;
    let mut drum_tuning = None;
    let mut drum_rack = None;
    let mut steps = None;
    let mut gate = None;
    let mut legacy_audio_loops: [Option<LoopSettings>; LOOP_SLOT_COUNT] =
        std::array::from_fn(|_| None);
    let mut insert_rack = None;
    let mut aux_routing = None;
    let mut master_strip = None;
    let mut order = None;
    let mut patterns: BTreeMap<u16, Pattern> = BTreeMap::new();
    let mut pattern_pages: BTreeMap<u16, BTreeMap<usize, Page>> = BTreeMap::new();
    let mut pattern_lanes = Vec::new();
    let mut pattern_columns = Vec::new();
    let mut pattern_setup = Vec::new();
    let mut pattern_drum_classes = Vec::new();
    let mut pattern_loops = Vec::new();
    let mut pattern_automation = Vec::new();
    let mut cells = Vec::new();
    for line in lines.filter(|line| !line.trim().is_empty() && !line.starts_with('#')) {
        let (key, value) = line.split_once('=').context("invalid song line")?;
        match key {
            "name" => set_once(&mut name, unescape(value)?, "name")?,
            "project_key" if version >= 12 => {
                let (root, kind) = value.split_once('|').context("invalid Project key")?;
                set_once(
                    &mut project_key,
                    Scale {
                        root: root.parse()?,
                        kind: match kind {
                            "major" => ScaleKind::Major,
                            "minor" => ScaleKind::NaturalMinor,
                            _ => bail!("invalid Project key mode"),
                        },
                    },
                    "project_key",
                )?;
            }
            "drum_kit" if version >= 12 => set_once(&mut drum_kit, unescape(value)?, "drum_kit")?,
            "drum_tuning" if version >= 12 => set_once(
                &mut drum_tuning,
                serde_json::from_str::<shr_drums::KitTuning>(&unescape(value)?)?,
                "drum_tuning",
            )?,
            "drum_rack" if version >= 13 => set_once(
                &mut drum_rack,
                serde_json::from_str::<InsertRack>(&unescape(value)?)?,
                "drum_rack",
            )?,
            "steps" => set_once(&mut steps, value.parse()?, "steps")?,
            "gate" => set_once(&mut gate, value.parse()?, "gate")?,
            "loop" if version <= 6 => {
                let f = value.split('|').collect::<Vec<_>>();
                if f.len() != 6 {
                    bail!("invalid loop settings");
                }
                set_once(
                    &mut legacy_audio_loops[0],
                    LoopSettings::new(
                        unescape(f[0])?,
                        f[1].parse()?,
                        parse_bpm_interpretation(f[2])?,
                        f[3].parse()?,
                        f[4].parse()?,
                        f[5].parse()?,
                    ),
                    "loop",
                )?;
            }
            "loop_slot" if version == 7 => {
                let f = value.split('|').collect::<Vec<_>>();
                if f.len() != 9 {
                    bail!("invalid loop slot settings");
                }
                let slot = f[0]
                    .parse::<usize>()?
                    .checked_sub(1)
                    .filter(|slot| *slot < LOOP_SLOT_COUNT)
                    .context("loop slot must be 1..=4")?;
                set_once(
                    &mut legacy_audio_loops[slot],
                    LoopSettings {
                        file: unescape(f[1])?,
                        source_bpm_x100: f[2].parse()?,
                        interpretation: parse_bpm_interpretation(f[3])?,
                        start_beat: f[4].parse()?,
                        length_beats: f[5].parse()?,
                        offset_beats: f[6].parse()?,
                        level_x1000: f[7].parse()?,
                        filter_x1000: f[8].parse()?,
                    },
                    "loop_slot",
                )?;
            }
            "insert_rack" if version >= 2 => set_once(
                &mut insert_rack,
                serde_json::from_str::<InsertRack>(&unescape(value)?)?,
                "insert_rack",
            )?,
            "aux_routing" if version >= 3 => set_once(
                &mut aux_routing,
                serde_json::from_str::<ProjectAuxRouting>(&unescape(value)?)?,
                "aux_routing",
            )?,
            "master_strip" if version >= 9 => set_once(
                &mut master_strip,
                serde_json::from_str::<MasterStripSettings>(&unescape(value)?)?,
                "master_strip",
            )?,
            "order" => {
                let parsed = value
                    .split(',')
                    .map(str::parse)
                    .collect::<std::result::Result<Vec<u16>, _>>()?;
                if parsed.len() > MAX_ARRANGEMENT_STEPS {
                    bail!("arrangement exceeds {MAX_ARRANGEMENT_STEPS} steps");
                }
                set_once(&mut order, parsed, "order")?;
            }
            "pattern" => {
                let f = value.split('|').collect::<Vec<_>>();
                let fields = match (version, f.as_slice()) {
                    (0..=14, [number, rows, tempo, meter]) => {
                        (*number, *rows, *tempo, *meter, SwingDivision::default(), 50)
                    }
                    (15..=17, [number, rows, tempo, meter, division, amount]) => (
                        *number,
                        *rows,
                        *tempo,
                        *meter,
                        parse_swing_division(division)?,
                        amount.parse()?,
                    ),
                    _ => bail!("invalid pattern"),
                };
                match fields {
                    (number, rows, tempo, meter, swing_division, swing_percent) => {
                        let number = number.parse()?;
                        let rows = rows.parse::<usize>()?;
                        if !(1..=256).contains(&rows) {
                            bail!("pattern must have 1..=256 rows");
                        }
                        if patterns.len() >= MAX_PROJECT_PATTERNS {
                            bail!("project exceeds {MAX_PROJECT_PATTERNS} patterns");
                        }
                        if patterns
                            .insert(
                                number,
                                Pattern {
                                    tempo: if version >= 10 {
                                        Bpm::from_hundredths(tempo.parse()?).context(
                                            "pattern tempo must be 2000..=30000 hundredths",
                                        )?
                                    } else {
                                        Bpm::from_whole(tempo.parse()?)
                                            .context("legacy pattern tempo must be 20..=300 BPM")?
                                    },
                                    meter: meter.parse()?,
                                    swing_division,
                                    swing_percent,
                                    audio_loops: std::array::from_fn(|_| None),
                                    automation: Vec::new(),
                                    pages: Vec::new(),
                                    rows: vec![Vec::new(); rows],
                                },
                            )
                            .is_some()
                        {
                            bail!("duplicate pattern {number}");
                        }
                    }
                }
            }
            "pattern_page" => {
                let f = value.split('|').collect::<Vec<_>>();
                let (page, legacy_column) = match (version, f.as_slice()) {
                    (
                        0,
                        [_, _, name, enabled, channel, bank_msb, bank_lsb, program, velocity, percussion, target],
                    ) => {
                        let percussion = binary_flag(percussion, "pattern page percussion")?;
                        (
                            Page {
                                name: unescape(name)?,
                                enabled: binary_flag(enabled, "pattern page enabled")?,
                                columns: [ColumnSetup {
                                    channel: one_based_channel(channel)?,
                                    bank_msb: midi_value(bank_msb)?,
                                    bank_lsb: midi_value(bank_lsb)?,
                                    program: midi_value(program)?,
                                }; LANES_PER_PAGE],
                                velocity: midi_value(velocity)?,
                                percussion,
                                note_off_enabled: !percussion,
                                entry_mode: legacy_entry_mode(percussion),
                                entry_anchor: 0,
                                drum_class_overrides: BTreeMap::new(),
                                target: parse_target(target, version)?,
                                device_profile: None,
                                setup: Vec::new(),
                                lanes: Vec::new(),
                            },
                            true,
                        )
                    }
                    (1..=4, [_, _, name, enabled, velocity, percussion, target]) => {
                        let percussion = binary_flag(percussion, "pattern page percussion")?;
                        (
                            Page {
                                name: unescape(name)?,
                                enabled: binary_flag(enabled, "pattern page enabled")?,
                                columns: [ColumnSetup::default(); LANES_PER_PAGE],
                                velocity: midi_value(velocity)?,
                                percussion,
                                note_off_enabled: !percussion,
                                entry_mode: legacy_entry_mode(percussion),
                                entry_anchor: 0,
                                drum_class_overrides: BTreeMap::new(),
                                target: parse_target(target, version)?,
                                device_profile: None,
                                setup: Vec::new(),
                                lanes: Vec::new(),
                            },
                            false,
                        )
                    }
                    (5, [_, _, name, enabled, velocity, percussion, target, profile]) => {
                        let percussion = binary_flag(percussion, "pattern page percussion")?;
                        (
                            Page {
                                name: unescape(name)?,
                                enabled: binary_flag(enabled, "pattern page enabled")?,
                                columns: [ColumnSetup::default(); LANES_PER_PAGE],
                                velocity: midi_value(velocity)?,
                                percussion,
                                note_off_enabled: !percussion,
                                entry_mode: legacy_entry_mode(percussion),
                                entry_anchor: 0,
                                drum_class_overrides: BTreeMap::new(),
                                target: parse_target(target, version)?,
                                device_profile: (*profile != "-")
                                    .then(|| unescape(profile))
                                    .transpose()?,
                                setup: Vec::new(),
                                lanes: Vec::new(),
                            },
                            false,
                        )
                    }
                    (
                        6..=10,
                        [_, _, name, enabled, velocity, percussion, target, profile, entry_mode, entry_anchor],
                    ) => {
                        let percussion = binary_flag(percussion, "pattern page percussion")?;
                        (
                            Page {
                                name: unescape(name)?,
                                enabled: binary_flag(enabled, "pattern page enabled")?,
                                columns: [ColumnSetup::default(); LANES_PER_PAGE],
                                velocity: midi_value(velocity)?,
                                percussion,
                                note_off_enabled: !percussion,
                                entry_mode: parse_entry_mode(entry_mode)?,
                                entry_anchor: one_based_entry_anchor(entry_anchor)?,
                                drum_class_overrides: BTreeMap::new(),
                                target: parse_target(target, version)?,
                                device_profile: (*profile != "-")
                                    .then(|| unescape(profile))
                                    .transpose()?,
                                setup: Vec::new(),
                                lanes: Vec::new(),
                            },
                            false,
                        )
                    }
                    (
                        11..=17,
                        [_, _, name, enabled, velocity, percussion, target, profile, entry_mode, entry_anchor, note_off_enabled],
                    ) => (
                        Page {
                            name: unescape(name)?,
                            enabled: binary_flag(enabled, "pattern page enabled")?,
                            columns: [ColumnSetup::default(); LANES_PER_PAGE],
                            velocity: midi_value(velocity)?,
                            percussion: binary_flag(percussion, "pattern page percussion")?,
                            note_off_enabled: binary_flag(
                                note_off_enabled,
                                "pattern page automatic note off",
                            )?,
                            entry_mode: parse_entry_mode(entry_mode)?,
                            entry_anchor: one_based_entry_anchor(entry_anchor)?,
                            drum_class_overrides: BTreeMap::new(),
                            target: parse_target(target, version)?,
                            device_profile: (*profile != "-")
                                .then(|| unescape(profile))
                                .transpose()?,
                            setup: Vec::new(),
                            lanes: Vec::new(),
                        },
                        false,
                    ),
                    _ => bail!("invalid pattern page"),
                };
                let page_number = f[1].parse::<usize>()?;
                let replaced = pattern_pages
                    .entry(f[0].parse::<u16>()?)
                    .or_default()
                    .insert(page_number, page);
                if replaced.is_some() {
                    bail!("duplicate pattern page {page_number}");
                }
                debug_assert_eq!(legacy_column, version == 0);
            }
            "pattern_lane" => pattern_lanes.push(value.to_owned()),
            "pattern_column" if version >= 1 => pattern_columns.push(value.to_owned()),
            "pattern_setup" => pattern_setup.push(value.to_owned()),
            "pattern_drum_class" if version >= 6 => pattern_drum_classes.push(value.to_owned()),
            "pattern_loop" if version >= 8 => pattern_loops.push(value.to_owned()),
            "pattern_automation" if version >= 14 => pattern_automation.push(value.to_owned()),
            "cell" => cells.push(value.to_owned()),
            _ => bail!("unknown song field {key}; file was not changed"),
        }
    }
    for (number, pages) in pattern_pages {
        if !pages.keys().copied().eq(0..pages.len()) {
            bail!("pattern pages must be contiguous from zero");
        }
        let pattern = patterns.get_mut(&number).context("pattern page missing")?;
        pattern.pages = pages.into_values().collect();
    }
    attach_pattern_lanes(&mut patterns, pattern_lanes, version)?;
    if version >= 1 {
        attach_pattern_columns(&mut patterns, pattern_columns, version)?;
    }
    attach_pattern_setup(&mut patterns, pattern_setup)?;
    attach_pattern_drum_classes(&mut patterns, pattern_drum_classes)?;
    if version >= 8 {
        attach_pattern_loops(&mut patterns, pattern_loops)?;
    } else {
        for pattern in patterns.values_mut() {
            pattern.audio_loops = legacy_audio_loops.clone();
        }
    }
    attach_pattern_automation(&mut patterns, pattern_automation)?;
    let total_cells = patterns.values().try_fold(0usize, |total, pattern| {
        total
            .checked_add(
                pattern
                    .rows
                    .len()
                    .checked_mul(pattern.total_lanes())
                    .context("project cell count overflow")?,
            )
            .context("project cell count overflow")
    })?;
    if total_cells > MAX_PROJECT_CELLS {
        bail!("project exceeds {MAX_PROJECT_CELLS} cells");
    }
    for pattern in patterns.values_mut() {
        let total_lanes = pattern.pages.len() * LANES_PER_PAGE;
        for row in &mut pattern.rows {
            row.resize(total_lanes, Cell::default());
        }
    }
    let mut occupied_cells = BTreeSet::new();
    for value in cells {
        let f = value.split('|').collect::<Vec<_>>();
        if (version <= 14 && f.len() != 8)
            || (version == 15 && f.len() != 9)
            || (version >= 16 && f.len() != 11)
        {
            bail!("invalid cell");
        }
        let pattern = patterns
            .get_mut(&f[0].parse()?)
            .context("cell pattern missing")?;
        let row_index = f[1].parse::<usize>()?;
        let track_index = f[2].parse::<usize>()?;
        if !occupied_cells.insert((f[0].parse::<u16>()?, row_index, track_index)) {
            bail!("duplicate cell");
        }
        let cell = pattern
            .rows
            .get_mut(row_index)
            .and_then(|r| r.get_mut(track_index))
            .context("cell outside pattern")?;
        *cell = Cell {
            note: parse_note(f[3])?,
            velocity: optional_midi(f[4])?,
            program: optional_midi(f[5])?,
            gate: optional_gate(f[6])?,
            command: parse_command(f[7], version)?,
            nudge: if version >= 15 { f[8].parse()? } else { 0 },
            probability: if version >= 16 { f[9].parse()? } else { 100 },
            condition: if version >= 16 {
                parse_condition(f[10])?
            } else {
                StepCondition::Always
            },
        };
    }
    let drum_kit = if version >= 12 {
        drum_kit.context("missing Project drum kit")?
    } else {
        "electronic-house".into()
    };
    let insert_rack = if version >= 2 {
        insert_rack.context("missing insert rack")?
    } else {
        InsertRack::default()
    };
    let aux_routing = if version >= 3 {
        aux_routing.context("missing aux routing")?
    } else {
        ProjectAuxRouting::default()
    };
    let migrated_drum_id = if version < 13 {
        Some(
            insert_rack
                .effects
                .iter()
                .chain(aux_routing.master_rack.effects.iter())
                .chain(
                    aux_routing
                        .buses
                        .iter()
                        .flat_map(|bus| bus.rack.effects.iter()),
                )
                .map(|effect| effect.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .filter(|id| *id != 0)
                .context("drum effect ID space exhausted")?,
        )
    } else {
        None
    };
    let song = Song {
        name: name.context("missing name")?,
        project_key: if version >= 12 {
            project_key.context("missing Project key")?
        } else {
            Scale::default()
        },
        drum_kit: drum_kit.clone(),
        drum_tuning: if version >= 12 {
            drum_tuning.context("missing Project drum tuning")?
        } else {
            shr_drums::KitTuning::default()
        },
        drum_rack: if version >= 13 {
            drum_rack.context("missing Project drum effects")?
        } else {
            default_drum_rack(
                &drum_kit,
                migrated_drum_id.context("missing migrated drum effect ID")?,
            )
            .map_err(|error| anyhow!(error.to_string()))?
        },
        steps_per_beat: steps.context("missing steps")?,
        gate_percent: gate.context("missing gate")?,
        insert_rack,
        aux_routing,
        master_strip: if version >= 9 {
            master_strip.context("missing MASTER STRIP")?
        } else {
            MasterStripSettings::default()
        },
        order: order.context("missing order")?,
        patterns,
    };
    song.validate()?;
    Ok(song)
}

fn attach_pattern_lanes(
    patterns: &mut BTreeMap<u16, Pattern>,
    lanes: Vec<String>,
    version: u8,
) -> Result<()> {
    for value in lanes {
        let f = value.split('|').collect::<Vec<_>>();
        if (version <= 16 && f.len() != 5) || (version >= 17 && f.len() != 8) {
            bail!("invalid pattern lane");
        }
        let pattern = patterns
            .get_mut(&f[0].parse::<u16>()?)
            .context("lane pattern missing")?;
        let page = pattern
            .pages
            .get_mut(f[1].parse::<usize>()?)
            .context("lane page missing")?;
        let index = f[2].parse::<usize>()?;
        if index != page.lanes.len() {
            bail!("lanes must be contiguous");
        }
        page.lanes.push(Lane {
            name: unescape(f[3])?,
            enabled: binary_flag(f[4], "pattern lane enabled")?,
            playback: if version >= 17 {
                LanePlayback {
                    cycle_rows: f[5].parse()?,
                    rate: parse_lane_rate(f[6])?,
                    direction: parse_lane_direction(f[7])?,
                }
            } else {
                LanePlayback::default()
            },
        });
    }
    Ok(())
}

fn attach_pattern_loops(patterns: &mut BTreeMap<u16, Pattern>, loops: Vec<String>) -> Result<()> {
    let mut occupied = BTreeSet::new();
    for value in loops {
        let f = value.split('|').collect::<Vec<_>>();
        if f.len() != 10 {
            bail!("invalid Pattern Loop Mix settings");
        }
        let pattern_number = f[0].parse::<u16>()?;
        let slot = f[1]
            .parse::<usize>()?
            .checked_sub(1)
            .filter(|slot| *slot < LOOP_SLOT_COUNT)
            .context("Pattern Loop Mix slot must be 1..=4")?;
        if !occupied.insert((pattern_number, slot)) {
            bail!("duplicate Pattern Loop Mix slot");
        }
        let settings = LoopSettings {
            file: unescape(f[2])?,
            source_bpm_x100: f[3].parse()?,
            interpretation: parse_bpm_interpretation(f[4])?,
            start_beat: f[5].parse()?,
            length_beats: f[6].parse()?,
            offset_beats: f[7].parse()?,
            level_x1000: f[8].parse()?,
            filter_x1000: f[9].parse()?,
        };
        validate_loop_settings(&settings)?;
        patterns
            .get_mut(&pattern_number)
            .context("Pattern Loop Mix owner is missing")?
            .audio_loops[slot] = Some(settings);
    }
    Ok(())
}

fn attach_pattern_automation(
    patterns: &mut BTreeMap<u16, Pattern>,
    lanes: Vec<String>,
) -> Result<()> {
    let mut count = 0usize;
    for value in lanes {
        let (pattern_number, encoded) = value
            .split_once('|')
            .context("invalid Pattern automation lane")?;
        let pattern = patterns
            .get_mut(&pattern_number.parse::<u16>()?)
            .context("Pattern automation owner is missing")?;
        if pattern.automation.len() >= MAX_AUTOMATION_LANES_PER_PATTERN {
            bail!("Pattern exceeds {MAX_AUTOMATION_LANES_PER_PATTERN} automation lanes");
        }
        let lane = serde_json::from_str::<AutomationLane>(&unescape(encoded)?)
            .context("invalid Pattern automation lane")?;
        count = count
            .checked_add(lane.points.len())
            .context("Project automation point count overflow")?;
        if count > MAX_PROJECT_AUTOMATION_POINTS {
            bail!("Project exceeds {MAX_PROJECT_AUTOMATION_POINTS} automation points");
        }
        pattern.automation.push(lane);
    }
    Ok(())
}

fn attach_pattern_columns(
    patterns: &mut BTreeMap<u16, Pattern>,
    columns: Vec<String>,
    version: u8,
) -> Result<()> {
    let expected = patterns
        .values()
        .map(|pattern| pattern.pages.len() * LANES_PER_PAGE)
        .sum::<usize>();
    if columns.len() != expected {
        bail!("each pattern page needs exactly four column setups");
    }
    let mut occupied = BTreeSet::new();
    for value in columns {
        let f = value.split('|').collect::<Vec<_>>();
        if f.len() != 7 {
            bail!("invalid pattern column");
        }
        let pattern_number = f[0].parse::<u16>()?;
        let page_index = f[1].parse::<usize>()?;
        let column_index = f[2].parse::<usize>()?;
        if column_index >= LANES_PER_PAGE
            || !occupied.insert((pattern_number, page_index, column_index))
        {
            bail!("duplicate or invalid pattern column");
        }
        let page = patterns
            .get_mut(&pattern_number)
            .and_then(|pattern| pattern.pages.get_mut(page_index))
            .context("column page missing")?;
        let portable = version >= 4 && page.target == PageTarget::Default;
        let all_default = f[3..].iter().all(|field| *field == "default");
        if portable != all_default {
            bail!("portable page columns require four default routing markers");
        }
        page.columns[column_index] = ColumnSetup {
            channel: if portable {
                0
            } else {
                one_based_channel(f[3])?
            },
            bank_msb: if portable { 0 } else { midi_value(f[4])? },
            bank_lsb: if portable { 0 } else { midi_value(f[5])? },
            program: if portable { 0 } else { midi_value(f[6])? },
        };
    }
    Ok(())
}

fn attach_pattern_setup(patterns: &mut BTreeMap<u16, Pattern>, setup: Vec<String>) -> Result<()> {
    for value in setup {
        let f = value.split('|').collect::<Vec<_>>();
        if f.len() != 3 {
            bail!("invalid pattern setup");
        }
        let pattern = patterns
            .get_mut(&f[0].parse::<u16>()?)
            .context("setup pattern missing")?;
        let page = pattern
            .pages
            .get_mut(f[1].parse::<usize>()?)
            .context("setup page missing")?;
        if page.setup.len() >= MAX_SETUP_MESSAGES_PER_PAGE {
            bail!("page exceeds {MAX_SETUP_MESSAGES_PER_PAGE} setup messages");
        }
        page.setup.push(parse_setup_message(f[2])?);
    }
    Ok(())
}

fn attach_pattern_drum_classes(
    patterns: &mut BTreeMap<u16, Pattern>,
    classes: Vec<String>,
) -> Result<()> {
    for value in classes {
        let f = value.split('|').collect::<Vec<_>>();
        if f.len() != 5 {
            bail!("invalid pattern drum classification");
        }
        let pattern = patterns
            .get_mut(&f[0].parse::<u16>()?)
            .context("drum classification pattern missing")?;
        let page = pattern
            .pages
            .get_mut(f[1].parse::<usize>()?)
            .context("drum classification page missing")?;
        let note = midi_value(f[2])?;
        let class = DrumNoteClass {
            role: parse_drum_role(f[3])?,
            choke_group: if f[4] == "-" {
                None
            } else {
                let group = midi_value(f[4])?;
                if group == 0 {
                    bail!("drum choke group must be 1..=127");
                }
                Some(group)
            },
        };
        if page.drum_class_overrides.insert(note, class).is_some() {
            bail!("duplicate drum classification for note {note}");
        }
    }
    Ok(())
}

fn set_once<T>(slot: &mut Option<T>, value: T, field: &str) -> Result<()> {
    if slot.replace(value).is_some() {
        bail!("duplicate song field {field}");
    }
    Ok(())
}

fn binary_flag(value: &str, description: &str) -> Result<bool> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => bail!("{description} must be 0 or 1"),
    }
}

fn parse_setup_message(bytes: &str) -> Result<Vec<u8>> {
    let message = if bytes.is_empty() {
        Vec::new()
    } else {
        bytes
            .split(':')
            .map(|byte| u8::from_str_radix(byte, 16).context("invalid setup byte"))
            .collect::<Result<Vec<_>>>()?
    };
    if message.is_empty() || message.len() > 256 {
        bail!("setup message must contain 1..=256 bytes");
    }
    Ok(message)
}

pub fn save(base: &Path, song: &Song, overwrite: bool) -> Result<PathBuf> {
    fs::create_dir_all(base)?;
    let path = base.join(format!("{}.shsong", safe_name(&song.name)));
    if path.exists() && !overwrite {
        bail!("song already exists; confirm overwrite explicitly");
    }
    if path.exists() && overwrite {
        let existing = fs::read_to_string(&path)?;
        decode(&existing)
            .context("refusing to overwrite unsupported, malformed, or unknown project data")?;
    }
    let encoded = encode(song)?;
    if overwrite {
        crate::fsutil::atomic_write(&path, encoded.as_bytes())?;
    } else {
        crate::fsutil::atomic_write_noreplace(&path, encoded.as_bytes())
            .context("publish song without replacement")?;
    }
    Ok(path)
}

pub fn load(base: &Path, name: &str) -> Result<Song> {
    decode(&fs::read_to_string(song_path(base, name)?)?)
}

pub fn delete(base: &Path, name: &str) -> Result<()> {
    fs::remove_file(song_path(base, name)?)?;
    Ok(())
}

/// Publish a renamed Project without replacing either source or destination.
/// The destination is fully encoded before the old directory entry is removed,
/// so every failure before removal preserves the original Project.
pub fn rename_project(base: &Path, old_stem: &str, display_name: &str) -> Result<(Song, PathBuf)> {
    validate_label(display_name, "project name", 64)?;
    let new_stem = safe_name(display_name);
    let old_path = song_path(base, old_stem)?;
    let new_path = song_path(base, &new_stem)?;
    let mut song = load(base, old_stem)?;
    song.name = display_name.to_owned();
    song.validate()?;
    if old_path == new_path {
        crate::fsutil::atomic_write(&old_path, encode(&song)?.as_bytes())?;
    } else {
        crate::fsutil::atomic_write_noreplace(&new_path, encode(&song)?.as_bytes())
            .context("publish renamed Project without replacement")?;
        if let Err(error) = fs::remove_file(&old_path) {
            let _ = fs::remove_file(&new_path);
            return Err(error).context("remove old Project name");
        }
        fs::File::open(base)?.sync_all()?;
    }
    Ok((song, new_path))
}

fn song_path(base: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty() || safe_name(name) != name {
        bail!("invalid song name");
    }
    Ok(base.join(format!("{name}.shsong")))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledMessage {
    pub at: Duration,
    /// Empty for an internal transport-row marker. Row markers advance the
    /// UI and preserve the full pattern duration, but are never transmitted.
    pub bytes: Vec<u8>,
    pub order: usize,
    pub row: usize,
    pub lane: Option<usize>,
    pub target: Option<PageTarget>,
    /// Generated from a sparse automation lane. Transport applies changed-value
    /// suppression and bounded publication only to these messages.
    pub automation: bool,
    pub effect: Option<ScheduledEffectAutomation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledEffectAutomation {
    pub effect_id: crate::audio_graph::EffectId,
    pub effect_kind: crate::audio_graph::EffectKind,
    pub effect_version: u32,
    pub parameter: Option<String>,
    pub value: u16,
}

pub fn schedule(
    song: &Song,
    config: &ExternalMidiConfig,
    start_order: usize,
    start_row: usize,
) -> Result<Vec<ScheduledMessage>> {
    schedule_for_pass(song, config, start_order, start_row, 1, false)
}

pub fn schedule_for_pass(
    song: &Song,
    config: &ExternalMidiConfig,
    start_order: usize,
    start_row: usize,
    pass: u32,
    fill: bool,
) -> Result<Vec<ScheduledMessage>> {
    Ok(crate::timeline::compile_with_conditions(
        song,
        config,
        start_order,
        start_row,
        ConditionContext::playback(pass, fill),
    )?
    .scheduled_messages())
}

/// Route/engine preflight includes every conditional trigger without changing
/// the deterministic playback result.
pub fn schedule_preflight(
    song: &Song,
    config: &ExternalMidiConfig,
    start_order: usize,
    start_row: usize,
) -> Result<Vec<ScheduledMessage>> {
    Ok(crate::timeline::compile_with_conditions(
        song,
        config,
        start_order,
        start_row,
        ConditionContext::preflight(),
    )?
    .scheduled_messages())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ConditionContext {
    pass: u32,
    fill: bool,
    include_all: bool,
}

impl ConditionContext {
    pub(crate) const fn playback(pass: u32, fill: bool) -> Self {
        Self {
            pass: if pass == 0 { 1 } else { pass },
            fill,
            include_all: false,
        }
    }

    const fn preflight() -> Self {
        Self {
            pass: 1,
            fill: true,
            include_all: true,
        }
    }
}

#[cfg(test)]
pub(crate) fn schedule_elapsed(
    song: &Song,
    config: &ExternalMidiConfig,
    start_order: usize,
    start_row: usize,
) -> Result<Vec<ScheduledMessage>> {
    schedule_elapsed_with_conditions(
        song,
        config,
        start_order,
        start_row,
        ConditionContext::playback(1, false),
    )
}

pub(crate) fn schedule_elapsed_with_conditions(
    song: &Song,
    config: &ExternalMidiConfig,
    start_order: usize,
    start_row: usize,
    conditions: ConditionContext,
) -> Result<Vec<ScheduledMessage>> {
    song.validate()?;
    let device_profiles = DeviceProfiles::discover();
    let first_pattern = song
        .order
        .get(start_order)
        .and_then(|number| song.patterns.get(number))
        .context("start order outside arrangement")?;
    if start_row >= first_pattern.rows.len() {
        bail!("start row outside pattern");
    }
    let mut result = Vec::new();
    let mut at = Duration::ZERO;
    let mut clock_step = 0usize;
    let mut active: BTreeMap<usize, (PageTarget, u8, u8, bool)> = BTreeMap::new();
    for (order_index, pattern_number) in song.order.iter().enumerate().skip(start_order) {
        let pattern = &song.patterns[pattern_number];
        let first_row = if order_index == start_order {
            start_row.min(pattern.rows.len())
        } else {
            0
        };
        let pattern_start = at;
        let mut row_timings = Vec::with_capacity(pattern.rows.len().saturating_sub(first_row));
        let mut timing_at = pattern_start;
        let mut timing_tempo = pattern.tempo;
        for (row_index, row) in pattern.rows.iter().enumerate().skip(first_row) {
            let duration = Duration::from_secs_f64(
                60.0 / timing_tempo.as_f64() / f64::from(song.steps_per_beat),
            );
            row_timings.push(RowTiming {
                row: row_index,
                start: timing_at,
                end: timing_at + duration,
                duration,
            });
            timing_at += duration;
            if let Some(new_tempo) = row
                .iter()
                .filter_map(|cell| match cell.command {
                    Command::Tempo(tempo) => Some(tempo),
                    _ => None,
                })
                .next_back()
            {
                timing_tempo = new_tempo;
            }
        }
        let pattern_end = timing_at;
        let last_event_at = pattern_end.saturating_sub(Duration::from_nanos(1));
        let mut pending = Vec::new();
        let mut programmed = vec![false; pattern.total_lanes()];
        let mut previous_result = vec![false; pattern.total_lanes()];
        for page in pattern.pages.iter().filter(|page| page.enabled) {
            for message in &page.setup {
                push(
                    &mut result,
                    at,
                    order_index,
                    0,
                    message.clone(),
                    Some(page.target.clone()),
                );
            }
        }
        for (timing_index, timing) in row_timings.iter().copied().enumerate() {
            let row_index = timing.row;
            let row_duration = timing.duration;
            let row_start = timing.start;
            // Row cursor and MIDI clock deliberately stay on the straight
            // transport grid. Swing and nudge affect only cell events.
            push(
                &mut result,
                row_start,
                order_index,
                row_index,
                Vec::new(),
                None,
            );
            if config.send_transport {
                let targets = pattern
                    .pages
                    .iter()
                    .filter(|page| page.enabled)
                    .map(|page| page.target.clone())
                    .collect::<BTreeSet<_>>();
                for target in targets {
                    for offset in midi_clock_offsets(clock_step, song.steps_per_beat, row_duration)
                    {
                        push(
                            &mut result,
                            row_start + offset,
                            order_index,
                            row_index,
                            vec![0xf8],
                            Some(target.clone()),
                        );
                    }
                }
            }
            for lane_index in 0..pattern.total_lanes() {
                let page_index = lane_index / LANES_PER_PAGE;
                let column_index = lane_index % LANES_PER_PAGE;
                let page = &pattern.pages[page_index];
                if !page.enabled || !page.lanes[column_index].enabled {
                    continue;
                }
                let playback = page.lanes[column_index].playback;
                let steps = lane_steps_for_row(
                    playback,
                    pattern.rows.len(),
                    row_index,
                    *pattern_number,
                    order_index,
                    lane_index,
                    conditions.pass,
                    conditions.include_all,
                );
                for step in steps {
                    let cell = pattern.rows[step.source_row][lane_index];
                    let swing = swing_offset(pattern, row_index, song.steps_per_beat, row_duration);
                    let next_timing = row_timings.get(timing_index + 1).copied();
                    let (step_start, step_end) = lane_step_times(
                        if conditions.include_all {
                            LaneRate::Normal
                        } else {
                            playback.rate
                        },
                        step,
                        timing,
                        next_timing,
                        &row_timings,
                        pattern,
                        song.steps_per_beat,
                        pattern_end,
                    );
                    if step.boundary_before {
                        pending.push(PendingCellEvent {
                            at: step_start.min(last_event_at),
                            row_start: step_start,
                            row_end: step_end,
                            pattern_end,
                            order: order_index,
                            row: row_index,
                            lane: lane_index,
                            page: page_index,
                            column: column_index,
                            cell,
                            kind: PendingCellKind::Boundary,
                        });
                    }
                    let triggered = if matches!(cell.note, Note::On(_)) {
                        let result = cell_triggered(
                            cell,
                            conditions,
                            *pattern_number,
                            order_index,
                            step.source_row,
                            lane_index,
                            step.probability_occurrence,
                            previous_result[lane_index],
                        );
                        previous_result[lane_index] = result;
                        result
                    } else {
                        true
                    };
                    let step_duration = step_end.saturating_sub(step_start);
                    let boundary_floor = if step.boundary_before {
                        step_start
                    } else {
                        pattern_start
                    };
                    let boundary_ceiling = if step.boundary_after {
                        step_end.saturating_sub(Duration::from_nanos(1))
                    } else {
                        last_event_at
                    };
                    if triggered && !matches!(cell.note, Note::Empty) {
                        let delay = match cell.command {
                            Command::Delay(tick) => {
                                step_duration.mul_f64(f64::from(tick.min(15)) / 16.0)
                            }
                            _ => Duration::ZERO,
                        };
                        let swung = if playback.rate == LaneRate::Normal {
                            row_start + swing
                        } else {
                            step_start
                        };
                        let nudged = offset_duration(swung, step_duration, cell.nudge);
                        let event_at = (nudged + delay).max(boundary_floor).min(boundary_ceiling);
                        pending.push(PendingCellEvent {
                            at: event_at,
                            row_start: step_start,
                            row_end: step_end,
                            pattern_end,
                            order: order_index,
                            row: row_index,
                            lane: lane_index,
                            page: page_index,
                            column: column_index,
                            cell,
                            kind: PendingCellKind::Note,
                        });
                    }
                    if triggered {
                        if let Command::Cut(tick) = cell.command {
                            pending.push(PendingCellEvent {
                                at: (step_start
                                    + step_duration.mul_f64(f64::from(tick.min(15)) / 16.0))
                                .min(boundary_ceiling),
                                row_start: step_start,
                                row_end: step_end,
                                pattern_end,
                                order: order_index,
                                row: row_index,
                                lane: lane_index,
                                page: page_index,
                                column: column_index,
                                cell,
                                kind: PendingCellKind::Cut,
                            });
                        }
                    }
                }
            }
            clock_step += 1;
        }
        pending.sort_by_key(|event| (event.at, event.kind, event.row, event.lane));
        for (event_index, event) in pending.iter().copied().enumerate() {
            let next_lane_event_at = pending[event_index + 1..]
                .iter()
                .find(|candidate| candidate.lane == event.lane)
                .map(|candidate| candidate.at);
            let page = &pattern.pages[event.page];
            let mut column = *page.column(event.column);
            column.channel = page.runtime_channel(event.column, config);
            match event.kind {
                PendingCellKind::Boundary | PendingCellKind::Cut => {
                    if let Some((target, channel, note, _)) = active.remove(&event.lane) {
                        push_lane(
                            &mut result,
                            event.at,
                            event.order,
                            event.row,
                            vec![0x80 | channel, note, 0],
                            event.lane,
                            &target,
                        );
                    }
                }
                PendingCellKind::Note => match event.cell.note {
                    Note::On(note) => {
                        if event.cell.program.is_some() || !programmed[event.lane] {
                            append_program(
                                &mut result,
                                SchedulePosition {
                                    at: event.at,
                                    order: event.order,
                                    row: event.row,
                                },
                                page,
                                &column,
                                event.cell.program.unwrap_or(column.program),
                                config,
                                &device_profiles,
                            );
                            programmed[event.lane] = true;
                        }
                        if let Some((old_target, old_channel, old, old_percussion)) =
                            active.remove(&event.lane)
                        {
                            if !old_percussion {
                                push_lane(
                                    &mut result,
                                    event.at,
                                    event.order,
                                    event.row,
                                    vec![0x80 | old_channel, old, 0],
                                    event.lane,
                                    &old_target,
                                );
                            }
                        }
                        active.insert(
                            event.lane,
                            (page.target.clone(), column.channel, note, page.percussion),
                        );
                        let pulses = match event.cell.command {
                            Command::Retrigger(count) => count,
                            _ => 1,
                        };
                        let pulse_span = event
                            .row_end
                            .saturating_sub(event.row_start)
                            .div_f64(f64::from(pulses));
                        let gate = pulse_span.mul_f64(
                            f64::from(event.cell.gate.unwrap_or(song.gate_percent)) / 100.0,
                        );
                        let explicit_release = event.cell.gate == Some(100)
                            && pulses == 1
                            && (!page.note_off_enabled
                                || has_later_lane_event(song, event.order, event.row, event.lane));
                        for pulse in 0..pulses {
                            let pulse_at = (event.at
                                + event
                                    .row_end
                                    .saturating_sub(event.row_start)
                                    .mul_f64(f64::from(pulse) / f64::from(pulses)))
                            .min(event.pattern_end.saturating_sub(Duration::from_nanos(1)));
                            push_lane(
                                &mut result,
                                pulse_at,
                                event.order,
                                event.row,
                                vec![
                                    0x90 | column.channel,
                                    note,
                                    event.cell.velocity.unwrap_or(page.velocity),
                                ],
                                event.lane,
                                &page.target,
                            );
                            if !page.percussion && !explicit_release {
                                let release_at = (pulse_at + gate).min(event.pattern_end);
                                if next_lane_event_at.is_some_and(|next| next < release_at) {
                                    continue;
                                }
                                push_lane(
                                    &mut result,
                                    release_at,
                                    event.order,
                                    event.row,
                                    vec![0x80 | column.channel, note, 0],
                                    event.lane,
                                    &page.target,
                                );
                            }
                        }
                    }
                    Note::Off => {
                        if let Some((target, channel, note, _)) = active.remove(&event.lane) {
                            let release_at = if event.cell.gate == Some(100) {
                                event.row_end
                            } else {
                                event.at
                            };
                            push_lane(
                                &mut result,
                                release_at.min(event.pattern_end),
                                event.order,
                                event.row,
                                vec![0x80 | channel, note, 0],
                                event.lane,
                                &target,
                            );
                        }
                    }
                    Note::Empty => {}
                },
            }
        }
        at = pattern_end;
    }
    release_active_notes(
        &mut result,
        at,
        song.order.len().saturating_sub(1),
        0,
        &mut active,
    );
    // Do not loop as soon as the last note's gate closes: the final rest rows
    // are musically significant. This boundary marker holds the transport to
    // the exact end of the scheduled pattern/order span.
    if let Some((order, pattern_number)) = song.order.iter().enumerate().next_back() {
        let row = song.patterns[pattern_number].rows.len().saturating_sub(1);
        push(&mut result, at, order, row, Vec::new(), None);
    }
    result.sort_by_key(|message| message.at);
    Ok(result)
}

#[derive(Clone, Copy, Debug)]
struct RowTiming {
    row: usize,
    start: Duration,
    end: Duration,
    duration: Duration,
}

#[derive(Clone, Copy, Debug)]
struct LaneStep {
    source_row: usize,
    substep: usize,
    subdivisions: usize,
    slow_rows: usize,
    boundary_before: bool,
    boundary_after: bool,
    probability_occurrence: Option<usize>,
}

#[allow(clippy::too_many_arguments)]
fn lane_steps_for_row(
    playback: LanePlayback,
    pattern_rows: usize,
    transport_row: usize,
    pattern: u16,
    order: usize,
    lane: usize,
    pass: u32,
    preflight: bool,
) -> Vec<LaneStep> {
    if preflight {
        return vec![LaneStep {
            source_row: transport_row,
            substep: 0,
            subdivisions: 1,
            slow_rows: 1,
            boundary_before: false,
            boundary_after: false,
            probability_occurrence: None,
        }];
    }
    let length = playback.effective_rows(pattern_rows).max(1);
    let (subdivisions, slow_rows) = playback.rate.ratio();
    let first_step = if slow_rows > 1 {
        if !transport_row.is_multiple_of(slow_rows) {
            return Vec::new();
        }
        transport_row / slow_rows
    } else {
        transport_row.saturating_mul(subdivisions)
    };
    (0..subdivisions)
        .map(|substep| {
            let step = first_step + substep;
            let source_row =
                lane_source_row(playback.direction, length, step, pattern, order, lane, pass);
            let legacy = playback == LanePlayback::default() && source_row == transport_row;
            LaneStep {
                source_row,
                substep,
                subdivisions,
                slow_rows,
                boundary_before: lane_ownership_boundary(playback.direction, length, step),
                boundary_after: lane_ownership_boundary(
                    playback.direction,
                    length,
                    step.saturating_add(1),
                ),
                probability_occurrence: (!legacy).then_some(step),
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn lane_source_row(
    direction: LaneDirection,
    length: usize,
    step: usize,
    pattern: u16,
    order: usize,
    lane: usize,
    pass: u32,
) -> usize {
    match direction {
        LaneDirection::Forward => step % length,
        LaneDirection::Reverse => length - 1 - step % length,
        LaneDirection::Pendulum if length == 1 => 0,
        LaneDirection::Pendulum => {
            let period = length.saturating_mul(2).saturating_sub(2);
            let phase = step % period;
            if phase < length {
                phase
            } else {
                period - phase
            }
        }
        LaneDirection::Variation => {
            let cycle = step / length;
            let position = step % length;
            variation_position(length, position, pattern, order, lane, pass, cycle)
        }
    }
}

fn lane_ownership_boundary(direction: LaneDirection, length: usize, step: usize) -> bool {
    if step == 0 {
        return false;
    }
    match direction {
        LaneDirection::Forward | LaneDirection::Reverse | LaneDirection::Variation => {
            step.is_multiple_of(length)
        }
        LaneDirection::Pendulum if length == 1 => true,
        LaneDirection::Pendulum if length == 2 => step >= 2,
        LaneDirection::Pendulum => {
            let period = length * 2 - 2;
            let phase = step % period;
            phase == length || (phase == 1 && step > 1)
        }
    }
}

fn variation_position(
    length: usize,
    position: usize,
    pattern: u16,
    order: usize,
    lane: usize,
    pass: u32,
    cycle: usize,
) -> usize {
    if length <= 1 {
        return 0;
    }
    let seed = deterministic_lane_seed(pattern, order, lane, pass, cycle);
    let mut multiplier = (seed as usize % length).max(1);
    while greatest_common_divisor(multiplier, length) != 1 {
        multiplier = multiplier % (length - 1) + 1;
    }
    let offset = (seed.rotate_left(29) as usize) % length;
    (multiplier * position + offset) % length
}

fn greatest_common_divisor(mut left: usize, mut right: usize) -> usize {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

fn deterministic_lane_seed(
    pattern: u16,
    order: usize,
    lane: usize,
    pass: u32,
    cycle: usize,
) -> u64 {
    let mut value = 0x6a09_e667_f3bc_c909u64;
    for part in [
        u64::from(pattern),
        order as u64,
        lane as u64,
        u64::from(pass),
        cycle as u64,
    ] {
        value ^= part.wrapping_add(0x9e37_79b9_7f4a_7c15);
        value = value.rotate_left(27).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^= value >> 31;
    }
    value
}

#[allow(clippy::too_many_arguments)]
fn lane_step_times(
    rate: LaneRate,
    step: LaneStep,
    timing: RowTiming,
    next_timing: Option<RowTiming>,
    timings: &[RowTiming],
    pattern: &Pattern,
    steps_per_beat: u8,
    pattern_end: Duration,
) -> (Duration, Duration) {
    if rate == LaneRate::Normal {
        return (timing.start, timing.end);
    }
    let swing = swing_offset(pattern, timing.row, steps_per_beat, timing.duration);
    let container_start = timing.start + swing;
    let container_end = if step.slow_rows > 1 {
        timings
            .iter()
            .find(|candidate| candidate.row == timing.row + step.slow_rows)
            .map(|candidate| {
                candidate.start
                    + swing_offset(pattern, candidate.row, steps_per_beat, candidate.duration)
            })
            .unwrap_or(pattern_end)
    } else {
        next_timing
            .map(|candidate| {
                candidate.start
                    + swing_offset(pattern, candidate.row, steps_per_beat, candidate.duration)
            })
            .unwrap_or(pattern_end)
    }
    .max(container_start);
    let span = container_end.saturating_sub(container_start);
    let start =
        container_start + span.mul_f64(step.substep as f64 / step.subdivisions.max(1) as f64);
    let end =
        container_start + span.mul_f64((step.substep + 1) as f64 / step.subdivisions.max(1) as f64);
    (start, end.max(start))
}

fn cell_triggered(
    cell: Cell,
    context: ConditionContext,
    pattern: u16,
    order: usize,
    row: usize,
    lane: usize,
    occurrence: Option<usize>,
    previous: bool,
) -> bool {
    if context.include_all {
        return true;
    }
    let pass = context.pass.max(1);
    let condition = match cell.condition {
        StepCondition::Always => true,
        StepCondition::First => pass == 1,
        StepCondition::Last(length) => pass_position(pass, length) == length,
        StepCondition::Ratio { hit, cycle } => pass_position(pass, cycle) == hit,
        StepCondition::Previous => previous,
        StepCondition::Fill => context.fill,
    };
    condition
        && (cell.probability == 100
            || deterministic_percent(pattern, order, row, lane, pass, occurrence)
                <= cell.probability)
}

fn pass_position(pass: u32, cycle: u8) -> u8 {
    ((pass.saturating_sub(1) % u32::from(cycle)) + 1) as u8
}

fn deterministic_percent(
    pattern: u16,
    order: usize,
    row: usize,
    lane: usize,
    pass: u32,
    occurrence: Option<usize>,
) -> u8 {
    let mut value = 0x9e37_79b9_7f4a_7c15u64;
    for part in [
        u64::from(pattern),
        order as u64,
        row as u64,
        lane as u64,
        u64::from(pass),
    ]
    .into_iter()
    .chain(occurrence.map(|value| value as u64))
    {
        value ^= part
            .wrapping_add(0x9e37_79b9)
            .wrapping_add(value << 6)
            .wrapping_add(value >> 2);
        value ^= value >> 30;
        value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value ^= value >> 27;
        value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^= value >> 31;
    }
    (value % 100 + 1) as u8
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum PendingCellKind {
    Boundary,
    Note,
    Cut,
}

#[derive(Clone, Copy, Debug)]
struct PendingCellEvent {
    at: Duration,
    row_start: Duration,
    row_end: Duration,
    pattern_end: Duration,
    order: usize,
    row: usize,
    lane: usize,
    page: usize,
    column: usize,
    cell: Cell,
    kind: PendingCellKind,
}

fn swing_offset(
    pattern: &Pattern,
    row: usize,
    steps_per_beat: u8,
    row_duration: Duration,
) -> Duration {
    if pattern.swing_percent == 50 {
        return Duration::ZERO;
    }
    let steps = usize::from(steps_per_beat);
    let pair_rows = match pattern.swing_division {
        SwingDivision::Eighth => steps,
        SwingDivision::Sixteenth if steps.is_multiple_of(2) => steps / 2,
        SwingDivision::Sixteenth => return Duration::ZERO,
    };
    if pair_rows < 2 || !pair_rows.is_multiple_of(2) || row % pair_rows != pair_rows / 2 {
        return Duration::ZERO;
    }
    let shift_rows = f64::from(pattern.swing_percent - 50) * pair_rows as f64 / 100.0;
    row_duration.mul_f64(shift_rows)
}

fn offset_duration(at: Duration, row_duration: Duration, nudge: i8) -> Duration {
    let offset = row_duration
        .mul_f64(f64::from(nudge.unsigned_abs()) / f64::from(TIMING_UNITS_PER_ROW as u8));
    if nudge < 0 {
        at.saturating_sub(offset)
    } else {
        at + offset
    }
}

fn has_later_lane_event(
    song: &Song,
    order_index: usize,
    row_index: usize,
    lane_index: usize,
) -> bool {
    song.order
        .iter()
        .enumerate()
        .skip(order_index)
        .find_map(|(candidate_order, pattern_number)| {
            let pattern = song.patterns.get(pattern_number)?;
            let first_row = if candidate_order == order_index {
                row_index.saturating_add(1)
            } else {
                0
            };
            pattern
                .rows
                .iter()
                .skip(first_row)
                .filter_map(|row| row.get(lane_index))
                .find_map(|cell| match cell.note {
                    Note::Off | Note::On(_) => Some(true),
                    Note::Empty => None,
                })
        })
        .unwrap_or(false)
}

fn playback_schedules(
    song: &Song,
    config: &ExternalMidiConfig,
    order: usize,
    row: usize,
) -> Result<(Vec<ScheduledMessage>, Vec<ScheduledMessage>)> {
    let first = schedule_for_pass(song, config, order, row, 1, false)?;
    let repeat = schedule_for_pass(song, config, order, 0, 2, false)?;
    Ok((first, repeat))
}

/// MIDI clock is always 24 pulses per quarter note. When the tracker uses a
/// row count that does not divide 24, distribute pulses across rows without
/// changing the average clock rate.
fn midi_clock_offsets(
    step: usize,
    steps_per_beat: u8,
    row_duration: Duration,
) -> impl Iterator<Item = Duration> {
    let steps = usize::from(steps_per_beat);
    let phase = step % steps;
    let first_tick = (phase * 24).div_ceil(steps);
    let end_tick = ((phase + 1) * 24).div_ceil(steps);
    (first_tick..end_tick).map(move |tick| {
        let numerator = tick * steps - phase * 24;
        row_duration.mul_f64(numerator as f64 / 24.0)
    })
}

fn release_active_notes(
    out: &mut Vec<ScheduledMessage>,
    at: Duration,
    order: usize,
    row: usize,
    active: &mut BTreeMap<usize, (PageTarget, u8, u8, bool)>,
) {
    for (lane_index, (target, channel, note, percussion)) in std::mem::take(active) {
        // Preserve one-shot drum tails across the scheduled arrangement end.
        // Stop/route changes issue their own explicit cleanup.
        if percussion {
            continue;
        }
        push_lane(
            out,
            at,
            order,
            row,
            vec![0x80 | channel, note, 0],
            lane_index,
            &target,
        );
    }
}

#[derive(Clone, Copy)]
struct SchedulePosition {
    at: Duration,
    order: usize,
    row: usize,
}

fn append_program(
    out: &mut Vec<ScheduledMessage>,
    position: SchedulePosition,
    page: &Page,
    column: &ColumnSetup,
    mut program: u8,
    config: &ExternalMidiConfig,
    device_profiles: &DeviceProfiles,
) {
    if matches!(
        page.target,
        PageTarget::ActiveInstrument
            | PageTarget::Synthv1(_)
            | PageTarget::Software(_)
            | PageTarget::InternalDrums(_)
    ) {
        return;
    }
    let mut selection = config.clone();
    if page.target == PageTarget::Default {
        let Some(machine_program) = page
            .percussion
            .then_some(config.percussion_program)
            .flatten()
        else {
            return;
        };
        program = machine_program;
    }
    let profile = page
        .device_profile
        .as_deref()
        .and_then(|id| device_profiles.by_id(id))
        .or_else(|| {
            matches!(
                page.target,
                PageTarget::Default | PageTarget::ConfiguredExternal
            )
            .then(|| device_profiles.by_id(&config.profile))
            .flatten()
        });
    if let Some(profile) = profile {
        profile.apply_midi_selection(&mut selection);
    }
    match selection.bank_select {
        BankSelectMode::Off => {}
        BankSelectMode::Cc0 => push(
            out,
            position.at,
            position.order,
            position.row,
            vec![0xb0 | column.channel, 0, column.bank_msb],
            Some(page.target.clone()),
        ),
        BankSelectMode::Cc0Cc32 => {
            push(
                out,
                position.at,
                position.order,
                position.row,
                vec![0xb0 | column.channel, 0, column.bank_msb],
                Some(page.target.clone()),
            );
            push(
                out,
                position.at,
                position.order,
                position.row,
                vec![0xb0 | column.channel, 32, column.bank_lsb],
                Some(page.target.clone()),
            );
        }
    }
    if selection.program_changes {
        push(
            out,
            position.at,
            position.order,
            position.row,
            vec![0xc0 | column.channel, program],
            Some(page.target.clone()),
        );
    }
}
fn push(
    out: &mut Vec<ScheduledMessage>,
    at: Duration,
    order: usize,
    row: usize,
    bytes: Vec<u8>,
    target: Option<PageTarget>,
) {
    out.push(ScheduledMessage {
        at,
        bytes,
        order,
        row,
        lane: None,
        target,
        automation: false,
        effect: None,
    });
}

fn push_lane(
    out: &mut Vec<ScheduledMessage>,
    at: Duration,
    order: usize,
    row: usize,
    bytes: Vec<u8>,
    lane: usize,
    target: &PageTarget,
) {
    out.push(ScheduledMessage {
        at,
        bytes,
        order,
        row,
        lane: Some(lane),
        target: Some(target.clone()),
        automation: false,
        effect: None,
    });
}

#[cfg(test)]
fn message_channel(bytes: &[u8]) -> Option<u8> {
    let status = *bytes.first()?;
    (0x80..=0xef).contains(&status).then_some(status & 0x0f)
}

pub fn panic_messages(channels: impl IntoIterator<Item = u8>) -> Vec<Vec<u8>> {
    let channels = channels.into_iter().collect::<BTreeSet<_>>();
    channels
        .into_iter()
        .flat_map(|ch| {
            [
                vec![0xb0 | ch, 64, 0],
                vec![0xb0 | ch, 123, 0],
                vec![0xb0 | ch, 120, 0],
            ]
        })
        .collect()
}

#[derive(Clone, Debug, Default)]
pub struct SequencerStatus {
    pub available: bool,
    pub playing: bool,
    pub order: usize,
    pub row: usize,
    pub error: Option<String>,
    pub generation: u64,
    pub targets: BTreeMap<PageTarget, Option<String>>,
    pub fallbacks: BTreeMap<PageTarget, String>,
    pub live_pattern: Option<u16>,
    pub queued_pattern: Option<crate::live_performance::QueuedPattern>,
    /// A quantized launch has reached its boundary and is waiting for the UI
    /// thread to replace/configure the single managed software engine.
    pub live_prepare: Option<crate::live_performance::QueuedPattern>,
    pub live_activation_serial: u64,
    pub live_activation: Option<crate::live_performance::ActivatedPattern>,
    /// Pattern-owned Loop Mix transport identity. `loop_order` distinguishes
    /// repeated Arrangement references to the same Pattern.
    pub loop_pattern: Option<u16>,
    pub loop_order: Option<usize>,
    pub loop_row: usize,
    pub loop_activation_serial: u64,
    pub row_started_at: Option<Instant>,
    pub row_duration: Duration,
    pub pattern_tick: u32,
    pub count_in: Option<u8>,
    /// Performance Fill latch. Changes are heard at the next playback cycle boundary.
    pub fill: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportPosition {
    pub order: usize,
    pub row: usize,
    pub pattern_tick: u32,
}
enum Transport {
    Play(u64, Song, usize, usize),
    RefreshLoop(Song),
    Stop(u64),
    Mute(usize, bool),
    Thru(PageTarget, Vec<u8>),
    CancelThru(PageTarget, u8),
    Tempo(Bpm),
    Fill(bool),
    PrepareLiveSwitch(mpsc::Sender<()>),
    LiveQueue(Song, crate::live_performance::QueuedPattern, bool),
    LivePrepared(bool, Option<String>),
    LiveCancel,
    LiveImmediate(Song, u16, bool),
    Shutdown,
}

enum CountInCommand {
    Start {
        generation: u64,
        song: Song,
        order: usize,
        row: usize,
        beats: u8,
        beat: Duration,
    },
    Cancel,
    Shutdown,
}

struct ActiveCountIn {
    generation: u64,
    song: Song,
    order: usize,
    row: usize,
    remaining: u8,
    beat: Duration,
    deadline: Instant,
}

#[derive(Clone)]
pub struct LiveInput {
    tx: mpsc::Sender<Transport>,
}

impl LiveInput {
    pub fn send(&self, target: &PageTarget, message: &[u8]) {
        let _ = self
            .tx
            .send(Transport::Thru(target.clone(), message.to_vec()));
    }

    pub fn cancel(&self, target: &PageTarget, channel: u8) {
        let _ = self.tx.send(Transport::CancelThru(target.clone(), channel));
    }
}

pub struct Sequencer {
    tx: mpsc::Sender<Transport>,
    count_in_tx: mpsc::Sender<CountInCommand>,
    status: Arc<Mutex<SequencerStatus>>,
    thread: Option<thread::JoinHandle<()>>,
    count_in_thread: Option<thread::JoinHandle<()>>,
    config: ExternalMidiConfig,
}
impl Sequencer {
    pub fn start_with_clock(
        config: &ExternalMidiConfig,
        instrument: crate::engine::SharedOutput,
        drums: crate::drums_host::SharedDrumOutput,
        clock: Arc<crate::loop_player::TransportClock>,
        effect_hub: Arc<crate::effects::EffectControlHub>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(SequencerStatus::default()));
        let thread_status = Arc::clone(&status);
        let cfg = config.clone();
        let handle = thread::Builder::new()
            .name("shsynth-sequencer".into())
            .spawn(move || {
                run_transport(rx, thread_status, cfg, instrument, drums, clock, effect_hub)
            })
            .ok();
        let (count_in_tx, count_in_rx) = mpsc::channel();
        let count_in_status = Arc::clone(&status);
        let count_in_transport = tx.clone();
        let count_in_thread = thread::Builder::new()
            .name("shsynth-count-in".into())
            .spawn(move || run_count_in(count_in_rx, count_in_transport, count_in_status))
            .ok();
        Self {
            tx,
            count_in_tx,
            status,
            thread: handle,
            count_in_thread,
            config: config.clone(),
        }
    }
    pub fn play(&self, song: &Song, order: usize, row: usize) {
        let _ = self.count_in_tx.send(CountInCommand::Cancel);
        let generation = if let Ok(mut status) = self.status.lock() {
            status.playing = true;
            status.order = order;
            status.row = row;
            status.generation = status.generation.wrapping_add(1);
            status.generation
        } else {
            0
        };
        let _ = self
            .tx
            .send(Transport::Play(generation, song.clone(), order, row));
    }
    pub fn count_in(&self, song: &Song, order: usize, row: usize, beats: u8) {
        let tempo = song
            .order
            .get(order)
            .and_then(|number| song.patterns.get(number))
            .map_or(self.config.default_tempo, |pattern| pattern.tempo);
        let generation = if let Ok(mut status) = self.status.lock() {
            status.playing = false;
            status.order = order;
            status.row = row;
            status.count_in = Some(beats);
            status.row_started_at = None;
            status.pattern_tick = u32::try_from(row)
                .unwrap_or_default()
                .saturating_mul(AUTOMATION_TICKS_PER_ROW);
            status.generation = status.generation.wrapping_add(1);
            status.generation
        } else {
            return;
        };
        let _ = self.count_in_tx.send(CountInCommand::Start {
            generation,
            song: song.clone(),
            order,
            row,
            beats,
            beat: Duration::from_secs_f64(60.0 / tempo.as_f64()),
        });
    }
    /// Replace the material used at the next loop boundary without disturbing
    /// the cycle that is currently sounding.
    pub fn refresh_loop(&self, song: &Song) {
        let _ = self.tx.send(Transport::RefreshLoop(song.clone()));
    }
    pub fn live_input(&self) -> LiveInput {
        LiveInput {
            tx: self.tx.clone(),
        }
    }
    pub fn stop(&self) {
        let _ = self.count_in_tx.send(CountInCommand::Cancel);
        let generation = if let Ok(mut status) = self.status.lock() {
            status.playing = false;
            status.count_in = None;
            status.generation = status.generation.wrapping_add(1);
            status.generation
        } else {
            0
        };
        let _ = self.tx.send(Transport::Stop(generation));
    }
    pub fn mute(&self, track: usize, muted: bool) {
        let _ = self.tx.send(Transport::Mute(track, muted));
    }
    pub fn mute_page(&self, page: usize, muted: bool) {
        for lane in 0..LANES_PER_PAGE {
            let _ = self
                .tx
                .send(Transport::Mute(page * LANES_PER_PAGE + lane, muted));
        }
    }
    pub fn tempo(&self, bpm: Bpm) {
        let _ = self.tx.send(Transport::Tempo(bpm));
    }
    pub fn fill(&self, enabled: bool) {
        if let Ok(mut status) = self.status.lock() {
            status.fill = enabled;
        }
        let _ = self.tx.send(Transport::Fill(enabled));
    }
    /// Stop scheduled owners and wait until their exact note-offs have been
    /// sent before the UI replaces the one managed software engine.
    pub fn prepare_live_switch(&self) -> bool {
        let (tx, rx) = mpsc::channel();
        self.tx.send(Transport::PrepareLiveSwitch(tx)).is_ok()
            && rx.recv_timeout(Duration::from_secs(2)).is_ok()
    }
    pub fn live_queue(
        &self,
        song: &Song,
        queued: crate::live_performance::QueuedPattern,
        requires_engine_prepare: bool,
    ) {
        let _ = self.tx.send(Transport::LiveQueue(
            song.clone(),
            queued,
            requires_engine_prepare,
        ));
    }
    pub fn live_prepared(&self, success: bool, error: Option<String>) {
        let _ = self.tx.send(Transport::LivePrepared(success, error));
    }
    pub fn live_cancel(&self) {
        let _ = self.tx.send(Transport::LiveCancel);
    }
    pub fn live_immediate(&self, song: &Song, pattern: u16, retrigger: bool) {
        let _ = self
            .tx
            .send(Transport::LiveImmediate(song.clone(), pattern, retrigger));
    }
    pub fn thru(&self, message: &[u8]) {
        if self.config.live_thru {
            let _ = self.tx.send(Transport::Thru(
                PageTarget::ConfiguredExternal,
                message.to_vec(),
            ));
        }
    }
    pub fn status(&self) -> SequencerStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn position_at(&self, received: Instant) -> Option<TransportPosition> {
        let status = self.status.lock().ok()?;
        if !status.playing {
            return None;
        }
        let row_started = status.row_started_at?;
        let elapsed = received.saturating_duration_since(row_started);
        let within = if status.row_duration.is_zero() {
            0
        } else {
            elapsed
                .as_nanos()
                .saturating_mul(u128::from(AUTOMATION_TICKS_PER_ROW))
                .checked_div(status.row_duration.as_nanos())
                .unwrap_or_default()
                .min(u128::from(AUTOMATION_TICKS_PER_ROW.saturating_sub(1))) as u32
        };
        Some(TransportPosition {
            order: status.order,
            row: status.row,
            pattern_tick: status.pattern_tick.saturating_add(within),
        })
    }
}
impl Drop for Sequencer {
    fn drop(&mut self) {
        let _ = self.count_in_tx.send(CountInCommand::Shutdown);
        let _ = self.tx.send(Transport::Shutdown);
        if let Some(handle) = self.count_in_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum NoteOwner {
    Lane(usize),
    Live,
}

type NoteOwners = BTreeMap<(PageTarget, u8, u8), BTreeSet<NoteOwner>>;

fn run_count_in(
    rx: mpsc::Receiver<CountInCommand>,
    transport: mpsc::Sender<Transport>,
    status: Arc<Mutex<SequencerStatus>>,
) {
    let mut active: Option<ActiveCountIn> = None;
    loop {
        let timeout = active.as_ref().map_or(Duration::from_secs(60), |count| {
            count.deadline.saturating_duration_since(Instant::now())
        });
        match rx.recv_timeout(timeout) {
            Ok(CountInCommand::Start {
                generation,
                song,
                order,
                row,
                beats,
                beat,
            }) => {
                active = Some(ActiveCountIn {
                    generation,
                    song,
                    order,
                    row,
                    remaining: beats,
                    beat,
                    deadline: Instant::now() + beat,
                });
            }
            Ok(CountInCommand::Cancel) => active = None,
            Ok(CountInCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let Some(mut count) = active.take() else {
                    continue;
                };
                if count.remaining > 1 {
                    count.remaining -= 1;
                    count.deadline += count.beat;
                    if let Ok(mut current) = status.lock() {
                        if current.generation != count.generation || current.count_in.is_none() {
                            continue;
                        }
                        current.count_in = Some(count.remaining);
                    }
                    active = Some(count);
                } else {
                    let valid = status.lock().is_ok_and(|current| {
                        current.generation == count.generation && current.count_in.is_some()
                    });
                    if valid {
                        let _ = transport.send(Transport::Play(
                            count.generation,
                            count.song,
                            count.order,
                            count.row,
                        ));
                    }
                }
            }
        }
    }
}

fn run_transport(
    rx: mpsc::Receiver<Transport>,
    status: Arc<Mutex<SequencerStatus>>,
    config: ExternalMidiConfig,
    instrument: crate::engine::SharedOutput,
    drums: crate::drums_host::SharedDrumOutput,
    clock: Arc<crate::loop_player::TransportClock>,
    effect_hub: Arc<crate::effects::EffectControlHub>,
) {
    let mut outputs = DestinationPool::new(config.clone(), instrument, drums);
    let mut messages = Vec::new();
    let mut repeat_messages = Vec::new();
    let mut index = 0;
    let mut started = Instant::now();
    let mut muted = BTreeSet::new();
    let mut active_notes: BTreeMap<usize, (PageTarget, u8, BTreeSet<u8>)> = BTreeMap::new();
    let mut note_owners: NoteOwners = BTreeMap::new();
    let mut live_notes = BTreeSet::new();
    let mut transport_targets = BTreeSet::new();
    let mut transport_tempo = config.default_tempo;
    let mut loop_origin_beat = 0.0;
    let mut playback_song: Option<Song> = None;
    let mut playback_start_order = 0usize;
    let mut playback_pass = 0u32;
    let mut fill = false;
    let mut sounding_loop_order: Option<usize> = None;
    let mut live: Option<LiveRuntime> = None;
    let mut automation_cc = crate::automation::CcPublisher::new(Instant::now());
    loop {
        while let Some((target, bytes)) = automation_cc.flush(Instant::now()) {
            if let Err(error) = outputs.send(&target, &bytes) {
                if let Ok(mut s) = status.lock() {
                    s.targets.insert(target, Some(error.clone()));
                    s.error = Some(error);
                }
                break;
            }
        }
        let timeout = messages
            .get(index)
            .map(|m: &ScheduledMessage| (started + m.at).saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_millis(50))
            .min(Duration::from_millis(50));
        match rx.recv_timeout(timeout) {
            Ok(Transport::Play(generation, song, order, row)) => {
                automation_cc.clear(Instant::now());
                live = None;
                cleanup_owned_notes(&mut outputs, &mut note_owners);
                active_notes.clear();
                live_notes.clear();
                match playback_schedules(&song, &config, order, row) {
                    Ok((first, repeat)) => {
                        messages = first;
                        repeat_messages = repeat;
                    }
                    Err(error) => {
                        messages.clear();
                        repeat_messages.clear();
                        transport_targets.clear();
                        if let Ok(mut s) = status.lock() {
                            if s.generation != generation {
                                continue;
                            }
                            s.playing = false;
                            s.targets.clear();
                            s.fallbacks.clear();
                            s.error = Some(error.to_string());
                        }
                        continue;
                    }
                }
                transport_targets = messages
                    .iter()
                    .chain(repeat_messages.iter())
                    .filter_map(|message| message.target.clone())
                    .collect();
                for target in &transport_targets {
                    outputs.refresh(target);
                }
                update_target_status(&status, &outputs, &transport_targets);
                index = 0;
                started = Instant::now();
                transport_tempo = song
                    .order
                    .get(order)
                    .and_then(|number| song.patterns.get(number))
                    .map_or(config.default_tempo, |pattern| pattern.tempo);
                let playback_steps = song.steps_per_beat;
                let first_origin_beat = row as f64 / f64::from(song.steps_per_beat);
                loop_origin_beat = 0.0;
                playback_pass = 1;
                playback_start_order = order;
                fill = false;
                clock.play(first_origin_beat, transport_tempo);
                let loop_pattern = song.order.get(order).copied();
                playback_song = Some(song);
                sounding_loop_order = Some(order);
                muted.clear();
                active_notes.clear();
                note_owners.clear();
                live_notes.clear();
                if config.send_transport {
                    for target in &transport_targets {
                        let _ = outputs.send(target, &[0xfa]);
                    }
                }
                if let Ok(mut s) = status.lock() {
                    if s.generation != generation {
                        continue;
                    }
                    s.playing = true;
                    s.count_in = None;
                    s.order = order;
                    s.row = row;
                    s.loop_pattern = loop_pattern;
                    s.loop_order = Some(order);
                    s.loop_row = row;
                    s.loop_activation_serial = s.loop_activation_serial.wrapping_add(1);
                    s.row_started_at = Some(started);
                    s.row_duration = Duration::from_secs_f64(
                        60.0 / transport_tempo.as_f64() / f64::from(playback_steps),
                    );
                    s.pattern_tick = u32::try_from(row)
                        .unwrap_or_default()
                        .saturating_mul(AUTOMATION_TICKS_PER_ROW);
                    s.fill = false;
                }
            }
            Ok(Transport::RefreshLoop(song)) => {
                match schedule_for_pass(
                    &song,
                    &config,
                    playback_start_order,
                    0,
                    playback_pass.saturating_add(1),
                    fill,
                ) {
                    Ok(next_cycle) => {
                        repeat_messages = next_cycle;
                        playback_song = Some(song);
                        transport_targets.extend(
                            repeat_messages
                                .iter()
                                .filter_map(|message| message.target.clone()),
                        );
                        for target in &transport_targets {
                            outputs.refresh(target);
                        }
                        update_target_status(&status, &outputs, &transport_targets);
                    }
                    Err(error) => {
                        if let Ok(mut s) = status.lock() {
                            s.error = Some(error.to_string());
                        }
                    }
                }
            }
            Ok(Transport::Stop(generation)) => {
                automation_cc.clear(Instant::now());
                clock.stop();
                messages.clear();
                repeat_messages.clear();
                index = 0;
                cleanup_owned_notes(&mut outputs, &mut note_owners);
                active_notes.clear();
                live_notes.clear();
                if config.send_transport {
                    for target in &transport_targets {
                        let _ = outputs.send(target, &[0xfc]);
                    }
                }
                if let Ok(mut s) = status.lock() {
                    if s.generation == generation {
                        s.playing = false;
                        s.count_in = None;
                    }
                    s.live_pattern = None;
                    s.queued_pattern = None;
                    s.live_prepare = None;
                    s.loop_pattern = None;
                    s.loop_order = None;
                    s.row_started_at = None;
                    s.fill = false;
                }
                live = None;
                playback_song = None;
                playback_start_order = 0;
                playback_pass = 0;
                fill = false;
                sounding_loop_order = None;
            }
            Ok(Transport::Mute(lane, value)) => {
                if value {
                    muted.insert(lane);
                    if let Some((target, channel, notes)) = active_notes.remove(&lane) {
                        for note in notes {
                            if release_note_owner(
                                &mut note_owners,
                                NoteOwner::Lane(lane),
                                &target,
                                channel,
                                note,
                            ) {
                                outputs.send_cleanup(&target, &[0x80 | channel, note, 0]);
                            }
                        }
                    }
                } else {
                    muted.remove(&lane);
                }
            }
            Ok(Transport::Thru(target, message)) => {
                let mut suppress = false;
                if let [status, note, velocity, ..] = message.as_slice() {
                    let channel = status & 0x0f;
                    let key = (target.clone(), channel, *note);
                    match status & 0xf0 {
                        0x90 if *velocity > 0 => {
                            suppress = !claim_note_owner(
                                &mut note_owners,
                                NoteOwner::Live,
                                &target,
                                channel,
                                *note,
                            );
                            live_notes.insert(key);
                        }
                        0x80 | 0x90 => {
                            suppress = !release_note_owner(
                                &mut note_owners,
                                NoteOwner::Live,
                                &target,
                                channel,
                                *note,
                            );
                            live_notes.remove(&key);
                        }
                        _ => {}
                    }
                }
                if !suppress {
                    if let Err(error) = outputs.send(&target, &message) {
                        if let Ok(mut s) = status.lock() {
                            s.available = false;
                            s.error = Some(error);
                        }
                    }
                }
            }
            Ok(Transport::CancelThru(target, channel)) => {
                let matching = live_notes
                    .iter()
                    .filter(|(candidate, candidate_channel, _)| {
                        candidate == &target && *candidate_channel == channel
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                for (_, _, note) in matching {
                    live_notes.remove(&(target.clone(), channel, note));
                    if release_note_owner(&mut note_owners, NoteOwner::Live, &target, channel, note)
                    {
                        outputs.send_cleanup(&target, &[0x80 | channel, note, 0]);
                    }
                }
            }
            Ok(Transport::Tempo(bpm)) => {
                let elapsed = started.elapsed();
                rescale_schedule(&mut messages, index, elapsed, transport_tempo, bpm);
                if !repeat_messages.is_empty() {
                    rescale_schedule(
                        &mut repeat_messages,
                        0,
                        Duration::ZERO,
                        transport_tempo,
                        bpm,
                    );
                }
                transport_tempo = bpm;
                clock.tempo(bpm);
            }
            Ok(Transport::Fill(enabled)) => {
                fill = enabled;
            }
            Ok(Transport::PrepareLiveSwitch(reply)) => {
                automation_cc.clear(Instant::now());
                clock.stop();
                messages.clear();
                repeat_messages.clear();
                index = 0;
                cleanup_owned_notes(&mut outputs, &mut note_owners);
                active_notes.clear();
                live_notes.clear();
                live = None;
                playback_song = None;
                sounding_loop_order = None;
                if let Ok(mut status) = status.lock() {
                    status.playing = false;
                    status.live_pattern = None;
                    status.queued_pattern = None;
                    status.live_prepare = None;
                    status.loop_pattern = None;
                    status.loop_order = None;
                }
                let _ = reply.send(());
            }
            Ok(Transport::LiveQueue(song, queued, requires_engine_prepare)) => {
                if let Some(runtime) = live.as_mut() {
                    runtime.queued = Some(QueuedLive {
                        song,
                        requested: queued,
                        requires_engine_prepare,
                    });
                    if let Ok(mut status) = status.lock() {
                        status.queued_pattern = Some(queued);
                        status.live_prepare = None;
                        status.error = None;
                    }
                } else {
                    publish_live_failure(&status, "Live Patterns is not playing".into());
                }
            }
            Ok(Transport::LivePrepared(success, prepare_error)) => {
                let pending = live.as_mut().and_then(|runtime| runtime.pending.take());
                if let Some(pending) = pending {
                    if success {
                        match activate_live_pattern(
                            &pending.song,
                            pending.requested,
                            &config,
                            &mut outputs,
                            &mut messages,
                            &mut active_notes,
                            &mut note_owners,
                            1,
                            fill,
                            true,
                        ) {
                            Ok(targets) => {
                                muted.clear();
                                let runtime = live.as_mut().expect("pending Live runtime");
                                runtime.current = pending.requested.pattern;
                                runtime.current_song = pending.song;
                                runtime.pass = 1;
                                transport_targets = targets;
                                index = 0;
                                started = Instant::now();
                                transport_tempo =
                                    runtime.current_song.patterns[&runtime.current].tempo;
                                clock.play(0.0, transport_tempo);
                                publish_live_activation(&status, pending.requested);
                                update_target_status(&status, &outputs, &transport_targets);
                            }
                            Err(error) => publish_live_failure(&status, error),
                        }
                    } else {
                        let error = prepare_error
                            .unwrap_or_else(|| "managed instrument could not be prepared".into());
                        publish_live_failure(&status, error.clone());
                        if let Some(runtime) = live.as_mut() {
                            let fallback = crate::live_performance::QueuedPattern {
                                pattern: runtime.current,
                                quantization: crate::live_performance::LaunchQuantization::Pattern,
                                retrigger: true,
                            };
                            if let Ok(targets) = activate_live_pattern(
                                &runtime.current_song,
                                fallback,
                                &config,
                                &mut outputs,
                                &mut messages,
                                &mut active_notes,
                                &mut note_owners,
                                runtime.pass.saturating_add(1),
                                fill,
                                true,
                            ) {
                                muted.clear();
                                runtime.pass = runtime.pass.saturating_add(1);
                                transport_targets = targets;
                                index = 0;
                                started = Instant::now();
                                transport_tempo =
                                    runtime.current_song.patterns[&runtime.current].tempo;
                                clock.play(0.0, transport_tempo);
                                update_target_status(&status, &outputs, &transport_targets);
                                if let Ok(mut status) = status.lock() {
                                    status.playing = true;
                                    status.error = Some(error);
                                }
                            }
                        }
                    }
                }
                if let Ok(mut status) = status.lock() {
                    status.live_prepare = None;
                }
            }
            Ok(Transport::LiveCancel) => {
                if let Some(runtime) = live.as_mut() {
                    runtime.queued = None;
                }
                if let Ok(mut status) = status.lock() {
                    status.queued_pattern = None;
                    status.live_prepare = None;
                }
            }
            Ok(Transport::LiveImmediate(song, pattern, retrigger)) => {
                let requested = crate::live_performance::QueuedPattern {
                    pattern,
                    quantization: crate::live_performance::LaunchQuantization::Pattern,
                    retrigger,
                };
                match activate_live_pattern(
                    &song,
                    requested,
                    &config,
                    &mut outputs,
                    &mut messages,
                    &mut active_notes,
                    &mut note_owners,
                    1,
                    fill,
                    true,
                ) {
                    Ok(targets) => {
                        muted.clear();
                        transport_targets = targets;
                        index = 0;
                        started = Instant::now();
                        transport_tempo = song.patterns[&pattern].tempo;
                        clock.play(0.0, transport_tempo);
                        live = Some(LiveRuntime {
                            current_song: song,
                            current: pattern,
                            pass: 1,
                            queued: None,
                            pending: None,
                        });
                        playback_song = None;
                        sounding_loop_order = None;
                        publish_live_activation(&status, requested);
                        update_target_status(&status, &outputs, &transport_targets);
                    }
                    Err(error) => publish_live_failure(&status, error),
                }
            }
            Ok(Transport::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                clock.stop();
                cleanup_owned_notes(&mut outputs, &mut note_owners);
                active_notes.clear();
                live_notes.clear();
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if let Some(runtime) = live.as_mut() {
            let due_bar_launch = runtime.queued.as_ref().is_some_and(|queued| {
                queued.requested.quantization == crate::live_performance::LaunchQuantization::Bar
                    && messages.get(index).is_some_and(|message| {
                        message.bytes.is_empty()
                            && message.effect.is_none()
                            && started + message.at <= Instant::now()
                            && crate::live_performance::is_launch_boundary(
                                queued.requested.quantization,
                                message.row,
                                runtime.current_song.patterns[&runtime.current].rows.len(),
                                runtime.current_song.steps_per_beat,
                                runtime.current_song.patterns[&runtime.current].meter,
                            )
                    })
            });
            if due_bar_launch {
                let queued = runtime.queued.take().expect("bar launch was queued");
                if queued.requires_engine_prepare {
                    // The boundary belongs to the queued Pattern even while its
                    // managed instrument is prepared. Invalidate the outgoing
                    // Pattern's loop renderers synchronously so none can sound
                    // underneath the replacement or its recovery path.
                    clock.restart_cycle(0.0);
                    stage_live_prepare(
                        runtime,
                        queued,
                        &status,
                        &mut outputs,
                        &mut messages,
                        &mut active_notes,
                        &mut note_owners,
                    );
                    index = 0;
                } else {
                    match activate_live_pattern(
                        &queued.song,
                        queued.requested,
                        &config,
                        &mut outputs,
                        &mut messages,
                        &mut active_notes,
                        &mut note_owners,
                        1,
                        fill,
                        false,
                    ) {
                        Ok(targets) => {
                            muted.clear();
                            runtime.current = queued.requested.pattern;
                            runtime.current_song = queued.song;
                            runtime.pass = 1;
                            transport_targets = targets;
                            index = 0;
                            started = Instant::now();
                            transport_tempo = runtime.current_song.patterns[&runtime.current].tempo;
                            clock.restart_cycle(0.0);
                            clock.tempo(transport_tempo);
                            publish_live_activation(&status, queued.requested);
                            update_target_status(&status, &outputs, &transport_targets);
                        }
                        Err(error) => publish_live_failure(&status, error),
                    }
                }
            }
        }
        while let Some(message) = messages
            .get(index)
            .filter(|m| started + m.at <= Instant::now())
        {
            if message.bytes.is_empty() && message.effect.is_none() {
                if live.is_none() && sounding_loop_order != Some(message.order) {
                    if let Some(song) = playback_song.as_ref() {
                        if let Some(pattern_number) = song.order.get(message.order).copied() {
                            let pattern = &song.patterns[&pattern_number];
                            transport_tempo = pattern.tempo;
                            clock
                                .restart_cycle(message.row as f64 / f64::from(song.steps_per_beat));
                            clock.tempo(transport_tempo);
                            sounding_loop_order = Some(message.order);
                            if let Ok(mut state) = status.lock() {
                                state.loop_pattern = Some(pattern_number);
                                state.loop_order = Some(message.order);
                                state.loop_row = message.row;
                                state.loop_activation_serial =
                                    state.loop_activation_serial.wrapping_add(1);
                            }
                        }
                    }
                }
                if let Some(next) = messages[index + 1..].iter().find(|candidate| {
                    candidate.bytes.is_empty()
                        && candidate.effect.is_none()
                        && candidate.at > message.at
                }) {
                    let seconds = (next.at - message.at).as_secs_f64();
                    if seconds > 0.0 {
                        if let Ok(tapped) =
                            format!("{:.2}", 60.0 / seconds / f64::from(config.steps_per_beat))
                                .parse::<Bpm>()
                        {
                            clock.tempo(tapped);
                        }
                    }
                }
            }
            if let Some(effect) = message.effect.as_ref() {
                if let Err(error) = effect_hub.publish_normalized(
                    effect.effect_id,
                    effect.effect_kind,
                    effect.effect_version,
                    effect.parameter.as_deref(),
                    effect.value,
                ) {
                    if let Ok(mut s) = status.lock() {
                        s.error = Some(format!("EFFECT AUTOMATION REJECTED · {error}"));
                    }
                }
                index += 1;
                continue;
            }
            let muted_message = message.lane.is_some_and(|lane| muted.contains(&lane));
            let mut owned_note_suppressed = false;
            if !muted_message {
                if let (Some(lane), Some(target), [midi_status, note, ..]) = (
                    message.lane,
                    message.target.as_ref(),
                    message.bytes.as_slice(),
                ) {
                    let channel = midi_status & 0x0f;
                    match midi_status & 0xf0 {
                        0x90 if message.bytes.get(2).copied().unwrap_or(0) > 0 => {
                            owned_note_suppressed = !claim_note_owner(
                                &mut note_owners,
                                NoteOwner::Lane(lane),
                                target,
                                channel,
                                *note,
                            );
                        }
                        0x80 | 0x90 => {
                            owned_note_suppressed = !release_note_owner(
                                &mut note_owners,
                                NoteOwner::Lane(lane),
                                target,
                                channel,
                                *note,
                            );
                        }
                        _ => {}
                    }
                }
            }
            let automation_bytes = if message.automation {
                message
                    .target
                    .as_ref()
                    .and_then(|target| automation_cc.offer(target, &message.bytes, Instant::now()))
            } else {
                Some(message.bytes.clone())
            };
            let send_error = if message.bytes.is_empty()
                || muted_message
                || owned_note_suppressed
                || automation_bytes.is_none()
            {
                None
            } else {
                message
                    .target
                    .as_ref()
                    .and_then(|target| outputs.send(target, automation_bytes.as_ref()?).err())
            };
            if !muted_message {
                update_active_notes(
                    &mut active_notes,
                    message.lane,
                    message.target.as_ref(),
                    &message.bytes,
                );
            }
            if let Some(error) = send_error {
                if let Some(target) = message.target.clone() {
                    release_target_notes(
                        &mut outputs,
                        &mut note_owners,
                        &mut active_notes,
                        &target,
                    );
                }
                if let Ok(mut s) = status.lock() {
                    s.available = false;
                    if let Some(target) = &message.target {
                        s.targets.insert(target.clone(), Some(error.clone()));
                    }
                    s.error = Some(error);
                }
            }
            if message.bytes.is_empty() && message.effect.is_none() {
                if let Ok(mut s) = status.lock() {
                    s.order = message.order;
                    s.row = message.row;
                    s.row_started_at = Some(started + message.at);
                    let steps = playback_song
                        .as_ref()
                        .map_or(config.steps_per_beat, |song| song.steps_per_beat);
                    s.row_duration =
                        Duration::from_secs_f64(60.0 / transport_tempo.as_f64() / f64::from(steps));
                    s.pattern_tick = u32::try_from(message.row)
                        .unwrap_or_default()
                        .saturating_mul(AUTOMATION_TICKS_PER_ROW);
                }
            }
            index += 1;
        }
        if !messages.is_empty() && index == messages.len() {
            if let Some(runtime) = live.as_mut() {
                let queued_command = runtime.queued.take();
                if queued_command
                    .as_ref()
                    .is_some_and(|queued| queued.requires_engine_prepare)
                {
                    let queued = queued_command.expect("queued engine preparation");
                    clock.restart_cycle(0.0);
                    stage_live_prepare(
                        runtime,
                        queued,
                        &status,
                        &mut outputs,
                        &mut messages,
                        &mut active_notes,
                        &mut note_owners,
                    );
                    index = 0;
                    continue;
                }
                let queued_activation = queued_command.is_some();
                let (next_song, requested) = queued_command.as_ref().map_or_else(
                    || {
                        (
                            &runtime.current_song,
                            crate::live_performance::QueuedPattern {
                                pattern: runtime.current,
                                quantization: crate::live_performance::LaunchQuantization::Pattern,
                                retrigger: true,
                            },
                        )
                    },
                    |queued| (&queued.song, queued.requested),
                );
                let next_pass = if queued_activation {
                    1
                } else {
                    runtime.pass.saturating_add(1)
                };
                match activate_live_pattern(
                    next_song,
                    requested,
                    &config,
                    &mut outputs,
                    &mut messages,
                    &mut active_notes,
                    &mut note_owners,
                    next_pass,
                    fill,
                    false,
                ) {
                    Ok(targets) => {
                        muted.clear();
                        runtime.current = requested.pattern;
                        runtime.pass = next_pass;
                        if let Some(queued) = queued_command {
                            runtime.current_song = queued.song;
                        }
                        transport_targets = targets;
                        index = 0;
                        started = Instant::now();
                        transport_tempo = runtime.current_song.patterns[&requested.pattern].tempo;
                        clock.restart_cycle(0.0);
                        clock.tempo(transport_tempo);
                        if queued_activation {
                            publish_live_activation(&status, requested);
                        }
                        update_target_status(&status, &outputs, &transport_targets);
                    }
                    Err(error) => {
                        publish_live_failure(&status, error);
                        let fallback = crate::live_performance::QueuedPattern {
                            pattern: runtime.current,
                            quantization: crate::live_performance::LaunchQuantization::Pattern,
                            retrigger: true,
                        };
                        if let Ok(targets) = activate_live_pattern(
                            &runtime.current_song,
                            fallback,
                            &config,
                            &mut outputs,
                            &mut messages,
                            &mut active_notes,
                            &mut note_owners,
                            runtime.pass.saturating_add(1),
                            fill,
                            false,
                        ) {
                            muted.clear();
                            runtime.pass = runtime.pass.saturating_add(1);
                            transport_targets = targets;
                            index = 0;
                            started = Instant::now();
                        }
                    }
                }
            } else {
                cleanup_owned_notes(&mut outputs, &mut note_owners);
                active_notes.clear();
                live_notes.clear();
                playback_pass = playback_pass.saturating_add(1).max(2);
                if playback_pass == 2 && !repeat_messages.is_empty() {
                    messages = std::mem::take(&mut repeat_messages);
                } else if let Some(song) = playback_song.as_ref() {
                    match schedule_for_pass(
                        song,
                        &config,
                        playback_start_order,
                        0,
                        playback_pass,
                        fill,
                    ) {
                        Ok(next) => messages = next,
                        Err(error) => {
                            messages.clear();
                            if let Ok(mut state) = status.lock() {
                                state.playing = false;
                                state.error = Some(error.to_string());
                            }
                            continue;
                        }
                    }
                }
                index = 0;
                started = Instant::now();
                clock.restart_cycle(loop_origin_beat);
                sounding_loop_order = None;
            }
        }
    }
}

struct LiveRuntime {
    current_song: Song,
    current: u16,
    pass: u32,
    queued: Option<QueuedLive>,
    pending: Option<QueuedLive>,
}

struct QueuedLive {
    song: Song,
    requested: crate::live_performance::QueuedPattern,
    requires_engine_prepare: bool,
}

#[allow(clippy::too_many_arguments)]
fn stage_live_prepare(
    runtime: &mut LiveRuntime,
    queued: QueuedLive,
    status: &Arc<Mutex<SequencerStatus>>,
    outputs: &mut DestinationPool,
    messages: &mut Vec<ScheduledMessage>,
    active_notes: &mut BTreeMap<usize, (PageTarget, u8, BTreeSet<u8>)>,
    note_owners: &mut NoteOwners,
) {
    cleanup_owned_notes(outputs, note_owners);
    active_notes.clear();
    messages.clear();
    if let Ok(mut status) = status.lock() {
        status.live_prepare = Some(queued.requested);
        status.queued_pattern = Some(queued.requested);
        status.playing = false;
        status.error = None;
    }
    runtime.pending = Some(queued);
}

#[allow(clippy::too_many_arguments)]
fn activate_live_pattern(
    song: &Song,
    requested: crate::live_performance::QueuedPattern,
    config: &ExternalMidiConfig,
    outputs: &mut DestinationPool,
    messages: &mut Vec<ScheduledMessage>,
    active_notes: &mut BTreeMap<usize, (PageTarget, u8, BTreeSet<u8>)>,
    note_owners: &mut NoteOwners,
    pass: u32,
    fill: bool,
    cleanup: bool,
) -> std::result::Result<BTreeSet<PageTarget>, String> {
    let pattern = song
        .patterns
        .get(&requested.pattern)
        .ok_or_else(|| format!("Pattern {:02} is missing", requested.pattern))?;
    let targets = pattern
        .pages
        .iter()
        .filter(|page| page.enabled)
        .map(|page| page.target.clone())
        .collect::<BTreeSet<_>>();
    for target in &targets {
        outputs.refresh(target);
    }
    if let Some((target, error)) = targets
        .iter()
        .find_map(|target| outputs.error(target).map(|error| (target, error)))
    {
        return Err(format!("{}: {error}", target.label()));
    }
    let mut live_song = song.clone();
    live_song.order = vec![requested.pattern];
    let mut scheduled = schedule_for_pass(&live_song, config, 0, 0, pass, fill)
        .map_err(|error| error.to_string())?;
    strip_live_boundary_releases(requested.pattern, pattern, pass, fill, &mut scheduled);
    if cleanup {
        cleanup_owned_notes(outputs, note_owners);
        active_notes.clear();
    } else {
        transfer_held_lanes(pattern, config, active_notes, &mut scheduled);
    }
    *messages = scheduled;
    Ok(targets)
}

fn strip_live_boundary_releases(
    pattern_number: u16,
    pattern: &Pattern,
    pass: u32,
    fill: bool,
    messages: &mut Vec<ScheduledMessage>,
) {
    let boundary = messages
        .iter()
        .filter(|message| message.bytes.is_empty() && message.effect.is_none())
        .map(|message| message.at)
        .max()
        .unwrap_or_default();
    let held_lanes = (0..pattern.total_lanes())
        .filter(|lane| lane_holds_at_pattern_end(pattern_number, pattern, *lane, pass, fill))
        .collect::<BTreeSet<_>>();
    messages.retain(|message| {
        !(message.at == boundary
            && message.lane.is_some_and(|lane| held_lanes.contains(&lane))
            && message
                .bytes
                .first()
                .is_some_and(|status| status & 0xf0 == 0x80))
    });
}

fn lane_holds_at_pattern_end(
    pattern_number: u16,
    pattern: &Pattern,
    lane: usize,
    pass: u32,
    fill: bool,
) -> bool {
    let page = &pattern.pages[lane / LANES_PER_PAGE];
    let playback = page.lanes[lane % LANES_PER_PAGE].playback;
    let context = ConditionContext::playback(pass, fill);
    let mut previous = false;
    let mut held = false;
    let final_emitted_position = (0..pattern.rows.len()).rev().find_map(|transport_row| {
        let count = lane_steps_for_row(
            playback,
            pattern.rows.len(),
            transport_row,
            pattern_number,
            0,
            lane,
            pass,
            false,
        )
        .len();
        (count > 0).then_some((transport_row, count - 1))
    });
    for transport_row in 0..pattern.rows.len() {
        let steps = lane_steps_for_row(
            playback,
            pattern.rows.len(),
            transport_row,
            pattern_number,
            0,
            lane,
            pass,
            false,
        );
        for (step_index, step) in steps.into_iter().enumerate() {
            if step.boundary_before {
                held = false;
            }
            let cell = pattern.rows[step.source_row][lane];
            let triggered = if matches!(cell.note, Note::On(_)) {
                let result = cell_triggered(
                    cell,
                    context,
                    pattern_number,
                    0,
                    step.source_row,
                    lane,
                    step.probability_occurrence,
                    previous,
                );
                previous = result;
                result
            } else {
                true
            };
            if triggered {
                match cell.note {
                    Note::On(_) => held = cell.gate == Some(100),
                    Note::Off => held = false,
                    Note::Empty => {}
                }
            }
            let final_emitted_step = final_emitted_position == Some((transport_row, step_index));
            if step.boundary_after && !final_emitted_step {
                held = false;
            }
        }
    }
    held
}

fn transfer_held_lanes(
    _pattern: &Pattern,
    _config: &ExternalMidiConfig,
    active_notes: &BTreeMap<usize, (PageTarget, u8, BTreeSet<u8>)>,
    messages: &mut Vec<ScheduledMessage>,
) {
    for (&lane, (old_target, old_channel, notes)) in active_notes {
        let first_attack = messages
            .iter()
            .filter(|message| message.lane == Some(lane))
            .find_map(|message| match message.bytes.as_slice() {
                [status, note, velocity, ..] if status & 0xf0 == 0x90 && *velocity > 0 => {
                    Some((message.at, message.target.clone(), status & 0x0f, *note))
                }
                _ => None,
            })
            .filter(|(at, _, _, _)| at.is_zero());
        for &old_note in notes {
            let same = first_attack
                .as_ref()
                .is_some_and(|(_, target, channel, note)| {
                    *note == old_note
                        && target.as_ref() == Some(old_target)
                        && *channel == *old_channel
                });
            if !same {
                let release = ScheduledMessage {
                    at: Duration::ZERO,
                    bytes: vec![0x80 | old_channel, old_note, 0],
                    order: 0,
                    row: 0,
                    lane: Some(lane),
                    target: Some(old_target.clone()),
                    automation: false,
                    effect: None,
                };
                let insert_at = messages
                    .iter()
                    .position(|message| !message.at.is_zero() || !message.bytes.is_empty())
                    .unwrap_or(messages.len());
                messages.insert(insert_at, release);
            }
        }
    }
}

fn publish_live_activation(
    status: &Arc<Mutex<SequencerStatus>>,
    requested: crate::live_performance::QueuedPattern,
) {
    if let Ok(mut status) = status.lock() {
        status.live_activation_serial = status.live_activation_serial.wrapping_add(1);
        status.live_pattern = Some(requested.pattern);
        status.queued_pattern = None;
        status.live_prepare = None;
        status.live_activation = Some(crate::live_performance::ActivatedPattern {
            serial: status.live_activation_serial,
            pattern: requested.pattern,
            retrigger: requested.retrigger,
        });
        status.loop_pattern = Some(requested.pattern);
        status.loop_order = None;
        status.loop_row = 0;
        status.loop_activation_serial = status.loop_activation_serial.wrapping_add(1);
        status.playing = true;
        status.error = None;
    }
}

fn publish_live_failure(status: &Arc<Mutex<SequencerStatus>>, error: String) {
    if let Ok(mut status) = status.lock() {
        status.queued_pattern = None;
        status.live_prepare = None;
        status.error = Some(error);
    }
}

fn update_active_notes(
    active: &mut BTreeMap<usize, (PageTarget, u8, BTreeSet<u8>)>,
    lane: Option<usize>,
    target: Option<&PageTarget>,
    bytes: &[u8],
) {
    let (Some(lane), Some(target), [status, note, velocity, ..]) = (lane, target, bytes) else {
        return;
    };
    let channel = status & 0x0f;
    match status & 0xf0 {
        0x90 if *velocity > 0 => {
            active
                .entry(lane)
                .or_insert_with(|| (target.clone(), channel, BTreeSet::new()))
                .2
                .insert(*note);
        }
        0x80 | 0x90 => {
            let empty = active.get_mut(&lane).is_some_and(|(_, _, notes)| {
                notes.remove(note);
                notes.is_empty()
            });
            if empty {
                active.remove(&lane);
            }
        }
        _ => {}
    }
}

struct DestinationPool {
    config: ExternalMidiConfig,
    instrument: crate::engine::SharedOutput,
    drums: crate::drums_host::SharedDrumOutput,
    destinations: BTreeMap<PageTarget, RuntimeDestination>,
}

enum RuntimeDestination {
    Instrument {
        notice: Option<String>,
    },
    Hardware {
        connection: MidiOutputConnection,
        notice: Option<String>,
    },
    InternalDrums,
    Unavailable(String),
}

impl DestinationPool {
    fn new(
        config: ExternalMidiConfig,
        instrument: crate::engine::SharedOutput,
        drums: crate::drums_host::SharedDrumOutput,
    ) -> Self {
        Self {
            config,
            instrument,
            drums,
            destinations: BTreeMap::new(),
        }
    }

    fn ensure(&mut self, target: &PageTarget) {
        if self.destinations.contains_key(target) {
            return;
        }
        let destination = if matches!(target, PageTarget::InternalDrums(_)) {
            if self.drums.lock().is_ok_and(|output| output.is_some()) {
                RuntimeDestination::InternalDrums
            } else {
                RuntimeDestination::Unavailable("SHR Drums kit is offline".into())
            }
        } else {
            let instrument_online = self.instrument.lock().is_ok_and(|output| output.is_some());
            open_runtime_destination(&self.config, target, instrument_online)
                .unwrap_or_else(|error| RuntimeDestination::Unavailable(error.to_string()))
        };
        self.destinations.insert(target.clone(), destination);
    }

    fn refresh(&mut self, target: &PageTarget) {
        self.destinations.remove(target);
        self.ensure(target);
    }

    fn send(&mut self, target: &PageTarget, bytes: &[u8]) -> std::result::Result<(), String> {
        self.ensure(target);
        let output = self
            .destinations
            .get_mut(target)
            .expect("target was ensured");
        let result = match output {
            RuntimeDestination::Instrument { .. } => self
                .instrument
                .lock()
                .map_err(|_| "active instrument route lock failed".to_string())?
                .as_mut()
                .ok_or_else(|| "active SHR-DAW instrument is offline".to_string())?
                .send(bytes)
                .map_err(|error| error.to_string()),
            RuntimeDestination::Hardware { connection, .. } => {
                connection.send(bytes).map_err(|error| error.to_string())
            }
            RuntimeDestination::InternalDrums => self
                .drums
                .lock()
                .map_err(|_| "SHR Drums output lock failed".to_string())?
                .as_ref()
                .ok_or_else(|| "SHR Drums kit is offline".to_string())
                .and_then(|sender| crate::drums_host::send_midi(sender, bytes)),
            RuntimeDestination::Unavailable(error) => return Err(error.clone()),
        };
        if let Err(error) = &result {
            *output = RuntimeDestination::Unavailable(error.clone());
        }
        result
    }

    fn send_cleanup(&mut self, target: &PageTarget, bytes: &[u8]) {
        if self.send(target, bytes).is_err() {
            self.refresh(target);
            let _ = self.send(target, bytes);
        }
    }

    fn error(&self, target: &PageTarget) -> Option<String> {
        self.destinations
            .get(target)
            .and_then(|output| match output {
                RuntimeDestination::Unavailable(error) => Some(error.clone()),
                _ => None,
            })
    }

    fn fallback(&self, target: &PageTarget) -> Option<String> {
        self.destinations
            .get(target)
            .and_then(|output| match output {
                RuntimeDestination::Instrument { notice }
                | RuntimeDestination::Hardware { notice, .. } => notice.clone(),
                RuntimeDestination::InternalDrums => None,
                RuntimeDestination::Unavailable(_) => None,
            })
    }
}

fn update_target_status(
    status: &Arc<Mutex<SequencerStatus>>,
    outputs: &DestinationPool,
    targets: &BTreeSet<PageTarget>,
) {
    if let Ok(mut status) = status.lock() {
        status.targets = targets
            .iter()
            .map(|target| (target.clone(), outputs.error(target)))
            .collect();
        status.fallbacks = targets
            .iter()
            .filter_map(|target| {
                outputs
                    .fallback(target)
                    .map(|notice| (target.clone(), notice))
            })
            .collect();
        status.available =
            status.targets.is_empty() || status.targets.values().any(Option::is_none);
        status.error = status.targets.iter().find_map(|(target, error)| {
            error
                .as_ref()
                .map(|error| format!("{}: {error}", target.label()))
        });
    }
}

fn cleanup_owned_notes(outputs: &mut DestinationPool, owners: &mut NoteOwners) {
    let cleanup = planned_note_cleanup(owners);
    let internal_targets = cleanup
        .iter()
        .filter_map(|(target, _)| {
            matches!(target, PageTarget::InternalDrums(_)).then_some(target.clone())
        })
        .collect::<BTreeSet<_>>();
    for (target, message) in cleanup {
        outputs.send_cleanup(&target, &message);
    }
    for target in internal_targets {
        outputs.send_cleanup(&target, &[0xb0, 123, 0]);
    }
    owners.clear();
}

fn release_target_notes(
    outputs: &mut DestinationPool,
    owners: &mut NoteOwners,
    active_notes: &mut BTreeMap<usize, (PageTarget, u8, BTreeSet<u8>)>,
    target: &PageTarget,
) {
    let matching = owners
        .keys()
        .filter(|(candidate, _, _)| candidate == target)
        .cloned()
        .collect::<Vec<_>>();
    for (target, channel, note) in matching {
        outputs.send_cleanup(&target, &[0x80 | channel, note, 0]);
        owners.remove(&(target, channel, note));
    }
    active_notes.retain(|_, (candidate, _, _)| candidate != target);
}

fn planned_note_cleanup(owners: &NoteOwners) -> Vec<(PageTarget, Vec<u8>)> {
    owners
        .keys()
        .map(|(target, channel, note)| (target.clone(), vec![0x80 | channel, *note, 0]))
        .collect()
}

fn claim_note_owner(
    owners: &mut NoteOwners,
    owner: NoteOwner,
    target: &PageTarget,
    channel: u8,
    note: u8,
) -> bool {
    let lanes = owners.entry((target.clone(), channel, note)).or_default();
    let first = lanes.is_empty();
    lanes.insert(owner);
    first
}

fn release_note_owner(
    owners: &mut NoteOwners,
    owner: NoteOwner,
    target: &PageTarget,
    channel: u8,
    note: u8,
) -> bool {
    let key = (target.clone(), channel, note);
    let last = if let Some(lanes) = owners.get_mut(&key) {
        lanes.remove(&owner);
        lanes.is_empty()
    } else {
        true
    };
    if last {
        owners.remove(&key);
    }
    last
}

fn rescale_schedule(
    messages: &mut [ScheduledMessage],
    index: usize,
    elapsed: Duration,
    old_tempo: Bpm,
    new_tempo: Bpm,
) {
    let scale = old_tempo.as_f64() / new_tempo.as_f64();
    for message in messages.iter_mut().skip(index) {
        let remaining = message.at.saturating_sub(elapsed);
        message.at = elapsed + remaining.mul_f64(scale);
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
enum MidiRouteChoice {
    Instrument,
    Hardware(usize),
    Unavailable(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedMidiRoute {
    choice: MidiRouteChoice,
    notice: Option<String>,
}

fn resolve_midi_route(
    config: &ExternalMidiConfig,
    target: &PageTarget,
    names: &[String],
    instrument_online: bool,
) -> ResolvedMidiRoute {
    let configured = || -> Result<Option<usize>> {
        if config.enabled {
            matching_output_index(names, &config.output_match, true).map(Some)
        } else {
            Ok(None)
        }
    };
    let instrument = |notice: Option<String>, unavailable: String| ResolvedMidiRoute {
        choice: if instrument_online {
            MidiRouteChoice::Instrument
        } else {
            MidiRouteChoice::Unavailable(unavailable)
        },
        notice: instrument_online.then_some(notice).flatten(),
    };
    match target {
        PageTarget::ActiveInstrument | PageTarget::Synthv1(_) | PageTarget::Software(_) => {
            instrument(None, "active SHR-DAW instrument is offline".into())
        }
        PageTarget::InternalDrums(_) => ResolvedMidiRoute {
            choice: MidiRouteChoice::Unavailable(
                "SHR Drums is resolved by the in-process destination".into(),
            ),
            notice: None,
        },
        PageTarget::Default => {
            if config.enabled {
                match configured() {
                    Ok(Some(index)) => ResolvedMidiRoute {
                        choice: MidiRouteChoice::Hardware(index),
                        notice: None,
                    },
                    Ok(None) => unreachable!("enabled configured route returned no destination"),
                    Err(error) => ResolvedMidiRoute {
                        choice: MidiRouteChoice::Unavailable(error.to_string()),
                        notice: None,
                    },
                }
            } else {
                instrument(
                    None,
                    "portable route has no active machine default; configure MIDI or load an instrument"
                        .into(),
                )
            }
        }
        PageTarget::ConfiguredExternal => match configured() {
            Ok(Some(index)) => ResolvedMidiRoute {
                choice: MidiRouteChoice::Hardware(index),
                notice: None,
            },
            Ok(None) => ResolvedMidiRoute {
                choice: MidiRouteChoice::Unavailable(format!(
                    "configured MIDI output {:?} is offline",
                    config.output_match
                )),
                notice: None,
            },
            Err(error) => ResolvedMidiRoute {
                choice: MidiRouteChoice::Unavailable(error.to_string()),
                notice: None,
            },
        },
        PageTarget::Midi(wanted) => match matching_output_index(names, wanted, false) {
            Ok(index) => ResolvedMidiRoute {
                choice: MidiRouteChoice::Hardware(index),
                notice: None,
            },
            Err(error) => ResolvedMidiRoute {
                choice: MidiRouteChoice::Unavailable(error.to_string()),
                notice: None,
            },
        },
    }
}

fn open_runtime_destination(
    config: &ExternalMidiConfig,
    target: &PageTarget,
    instrument_online: bool,
) -> Result<RuntimeDestination> {
    let output = MidiOutput::new(&config.client_name);
    let (output, ports, names) = match output {
        Ok(output) => {
            let ports = output.ports();
            let names = ports
                .iter()
                .map(|port| output.port_name(port).unwrap_or_default())
                .collect::<Vec<_>>();
            (Some(output), ports, names)
        }
        Err(_) => (None, Vec::new(), Vec::new()),
    };
    let resolved = resolve_midi_route(config, target, &names, instrument_online);
    match resolved.choice {
        MidiRouteChoice::Instrument => Ok(RuntimeDestination::Instrument {
            notice: resolved.notice,
        }),
        MidiRouteChoice::Hardware(index) => {
            let output = output.context("MIDI output backend unavailable")?;
            let connection = output
                .connect(&ports[index], "SHR-DAW tracker page")
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(RuntimeDestination::Hardware {
                connection,
                notice: resolved.notice,
            })
        }
        MidiRouteChoice::Unavailable(error) => Ok(RuntimeDestination::Unavailable(error)),
    }
}

#[cfg(test)]
fn connect_target(
    config: &ExternalMidiConfig,
    target: &PageTarget,
) -> Result<MidiOutputConnection> {
    match open_runtime_destination(config, target, false)? {
        RuntimeDestination::Hardware { connection, .. } => Ok(connection),
        RuntimeDestination::Instrument { .. } => bail!("unexpected instrument route"),
        RuntimeDestination::InternalDrums => bail!("unexpected SHR Drums route"),
        RuntimeDestination::Unavailable(error) => bail!(error),
    }
}

pub(crate) fn matching_output_index(
    names: &[String],
    wanted: &str,
    _allow_partial: bool,
) -> Result<usize> {
    crate::midi_endpoint::matching_index(names, wanted, "MIDI output")
}

pub fn available_midi_outputs(client_name: &str) -> Result<Vec<String>> {
    let output = MidiOutput::new(client_name)?;
    let mut names = output
        .ports()
        .iter()
        .filter_map(|port| output.port_name(port).ok())
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

pub fn diagnostic(config: &ExternalMidiConfig) -> Result<String> {
    let channel = *config
        .channels
        .first()
        .context("external MIDI has no configured channels")?;
    if channel > 15 {
        bail!("external MIDI channel out of range");
    }
    let output = MidiOutput::new(&config.client_name)?;
    let ports = output
        .ports()
        .iter()
        .filter_map(|p| output.port_name(p).ok())
        .collect::<Vec<_>>();
    let matches = matching_output_index(&ports, &config.output_match, true)
        .ok()
        .and_then(|index| ports.get(index).cloned())
        .into_iter()
        .collect::<Vec<_>>();
    let page = Page {
        name: "dry-run".into(),
        enabled: true,
        columns: [ColumnSetup {
            channel,
            bank_msb: 0,
            bank_lsb: 0,
            program: 0,
        }; LANES_PER_PAGE],
        velocity: 64,
        percussion: false,
        note_off_enabled: true,
        entry_mode: NoteEntryMode::Manual,
        entry_anchor: 0,
        drum_class_overrides: BTreeMap::new(),
        target: PageTarget::ConfiguredExternal,
        device_profile: None,
        setup: Vec::new(),
        lanes: (1..=LANES_PER_PAGE)
            .map(|lane| Lane {
                name: format!("L{lane}"),
                enabled: true,
                playback: LanePlayback::default(),
            })
            .collect(),
    };
    let mut dry = Vec::new();
    append_program(
        &mut dry,
        SchedulePosition {
            at: Duration::ZERO,
            order: 0,
            row: 0,
        },
        &page,
        page.column(0),
        0,
        config,
        &DeviceProfiles::discover(),
    );
    push(
        &mut dry,
        Duration::ZERO,
        0,
        0,
        vec![0x90 | page.column(0).channel, 60, 64],
        Some(page.target.clone()),
    );
    push(
        &mut dry,
        Duration::from_millis(250),
        0,
        0,
        vec![0x80 | page.column(0).channel, 60, 0],
        Some(page.target.clone()),
    );
    if let Some(channel) = config.percussion_channel {
        if config.program_changes {
            if let Some(program) = config.percussion_program {
                push(
                    &mut dry,
                    Duration::ZERO,
                    0,
                    0,
                    vec![0xc0 | channel, program],
                    Some(page.target.clone()),
                );
            }
        }
        push(
            &mut dry,
            Duration::ZERO,
            0,
            0,
            vec![0x90 | channel, 36, 96],
            Some(page.target.clone()),
        );
        push(
            &mut dry,
            Duration::from_millis(125),
            0,
            0,
            vec![0x80 | channel, 36, 0],
            Some(page.target.clone()),
        );
    }
    let messages = dry
        .iter()
        .map(|m| format!("{:?} @ {}ms", m.bytes, m.at.as_millis()))
        .chain(
            panic_messages(config.channels.iter().copied())
                .iter()
                .map(|m| format!("{m:?} panic")),
        )
        .collect::<Vec<_>>()
        .join("\n  ");
    Ok(format!("profile: {}\nenabled: {}\nconfigured match: {:?}\nmatching ports: {}\navailable MIDI outputs:\n  {}\nchannels: {}\npercussion: {}; percussion program: {}; input map: {} -> [{}]\nbank: {:?}; program: {}; clock/start/stop: {}; live thru: {}\ndry run (NOT transmitted):\n  {}\n",
        config.profile, config.enabled, config.output_match, if matches.is_empty() { "none".into() } else { matches.join(", ") }, if ports.is_empty() { "none".into() } else { ports.join("\n  ") },
        config.channels.iter().map(|c| (c+1).to_string()).collect::<Vec<_>>().join(","), config.percussion_channel.map(|c| (c+1).to_string()).unwrap_or_else(|| "off".into()), config.percussion_program.map(|p| p.to_string()).unwrap_or_else(|| "unchanged".into()), config.percussion_input_base, config.percussion_notes.iter().map(u8::to_string).collect::<Vec<_>>().join(","), config.bank_select, config.program_changes, config.send_transport, config.live_thru, messages))
}

fn escape(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('|', "%7C")
        .replace('\n', "%0A")
        .replace('\r', "%0D")
}
fn target_text(target: &PageTarget) -> String {
    match target {
        PageTarget::Default => "default".into(),
        PageTarget::ActiveInstrument => "instrument".into(),
        PageTarget::Synthv1(name) => format!("synthv1:{}", escape(name)),
        PageTarget::Software(route) => format!(
            "software:{}:{}",
            route.engine.label().to_ascii_lowercase(),
            escape(&route.instrument)
        ),
        PageTarget::InternalDrums(kit) => format!("shr-drums:{}", escape(kit)),
        PageTarget::ConfiguredExternal => "configured".into(),
        PageTarget::Midi(name) => format!(
            "midi:{}",
            escape(&crate::midi_endpoint::stable_identity(name))
        ),
    }
}
fn entry_mode_text(mode: NoteEntryMode) -> &'static str {
    match mode {
        NoteEntryMode::Manual => "manual",
        NoteEntryMode::OneColumn => "one",
        NoteEntryMode::DrumAuto => "drum",
    }
}
fn parse_entry_mode(value: &str) -> Result<NoteEntryMode> {
    match value {
        "manual" => Ok(NoteEntryMode::Manual),
        "one" => Ok(NoteEntryMode::OneColumn),
        "drum" => Ok(NoteEntryMode::DrumAuto),
        _ => bail!("invalid note-entry mode"),
    }
}
fn parse_bpm_interpretation(value: &str) -> Result<BpmInterpretation> {
    match value {
        "half" => Ok(BpmInterpretation::Half),
        "normal" => Ok(BpmInterpretation::Normal),
        "double" => Ok(BpmInterpretation::Double),
        _ => bail!("invalid loop BPM interpretation"),
    }
}
fn drum_role_text(role: DrumRole) -> &'static str {
    match role {
        DrumRole::Core => "core",
        DrumRole::LongTail => "long",
        DrumRole::Other => "other",
    }
}
fn parse_drum_role(value: &str) -> Result<DrumRole> {
    match value {
        "core" => Ok(DrumRole::Core),
        "long" => Ok(DrumRole::LongTail),
        "other" => Ok(DrumRole::Other),
        _ => bail!("invalid drum role"),
    }
}
pub(crate) const fn lane_rate_text(rate: LaneRate) -> &'static str {
    match rate {
        LaneRate::Quarter => "quarter",
        LaneRate::Half => "half",
        LaneRate::Normal => "normal",
        LaneRate::Double => "double",
        LaneRate::Quadruple => "quadruple",
    }
}
pub(crate) fn parse_lane_rate(value: &str) -> Result<LaneRate> {
    match value {
        "quarter" => Ok(LaneRate::Quarter),
        "half" => Ok(LaneRate::Half),
        "normal" => Ok(LaneRate::Normal),
        "double" => Ok(LaneRate::Double),
        "quadruple" => Ok(LaneRate::Quadruple),
        _ => bail!("invalid lane playback rate"),
    }
}
pub(crate) const fn lane_direction_text(direction: LaneDirection) -> &'static str {
    match direction {
        LaneDirection::Forward => "forward",
        LaneDirection::Reverse => "reverse",
        LaneDirection::Pendulum => "pendulum",
        LaneDirection::Variation => "variation",
    }
}
pub(crate) fn parse_lane_direction(value: &str) -> Result<LaneDirection> {
    match value {
        "forward" => Ok(LaneDirection::Forward),
        "reverse" => Ok(LaneDirection::Reverse),
        "pendulum" => Ok(LaneDirection::Pendulum),
        "variation" => Ok(LaneDirection::Variation),
        _ => bail!("invalid lane playback direction"),
    }
}
fn parse_target(value: &str, version: u8) -> Result<PageTarget> {
    match value {
        "default" if version >= 4 => Ok(PageTarget::Default),
        "default" => bail!("portable routing requires Project format 4"),
        "instrument" => Ok(PageTarget::ActiveInstrument),
        "configured" => Ok(PageTarget::ConfiguredExternal),
        _ if value.starts_with("synthv1:") => value
            .strip_prefix("synthv1:")
            .map(unescape)
            .transpose()?
            .filter(|name| !name.is_empty())
            .map(PageTarget::Synthv1)
            .context("invalid synthv1 page target"),
        _ if version >= 5 && value.starts_with("software:") => {
            let route = value.strip_prefix("software:").unwrap_or_default();
            let (engine, instrument) = route
                .split_once(':')
                .context("invalid software page target")?;
            Ok(PageTarget::Software(SoftwareRoute {
                engine: engine.parse()?,
                instrument: unescape(instrument)?,
            }))
        }
        _ if version >= 12 && value.starts_with("shr-drums:") => value
            .strip_prefix("shr-drums:")
            .map(unescape)
            .transpose()?
            .filter(|kit| !kit.is_empty())
            .map(PageTarget::InternalDrums)
            .context("invalid SHR Drums page target"),
        _ => value
            .strip_prefix("midi:")
            .map(unescape)
            .transpose()?
            .map(|name| PageTarget::Midi(crate::midi_endpoint::stable_identity(&name)))
            .context("invalid page target"),
    }
}
fn unescape(value: &str) -> Result<String> {
    Ok(value
        .replace("%0D", "\r")
        .replace("%0A", "\n")
        .replace("%7C", "|")
        .replace("%25", "%"))
}
fn one_based_channel(v: &str) -> Result<u8> {
    let n = v.parse::<u8>()?;
    if !(1..=16).contains(&n) {
        bail!("channel out of range");
    }
    Ok(n - 1)
}
fn one_based_entry_anchor(v: &str) -> Result<u8> {
    let anchor = v.parse::<u8>()?;
    if !(1..=LANES_PER_PAGE as u8).contains(&anchor) {
        bail!("entry anchor out of range");
    }
    Ok(anchor - 1)
}
fn midi_value(v: &str) -> Result<u8> {
    let n = v.parse::<u8>()?;
    if n > 127 {
        bail!("MIDI value out of range");
    }
    Ok(n)
}
fn optional_midi(v: &str) -> Result<Option<u8>> {
    if v == "-" {
        Ok(None)
    } else {
        midi_value(v).map(Some)
    }
}
fn optional_gate(v: &str) -> Result<Option<u8>> {
    if v == "-" {
        return Ok(None);
    }
    let gate = v.parse::<u8>()?;
    if !(1..=100).contains(&gate) {
        bail!("cell gate must be 1..=100 percent");
    }
    Ok(Some(gate))
}
fn note_text(n: Note) -> String {
    match n {
        Note::Empty => "---".into(),
        Note::Off => "OFF".into(),
        Note::On(n) => n.to_string(),
    }
}
fn parse_note(v: &str) -> Result<Note> {
    match v {
        "---" => Ok(Note::Empty),
        "OFF" => Ok(Note::Off),
        _ => midi_value(v).map(Note::On),
    }
}
fn swing_division_text(division: SwingDivision) -> &'static str {
    match division {
        SwingDivision::Eighth => "eighth",
        SwingDivision::Sixteenth => "sixteenth",
    }
}
fn parse_swing_division(value: &str) -> Result<SwingDivision> {
    match value {
        "eighth" => Ok(SwingDivision::Eighth),
        "sixteenth" => Ok(SwingDivision::Sixteenth),
        _ => bail!("invalid Pattern swing division"),
    }
}
fn command_text(c: Command) -> String {
    match c {
        Command::None => "-".into(),
        Command::Cut(v) => format!("C{v}"),
        Command::Delay(v) => format!("D{v}"),
        Command::Retrigger(v) => format!("R{v}"),
        Command::Tempo(v) => format!("T{}", v.hundredths()),
    }
}
fn parse_command(v: &str, version: u8) -> Result<Command> {
    if v == "-" {
        return Ok(Command::None);
    }
    let (kind, parameter) = v.split_at(v.char_indices().nth(1).map_or(v.len(), |(i, _)| i));
    if parameter.is_empty() {
        bail!("command parameter missing");
    }
    match kind {
        "C" => Ok(Command::Cut(parameter.parse()?)),
        "D" => Ok(Command::Delay(parameter.parse()?)),
        "R" => Ok(Command::Retrigger(parameter.parse()?)),
        "T" if version >= 10 => Ok(Command::Tempo(
            Bpm::from_hundredths(parameter.parse()?)
                .context("tempo command must be 2000..=30000 hundredths")?,
        )),
        "T" => Ok(Command::Tempo(
            Bpm::from_whole(parameter.parse()?)
                .context("legacy tempo command must be 20..=300 BPM")?,
        )),
        _ => bail!("unknown command"),
    }
}

pub(crate) fn condition_text(condition: StepCondition) -> String {
    match condition {
        StepCondition::Always => "-".into(),
        StepCondition::First => "first".into(),
        StepCondition::Last(length) => format!("last:{length}"),
        StepCondition::Ratio { hit, cycle } => format!("{hit}:{cycle}"),
        StepCondition::Previous => "pre".into(),
        StepCondition::Fill => "fill".into(),
    }
}

pub(crate) fn parse_condition(value: &str) -> Result<StepCondition> {
    match value {
        "-" => Ok(StepCondition::Always),
        "first" => Ok(StepCondition::First),
        "pre" => Ok(StepCondition::Previous),
        "fill" => Ok(StepCondition::Fill),
        _ if value.starts_with("last:") => Ok(StepCondition::Last(
            value.trim_start_matches("last:").parse()?,
        )),
        _ => {
            let (hit, cycle) = value.split_once(':').context("invalid step condition")?;
            Ok(StepCondition::Ratio {
                hit: hit.parse()?,
                cycle: cycle.parse()?,
            })
        }
    }
}

pub fn note_name(note: Note) -> String {
    match note {
        Note::Empty => "---".into(),
        Note::Off => "OFF".into(),
        Note::On(n) => {
            const N: [&str; 12] = [
                "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-",
            ];
            format!("{}{}", N[usize::from(n % 12)], i16::from(n) / 12 - 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gm_drums_route() -> SoftwareRoute {
        SoftwareRoute {
            engine: BackendKind::FluidSynth,
            instrument: "sf0:test.sf2:128:0".into(),
        }
    }
    fn config() -> ExternalMidiConfig {
        let mut c = crate::config::RuntimeConfig::default().external_midi;
        c.program_changes = true;
        c.bank_select = BankSelectMode::Cc0Cc32;
        c
    }
    fn pages(song: &Song) -> &[Page] {
        &song.patterns[&0].pages
    }
    fn pages_mut(song: &mut Song) -> &mut [Page] {
        &mut song.patterns.get_mut(&0).unwrap().pages
    }
    fn pattern_resize_fixture(rows: usize) -> Pattern {
        Pattern::new(
            rows,
            Bpm::from_hundredths(12_345).unwrap(),
            3,
            vec![
                Page::new("MELODY", 0, false, 4),
                Page::new("DRUMS", 9, true, 0),
            ],
        )
    }

    #[test]
    fn transport_position_uses_the_received_subrow_timestamp() {
        let (tx, _rx) = mpsc::channel();
        let (count_in_tx, _count_in_rx) = mpsc::channel();
        let started = Instant::now();
        let status = Arc::new(Mutex::new(SequencerStatus {
            playing: true,
            order: 2,
            row: 7,
            row_started_at: Some(started),
            row_duration: Duration::from_millis(200),
            pattern_tick: 7 * AUTOMATION_TICKS_PER_ROW,
            ..SequencerStatus::default()
        }));
        let sequencer = Sequencer {
            tx,
            count_in_tx,
            status,
            thread: None,
            count_in_thread: None,
            config: config(),
        };
        assert_eq!(
            sequencer.position_at(started + Duration::from_millis(150)),
            Some(TransportPosition {
                order: 2,
                row: 7,
                pattern_tick: 7 * AUTOMATION_TICKS_PER_ROW + AUTOMATION_TICKS_PER_ROW * 3 / 4,
            })
        );
    }

    #[test]
    fn one_beat_count_in_crosses_to_recording_at_row_zero() {
        let mut c = config();
        c.send_transport = false;
        let mut song = Song::new(&c);
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.tempo = Bpm::from_whole(300).unwrap();
        let clock = Arc::new(crate::loop_player::TransportClock::new(
            &crate::config::RuntimeConfig::default().controller_clock,
            pattern.tempo,
        ));
        let sequencer = Sequencer::start_with_clock(
            &c,
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(None)),
            clock,
            Arc::new(crate::effects::EffectControlHub::default()),
        );
        sequencer.count_in(&song, 0, 0, 1);
        assert_eq!(sequencer.status().count_in, Some(1));
        assert!(!sequencer.status().playing);
        let deadline = Instant::now() + Duration::from_millis(500);
        while !sequencer.status().playing && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let status = sequencer.status();
        assert!(status.playing, "count-in did not reach the record boundary");
        assert_eq!((status.order, status.row, status.count_in), (0, 0, None));
        assert_eq!(status.pattern_tick, 0);
        sequencer.stop();
        sequencer.count_in(&song, 0, 0, 1);
        assert_eq!(sequencer.status().count_in, Some(1));
        sequencer.stop();
        thread::sleep(Duration::from_millis(250));
        assert!(!sequencer.status().playing);
        assert_eq!(sequencer.status().count_in, None);
    }

    #[test]
    fn pattern_resize_half_keeps_top_or_bottom_across_every_page_and_lane() {
        let mut pattern = pattern_resize_fixture(4);
        pattern.rows[0][0] = Cell {
            note: Note::On(60),
            velocity: Some(111),
            program: Some(7),
            gate: Some(73),
            nudge: 0,
            command: Command::Delay(5),
            ..Cell::default()
        };
        pattern.rows[1][5] = Cell {
            note: Note::Off,
            command: Command::Cut(2),
            ..Cell::default()
        };
        pattern.rows[2][1] = Cell {
            note: Note::On(64),
            velocity: Some(87),
            gate: Some(100),
            command: Command::Retrigger(3),
            ..Cell::default()
        };
        pattern.rows[3][7] = Cell {
            note: Note::On(38),
            program: Some(9),
            command: Command::Tempo(Bpm::from_hundredths(9_876).unwrap()),
            ..Cell::default()
        };

        let top = pattern.halve_rows(PatternHalf::Top).unwrap();
        assert_eq!(top.pattern.rows, pattern.rows[..2]);
        assert_eq!(top.discarded_cells, 2);
        let bottom = pattern.halve_rows(PatternHalf::Bottom).unwrap();
        assert_eq!(bottom.pattern.rows, pattern.rows[2..]);
        assert_eq!(bottom.discarded_cells, 2);
        assert_eq!(bottom.pattern.tempo, pattern.tempo);
        assert_eq!(bottom.pattern.meter, pattern.meter);
        assert_eq!(bottom.pattern.pages, pattern.pages);
        assert_eq!(bottom.pattern.audio_loops, pattern.audio_loops);
    }

    #[test]
    fn pattern_resize_row_remove_and_insert_shift_complete_rows_atomically() {
        let mut pattern = pattern_resize_fixture(3);
        let complete = Cell {
            note: Note::On(72),
            velocity: Some(127),
            program: Some(126),
            gate: Some(1),
            nudge: 0,
            command: Command::Retrigger(8),
            ..Cell::default()
        };
        pattern.rows[1][6] = complete;
        pattern.rows[2][0].note = Note::On(48);

        let removed = pattern.remove_row(1).unwrap();
        assert_eq!(removed.discarded_cells, 1);
        assert_eq!(removed.pattern.rows.len(), 2);
        assert_eq!(removed.pattern.rows[1], pattern.rows[2]);
        assert_eq!(pattern.rows[1][6], complete);

        let inserted = pattern.insert_row_after(0).unwrap();
        assert_eq!(inserted.pattern.rows.len(), 4);
        assert!(inserted.pattern.rows[1]
            .iter()
            .all(|cell| *cell == Cell::default()));
        assert_eq!(inserted.pattern.rows[2], pattern.rows[1]);
        assert_eq!(inserted.pattern.rows[3], pattern.rows[2]);
    }

    #[test]
    fn pattern_resize_double_copies_complete_cells_or_appends_empty_rows() {
        let mut pattern = pattern_resize_fixture(2);
        pattern.rows[0][4] = Cell {
            note: Note::On(36),
            velocity: Some(99),
            program: Some(42),
            gate: Some(88),
            nudge: 0,
            command: Command::Delay(15),
            ..Cell::default()
        };

        let copied = pattern.double_rows(PatternDouble::Copy).unwrap();
        assert_eq!(copied.copied_cells, 1);
        assert_eq!(copied.pattern.rows[..2], pattern.rows);
        assert_eq!(copied.pattern.rows[2..], pattern.rows);
        assert_eq!(copied.pattern.rows[2][4], pattern.rows[0][4]);

        let empty = pattern.double_rows(PatternDouble::Empty).unwrap();
        assert_eq!(empty.copied_cells, 0);
        assert_eq!(empty.pattern.rows[..2], pattern.rows);
        assert!(empty.pattern.rows[2..]
            .iter()
            .flatten()
            .all(|cell| *cell == Cell::default()));
    }

    #[test]
    fn pattern_resize_enforces_one_to_256_row_boundaries() {
        let one = pattern_resize_fixture(1);
        assert!(one.halve_rows(PatternHalf::Top).is_err());
        assert!(one.remove_row(0).is_err());

        let odd = pattern_resize_fixture(3);
        assert!(odd.halve_rows(PatternHalf::Top).is_err());

        let at_double_limit = pattern_resize_fixture(128);
        assert_eq!(
            at_double_limit
                .double_rows(PatternDouble::Empty)
                .unwrap()
                .pattern
                .rows
                .len(),
            256
        );
        let above_double_limit = pattern_resize_fixture(129);
        assert!(above_double_limit
            .double_rows(PatternDouble::Empty)
            .is_err());

        let maximum = pattern_resize_fixture(256);
        assert!(maximum.insert_row_after(0).is_err());
    }

    #[test]
    fn pattern_resize_clamps_explicit_lane_cycles_but_full_tracks_growth() {
        let mut pattern = pattern_resize_fixture(16);
        pattern.pages[0].lanes[0].playback.cycle_rows = 12;
        pattern.pages[0].lanes[1].playback.cycle_rows = 0;
        pattern.resize_rows(8).unwrap();
        assert_eq!(pattern.pages[0].lanes[0].playback.cycle_rows, 8);
        assert_eq!(pattern.pages[0].lanes[1].playback.cycle_rows, 0);
        pattern.resize_rows(24).unwrap();
        assert_eq!(pattern.pages[0].lanes[0].playback.cycle_rows, 8);
        assert_eq!(
            pattern.pages[0].lanes[1]
                .playback
                .effective_rows(pattern.rows.len()),
            24
        );
    }
    fn without_v15_rhythm_fields(text: &str) -> String {
        text.lines()
            .map(|line| {
                if let Some(value) = line.strip_prefix("pattern=") {
                    let fields = value.split('|').take(4).collect::<Vec<_>>();
                    format!("pattern={}", fields.join("|"))
                } else if let Some(value) = line.strip_prefix("cell=") {
                    let fields = value.split('|').take(8).collect::<Vec<_>>();
                    format!("cell={}", fields.join("|"))
                } else if let Some(value) = line.strip_prefix("pattern_lane=") {
                    let fields = value.split('|').take(5).collect::<Vec<_>>();
                    format!("pattern_lane={}", fields.join("|"))
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn without_v12_fields(text: &str) -> String {
        without_v15_rhythm_fields(text)
            .lines()
            .filter(|line| {
                !line.starts_with("project_key=")
                    && !line.starts_with("drum_kit=")
                    && !line.starts_with("drum_tuning=")
                    && !line.starts_with("drum_rack=")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn downgrade_tempo_fields(text: &str) -> String {
        without_v12_fields(text)
            .lines()
            .map(|line| {
                if let Some(value) = line.strip_prefix("pattern=") {
                    let mut fields = value.split('|').map(str::to_owned).collect::<Vec<_>>();
                    let hundredths = fields[2].parse::<u16>().unwrap();
                    assert_eq!(hundredths % 100, 0);
                    fields[2] = (hundredths / 100).to_string();
                    format!("pattern={}", fields[..4].join("|"))
                } else if let Some(value) = line.strip_prefix("cell=") {
                    let mut fields = value.split('|').map(str::to_owned).collect::<Vec<_>>();
                    if let Some(command) = fields.last_mut() {
                        if let Some(value) = command.strip_prefix('T') {
                            let hundredths = value.parse::<u16>().unwrap();
                            assert_eq!(hundredths % 100, 0);
                            *command = format!("T{}", hundredths / 100);
                        }
                    }
                    format!("cell={}", fields[..8].join("|"))
                } else if line.starts_with("pattern_page=") {
                    line.rsplit_once('|').unwrap().0.to_owned()
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn without_v5_profile_fields(text: &str) -> String {
        downgrade_tempo_fields(text)
            .lines()
            .filter(|line| {
                !line.starts_with("pattern_drum_class=") && !line.starts_with("master_strip=")
            })
            .map(|line| {
                if line.starts_with("pattern_page=") {
                    let without_anchor = line.rsplit_once('|').unwrap().0;
                    let without_mode = without_anchor.rsplit_once('|').unwrap().0;
                    without_mode.rsplit_once('|').unwrap().0
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn as_v5(text: &str) -> String {
        downgrade_tempo_fields(text)
            .lines()
            .filter(|line| {
                !line.starts_with("pattern_drum_class=") && !line.starts_with("master_strip=")
            })
            .map(|line| {
                if line.starts_with("SHSYNTH-SONG 17") {
                    "SHSYNTH-SONG 5"
                } else if line.starts_with("pattern_page=") {
                    let without_anchor = line.rsplit_once('|').unwrap().0;
                    without_anchor.rsplit_once('|').unwrap().0
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn enter_drums(pattern: &mut Pattern, row: usize, notes: &[u8]) -> Option<Vec<usize>> {
        let lanes = drum_auto_lanes(pattern, row, 0, notes, &[])?;
        for (&note, &lane) in notes.iter().zip(&lanes) {
            pattern.rows[row][lane] = Cell {
                note: Note::On(note),
                velocity: Some(100),
                ..Cell::default()
            };
        }
        Some(lanes)
    }

    #[test]
    fn live_pattern_boundary_preserves_valid_holds_and_releases_changes_before_note_on() {
        let cfg = config();
        let mut song = Song::new(&cfg);
        {
            let pattern = song.patterns.get_mut(&0).unwrap();
            pattern.rows.truncate(4);
            pattern.rows[3][0] = Cell {
                note: Note::On(60),
                gate: Some(100),
                ..Cell::default()
            };
        }
        let pattern = song.patterns[&0].clone();
        let mut scheduled = schedule(&song, &cfg, 0, 0).unwrap();
        let boundary = scheduled
            .iter()
            .filter(|message| message.bytes.is_empty())
            .map(|message| message.at)
            .max()
            .unwrap();
        assert!(scheduled
            .iter()
            .any(|message| message.at == boundary && message.bytes == [0x80, 60, 0]));
        strip_live_boundary_releases(0, &pattern, 1, false, &mut scheduled);
        assert!(!scheduled
            .iter()
            .any(|message| message.at == boundary && message.bytes == [0x80, 60, 0]));

        let mut next = Pattern::empty_like_setup(4, &pattern);
        next.rows[0][0].note = Note::On(61);
        let mut next_song = song.clone();
        next_song.patterns.insert(0, next.clone());
        let mut next_schedule = schedule(&next_song, &cfg, 0, 0).unwrap();
        let active = BTreeMap::from([(
            0,
            (
                next.pages[0].target.clone(),
                next.pages[0].runtime_channel(0, &cfg),
                BTreeSet::from([60]),
            ),
        )]);
        transfer_held_lanes(&next, &cfg, &active, &mut next_schedule);
        let release = next_schedule
            .iter()
            .position(|message| message.bytes == [0x80, 60, 0])
            .unwrap();
        let attack = next_schedule
            .iter()
            .position(|message| message.bytes == [0x90, 61, 96])
            .unwrap();
        assert!(
            release < attack,
            "old owner must release before the new note"
        );

        next.rows[0][0].note = Note::On(60);
        next_song.patterns.insert(0, next.clone());
        let mut same_schedule = schedule(&next_song, &cfg, 0, 0).unwrap();
        transfer_held_lanes(&next, &cfg, &active, &mut same_schedule);
        assert!(!same_schedule
            .iter()
            .any(|message| message.at == Duration::ZERO && message.bytes == [0x80, 60, 0]));
    }

    #[test]
    fn factory_pattern_has_synth_midi_and_drum_routes_with_zero_based_storage() {
        let drums_route = gm_drums_route();
        let pages = factory_routing_pages("First Sound", drums_route.clone());
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0].name, "Software Synth");
        assert_eq!(
            pages[0].target,
            PageTarget::Software(SoftwareRoute::synthv1("First Sound"))
        );
        assert!(!pages[0].percussion);
        assert_eq!(pages[1].name, "MIDI");
        assert_eq!(pages[1].target, PageTarget::ConfiguredExternal);
        assert_eq!(pages[1].column(0).channel, 0);
        assert_eq!(pages[1].column(0).program, 0);
        assert_eq!(pages[2].name, "Drums");
        assert!(pages[2].percussion);
        assert_eq!(pages[2].target, PageTarget::Software(drums_route));
        assert!(pages[2].columns.iter().all(|column| column.channel == 9));
        assert_eq!(musician_channel(pages[1].column(0).channel), 1);
        assert_eq!(musician_channel(pages[2].column(0).channel), 10);
        assert_eq!(musician_program(pages[1].column(0).program), 1);
    }

    #[test]
    fn routing_defaults_round_trip_and_seed_later_patterns() {
        let base = env::temp_dir().join(format!("shr-routing-defaults-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let path = base.join("defaults.shsong");
        let mut pages = factory_routing_pages("First Sound", gm_drums_route());
        pages[1].column_mut(0).channel = 6;
        pages[1].column_mut(0).program = 41;
        save_routing_defaults(&path, &pages).unwrap();
        let loaded =
            load_routing_defaults(&path, &factory_routing_pages("Fallback", gm_drums_route()))
                .unwrap();
        assert_eq!(loaded, pages);
        let pattern = Pattern::from_routing(&config(), 32, 4, &loaded);
        assert_eq!(pattern.pages[1].column(0).channel, 6);
        assert_eq!(pattern.pages[1].column(0).program, 41);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn fresh_default_project_predicate_rejects_empty_but_explicit_changes() {
        let cfg = config();
        let defaults = factory_routing_pages("First Sound", gm_drums_route());
        let mut song = Song::new_with_pages(&cfg, defaults.clone());
        song.name = "project 7".into();
        assert!(matches_new_empty_default_project(&song, &cfg, &defaults));

        song.patterns.get_mut(&0).unwrap().pages[0].target =
            PageTarget::Software(SoftwareRoute::synthv1("Explicit Sound"));
        assert!(!matches_new_empty_default_project(&song, &cfg, &defaults));
        assert!(!pattern_has_note_events(&song.patterns[&0]));

        let mut song = Song::new_with_pages(&cfg, defaults.clone());
        song.patterns.get_mut(&0).unwrap().rows[0][0].note = Note::On(60);
        assert!(!matches_new_empty_default_project(&song, &cfg, &defaults));

        let mut song = Song::new_with_pages(&cfg, defaults.clone());
        song.order.push(0);
        assert!(!matches_new_empty_default_project(&song, &cfg, &defaults));

        let mut song = Song::new_with_pages(&cfg, defaults.clone());
        song.insert_rack
            .add(crate::audio_graph::EffectKind::Compressor)
            .unwrap();
        assert!(!matches_new_empty_default_project(&song, &cfg, &defaults));
    }

    #[test]
    fn legacy_active_instrument_is_upgraded_only_in_memory() {
        let mut song = Song::new(&config());
        pages_mut(&mut song)[0].target = PageTarget::ActiveInstrument;
        let before = encode(&song).unwrap();
        assert!(before.contains("|instrument|-|manual|1|1\n"));
        let base = env::temp_dir().join(format!("shr-legacy-routing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        save(&base, &song, false).unwrap();
        let path = base.join("untitled.shsong");

        let mut loaded = load(&base, "untitled").unwrap();
        assert_eq!(upgrade_legacy_synth_routes(&mut loaded, "First Sound"), 1);
        assert_eq!(
            pages(&loaded)[0].target,
            PageTarget::Software(SoftwareRoute::synthv1("First Sound"))
        );
        assert_eq!(fs::read_to_string(path).unwrap(), before);
        assert!(encode(&loaded)
            .unwrap()
            .contains("|software:synthv1:First Sound|-|manual|1|1\n"));
        let _ = fs::remove_dir_all(base);
    }
    #[test]
    fn serialization_round_trip_requires_current_schema() {
        let mut s = Song::new(&config());
        s.name = "a|b".into();
        s.master_strip.input_bypass = false;
        s.master_strip.input_trim_db = 2.5;
        s.master_strip.glue_bypass = false;
        s.master_strip.glue_threshold_db = -16.0;
        s.master_strip.loud_db = 1.5;
        s.master_strip.ceiling_dbtp = -0.8;
        let compressor = s
            .insert_rack
            .add(crate::audio_graph::EffectKind::Compressor)
            .unwrap();
        s.insert_rack
            .effect_mut(compressor)
            .unwrap()
            .parameters
            .insert("threshold_db".into(), -27.5);
        let aux = s.aux_routing.add_bus().unwrap();
        let aux_effect = s
            .aux_routing
            .add_effect(&s.insert_rack, aux, crate::audio_graph::EffectKind::Reverb)
            .unwrap();
        s.aux_routing.buses[0]
            .rack
            .effect_mut(aux_effect)
            .unwrap()
            .bypass = true;
        s.aux_routing
            .set_send(
                &s.insert_rack,
                aux,
                -18.0,
                crate::audio_graph::SendPoint::PostInsert,
            )
            .unwrap();
        s.patterns.get_mut(&0).unwrap().rows[0][0].note = Note::On(60);
        let text = encode(&s).unwrap();
        assert!(text.starts_with("SHSYNTH-SONG 17\n"));
        assert_eq!(decode(&text).unwrap(), s);
        assert!(decode(&text.replace("gate=80\n", "")).is_err());
        assert!(decode(&text.replace("\"threshold_db\":-27.5", "\"threshold_db\":null")).is_err());
    }

    #[test]
    fn current_format_round_trips_project_key_internal_drums_tuning_and_effects() {
        let cfg = config();
        let mut song = Song::new_with_pages(&cfg, factory_routing_pages("Lead", gm_drums_route()));
        song.project_key = Scale {
            root: 1,
            kind: ScaleKind::NaturalMinor,
        };
        song.drum_kit = "experimental-noise".into();
        song.drum_tuning.mode = shr_drums::TuningMode::Manual;
        for piece in ["kick", "snare"] {
            song.drum_tuning.pieces.insert(
                piece.into(),
                shr_drums::ManualTuning {
                    target_pitch_class: Some(shr_drums::PitchClass(1)),
                    cents_adjustment: 0,
                },
            );
        }
        let drum_kit = song.drum_kit.clone();
        pages_mut(&mut song)[2].target = PageTarget::InternalDrums(drum_kit);

        let encoded = encode(&song).unwrap();
        assert!(encoded.starts_with("SHSYNTH-SONG 17\n"));
        assert!(encoded.contains("project_key=1|minor\n"));
        assert!(encoded.contains("drum_rack="));
        assert!(encoded.contains("|shr-drums:experimental-noise|"));
        assert_eq!(decode(&encoded).unwrap(), song);
    }

    #[test]
    fn format_fourteen_round_trips_sparse_automation_and_thirteen_migrates_empty() {
        let cfg = config();
        let mut song = Song::new(&cfg);
        let page = &mut song.patterns.get_mut(&0).unwrap().pages[0];
        page.target = PageTarget::ConfiguredExternal;
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.automation.push(AutomationLane {
            id: 9,
            target: AutomationTarget::MidiCc {
                page: 0,
                channel: 2,
                controller: 74,
            },
            curve: AutomationCurve::Linear,
            points: vec![
                AutomationPoint {
                    tick: 0,
                    value: 61_920,
                },
                AutomationPoint {
                    tick: AUTOMATION_TICKS_PER_ROW * 3,
                    value: 57_792,
                },
            ],
        });
        let encoded = encode(&song).unwrap();
        assert!(encoded.contains("pattern_automation=0|"));
        assert_eq!(decode(&encoded).unwrap(), song);

        let fourteen =
            without_v15_rhythm_fields(&encoded).replacen("SHSYNTH-SONG 17", "SHSYNTH-SONG 14", 1);
        let migrated = decode(&fourteen).unwrap();
        assert_eq!(
            migrated.patterns[&0].automation,
            song.patterns[&0].automation
        );

        let legacy = fourteen
            .lines()
            .filter(|line| !line.starts_with("pattern_automation="))
            .map(|line| {
                if line == "SHSYNTH-SONG 14" {
                    "SHSYNTH-SONG 13"
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let migrated = decode(&legacy).unwrap();
        assert!(migrated
            .patterns
            .values()
            .all(|pattern| pattern.automation.is_empty()));
    }

    #[test]
    fn automation_validation_rejects_bounds_order_curve_and_stale_effects() {
        let cfg = config();
        let mut song = Song::new(&cfg);
        song.patterns.get_mut(&0).unwrap().pages[0].target = PageTarget::ConfiguredExternal;
        let lane = AutomationLane {
            id: 1,
            target: AutomationTarget::MidiCc {
                page: 0,
                channel: 0,
                controller: 1,
            },
            curve: AutomationCurve::Linear,
            points: vec![AutomationPoint { tick: 0, value: 1 }],
        };
        song.patterns.get_mut(&0).unwrap().automation.push(lane);
        assert!(song.validate().is_ok());

        song.patterns.get_mut(&0).unwrap().automation[0]
            .points
            .push(AutomationPoint { tick: 0, value: 2 });
        assert!(song
            .validate()
            .unwrap_err()
            .to_string()
            .contains("strictly ordered"));
        song.patterns.get_mut(&0).unwrap().automation[0]
            .points
            .pop();
        song.patterns.get_mut(&0).unwrap().automation[0].curve = AutomationCurve::Step;
        assert!(song.validate().unwrap_err().to_string().contains("linear"));

        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.automation[0] = AutomationLane {
            id: 2,
            target: AutomationTarget::Effect {
                rack: EffectRackTarget::Source,
                effect_id: 99_999,
                effect_kind: crate::audio_graph::EffectKind::Utility,
                effect_version: crate::audio_graph::EFFECT_FORMAT_VERSION,
                parameter: "trim_db".into(),
            },
            curve: AutomationCurve::Linear,
            points: Vec::new(),
        };
        assert!(song.validate().unwrap_err().to_string().contains("stale"));
    }

    #[test]
    fn effect_automation_cleanup_is_exact_across_patterns_and_racks() {
        let cfg = config();
        let mut song = Song::new(&cfg);
        song.insert_rack
            .add_with_id(crate::audio_graph::EffectKind::Utility, 1)
            .unwrap();
        song.aux_routing
            .master_rack
            .add_with_id(crate::audio_graph::EffectKind::Utility, 2)
            .unwrap();
        let second = Pattern::new(
            cfg.default_pattern_rows,
            cfg.default_tempo,
            4,
            song.patterns[&0].pages.clone(),
        );
        let second_number = song.append_pattern(second).unwrap();
        for (pattern_number, rack, effect_id) in [
            (0, EffectRackTarget::Source, 1),
            (second_number, EffectRackTarget::Master, 2),
        ] {
            song.patterns
                .get_mut(&pattern_number)
                .unwrap()
                .automation
                .push(AutomationLane {
                    id: 1,
                    target: AutomationTarget::Effect {
                        rack,
                        effect_id,
                        effect_kind: crate::audio_graph::EffectKind::Utility,
                        effect_version: crate::audio_graph::EFFECT_FORMAT_VERSION,
                        parameter: "trim_db".into(),
                    },
                    curve: AutomationCurve::Linear,
                    points: vec![AutomationPoint { tick: 0, value: 7 }],
                });
        }
        song.validate().unwrap();

        song.insert_rack.remove(1).unwrap();
        assert_eq!(
            song.remove_effect_automation(EffectRackTarget::Source, 1),
            (1, 1)
        );

        assert!(song.patterns[&0].automation.is_empty());
        assert_eq!(song.patterns[&second_number].automation.len(), 1);
        song.validate().unwrap();
    }

    #[test]
    fn format_twelve_preserves_routing_and_migrates_safe_family_effect_defaults() {
        let cfg = config();
        let mut original =
            Song::new_with_pages(&cfg, factory_routing_pages("Lead", gm_drums_route()));
        original.drum_kit = "big-rock-muldjord".into();
        pages_mut(&mut original)[2].target = PageTarget::ConfiguredExternal;
        let legacy = without_v15_rhythm_fields(&encode(&original).unwrap())
            .lines()
            .filter(|line| !line.starts_with("drum_rack="))
            .map(|line| {
                if line == "SHSYNTH-SONG 17" {
                    "SHSYNTH-SONG 12"
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let migrated = decode(&legacy).unwrap();

        assert_eq!(pages(&migrated)[2].target, PageTarget::ConfiguredExternal);
        assert_eq!(migrated.drum_kit, "big-rock-muldjord");
        assert_eq!(
            crate::audio_graph::drum_effect_mode_label(&migrated.drum_rack),
            "REVERB"
        );
        let reverb = migrated
            .drum_rack
            .effect(migrated.drum_rack.order[0])
            .unwrap();
        assert_eq!(reverb.parameters["type"], 0.0);
        assert_eq!(reverb.parameters["predelay_ms"], 14.0);
        assert_eq!(
            encode(&migrated).unwrap().lines().next(),
            Some("SHSYNTH-SONG 17")
        );
    }

    #[test]
    fn format_eleven_preserves_legacy_drum_route_and_migrates_project_defaults() {
        let cfg = config();
        let mut original =
            Song::new_with_pages(&cfg, factory_routing_pages("Lead", gm_drums_route()));
        pages_mut(&mut original)[2].target = PageTarget::ConfiguredExternal;
        let legacy = without_v12_fields(&encode(&original).unwrap()).replacen(
            "SHSYNTH-SONG 17",
            "SHSYNTH-SONG 11",
            1,
        );
        let migrated = decode(&legacy).unwrap();

        assert_eq!(migrated.project_key, Scale::default());
        assert_eq!(migrated.drum_kit, "electronic-house");
        assert_eq!(migrated.drum_tuning, shr_drums::KitTuning::default());
        assert_eq!(pages(&migrated)[2].target, PageTarget::ConfiguredExternal);
    }

    #[test]
    fn internal_drums_and_moj_sint_schedule_together_without_duplicate_routes() {
        let cfg = config();
        let mut song = Song::new_with_pages(&cfg, factory_routing_pages("Lead", gm_drums_route()));
        let song_pages = pages_mut(&mut song);
        let moj = SoftwareRoute {
            engine: BackendKind::MojSint,
            instrument: "Model D Baseline".into(),
        };
        song_pages[0].target = PageTarget::Software(moj.clone());
        song_pages[2].target = PageTarget::InternalDrums("electronic-house".into());
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.rows[0][0].note = Note::On(60);
        pattern.rows[0][LANES_PER_PAGE * 2].note = Note::On(36);

        let scheduled = schedule(&song, &cfg, 0, 0).unwrap();
        assert!(scheduled.iter().any(|message| {
            message.target == Some(PageTarget::Software(moj.clone()))
                && message.bytes == [0x90, 60, 96]
        }));
        assert!(scheduled.iter().any(|message| {
            message.target == Some(PageTarget::InternalDrums("electronic-house".into()))
                && message.bytes == [0x99, 36, 96]
        }));
        let encoded = encode(&song).unwrap();
        let restored = decode(&encoded).unwrap();
        assert_eq!(
            pages(&restored)[0].target,
            PageTarget::Software(moj),
            "Project retains Moj route identity"
        );
    }

    #[test]
    fn current_format_round_trips_decimal_tempos_and_page_note_off_choice() {
        let mut song = Song::new(&config());
        let decimal = "100.50".parse::<Bpm>().unwrap();
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.tempo = decimal;
        pattern.pages[0].note_off_enabled = false;
        pattern.rows[3][0].command = Command::Tempo("99.75".parse().unwrap());
        pattern.rows[3][0].nudge = -24;
        let encoded = encode(&song).unwrap();
        assert!(encoded.starts_with("SHSYNTH-SONG 17\n"));
        assert!(encoded.contains("pattern=0|64|10050|4|sixteenth|50\n"));
        assert!(encoded.contains("|manual|1|0\n"));
        assert!(encoded.contains("|T9975|-24|100|-\n"));
        assert_eq!(decode(&encoded).unwrap(), song);
    }

    #[test]
    fn format_fourteen_migrates_to_straight_on_grid_without_rewriting() {
        let mut current = Song::new(&config());
        let pattern = current.patterns.get_mut(&0).unwrap();
        pattern.swing_division = SwingDivision::Eighth;
        pattern.swing_percent = 67;
        pattern.rows[1][0] = Cell {
            note: Note::On(60),
            nudge: 24,
            ..Cell::default()
        };
        let legacy = without_v15_rhythm_fields(&encode(&current).unwrap()).replacen(
            "SHSYNTH-SONG 17",
            "SHSYNTH-SONG 14",
            1,
        );
        let base = env::temp_dir().join(format!("shr-rhythm-v14-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let path = base.join("legacy.shsong");
        fs::write(&path, &legacy).unwrap();

        let loaded = load(&base, "legacy").unwrap();
        assert_eq!(loaded.patterns[&0].swing_division, SwingDivision::Sixteenth);
        assert_eq!(loaded.patterns[&0].swing_percent, 50);
        assert_eq!(loaded.patterns[&0].rows[1][0].nudge, 0);
        assert_eq!(fs::read_to_string(&path).unwrap(), legacy);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn format_ten_migrates_note_off_defaults_without_rewriting() {
        let current = encode(&Song::new(&config())).unwrap();
        let legacy = without_v15_rhythm_fields(&current)
            .lines()
            .map(|line| {
                if line == "SHSYNTH-SONG 17" {
                    "SHSYNTH-SONG 10".to_owned()
                } else if line.starts_with("project_key=")
                    || line.starts_with("drum_kit=")
                    || line.starts_with("drum_tuning=")
                    || line.starts_with("drum_rack=")
                {
                    String::new()
                } else if line.starts_with("pattern_page=") {
                    line.rsplit_once('|').unwrap().0.to_owned()
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let loaded = decode(&legacy).unwrap();
        let pages = &loaded.patterns[&0].pages;
        assert!(pages[0].note_off_enabled);
        assert!(!pages[1].note_off_enabled);
    }

    #[test]
    fn format_nine_whole_tempos_migrate_in_memory_without_rewriting() {
        let current = encode(&Song::new(&config())).unwrap();
        let legacy =
            downgrade_tempo_fields(&current).replacen("SHSYNTH-SONG 17", "SHSYNTH-SONG 9", 1);
        let base = env::temp_dir().join(format!("shr-tempo-v9-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let path = base.join("legacy.shsong");
        fs::write(&path, &legacy).unwrap();
        let loaded = load(&base, "legacy").unwrap();
        assert_eq!(loaded.patterns[&0].tempo, Bpm::DEFAULT);
        assert_eq!(fs::read_to_string(&path).unwrap(), legacy);
        assert!(encode(&loaded).unwrap().starts_with("SHSYNTH-SONG 17\n"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn format_eight_migrates_to_a_neutral_project_strip_without_rewriting() {
        let current = encode(&Song::new(&config())).unwrap();
        let legacy = downgrade_tempo_fields(&current)
            .lines()
            .filter(|line| !line.starts_with("master_strip="))
            .collect::<Vec<_>>()
            .join("\n")
            .replacen("SHSYNTH-SONG 17", "SHSYNTH-SONG 8", 1);
        let base = env::temp_dir().join(format!("shr-strip-v8-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let path = base.join("legacy.shsong");
        fs::write(&path, &legacy).unwrap();

        let loaded = load(&base, "legacy").unwrap();
        assert_eq!(loaded.master_strip, MasterStripSettings::default());
        assert_eq!(fs::read_to_string(&path).unwrap(), legacy);
        assert!(encode(&loaded).unwrap().starts_with("SHSYNTH-SONG 17\n"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn current_master_strip_record_is_strict_before_replacement() {
        let encoded = encode(&Song::new(&config())).unwrap();
        assert!(
            decode(&encoded.replacen("\"input_trim_db\":0.0", "\"input_trim_db\":99.0", 1))
                .is_err()
        );
        assert!(
            decode(&encoded.replacen("\"input_trim_db\":0.0", "\"input_trim_db\":null", 1))
                .is_err()
        );
        assert!(
            decode(&encoded.replacen("\"version\":1", "\"version\":1,\"unknown\":false", 1))
                .is_err()
        );
        assert!(decode(&encoded.replacen("\"version\":1", "\"version\":2", 1)).is_err());
        assert!(decode(&encoded.replacen("SHSYNTH-SONG 17", "SHSYNTH-SONG 18", 1)).is_err());
    }

    #[test]
    fn version_five_projects_preserve_manual_and_explicit_percussion_entry_defaults() {
        let current = encode(&Song::new(&config())).unwrap();
        let old = as_v5(&current);
        let loaded = decode(&old).unwrap();
        for page in loaded.patterns.values().flat_map(|pattern| &pattern.pages) {
            assert_eq!(page.entry_mode, legacy_entry_mode(page.percussion));
            assert_eq!(page.entry_anchor, 0);
            assert!(page.drum_class_overrides.is_empty());
        }
        assert!(loaded
            .patterns
            .values()
            .flat_map(|pattern| &pattern.pages)
            .any(|page| !page.percussion && page.entry_mode == NoteEntryMode::Manual));
    }

    #[test]
    fn page_entry_modes_anchors_and_drum_classes_round_trip_independently() {
        let mut song = Song::new(&config());
        let pages = &mut song.patterns.get_mut(&0).unwrap().pages;
        pages[0].entry_mode = NoteEntryMode::OneColumn;
        pages[0].entry_anchor = 2;
        pages[1].entry_mode = NoteEntryMode::DrumAuto;
        pages[1]
            .drum_class_overrides
            .insert(60, DrumNoteClass::new(DrumRole::LongTail, Some(7)));
        let loaded = decode(&encode(&song).unwrap()).unwrap();
        let pages = &loaded.patterns[&0].pages;
        assert_eq!(pages[0].entry_mode, NoteEntryMode::OneColumn);
        assert_eq!(pages[0].entry_anchor, 2);
        assert_eq!(pages[1].entry_mode, NoteEntryMode::DrumAuto);
        assert_eq!(
            pages[1].drum_class(60),
            DrumNoteClass::new(DrumRole::LongTail, Some(7))
        );
    }

    #[test]
    fn drum_auto_compacts_core_hits_and_splits_simultaneous_strikes() {
        let mut pattern = Pattern::new(8, Bpm::DEFAULT, 4, vec![Page::new("DRUMS", 9, true, 0)]);
        assert_eq!(enter_drums(&mut pattern, 0, &[36]), Some(vec![0]));
        assert_eq!(enter_drums(&mut pattern, 1, &[38]), Some(vec![0]));
        let lanes = enter_drums(&mut pattern, 2, &[36, 38, 42, 45]).unwrap();
        assert_eq!(lanes.len(), LANES_PER_PAGE);
        assert_eq!(lanes.iter().copied().collect::<BTreeSet<_>>().len(), 4);
        assert_eq!(
            pattern.rows[2]
                .iter()
                .filter(|cell| matches!(cell.note, Note::On(_)))
                .count(),
            4
        );
    }

    #[test]
    fn drum_auto_preserves_existing_row_events_and_is_atomic_when_full() {
        let mut pattern = Pattern::new(4, Bpm::DEFAULT, 4, vec![Page::new("DRUMS", 9, true, 0)]);
        pattern.rows[1][0].note = Note::On(36);
        pattern.rows[1][1].note = Note::On(38);
        let lanes = enter_drums(&mut pattern, 1, &[45]).unwrap();
        assert!(!lanes.contains(&0));
        assert!(!lanes.contains(&1));
        assert_eq!(pattern.rows[1][0].note, Note::On(36));
        assert_eq!(pattern.rows[1][1].note, Note::On(38));

        let before = pattern.rows[1].clone();
        pattern.rows[1][3].note = Note::On(49);
        let full = pattern.rows[1].clone();
        assert_eq!(drum_auto_lanes(&pattern, 1, 0, &[51], &[]), None);
        assert_eq!(pattern.rows[1], full);
        assert_ne!(pattern.rows[1], before);
    }

    #[test]
    fn drum_auto_avoids_unrelated_cymbal_tails_and_honours_choke_groups() {
        let mut pattern = Pattern::new(8, Bpm::DEFAULT, 4, vec![Page::new("DRUMS", 9, true, 0)]);
        let cymbal_lane = enter_drums(&mut pattern, 0, &[49]).unwrap()[0];
        let kick_lane = enter_drums(&mut pattern, 1, &[36]).unwrap()[0];
        assert_ne!(kick_lane, cymbal_lane);
        let second_cymbal = enter_drums(&mut pattern, 2, &[51]).unwrap()[0];
        assert_ne!(second_cymbal, cymbal_lane);

        let mut hats = Pattern::new(4, Bpm::DEFAULT, 4, vec![Page::new("HATS", 4, true, 0)]);
        let open_lane = enter_drums(&mut hats, 0, &[46]).unwrap()[0];
        let closed_lane = enter_drums(&mut hats, 1, &[42]).unwrap()[0];
        assert_eq!(closed_lane, open_lane);
    }

    #[test]
    fn drum_auto_is_deterministic_and_unknown_notes_are_short_other_percussion() {
        let mut first = Pattern::new(8, Bpm::DEFAULT, 4, vec![Page::new("KIT", 2, true, 0)]);
        first.rows[0][0].note = Note::On(36);
        first.rows[0][2].note = Note::On(49);
        let second = first.clone();
        let notes = [38, 60, 51];
        assert_eq!(
            drum_auto_lanes(&first, 3, 0, &notes, &[]),
            drum_auto_lanes(&second, 3, 0, &notes, &[])
        );
        assert_eq!(first.pages[0].drum_class(60).role, DrumRole::Other);
    }

    #[test]
    fn playback_does_not_manufacture_a_release_between_percussion_one_shots() {
        let cfg = config();
        let mut song = Song::new_with_pages(&cfg, vec![Page::new("DRUMS", 9, true, 0)]);
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.rows[0][0].note = Note::On(49);
        pattern.rows[1][0].note = Note::On(36);
        let scheduled = schedule(&song, &cfg, 0, 0).unwrap();
        let kick = scheduled
            .iter()
            .find(|message| message.bytes == [0x99, 36, 96])
            .unwrap();
        assert!(!scheduled.iter().any(|message| {
            message.at == kick.at && message.lane == Some(0) && message.bytes == [0x89, 49, 0]
        }));
    }
    #[test]
    fn current_format_loop_round_trips_and_old_shapes_are_rejected() {
        let mut with_loop = Song::new(&config());
        with_loop.patterns.get_mut(&0).unwrap().meter = 3;
        let second = Pattern::empty_like_setup(32, &with_loop.patterns[&0]);
        let second_number = with_loop.append_pattern(second).unwrap();
        for slot in 0..LOOP_SLOT_COUNT {
            let owner = if slot % 2 == 0 { 0 } else { second_number };
            with_loop.patterns.get_mut(&owner).unwrap().audio_loops[slot] = Some(LoopSettings {
                file: format!("stem-{}.wav", slot + 1),
                source_bpm_x100: 12_000,
                interpretation: BpmInterpretation::Half,
                start_beat: slot as u32,
                length_beats: 12 + slot as u32 * 3,
                offset_beats: slot as i32 * 3 - 4,
                level_x1000: 700 + slot as u16 * 200,
                filter_x1000: -750 + slot as i16 * 500,
            });
        }
        assert_eq!(decode(&encode(&with_loop).unwrap()).unwrap(), with_loop);
        let encoded = encode(&with_loop).unwrap();
        for slot in 1..=LOOP_SLOT_COUNT {
            let owner = if slot % 2 == 1 { 0 } else { second_number };
            assert!(encoded.contains(&format!("pattern_loop={owner}|{slot}|stem-{slot}.wav|")));
        }

        let missing_offset = encoded.replace("|0|12|-4|700|-750\n", "|0|12\n");
        assert!(decode(&missing_offset).is_err());

        let old_shared_pages = encode(&with_loop)
            .unwrap()
            .replace(
                "pattern=0|64|12000|3\n",
                "tempo=120\nmeter=3\npattern=0|64\n",
            )
            .replace("pattern_page=0|", "page=")
            .replace("pattern_lane=0|", "lane=");
        assert!(decode(&old_shared_pages).is_err());
    }

    #[test]
    fn version_seven_global_slots_migrate_to_every_distinct_pattern() {
        let mut current = Song::new(&config());
        let clone = current.patterns[&0].clone();
        let second = current.append_pattern(clone).unwrap();
        let legacy = downgrade_tempo_fields(&encode(&current).unwrap())
            .lines()
            .filter(|line| !line.starts_with("master_strip="))
            .collect::<Vec<_>>()
            .join("\n")
            .replacen("SHSYNTH-SONG 17", "SHSYNTH-SONG 7", 1)
            .replacen(
                "insert_rack=",
                "loop_slot=1|shared.wav|12000|normal|0|16|0|875|-200\ninsert_rack=",
                1,
            );
        let migrated = decode(&legacy).unwrap();
        assert_eq!(
            migrated.patterns[&0].audio_loops,
            migrated.patterns[&second].audio_loops
        );
        assert_eq!(
            migrated.patterns[&0].audio_loops[0]
                .as_ref()
                .map(|settings| settings.file.as_str()),
            Some("shared.wav")
        );
        assert!(encode(&migrated)
            .unwrap()
            .contains(&format!("pattern_loop={second}|1|shared.wav|")));
    }

    #[test]
    fn version_six_single_loop_migrates_to_slot_one_without_rewriting_the_file() {
        let mut source = Song::new(&config());
        let second = source.append_pattern(source.patterns[&0].clone()).unwrap();
        let current = encode(&source).unwrap();
        let legacy = downgrade_tempo_fields(&current)
            .lines()
            .filter(|line| !line.starts_with("master_strip="))
            .collect::<Vec<_>>()
            .join("\n")
            .replacen("SHSYNTH-SONG 17", "SHSYNTH-SONG 6", 1)
            .replacen(
                "insert_rack=",
                "loop=legacy.wav|9876|double|5|14|-8\ninsert_rack=",
                1,
            );
        let migrated = decode(&legacy).unwrap();
        assert_eq!(
            migrated.patterns[&0].audio_loops[0],
            Some(LoopSettings {
                file: "legacy.wav".into(),
                source_bpm_x100: 9_876,
                interpretation: BpmInterpretation::Double,
                start_beat: 5,
                length_beats: 14,
                offset_beats: -8,
                level_x1000: 1_000,
                filter_x1000: 0,
            })
        );
        assert!(migrated.patterns[&0].audio_loops[1..]
            .iter()
            .all(Option::is_none));
        assert_eq!(
            migrated.patterns[&0].audio_loops,
            migrated.patterns[&second].audio_loops
        );
        assert!(encode(&migrated)
            .unwrap()
            .contains("pattern_loop=0|1|legacy.wav|9876|double|5|14|-8|1000|0"));

        let base = env::temp_dir().join(format!("shr-v6-migration-{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        let path = base.join("legacy.shsong");
        fs::write(&path, &legacy).unwrap();
        assert_eq!(load(&base, "legacy").unwrap(), migrated);
        assert_eq!(fs::read_to_string(&path).unwrap(), legacy);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn pattern_operations_copy_detach_retain_and_never_delete_private_wavs() {
        let mut song = Song::new(&config());
        let settings = LoopSettings {
            file: "shared-private.wav".into(),
            source_bpm_x100: 12_000,
            interpretation: BpmInterpretation::Normal,
            start_beat: 2,
            length_beats: 16,
            offset_beats: -4,
            level_x1000: 875,
            filter_x1000: 250,
        };
        song.patterns.get_mut(&0).unwrap().audio_loops[2] = Some(settings.clone());

        let clone_number = song.append_pattern(song.patterns[&0].clone()).unwrap();
        assert_eq!(
            song.patterns[&clone_number].audio_loops[2],
            Some(settings.clone())
        );
        song.patterns.get_mut(&clone_number).unwrap().audio_loops[2]
            .as_mut()
            .unwrap()
            .level_x1000 = 500;
        assert_eq!(
            song.patterns[&0].audio_loops[2]
                .as_ref()
                .unwrap()
                .level_x1000,
            875,
            "a cloned Pattern owns an independent settings copy"
        );

        song.patterns.get_mut(&0).unwrap().resize_rows(24).unwrap();
        assert_eq!(song.patterns[&0].audio_loops[2], Some(settings.clone()));

        let cleared = Pattern::empty_like_setup(24, &song.patterns[&0]);
        assert!(cleared.audio_loops.iter().all(Option::is_none));
        assert_eq!(cleared.pages, song.patterns[&0].pages);

        let base = env::temp_dir().join(format!("shr-pattern-loop-clean-{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        let wav = base.join(&settings.file);
        fs::write(&wav, b"private fixture").unwrap();
        song.order.retain(|number| *number != clone_number);
        song.delete_unused_pattern(clone_number).unwrap();
        assert!(wav.exists(), "Pattern CLEAN must not delete a private WAV");
        fs::remove_dir_all(base).unwrap();

        let new_pattern = Pattern::empty_like_setup(32, &song.patterns[&0]);
        assert_eq!(new_pattern.audio_loops.len(), LOOP_SLOT_COUNT);
        assert!(new_pattern.audio_loops.iter().all(Option::is_none));
    }

    #[test]
    fn format_eight_refuses_malformed_duplicate_and_unowned_loop_records() {
        let song = Song::new(&config());
        let encoded = encode(&song).unwrap();
        let record = "pattern_loop=0|1|safe.wav|12000|normal|0|4|0|1000|0\n";
        let valid = encoded.replacen("pattern_page=", &format!("{record}pattern_page="), 1);
        assert!(decode(&valid).is_ok());
        assert!(decode(&valid.replacen(record, &format!("{record}{record}"), 1)).is_err());
        assert!(decode(&valid.replace("pattern_loop=0|1|", "pattern_loop=999|1|")).is_err());
        assert!(decode(&valid.replace("pattern_loop=0|1|", "pattern_loop=0|5|")).is_err());
        assert!(decode(&valid.replace("|1000|0\n", "|1501|0\n")).is_err());
        assert!(decode(&valid.replace("safe.wav", "../safe.wav")).is_err());
    }

    #[test]
    fn repeated_arrangement_references_share_one_pattern_loop_owner() {
        let mut song = Song::new(&config());
        song.order = vec![0, 0, 0];
        song.patterns.get_mut(&0).unwrap().audio_loops[0] = Some(LoopSettings::new(
            "shared.wav".into(),
            12_000,
            BpmInterpretation::Normal,
            0,
            4,
            0,
        ));
        song.patterns.get_mut(&0).unwrap().audio_loops[0]
            .as_mut()
            .unwrap()
            .filter_x1000 = -300;
        for pattern_number in &song.order {
            assert_eq!(
                song.patterns[pattern_number].audio_loops[0]
                    .as_ref()
                    .unwrap()
                    .filter_x1000,
                -300
            );
        }
    }
    #[test]
    fn current_song_format_round_trips_every_cell_field() {
        let mut song = Song::new(&config());
        song.patterns.get_mut(&0).unwrap().rows[0][0] = Cell {
            note: Note::On(64),
            velocity: Some(111),
            program: Some(17),
            gate: Some(37),
            command: Command::Delay(6),
            nudge: 24,
            probability: 73,
            condition: StepCondition::Ratio { hit: 2, cycle: 3 },
        };
        let encoded = encode(&song).unwrap();
        assert!(encoded.starts_with("SHSYNTH-SONG 17\n"));
        assert!(encoded.contains("|64|111|17|37|D6|24|73|2:3\n"));
        assert_eq!(decode(&encoded).unwrap(), song);
    }

    #[test]
    fn cell_nudge_schedules_early_on_grid_and_late_in_exact_row_fractions() {
        let c = config();
        let mut song = Song::new(&c);
        let pattern = song.patterns.get_mut(&0).unwrap();
        for (row, lane, note, nudge) in [(1, 0, 60, -48), (2, 1, 61, 0), (3, 2, 62, 48)] {
            pattern.rows[row][lane] = Cell {
                note: Note::On(note),
                nudge,
                ..Cell::default()
            };
        }

        let messages = schedule_elapsed(&song, &c, 0, 0).unwrap();
        let at = |note| {
            messages
                .iter()
                .find(|message| {
                    message.bytes.len() == 3
                        && message.bytes[0] & 0xf0 == 0x90
                        && message.bytes[1..] == [note, 96]
                })
                .unwrap()
                .at
        };
        assert_eq!(at(60), Duration::from_micros(62_500));
        assert_eq!(at(61), Duration::from_millis(250));
        assert_eq!(at(62), Duration::from_micros(437_500));
    }

    #[test]
    fn cell_nudge_fraction_is_tempo_relative_at_the_supported_extremes() {
        let c = config();
        for bpm in [20, 300] {
            let mut song = Song::new(&c);
            let pattern = song.patterns.get_mut(&0).unwrap();
            pattern.tempo = Bpm::from_whole(bpm).unwrap();
            pattern.rows[1][0] = Cell {
                note: Note::On(60),
                nudge: -48,
                ..Cell::default()
            };
            let row = Duration::from_secs_f64(60.0 / f64::from(bpm) / 4.0);
            let note = schedule_elapsed(&song, &c, 0, 0)
                .unwrap()
                .into_iter()
                .find(|message| message.bytes == [0x90, 60, 96])
                .unwrap();
            assert_eq!(note.at, row.div_f64(2.0));
        }
    }

    #[test]
    fn pattern_swing_moves_only_the_selected_alternating_subdivision() {
        let c = config();
        let note_at = |division, row: usize| {
            let mut song = Song::new(&c);
            let pattern = song.patterns.get_mut(&0).unwrap();
            pattern.swing_division = division;
            pattern.swing_percent = 67;
            pattern.rows[row][0].note = Note::On(60);
            schedule_elapsed(&song, &c, 0, 0)
                .unwrap()
                .into_iter()
                .find(|message| message.bytes == [0x90, 60, 96])
                .unwrap()
                .at
        };

        assert_eq!(note_at(SwingDivision::Sixteenth, 0), Duration::ZERO);
        assert_eq!(
            note_at(SwingDivision::Sixteenth, 1),
            Duration::from_micros(167_500)
        );
        assert_eq!(
            note_at(SwingDivision::Eighth, 2),
            Duration::from_millis(335)
        );
    }

    #[test]
    fn swing_does_not_move_the_steady_midi_clock() {
        let mut c = config();
        c.send_transport = true;
        let mut straight = Song::new(&c);
        straight.patterns.get_mut(&0).unwrap().rows.truncate(4);
        let mut swung = straight.clone();
        swung.patterns.get_mut(&0).unwrap().swing_percent = 75;
        let clocks = |song: &Song| {
            schedule_elapsed(song, &c, 0, 0)
                .unwrap()
                .into_iter()
                .filter(|message| message.bytes == [0xf8])
                .map(|message| message.at)
                .collect::<Vec<_>>()
        };
        assert_eq!(clocks(&straight), clocks(&swung));
    }

    #[test]
    fn crossed_same_lane_timing_suppresses_the_replaced_notes_stale_gate_release() {
        let c = config();
        let mut song = Song::new(&c);
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.rows[1][0] = Cell {
            note: Note::On(60),
            nudge: 48,
            ..Cell::default()
        };
        pattern.rows[2][0] = Cell {
            note: Note::On(61),
            nudge: -48,
            ..Cell::default()
        };
        let messages = schedule_elapsed(&song, &c, 0, 0).unwrap();
        let replacement = messages
            .iter()
            .find(|message| message.bytes == [0x90, 61, 96])
            .unwrap();
        let replacement_release = messages
            .iter()
            .find(|message| message.bytes == [0x80, 61, 0])
            .unwrap();
        assert_eq!(replacement.at, Duration::from_micros(187_500));
        assert!(replacement_release.at > replacement.at);
        assert!(!messages.iter().any(|message| {
            message.bytes == [0x80, 60, 0]
                && message.at > replacement.at
                && message.at < replacement_release.at
        }));
    }

    #[test]
    fn cell_nudge_validation_protects_bounds_and_pattern_edges() {
        let mut pattern = Pattern::empty(4, LANES_PER_PAGE);
        pattern.rows[1][0].nudge = MAX_CELL_NUDGE + 1;
        assert!(pattern.validate().is_err());
        pattern.rows[1][0].nudge = 0;
        pattern.rows[0][0].nudge = -1;
        assert!(pattern.validate().is_err());
        pattern.rows[0][0].nudge = 0;
        pattern.rows[3][0].nudge = MAX_CELL_NUDGE;
        assert!(pattern.validate().is_ok());
    }

    fn scheduled_note_on(messages: &[ScheduledMessage], note: u8) -> bool {
        messages.iter().any(|message| {
            matches!(message.bytes.as_slice(), [status, candidate, velocity]
                if status & 0xf0 == 0x90 && *candidate == note && *velocity > 0)
        })
    }

    #[test]
    fn step_conditions_follow_first_last_ratio_previous_and_fill_passes() {
        let cfg = config();
        let mut song = Song::new(&cfg);
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.rows.truncate(8);
        for (row, note, condition) in [
            (0, 60, StepCondition::First),
            (1, 61, StepCondition::Last(4)),
            (2, 62, StepCondition::Ratio { hit: 2, cycle: 3 }),
            (3, 63, StepCondition::Always),
            (4, 64, StepCondition::Previous),
            (5, 65, StepCondition::Fill),
        ] {
            pattern.rows[row][0] = Cell {
                note: Note::On(note),
                condition,
                ..Cell::default()
            };
        }
        pattern.rows[0][1] = Cell {
            note: Note::On(66),
            condition: StepCondition::First,
            ..Cell::default()
        };
        pattern.rows[1][1] = Cell {
            note: Note::On(67),
            condition: StepCondition::Previous,
            ..Cell::default()
        };

        let first = schedule_for_pass(&song, &cfg, 0, 0, 1, false).unwrap();
        assert!(scheduled_note_on(&first, 60));
        assert!(!scheduled_note_on(&first, 61));
        assert!(!scheduled_note_on(&first, 62));
        assert!(scheduled_note_on(&first, 63));
        assert!(scheduled_note_on(&first, 64));
        assert!(!scheduled_note_on(&first, 65));
        assert!(scheduled_note_on(&first, 66));
        assert!(scheduled_note_on(&first, 67));

        let second = schedule_for_pass(&song, &cfg, 0, 0, 2, true).unwrap();
        assert!(!scheduled_note_on(&second, 60));
        assert!(!scheduled_note_on(&second, 61));
        assert!(scheduled_note_on(&second, 62));
        assert!(scheduled_note_on(&second, 65));
        assert!(!scheduled_note_on(&second, 66));
        assert!(!scheduled_note_on(&second, 67));

        let fourth = schedule_for_pass(&song, &cfg, 0, 0, 4, false).unwrap();
        assert!(scheduled_note_on(&fourth, 61));
    }

    #[test]
    fn probability_is_repeatable_but_varies_by_pass_and_preflight_includes_all() {
        let cfg = config();
        let mut song = Song::new(&cfg);
        song.patterns.get_mut(&0).unwrap().rows[0][0] = Cell {
            note: Note::On(72),
            probability: 50,
            condition: StepCondition::Fill,
            ..Cell::default()
        };
        let outcomes = (1..=24)
            .map(|pass| {
                let one = schedule_for_pass(&song, &cfg, 0, 0, pass, true).unwrap();
                let two = schedule_for_pass(&song, &cfg, 0, 0, pass, true).unwrap();
                assert_eq!(one, two);
                scheduled_note_on(&one, 72)
            })
            .collect::<Vec<_>>();
        assert!(outcomes.contains(&true));
        assert!(outcomes.contains(&false));
        assert!(!scheduled_note_on(
            &schedule(&song, &cfg, 0, 0).unwrap(),
            72
        ));
        assert!(scheduled_note_on(
            &schedule_preflight(&song, &cfg, 0, 0).unwrap(),
            72
        ));
    }

    #[test]
    fn generated_fill_cells_follow_fill_pass_and_preflight_ownership() {
        let cfg = config();
        let mut song = Song::new(&cfg);
        let pattern = song.patterns.get_mut(&0).unwrap();
        let page = pattern
            .pages
            .iter()
            .position(|page| page.percussion)
            .unwrap();
        let lane = page * LANES_PER_PAGE;
        pattern.rows[1][lane] = Cell {
            note: Note::On(38),
            velocity: Some(84),
            ..Cell::default()
        };
        let mut recipe = crate::generative::Recipe::bounded_for(
            pattern,
            crate::generative::Tool::Fill,
            page,
            0,
            1,
            99,
        )
        .unwrap();
        recipe.length = 8;
        recipe.amount = 4;
        recipe.collision = crate::generative::CollisionPolicy::ReplaceNotes;
        let draft = crate::generative::build(pattern, recipe).unwrap();
        assert_eq!(draft.report.candidates, 4);
        assert!(draft
            .affected_rows
            .iter()
            .all(|row| draft.pattern.rows[*row][lane].condition == StepCondition::Fill));
        song.patterns.insert(0, draft.pattern);

        let off = schedule_for_pass(&song, &cfg, 0, 0, 1, false).unwrap();
        let on = schedule_for_pass(&song, &cfg, 0, 0, 1, true).unwrap();
        let preflight = schedule_preflight(&song, &cfg, 0, 0).unwrap();
        let count = |messages: &[ScheduledMessage]| {
            messages
                .iter()
                .filter(|message| {
                    matches!(message.bytes.as_slice(), [status, 38, velocity]
                        if status & 0xf0 == 0x90 && *velocity > 0)
                })
                .count()
        };
        assert!(count(&on) > count(&off));
        assert!(count(&preflight) >= count(&on));
    }

    fn lane_note_ons(messages: &[ScheduledMessage], lane: usize) -> Vec<(Duration, u8)> {
        messages
            .iter()
            .filter_map(|message| match message.bytes.as_slice() {
                [status, note, velocity, ..]
                    if status & 0xf0 == 0x90 && *velocity > 0 && message.lane == Some(lane) =>
                {
                    Some((message.at, *note))
                }
                _ => None,
            })
            .collect()
    }

    fn lane_cycle_song(rows: usize) -> (Song, ExternalMidiConfig) {
        let mut cfg = config();
        cfg.bank_select = BankSelectMode::Off;
        cfg.program_changes = false;
        let mut song = Song::new(&cfg);
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.rows.truncate(rows);
        for row in 0..rows {
            pattern.rows[row][0] = Cell {
                note: Note::On(60 + row as u8),
                gate: Some(25),
                ..Cell::default()
            };
        }
        (song, cfg)
    }

    #[test]
    fn independent_lane_length_forward_reverse_and_pendulum_are_exact() {
        let (mut song, cfg) = lane_cycle_song(8);
        let lane = &mut song.patterns.get_mut(&0).unwrap().pages[0].lanes[0];
        lane.playback.cycle_rows = 4;

        let notes = lane_note_ons(&schedule(&song, &cfg, 0, 0).unwrap(), 0)
            .into_iter()
            .map(|(_, note)| note)
            .collect::<Vec<_>>();
        assert_eq!(notes, [60, 61, 62, 63, 60, 61, 62, 63]);

        song.patterns.get_mut(&0).unwrap().pages[0].lanes[0]
            .playback
            .direction = LaneDirection::Reverse;
        let notes = lane_note_ons(&schedule(&song, &cfg, 0, 0).unwrap(), 0)
            .into_iter()
            .map(|(_, note)| note)
            .collect::<Vec<_>>();
        assert_eq!(notes, [63, 62, 61, 60, 63, 62, 61, 60]);

        song.patterns.get_mut(&0).unwrap().pages[0].lanes[0]
            .playback
            .direction = LaneDirection::Pendulum;
        let notes = lane_note_ons(&schedule(&song, &cfg, 0, 0).unwrap(), 0)
            .into_iter()
            .map(|(_, note)| note)
            .collect::<Vec<_>>();
        assert_eq!(notes, [60, 61, 62, 63, 62, 61, 60, 61]);
    }

    #[test]
    fn lane_rates_use_exact_pattern_row_fractions_and_partial_phase() {
        let expected = [
            (LaneRate::Quarter, Duration::from_millis(500)),
            (LaneRate::Half, Duration::from_millis(250)),
            (LaneRate::Normal, Duration::from_millis(125)),
            (LaneRate::Double, Duration::from_micros(62_500)),
            (LaneRate::Quadruple, Duration::from_micros(31_250)),
        ];
        for (rate, spacing) in expected {
            let (mut song, cfg) = lane_cycle_song(8);
            song.patterns.get_mut(&0).unwrap().pages[0].lanes[0]
                .playback
                .rate = rate;
            let notes = lane_note_ons(&schedule(&song, &cfg, 0, 0).unwrap(), 0);
            assert!(notes.len() >= 2, "{} needs two emitted steps", rate.label());
            assert_eq!(notes[1].0 - notes[0].0, spacing, "{}", rate.label());
        }

        let (mut song, cfg) = lane_cycle_song(8);
        song.patterns.get_mut(&0).unwrap().pages[0].lanes[0]
            .playback
            .rate = LaneRate::Half;
        let (first, repeat) = playback_schedules(&song, &cfg, 0, 2).unwrap();
        assert_eq!(
            lane_note_ons(&first, 0).first().map(|(_, note)| *note),
            Some(61)
        );
        assert_eq!(
            lane_note_ons(&repeat, 0).first().map(|(_, note)| *note),
            Some(60)
        );
    }

    #[test]
    fn deterministic_variation_is_a_bounded_permutation_per_cycle() {
        let (mut song, cfg) = lane_cycle_song(8);
        let playback = &mut song.patterns.get_mut(&0).unwrap().pages[0].lanes[0].playback;
        playback.cycle_rows = 4;
        playback.direction = LaneDirection::Variation;
        let first = schedule_for_pass(&song, &cfg, 0, 0, 1, false).unwrap();
        let repeated = schedule_for_pass(&song, &cfg, 0, 0, 1, false).unwrap();
        assert_eq!(first, repeated);
        let notes = lane_note_ons(&first, 0)
            .into_iter()
            .map(|(_, note)| note)
            .collect::<Vec<_>>();
        for cycle in notes.chunks_exact(4) {
            assert_eq!(
                cycle.iter().copied().collect::<BTreeSet<_>>(),
                BTreeSet::from([60, 61, 62, 63])
            );
        }
        assert!((2..=8).any(|pass| {
            let later = lane_note_ons(
                &schedule_for_pass(&song, &cfg, 0, 0, pass, false).unwrap(),
                0,
            )
            .into_iter()
            .map(|(_, note)| note)
            .collect::<Vec<_>>();
            later[..4] != notes[..4]
        }));
    }

    #[test]
    fn lane_wraps_release_held_owners() {
        let (mut song, cfg) = lane_cycle_song(4);
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern
            .rows
            .iter_mut()
            .for_each(|row| row[0] = Cell::default());
        pattern.rows[0][0] = Cell {
            note: Note::On(60),
            gate: Some(100),
            ..Cell::default()
        };
        pattern.pages[0].lanes[0].playback.cycle_rows = 2;
        let messages = schedule(&song, &cfg, 0, 0).unwrap();
        assert!(messages.iter().any(|message| {
            message.at == Duration::from_millis(250) && message.bytes == [0x80, 60, 0]
        }));
    }

    #[test]
    fn lane_cycles_keep_conditions_deterministic_and_preflight_scans_all_sources() {
        let (mut song, cfg) = lane_cycle_song(8);
        {
            let pattern = song.patterns.get_mut(&0).unwrap();
            pattern.pages[0].lanes[0].playback = LanePlayback {
                cycle_rows: 2,
                rate: LaneRate::Normal,
                direction: LaneDirection::Forward,
            };
            pattern.rows[0][0].condition = StepCondition::Previous;
            pattern.rows[1][0].condition = StepCondition::Always;
        }
        let notes = lane_note_ons(&schedule(&song, &cfg, 0, 0).unwrap(), 0)
            .into_iter()
            .map(|(_, note)| note)
            .collect::<Vec<_>>();
        assert_eq!(notes, [61, 60, 61, 60, 61, 60, 61]);

        song.patterns.get_mut(&0).unwrap().pages[0].lanes[0].playback = LanePlayback {
            cycle_rows: 8,
            rate: LaneRate::Quarter,
            direction: LaneDirection::Variation,
        };
        assert!(scheduled_note_on(
            &schedule_preflight(&song, &cfg, 0, 0).unwrap(),
            67
        ));
    }

    #[test]
    fn format_seventeen_round_trips_lane_playback_and_sixteen_migrates_defaults() {
        let mut song = Song::new(&config());
        song.patterns.get_mut(&0).unwrap().pages[0].lanes[2].playback = LanePlayback {
            cycle_rows: 7,
            rate: LaneRate::Double,
            direction: LaneDirection::Reverse,
        };
        let current = encode(&song).unwrap();
        assert!(current.starts_with("SHSYNTH-SONG 17\n"));
        assert!(current.contains("|7|double|reverse\n"));
        assert_eq!(decode(&current).unwrap(), song);

        let legacy = current
            .lines()
            .map(|line| {
                if line == "SHSYNTH-SONG 17" {
                    "SHSYNTH-SONG 16".into()
                } else if let Some(lane) = line.strip_prefix("pattern_lane=") {
                    format!(
                        "pattern_lane={}",
                        lane.split('|').take(5).collect::<Vec<_>>().join("|")
                    )
                } else {
                    line.into()
                }
            })
            .collect::<Vec<String>>()
            .join("\n");
        let migrated = decode(&legacy).unwrap();
        assert!(migrated.patterns[&0]
            .pages
            .iter()
            .flat_map(|page| &page.lanes)
            .all(|lane| lane.playback == LanePlayback::default()));
        assert!(decode(&current.replace("|7|double|reverse", "|999|double|reverse")).is_err());
        assert!(decode(&current.replace("|7|double|reverse", "|7|wild|reverse")).is_err());
    }

    #[test]
    fn format_sixteen_round_trips_conditions_and_fifteen_migrates_to_always() {
        let mut song = Song::new(&config());
        song.patterns.get_mut(&0).unwrap().rows[2][1] = Cell {
            note: Note::On(67),
            probability: 73,
            condition: StepCondition::Ratio { hit: 2, cycle: 5 },
            ..Cell::default()
        };
        let current = encode(&song).unwrap();
        assert!(current.starts_with("SHSYNTH-SONG 17\n"));
        assert!(current.contains("|73|2:5\n"));
        assert_eq!(decode(&current).unwrap(), song);

        let legacy = current
            .lines()
            .map(|line| {
                if line == "SHSYNTH-SONG 17" {
                    "SHSYNTH-SONG 15".into()
                } else if let Some(cell) = line.strip_prefix("cell=") {
                    format!(
                        "cell={}",
                        cell.split('|').take(9).collect::<Vec<_>>().join("|")
                    )
                } else if let Some(lane) = line.strip_prefix("pattern_lane=") {
                    format!(
                        "pattern_lane={}",
                        lane.split('|').take(5).collect::<Vec<_>>().join("|")
                    )
                } else {
                    line.into()
                }
            })
            .collect::<Vec<String>>()
            .join("\n");
        let migrated = decode(&legacy).unwrap();
        let cell = migrated.patterns[&0].rows[2][1];
        assert_eq!(cell.probability, 100);
        assert_eq!(cell.condition, StepCondition::Always);

        let base = env::temp_dir().join(format!("shr-condition-v15-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let path = base.join("legacy.shsong");
        fs::write(&path, &legacy).unwrap();
        let loaded = load(&base, "legacy").unwrap();
        assert_eq!(
            loaded.patterns[&0].rows[2][1].condition,
            StepCondition::Always
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), legacy);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn cell_gate_and_delay_end_within_the_row() {
        let c = config();
        let mut song = Song::new(&c);
        song.patterns.get_mut(&0).unwrap().rows[0][0] = Cell {
            note: Note::On(60),
            gate: Some(40),
            command: Command::Delay(8),
            ..Cell::default()
        };
        let messages = schedule(&song, &c, 0, 0).unwrap();
        let note_on = messages.iter().find(|m| m.bytes == [0x90, 60, 96]).unwrap();
        let note_off = messages
            .iter()
            .find(|m| m.bytes == [0x80, 60, 0] && m.at > note_on.at)
            .unwrap();
        assert_eq!(note_on.at, Duration::from_micros(62_500));
        assert_eq!(note_off.at, Duration::from_micros(112_500));
        assert!(note_off.at <= Duration::from_millis(125));
    }

    #[test]
    fn percussion_hits_are_one_shots_until_an_explicit_release() {
        let c = config();
        let mut song = Song::new_with_pages(&c, vec![Page::new("Drums", 9, true, 0)]);
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.pages[0].target = PageTarget::InternalDrums("big-rock".into());
        pattern.rows[0][0] = Cell {
            note: Note::On(49),
            velocity: Some(110),
            ..Cell::default()
        };
        pattern.rows[2][0] = Cell {
            note: Note::On(49),
            velocity: Some(100),
            command: Command::Retrigger(2),
            ..Cell::default()
        };
        pattern.rows[4][0].note = Note::Off;

        let messages = schedule(&song, &c, 0, 0).unwrap();
        let attacks = messages
            .iter()
            .filter(|message| {
                message
                    .bytes
                    .first()
                    .is_some_and(|status| *status & 0xf0 == 0x90)
            })
            .collect::<Vec<_>>();
        let releases = messages
            .iter()
            .filter(|message| {
                message
                    .bytes
                    .first()
                    .is_some_and(|status| *status & 0xf0 == 0x80)
            })
            .collect::<Vec<_>>();

        assert_eq!(attacks.len(), 3);
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].bytes, [0x89, 49, 0]);
        assert_eq!(releases[0].at, Duration::from_millis(500));
        assert_eq!(
            releases[0].target,
            Some(PageTarget::InternalDrums("big-rock".into()))
        );
    }

    #[test]
    fn percussion_tail_is_not_released_at_arrangement_end() {
        let c = config();
        let mut song = Song::new_with_pages(&c, vec![Page::new("Drums", 9, true, 0)]);
        song.patterns.get_mut(&0).unwrap().rows[0][0].note = Note::On(51);

        let messages = schedule(&song, &c, 0, 0).unwrap();
        assert!(messages
            .iter()
            .any(|message| message.bytes == [0x99, 51, 96]));
        assert!(!messages.iter().any(|message| {
            message
                .bytes
                .first()
                .is_some_and(|status| *status & 0xf0 == 0x80)
        }));
    }

    #[test]
    fn disabled_automatic_note_off_holds_until_retrigger_or_pattern_cleanup() {
        let c = config();
        let mut song = Song::new(&c);
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.pages[0].note_off_enabled = false;
        pattern.rows[0][0] = Cell {
            note: Note::On(60),
            gate: Some(100),
            ..Cell::default()
        };
        pattern.rows[2][0] = Cell {
            note: Note::On(62),
            gate: Some(100),
            ..Cell::default()
        };

        let messages = schedule(&song, &c, 0, 0).unwrap();
        let first_release = messages
            .iter()
            .find(|message| message.bytes == [0x80, 60, 0])
            .unwrap();
        let retrigger = messages
            .iter()
            .find(|message| message.bytes == [0x90, 62, 96])
            .unwrap();
        assert_eq!(first_release.at, retrigger.at);
        assert!(!messages
            .iter()
            .any(|message| { message.bytes == [0x80, 60, 0] && message.at < retrigger.at }));
        assert!(messages
            .iter()
            .any(|message| { message.bytes == [0x80, 62, 0] && message.at > retrigger.at }));
    }

    #[test]
    fn explicit_note_off_extends_a_step_edit_note_across_rows() {
        let c = config();
        let mut song = Song::new(&c);
        song.patterns.get_mut(&0).unwrap().rows[0][0] = Cell {
            note: Note::On(60),
            gate: Some(100),
            ..Cell::default()
        };
        song.patterns.get_mut(&0).unwrap().rows[4][0].note = Note::Off;
        let messages = schedule(&song, &c, 0, 0).unwrap();
        let releases = messages
            .iter()
            .filter(|message| message.bytes == [0x80, 60, 0])
            .map(|message| message.at)
            .collect::<Vec<_>>();
        assert_eq!(releases.first(), Some(&Duration::from_millis(500)));
    }

    #[test]
    fn every_command_schedules_deterministically_through_order_boundaries() {
        let c = config();
        let mut song = Song::new(&c);
        song.patterns
            .insert(0, Pattern::empty(4, song.total_lanes()));
        song.patterns
            .insert(1, Pattern::empty(1, song.total_lanes()));
        song.order = vec![0, 1];
        song.patterns.get_mut(&0).unwrap().rows[0][0] = Cell {
            note: Note::On(60),
            command: Command::Cut(4),
            ..Cell::default()
        };
        song.patterns.get_mut(&0).unwrap().rows[1][0] = Cell {
            note: Note::On(61),
            command: Command::Delay(8),
            ..Cell::default()
        };
        song.patterns.get_mut(&0).unwrap().rows[2][0] = Cell {
            note: Note::On(62),
            command: Command::Retrigger(4),
            ..Cell::default()
        };
        song.patterns.get_mut(&0).unwrap().rows[3][0].command =
            Command::Tempo(Bpm::from_whole(60).unwrap());
        song.patterns.get_mut(&1).unwrap().rows[0][0].note = Note::On(63);
        let messages = schedule(&song, &c, 0, 0).unwrap();
        assert!(messages
            .iter()
            .any(|m| m.bytes == [0x80, 60, 0] && m.at == Duration::from_micros(31_250)));
        assert!(messages
            .iter()
            .any(|m| m.bytes == [0x90, 61, 96] && m.at == Duration::from_micros(187_500)));
        let retriggers = messages
            .iter()
            .filter(|m| m.bytes == [0x90, 62, 96])
            .map(|m| m.at)
            .collect::<Vec<_>>();
        assert_eq!(
            retriggers,
            [
                Duration::from_millis(250),
                Duration::from_micros(281_250),
                Duration::from_micros(312_500),
                Duration::from_micros(343_750),
            ]
        );
        let boundary_note = messages.iter().find(|m| m.bytes == [0x90, 63, 96]).unwrap();
        assert_eq!(
            (boundary_note.order, boundary_note.at),
            (1, Duration::from_millis(500))
        );
    }
    #[test]
    fn invalid_cell_ranges_are_rejected_without_clamping_files() {
        let mut song = Song::new(&config());
        song.patterns.get_mut(&0).unwrap().rows[0][0] = Cell {
            note: Note::On(60),
            gate: Some(0),
            ..Cell::default()
        };
        assert!(song.validate().unwrap_err().to_string().contains("gate"));
        song.patterns.get_mut(&0).unwrap().rows[0][0] = Cell {
            note: Note::On(60),
            command: Command::Retrigger(9),
            ..Cell::default()
        };
        assert!(song
            .validate()
            .unwrap_err()
            .to_string()
            .contains("retrigger"));
    }
    #[test]
    fn effect_markers_are_stable_and_unambiguous() {
        assert_eq!(Command::None.marker(), ' ');
        assert_eq!(Command::Cut(0).marker(), 'C');
        assert_eq!(Command::Delay(0).marker(), 'D');
        assert_eq!(Command::Retrigger(2).marker(), 'R');
        assert_eq!(Command::Tempo(Bpm::from_whole(120).unwrap()).marker(), 'T');
    }
    #[test]
    fn atomic_save_refuses_overwrite() {
        let base = env::temp_dir().join(format!("shsong-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let s = Song::new(&config());
        save(&base, &s, false).unwrap();
        assert!(save(&base, &s, false).is_err());
        assert!(save(&base, &s, true).is_ok());
        assert!(!base.join(".untitled.tmp").exists());
        let _ = fs::remove_dir_all(base);
    }
    #[test]
    fn bank_and_program_precede_note_and_notes_end() {
        let c = config();
        let mut s = Song::new(&c);
        pages_mut(&mut s)[0] = Page::new("MELODY", c.melody_channel, false, 0);
        let cell = &mut s.patterns.get_mut(&0).unwrap().rows[0][0];
        cell.program = Some(7);
        cell.note = Note::On(60);
        cell.nudge = 24;
        let scheduled = schedule(&s, &c, 0, 0).unwrap();
        let m = scheduled
            .iter()
            .filter(|message| !message.bytes.is_empty())
            .collect::<Vec<_>>();
        assert_eq!(&m[0].bytes[..2], &[0xb0, 0]);
        assert_eq!(&m[1].bytes[..2], &[0xb0, 32]);
        assert_eq!(m[2].bytes[0] & 0xf0, 0xc0);
        assert_eq!(m[3].bytes[0] & 0xf0, 0x90);
        assert!(m[..4].iter().all(|message| message.at == m[3].at));
        assert!(m.iter().any(|x| x.bytes[0] & 0xf0 == 0x80));
    }

    #[test]
    fn exact_device_target_uses_its_own_program_selection_protocol() {
        let mut config = config();
        config.bank_select = BankSelectMode::Cc0Cc32;
        config.program_changes = true;
        let mut song = Song::new(&config);
        let page = &mut song.patterns.get_mut(&0).unwrap().pages[0];
        page.target = PageTarget::Midi("USB MIDI: Roland D-50".into());
        page.device_profile = Some("roland-d-50".into());
        for column in &mut page.columns {
            column.bank_msb = 5;
            column.bank_lsb = 9;
        }
        song.patterns.get_mut(&0).unwrap().rows[0][0] = Cell {
            note: Note::On(60),
            program: Some(7),
            ..Cell::default()
        };

        let transmitted = schedule(&song, &config, 0, 0)
            .unwrap()
            .into_iter()
            .filter(|message| !message.bytes.is_empty())
            .map(|message| message.bytes)
            .collect::<Vec<_>>();
        assert_eq!(transmitted[0], [0xc0, 7]);
        assert_eq!(transmitted[1], [0x90, 60, 96]);
        assert!(!transmitted.iter().any(|message| {
            message.len() >= 2 && message[0] & 0xf0 == 0xb0 && matches!(message[1], 0 | 32)
        }));
    }

    #[test]
    fn configured_profile_does_not_leak_into_an_explicit_raw_midi_route() {
        let mut config = config();
        config.profile = "roland-d-50".into();
        config.bank_select = BankSelectMode::Cc0Cc32;
        let mut song = Song::new(&config);
        let page = &mut song.patterns.get_mut(&0).unwrap().pages[0];
        page.target = PageTarget::Midi("Independent raw output".into());
        page.device_profile = None;
        page.columns = [ColumnSetup {
            channel: 2,
            bank_msb: 5,
            bank_lsb: 9,
            program: 7,
        }; LANES_PER_PAGE];
        song.patterns.get_mut(&0).unwrap().rows[0][0].note = Note::On(60);
        let messages = schedule(&song, &config, 0, 0).unwrap();
        assert!(messages.iter().any(|message| message.bytes == [0xb2, 0, 5]));
        assert!(messages
            .iter()
            .any(|message| message.bytes == [0xb2, 32, 9]));
        assert!(messages.iter().any(|message| message.bytes == [0xc2, 7]));
    }
    #[test]
    fn row_timing_pattern_transition_and_tempo() {
        let c = config();
        let mut s = Song::new(&c);
        s.patterns.insert(1, Pattern::empty(64, s.total_lanes()));
        s.order.push(1);
        s.patterns.get_mut(&0).unwrap().rows[1][0] = Cell {
            note: Note::On(61),
            command: Command::Tempo(Bpm::from_whole(60).unwrap()),
            ..Cell::default()
        };
        s.patterns.get_mut(&1).unwrap().rows[0][0].note = Note::On(62);
        let m = schedule(&s, &c, 0, 0).unwrap();
        let notes = m
            .iter()
            .filter(|x| x.bytes.first().is_some_and(|status| status & 0xf0 == 0x90))
            .collect::<Vec<_>>();
        assert_eq!(notes[0].at, Duration::from_millis(125));
        assert_eq!(notes[1].order, 1);
    }

    #[test]
    fn decimal_pattern_and_command_tempos_schedule_without_whole_bpm_rounding() {
        let c = config();
        let mut song = Song::new(&c);
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.resize_rows(2).unwrap();
        pattern.tempo = "100.50".parse().unwrap();
        pattern.rows[0][0] = Cell {
            note: Note::On(60),
            gate: Some(100),
            ..Cell::default()
        };
        pattern.rows[1][0].command = Command::Tempo("120.25".parse().unwrap());
        let scheduled = schedule(&song, &c, 0, 0).unwrap();
        let first_release = scheduled
            .iter()
            .find(|message| message.bytes == [0x80, 60, 0])
            .unwrap();
        let expected = Duration::from_secs_f64(60.0 / 100.50 / 4.0);
        assert_eq!(first_release.at, expected);
        assert_ne!(
            first_release.at,
            Duration::from_secs_f64(60.0 / 101.0 / 4.0)
        );
    }
    #[test]
    fn pattern_master_tempo_resets_at_arrangement_step() {
        let c = config();
        let mut song = Song::new(&c);
        let setup = song.patterns[&0].clone();
        song.patterns
            .insert(0, Pattern::empty_like_setup(2, &setup));
        song.patterns.get_mut(&0).unwrap().tempo = Bpm::from_whole(120).unwrap();
        song.patterns.get_mut(&0).unwrap().rows[0][0].command =
            Command::Tempo(Bpm::from_whole(60).unwrap());
        let mut second = Pattern::empty_like_setup(2, &song.patterns[&0]);
        second.tempo = Bpm::from_whole(240).unwrap();
        second.rows[1][0].note = Note::On(62);
        song.patterns.insert(1, second);
        song.order = vec![0, 1];
        let messages = schedule(&song, &c, 0, 0).unwrap();
        let second_note = messages
            .iter()
            .find(|message| message.order == 1 && message.bytes.first() == Some(&0x90))
            .unwrap();
        assert_eq!(second_note.at, Duration::from_micros(437_500));
    }
    #[test]
    fn arrangement_steps_use_referenced_pattern_page_setup() {
        let mut c = config();
        c.bank_select = BankSelectMode::Off;
        let mut song = Song::new(&c);
        pages_mut(&mut song)[0].target = PageTarget::Midi("A".into());
        pages_mut(&mut song)[0].column_mut(0).channel = 0;
        song.patterns.get_mut(&0).unwrap().rows[0][0].note = Note::On(60);
        let mut second = Pattern::empty_like_setup(1, &song.patterns[&0]);
        second.pages[0].target = PageTarget::Midi("B".into());
        second.pages[0].column_mut(0).channel = 5;
        second.rows[0][0].note = Note::On(61);
        song.patterns.insert(1, second);
        song.order = vec![0, 1];
        let notes = schedule(&song, &c, 0, 0)
            .unwrap()
            .into_iter()
            .filter(|message| {
                message
                    .bytes
                    .first()
                    .is_some_and(|status| status & 0xf0 == 0x90)
            })
            .collect::<Vec<_>>();
        assert!(notes.iter().any(
            |message| message.target == Some(PageTarget::Midi("A".into()))
                && message.bytes == [0x90, 60, 96]
        ));
        assert!(notes.iter().any(
            |message| message.target == Some(PageTarget::Midi("B".into()))
                && message.bytes == [0x95, 61, 96]
        ));
    }
    #[test]
    fn arrangement_boundary_does_not_add_an_extra_note_off() {
        let c = config();
        let mut song = Song::new(&c);
        song.patterns.get_mut(&0).unwrap().rows.truncate(1);
        song.patterns.get_mut(&0).unwrap().rows[0][0] = Cell {
            note: Note::On(60),
            gate: Some(1),
            ..Cell::default()
        };
        let mut second = Pattern::empty_like_setup(1, &song.patterns[&0]);
        second.rows[0][1].note = Note::On(64);
        song.patterns.insert(1, second);
        song.order = vec![0, 1];
        let messages = schedule(&song, &c, 0, 0).unwrap();
        let boundary = messages
            .iter()
            .find(|message| message.order == 1 && message.row == 0 && message.bytes.is_empty())
            .unwrap()
            .at;
        assert!(!messages
            .iter()
            .any(|message| message.at == boundary && message.bytes == [0x80, 60, 0]));
    }
    #[test]
    fn live_tempo_change_rescales_remaining_schedule_monotonically() {
        let c = config();
        let mut song = Song::new(&c);
        song.patterns
            .insert(0, Pattern::empty(4, song.total_lanes()));
        let mut messages = schedule(&song, &c, 0, 0).unwrap();
        rescale_schedule(
            &mut messages,
            1,
            Duration::from_millis(100),
            Bpm::from_whole(120).unwrap(),
            Bpm::from_whole(60).unwrap(),
        );
        let times = messages
            .iter()
            .skip(1)
            .map(|message| message.at)
            .collect::<Vec<_>>();
        assert!(times.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(times.contains(&Duration::from_millis(150)));
        assert_eq!(times.last(), Some(&Duration::from_millis(900)));
    }
    #[test]
    fn panic_covers_every_channel_with_sound_off() {
        let c = config();
        let p = panic_messages(c.channels.iter().copied());
        for ch in c.channels {
            assert!(p.contains(&vec![0xb0 | ch, 120, 0]));
            assert!(p.contains(&vec![0xb0 | ch, 123, 0]));
        }
    }
    #[test]
    fn installed_profile_has_four_lane_drum_page_on_channel_two() {
        let c = config();
        let mut song = Song::new(&c);
        assert_eq!(pages(&song)[1].column(0), &ColumnSetup::default());
        assert_eq!(pages(&song)[1].runtime_channel(0, &c), 1);
        assert!(pages(&song)[1].percussion);
        song.patterns.get_mut(&0).unwrap().rows[0][4].note = Note::On(36);
        assert!(schedule(&song, &c, 0, 0).unwrap().iter().any(|message| {
            message.bytes.first() == Some(&0x91) && message.bytes.get(1) == Some(&36)
        }));
    }
    #[test]
    fn mt240_profile_uses_channel_two_and_selects_percussion_first() {
        let mut c = config();
        c.channels = vec![0, 1];
        c.melody_channel = 0;
        c.percussion_channel = Some(1);
        c.percussion_program = Some(9);
        c.max_tracks = 2;
        c.bank_select = BankSelectMode::Off;
        let mut song = Song::new(&c);
        assert_eq!(pages(&song)[1].column(0), &ColumnSetup::default());
        assert_eq!(pages(&song)[1].runtime_channel(0, &c), 1);
        assert!(pages(&song)[1].percussion);
        song.patterns.get_mut(&0).unwrap().rows[0][4].note = Note::On(36);
        let midi = schedule(&song, &c, 0, 0)
            .unwrap()
            .into_iter()
            .filter(|message| !message.bytes.is_empty())
            .collect::<Vec<_>>();
        assert_eq!(midi[0].bytes, [0xc1, 9]);
        assert_eq!(midi[1].bytes, [0x91, 36, 96]);
    }
    #[test]
    fn disabled_track_never_schedules_notes() {
        let c = config();
        let mut s = Song::new(&c);
        pages_mut(&mut s)[0].lanes[0].enabled = false;
        s.patterns.get_mut(&0).unwrap().rows[0][0].note = Note::On(60);
        assert!(schedule(&s, &c, 0, 0)
            .unwrap()
            .iter()
            .all(|message| message.bytes.is_empty()));
    }
    #[test]
    fn empty_rows_advance_at_row_timing_and_hold_the_loop_boundary() {
        let c = config();
        let mut s = Song::new(&c);
        s.patterns.insert(0, Pattern::empty(4, s.total_lanes()));
        let m = schedule(&s, &c, 0, 0).unwrap();
        let ticks = m
            .iter()
            .filter(|message| message.bytes.is_empty())
            .map(|message| (message.at, message.row))
            .collect::<Vec<_>>();
        assert_eq!(
            ticks,
            vec![
                (Duration::ZERO, 0),
                (Duration::from_millis(125), 1),
                (Duration::from_millis(250), 2),
                (Duration::from_millis(375), 3),
                (Duration::from_millis(500), 3),
            ]
        );
        assert_eq!(m.last().unwrap().at, Duration::from_millis(500));
    }
    #[test]
    fn partial_playback_repeats_from_the_selected_pattern_start() {
        let c = config();
        let mut s = Song::new(&c);
        s.patterns.insert(0, Pattern::empty(4, s.total_lanes()));

        let (first, repeat) = playback_schedules(&s, &c, 0, 2).unwrap();
        let marker_rows = |messages: &[ScheduledMessage]| {
            messages
                .iter()
                .filter(|message| message.bytes.is_empty())
                .map(|message| message.row)
                .collect::<Vec<_>>()
        };

        assert_eq!(marker_rows(&first), vec![2, 3, 3]);
        assert_eq!(marker_rows(&repeat), vec![0, 1, 2, 3, 3]);
    }
    #[test]
    fn system_realtime_messages_do_not_have_a_mute_channel() {
        assert_eq!(message_channel(&[]), None);
        assert_eq!(message_channel(&[0xf8]), None);
        assert_eq!(message_channel(&[0x99, 36, 100]), Some(9));
    }
    #[test]
    fn both_four_lane_pages_schedule_together_on_shared_page_channels() {
        let mut c = config();
        c.bank_select = BankSelectMode::Off;
        let mut song = Song::new(&c);
        let row = &mut song.patterns.get_mut(&0).unwrap().rows[0];
        for (lane, note) in [60, 64, 67, 71].into_iter().enumerate() {
            row[lane] = Cell {
                note: Note::On(note),
                velocity: Some(80 + lane as u8),
                ..Cell::default()
            };
        }
        for (lane, note) in [36, 38, 40, 41].into_iter().enumerate() {
            row[LANES_PER_PAGE + lane] = Cell {
                note: Note::On(note),
                velocity: Some(100 + lane as u8),
                ..Cell::default()
            };
        }
        let messages = schedule(&song, &c, 0, 0).unwrap();
        let note_ons = messages
            .iter()
            .filter(|message| {
                message
                    .bytes
                    .first()
                    .is_some_and(|status| status & 0xf0 == 0x90)
            })
            .collect::<Vec<_>>();
        assert_eq!(note_ons.len(), 8);
        for lane in 0..LANES_PER_PAGE {
            assert!(note_ons.iter().any(|message| {
                message.lane == Some(lane)
                    && message.bytes[0] == 0x90 | pages(&song)[0].runtime_channel(lane, &c)
            }));
        }
        let percussion_channel = pages(&song)[1].runtime_channel(0, &c);
        let melodic_on_percussion_channel = (0..LANES_PER_PAGE)
            .filter(|lane| pages(&song)[0].runtime_channel(*lane, &c) == percussion_channel)
            .count();
        assert_eq!(
            note_ons
                .iter()
                .filter(|message| message.bytes[0] == 0x90 | percussion_channel)
                .count(),
            LANES_PER_PAGE + melodic_on_percussion_channel
        );
        assert!(note_ons.iter().all(|message| message.at == Duration::ZERO));
        assert_eq!(
            note_ons.iter().map(|m| m.bytes[2]).collect::<Vec<_>>(),
            [80, 81, 82, 83, 100, 101, 102, 103]
        );
        let program = messages.iter().position(|m| m.bytes == [0xc1, 9]).unwrap();
        let first_drum = messages
            .iter()
            .position(|message| {
                message.lane.is_some_and(|lane| lane >= LANES_PER_PAGE)
                    && message.bytes.first() == Some(&0x91)
            })
            .unwrap();
        assert!(program < first_drum);
    }

    #[test]
    fn shared_channel_lanes_keep_independent_note_off_identity() {
        let c = config();
        let mut song = Song::new(&c);
        pages_mut(&mut song)[0] = Page::new("MELODY", 0, false, 0);
        let row = &mut song.patterns.get_mut(&0).unwrap().rows[0];
        row[0].note = Note::On(60);
        row[1].note = Note::On(64);
        let messages = schedule(&song, &c, 0, 0).unwrap();
        assert!(messages
            .iter()
            .any(|m| m.lane == Some(0) && m.bytes == [0x80, 60, 0]));
        assert!(messages
            .iter()
            .any(|m| m.lane == Some(1) && m.bytes == [0x80, 64, 0]));
        assert!(!messages
            .iter()
            .any(|m| m.lane == Some(0) && m.bytes == [0x80, 64, 0]));
    }

    #[test]
    fn gesture_waits_sorts_preserves_velocity_and_accepts_staggered_notes() {
        let start = Instant::now();
        let mut gesture = GestureCapture::default();
        gesture.observe(start, &[0x90, 67, 91]);
        gesture.observe(start + Duration::from_millis(5), &[0x80, 67, 0]);
        assert_eq!(
            gesture.finish(start + Duration::from_millis(30), DEFAULT_GESTURE_SETTLE),
            None
        );
        gesture.observe(start + Duration::from_millis(35), &[0x90, 60, 73]);
        gesture.observe(start + Duration::from_millis(40), &[0x90, 64, 82]);
        gesture.observe(start + Duration::from_millis(45), &[0x90, 60, 0]);
        gesture.observe(start + Duration::from_millis(50), &[0x80, 64, 0]);
        let commit = gesture
            .finish(start + Duration::from_millis(100), DEFAULT_GESTURE_SETTLE)
            .unwrap();
        assert_eq!(commit.notes, [(60, 73), (64, 82), (67, 91)]);
        assert!(!commit.overflowed);
    }

    #[test]
    fn gesture_repeated_notes_and_fifth_note_are_deterministic() {
        let start = Instant::now();
        let mut gesture = GestureCapture::default();
        for (offset, note) in [60, 60, 62, 64, 65, 67].into_iter().enumerate() {
            gesture.observe(
                start + Duration::from_millis(offset as u64),
                &[0x90, note, 90 + offset as u8],
            );
        }
        for note in [60, 60, 62, 64, 65, 67] {
            gesture.observe(start + Duration::from_millis(10), &[0x90, note, 0]);
        }
        let commit = gesture
            .finish(start + Duration::from_millis(60), DEFAULT_GESTURE_SETTLE)
            .unwrap();
        assert_eq!(commit.notes.len(), 4);
        assert_eq!(commit.notes[0], (60, 90));
        assert!(commit.overflowed);
    }

    #[test]
    fn overwrite_refuses_newer_or_unknown_song_files() {
        let base = env::temp_dir().join(format!("shsong-newer-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let path = base.join("untitled.shsong");
        fs::write(&path, "SHSYNTH-SONG 99\nfuture=data\n").unwrap();
        assert!(save(&base, &Song::new(&config()), true).is_err());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "SHSYNTH-SONG 99\nfuture=data\n"
        );
        let unknown = encode(&Song::new(&config()))
            .unwrap()
            .replace("name=untitled\n", "name=untitled\nfuture=data\n");
        fs::write(&path, &unknown).unwrap();
        assert!(save(&base, &Song::new(&config()), true).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), unknown);
        let _ = fs::remove_dir_all(base);
    }
    #[test]
    fn song_delete_accepts_any_listed_song_version() {
        let base = env::temp_dir().join(format!("shsong-delete-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let song = Song::new(&config());
        let path = save(&base, &song, false).unwrap();
        delete(&base, &song.name).unwrap();
        assert!(!path.exists());
        fs::write(&path, "SHSYNTH-SONG 99\nfuture=data\n").unwrap();
        delete(&base, &song.name).unwrap();
        assert!(!path.exists());
        let _ = fs::remove_dir_all(base);
    }
    #[test]
    fn dry_run_is_non_transmitting_and_descriptive() {
        let mut c = config();
        c.enabled = false;
        let d = diagnostic(&c).unwrap();
        assert!(d.contains("NOT transmitted"));
        assert!(d.contains("profile:"));
    }
    #[test]
    fn disabled_or_missing_destination_is_an_offline_error_only() {
        let mut c = config();
        c.enabled = false;
        assert!(connect_target(&c, &PageTarget::ConfiguredExternal)
            .err()
            .expect("disabled output must stay offline")
            .to_string()
            .contains("offline"));
        let song = Song::new(&c);
        assert!(schedule(&song, &c, 0, 0).is_ok());
    }

    #[test]
    fn output_matching_uses_stable_identity_and_never_partial_matches() {
        let names = vec![
            "USB MIDI".to_owned(),
            "USB MIDI Through".to_owned(),
            "DIN Output".to_owned(),
        ];
        assert_eq!(matching_output_index(&names, "USB MIDI", true).unwrap(), 0);
        assert!(matching_output_index(&names, "DIN", true).is_err());
        assert!(matching_output_index(&names, "MIDI", true).is_err());
        assert!(matching_output_index(&names, "DIN", false)
            .unwrap_err()
            .to_string()
            .contains("offline"));

        let duplicates = vec!["Same Port".to_owned(), "Same Port".to_owned()];
        assert!(matching_output_index(&duplicates, "Same Port", false)
            .unwrap_err()
            .to_string()
            .contains("2 stable identity matches"));
    }

    #[test]
    fn pages_can_be_added_and_every_page_stays_four_lanes_wide() {
        let mut song = Song::new(&config());
        song.add_page(PageTarget::Midi("Port B".into()), 4).unwrap();
        song.add_page(PageTarget::ActiveInstrument, 7).unwrap();
        assert_eq!(pages(&song).len(), 4);
        assert!(pages(&song)
            .iter()
            .all(|page| page.lanes.len() == LANES_PER_PAGE));
        assert!(song.patterns[&0].rows.iter().all(|row| row.len() == 16));
    }

    #[test]
    fn bounded_project_mutations_leave_the_song_unchanged_on_error() {
        let config = config();
        let mut pages_song = Song::new(&config);
        while pages_song.patterns[&0].pages.len() < 64 {
            pages_song
                .add_page(PageTarget::ConfiguredExternal, 0)
                .unwrap();
        }
        let page_snapshot = pages_song.clone();
        assert!(pages_song
            .add_page(PageTarget::ConfiguredExternal, 0)
            .is_err());
        assert_eq!(pages_song, page_snapshot);

        let mut pattern_song = Song::new(&config);
        let pattern = pattern_song.patterns[&0].clone();
        for number in 1..MAX_PROJECT_PATTERNS as u16 {
            pattern_song.patterns.insert(number, pattern.clone());
        }
        let pattern_snapshot = pattern_song.clone();
        assert!(pattern_song.append_pattern(pattern).is_err());
        assert_eq!(pattern_song, pattern_snapshot);

        let mut arrangement_song = Song::new(&config);
        arrangement_song.order = vec![0; MAX_ARRANGEMENT_STEPS];
        let arrangement_snapshot = arrangement_song.clone();
        assert!(arrangement_song.insert_arrangement_step(0, 0).is_err());
        assert_eq!(arrangement_song, arrangement_snapshot);

        let mut replacement_song = Song::new(&config);
        let replacement_snapshot = replacement_song.clone();
        let invalid = Pattern::new(0, Bpm::DEFAULT, 4, default_pages(&config));
        assert!(replacement_song.replace_pattern(0, invalid).is_err());
        assert_eq!(replacement_song, replacement_snapshot);
    }

    #[test]
    fn pages_schedule_simultaneously_to_independent_devices_and_channels() {
        let c = config();
        let mut song = Song::new(&c);
        pages_mut(&mut song)[0].target = PageTarget::Midi("Hardware A".into());
        pages_mut(&mut song)[0].column_mut(0).channel = 2;
        pages_mut(&mut song)[1].target = PageTarget::Midi("Hardware B".into());
        pages_mut(&mut song)[1].column_mut(0).channel = 11;
        let row = &mut song.patterns.get_mut(&0).unwrap().rows[0];
        row[0].note = Note::On(60);
        row[4].note = Note::On(36);
        let notes = schedule(&song, &c, 0, 0)
            .unwrap()
            .into_iter()
            .filter(|message| message.bytes.first().is_some_and(|b| b & 0xf0 == 0x90))
            .collect::<Vec<_>>();
        assert!(notes.iter().any(|message| {
            message.target == Some(PageTarget::Midi("Hardware A".into()))
                && message.bytes[0] == 0x92
        }));
        assert!(notes.iter().any(|message| {
            message.target == Some(PageTarget::Midi("Hardware B".into()))
                && message.bytes[0] == 0x9b
        }));
    }

    #[test]
    fn per_cell_programs_precede_notes_and_stay_page_scoped() {
        let mut c = config();
        c.bank_select = BankSelectMode::Off;
        let mut song = Song::new(&c);
        pages_mut(&mut song)[0].target = PageTarget::Midi("A".into());
        pages_mut(&mut song)[0].column_mut(0).channel = 2;
        pages_mut(&mut song)[1].target = PageTarget::Midi("B".into());
        pages_mut(&mut song)[1].column_mut(0).channel = 7;
        song.patterns.get_mut(&0).unwrap().rows[0][0] = Cell {
            note: Note::On(60),
            program: Some(11),
            ..Cell::default()
        };
        song.patterns.get_mut(&0).unwrap().rows[0][4] = Cell {
            note: Note::On(36),
            program: Some(22),
            ..Cell::default()
        };
        let messages = schedule(&song, &c, 0, 0).unwrap();
        for (target, program, note_status) in [
            (PageTarget::Midi("A".into()), vec![0xc2, 11], 0x92),
            (PageTarget::Midi("B".into()), vec![0xc7, 22], 0x97),
        ] {
            let program_at = messages
                .iter()
                .position(|message| {
                    message.target == Some(target.clone()) && message.bytes == program
                })
                .unwrap();
            let note_at = messages
                .iter()
                .position(|message| {
                    message.target == Some(target.clone())
                        && message.bytes.first() == Some(&note_status)
                })
                .unwrap();
            assert!(program_at < note_at);
        }
    }

    #[test]
    fn active_instrument_and_shared_device_channels_remain_distinct() {
        let c = config();
        let mut song = Song::new(&c);
        pages_mut(&mut song)[0].target = PageTarget::ActiveInstrument;
        pages_mut(&mut song)[0].column_mut(0).channel = 5;
        pages_mut(&mut song)[1].target = PageTarget::Midi("One box".into());
        pages_mut(&mut song)[1].column_mut(0).channel = 9;
        song.add_page(PageTarget::Midi("One box".into()), 10)
            .unwrap();
        let row = &mut song.patterns.get_mut(&0).unwrap().rows[0];
        row[0].note = Note::On(60);
        row[4].note = Note::On(61);
        row[8].note = Note::On(62);
        let notes = schedule(&song, &c, 0, 0)
            .unwrap()
            .into_iter()
            .filter(|message| message.bytes.first().is_some_and(|b| b & 0xf0 == 0x90))
            .collect::<Vec<_>>();
        assert!(notes
            .iter()
            .any(|m| { m.target == Some(PageTarget::ActiveInstrument) && m.bytes[0] == 0x95 }));
        assert!(notes.iter().any(|m| m.bytes[0] == 0x99));
        assert!(notes.iter().any(|m| m.bytes[0] == 0x9a));
    }

    #[test]
    fn offline_exact_target_and_setup_round_trip_without_rebinding() {
        let mut song = Song::new(&config());
        pages_mut(&mut song)[0].target = PageTarget::Midi("Missing forever".into());
        pages_mut(&mut song)[0].device_profile = Some("roland-d-50".into());
        pages_mut(&mut song)[0].setup = vec![vec![0xb3, 0, 12], vec![0xc3, 7]];
        let decoded = decode(&encode(&song).unwrap()).unwrap();
        assert_eq!(decoded, song);
        assert!(schedule(&decoded, &config(), 0, 0)
            .unwrap()
            .iter()
            .any(|m| {
                m.target == Some(PageTarget::Midi("Missing forever".into()))
                    && m.bytes == [0xb3, 0, 12]
            }));
        assert_eq!(
            decoded.patterns[&0].pages[0].device_profile.as_deref(),
            Some("roland-d-50")
        );
    }

    #[test]
    fn tracker_persistence_strips_only_volatile_alsa_numeric_address() {
        let mut song = Song::new(&config());
        pages_mut(&mut song)[0].target =
            PageTarget::Midi("AudioBox USB 96:AudioBox MIDI 32:0".into());
        let encoded = encode(&song).unwrap();
        assert!(encoded.contains("midi:AudioBox USB 96:AudioBox MIDI"));
        assert!(!encoded.contains(" 32:0"));
        assert_eq!(
            pages(&decode(&encoded).unwrap())[0].target,
            PageTarget::Midi("AudioBox USB 96:AudioBox MIDI".into())
        );
    }

    #[test]
    fn cleanup_is_owned_by_lane_destination_and_channel() {
        let owners = BTreeMap::from([
            (
                (PageTarget::Midi("A".into()), 0, 60),
                BTreeSet::from([NoteOwner::Lane(0)]),
            ),
            (
                (PageTarget::Midi("A".into()), 1, 61),
                BTreeSet::from([NoteOwner::Lane(1)]),
            ),
            (
                (PageTarget::ActiveInstrument, 0, 62),
                BTreeSet::from([NoteOwner::Lane(2)]),
            ),
        ]);
        assert_eq!(
            planned_note_cleanup(&owners),
            vec![
                (PageTarget::ActiveInstrument, vec![0x80, 62, 0]),
                (PageTarget::Midi("A".into()), vec![0x80, 60, 0]),
                (PageTarget::Midi("A".into()), vec![0x81, 61, 0]),
            ]
        );
    }

    #[test]
    fn shared_note_is_released_only_after_its_last_lane_owner() {
        let target = PageTarget::Midi("shared".into());
        let key = (target.clone(), 3, 60);
        let mut owners = BTreeMap::from([(
            key.clone(),
            BTreeSet::from([NoteOwner::Lane(0), NoteOwner::Lane(4)]),
        )]);
        assert!(!release_note_owner(
            &mut owners,
            NoteOwner::Lane(0),
            &target,
            3,
            60
        ));
        assert_eq!(owners[&key], BTreeSet::from([NoteOwner::Lane(4)]));
        assert!(release_note_owner(
            &mut owners,
            NoteOwner::Lane(4),
            &target,
            3,
            60
        ));
        assert!(!owners.contains_key(&key));
    }

    #[test]
    fn two_shared_four_lane_pages_schedule_eight_independent_note_ons() {
        let route = SoftwareRoute {
            engine: BackendKind::FluidSynth,
            instrument: "sf0:band.sf2:0:32".into(),
        };
        let mut song = Song::new(&config());
        pages_mut(&mut song)[0].target = PageTarget::Software(route.clone());
        for column in &mut pages_mut(&mut song)[0].columns {
            column.channel = 0;
        }
        let second_page = song
            .add_page(PageTarget::Software(route.clone()), 0)
            .unwrap();
        let first_lane = 0;
        let second_lane = second_page * LANES_PER_PAGE;
        let row = &mut song.patterns.get_mut(&0).unwrap().rows[0];
        for lane in 0..LANES_PER_PAGE {
            row[first_lane + lane].note = Note::On(36 + lane as u8);
            row[second_lane + lane].note = Note::On(48 + lane as u8);
        }

        let note_ons = schedule(&song, &config(), 0, 0)
            .unwrap()
            .into_iter()
            .filter(|message| {
                message.target == Some(PageTarget::Software(route.clone()))
                    && matches!(message.bytes.as_slice(), [status, _, velocity]
                        if status & 0xf0 == 0x90 && *velocity > 0)
            })
            .collect::<Vec<_>>();
        assert_eq!(note_ons.len(), 8);
        assert_eq!(
            note_ons
                .iter()
                .filter_map(|message| message.lane)
                .collect::<BTreeSet<_>>()
                .len(),
            8
        );
    }

    #[test]
    fn shared_software_route_and_channel_round_trip_on_repeated_pages() {
        let route = SoftwareRoute {
            engine: BackendKind::FluidSynth,
            instrument: "sf1:orchestra.sf3:0:89".into(),
        };
        let mut song = Song::new(&config());
        pages_mut(&mut song)[0].target = PageTarget::Software(route.clone());
        pages_mut(&mut song)[0].column_mut(0).channel = 2;
        pages_mut(&mut song)[0].entry_mode = NoteEntryMode::OneColumn;
        pages_mut(&mut song)[0].entry_anchor = 1;
        let repeated = song
            .add_page(PageTarget::Software(route.clone()), 2)
            .unwrap();
        pages_mut(&mut song)[repeated].column_mut(1).channel = 2;
        pages_mut(&mut song)[repeated].entry_mode = NoteEntryMode::DrumAuto;

        let decoded = decode(&encode(&song).unwrap()).unwrap();
        assert_eq!(
            pages(&decoded)[0].target,
            PageTarget::Software(route.clone())
        );
        assert_eq!(
            pages(&decoded)[repeated].target,
            PageTarget::Software(route)
        );
        assert_eq!(pages(&decoded)[0].column(0).channel, 2);
        assert_eq!(pages(&decoded)[repeated].column(1).channel, 2);
        assert_eq!(pages(&decoded)[0].entry_mode, NoteEntryMode::OneColumn);
        assert_eq!(pages(&decoded)[0].entry_anchor, 1);
        assert_eq!(
            pages(&decoded)[repeated].entry_mode,
            NoteEntryMode::DrumAuto
        );
    }

    #[test]
    fn muting_one_shared_note_owner_keeps_the_other_page_sounding() {
        let target = PageTarget::Software(SoftwareRoute {
            engine: BackendKind::FluidSynth,
            instrument: "sf0:band.sf2:0:32".into(),
        });
        let mut owners = NoteOwners::new();
        assert!(claim_note_owner(
            &mut owners,
            NoteOwner::Lane(0),
            &target,
            0,
            60
        ));
        assert!(!claim_note_owner(
            &mut owners,
            NoteOwner::Lane(4),
            &target,
            0,
            60
        ));
        assert!(!release_note_owner(
            &mut owners,
            NoteOwner::Lane(0),
            &target,
            0,
            60
        ));
        assert_eq!(
            owners[&(target, 0, 60)],
            BTreeSet::from([NoteOwner::Lane(4)])
        );
    }

    #[test]
    fn audition_release_does_not_cut_a_scheduled_owner() {
        let target = PageTarget::Software(SoftwareRoute {
            engine: BackendKind::FluidSynth,
            instrument: "sf0:band.sf2:0:32".into(),
        });
        let mut owners = NoteOwners::new();
        assert!(claim_note_owner(
            &mut owners,
            NoteOwner::Lane(4),
            &target,
            0,
            60
        ));
        assert!(!claim_note_owner(
            &mut owners,
            NoteOwner::Live,
            &target,
            0,
            60
        ));
        assert!(!release_note_owner(
            &mut owners,
            NoteOwner::Live,
            &target,
            0,
            60
        ));
        assert_eq!(
            owners[&(target, 0, 60)],
            BTreeSet::from([NoteOwner::Lane(4)])
        );
    }

    #[test]
    fn cleanup_deduplicates_shared_notes_and_covers_every_used_channel() {
        let target = PageTarget::Software(SoftwareRoute {
            engine: BackendKind::FluidSynth,
            instrument: "sf0:band.sf2:0:32".into(),
        });
        let owners = NoteOwners::from([
            (
                (target.clone(), 0, 60),
                BTreeSet::from([NoteOwner::Lane(0), NoteOwner::Lane(4)]),
            ),
            (
                (target.clone(), 2, 67),
                BTreeSet::from([NoteOwner::Lane(8)]),
            ),
            ((target.clone(), 9, 36), BTreeSet::from([NoteOwner::Live])),
        ]);
        assert_eq!(
            planned_note_cleanup(&owners),
            [
                (target.clone(), vec![0x80, 60, 0]),
                (target.clone(), vec![0x82, 67, 0]),
                (target, vec![0x89, 36, 0]),
            ]
        );
        assert_eq!(panic_messages([0, 2, 9]).len(), 9);
    }

    #[test]
    fn project_list_ignores_temporary_unrelated_and_directory_entries() {
        let base = env::temp_dir().join(format!("shsong-list-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("fake.shsong")).unwrap();
        fs::write(base.join("alpha.shsong"), "project").unwrap();
        fs::write(base.join("BETA.SHSONG"), "project").unwrap();
        std::os::unix::fs::symlink(base.join("alpha.shsong"), base.join("alias.shsong")).unwrap();
        fs::write(base.join(".alpha.123.tmp"), "temporary").unwrap();
        fs::write(base.join("notes.txt"), "unrelated").unwrap();
        assert_eq!(list(&base), ["BETA", "alpha"]);
        assert!(load(&base, "../alpha").is_err());
        assert!(delete(&base, "../alpha").is_err());
        assert!(base.join("alpha.shsong").exists());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn in_memory_page_values_and_setup_are_validated_before_save() {
        let mut song = Song::new(&config());
        pages_mut(&mut song)[0].target = PageTarget::ConfiguredExternal;
        pages_mut(&mut song)[0].column_mut(0).channel = 16;
        assert!(encode(&song)
            .unwrap_err()
            .to_string()
            .contains("MIDI value"));

        pages_mut(&mut song)[0].column_mut(0).channel = 0;
        pages_mut(&mut song)[0].setup = vec![Vec::new()];
        assert!(encode(&song)
            .unwrap_err()
            .to_string()
            .contains("setup message"));
    }

    #[test]
    fn software_route_owns_bank_and_program_while_other_channel_setup_remains_valid() {
        let mut song = Song::new(&config());
        pages_mut(&mut song)[0].target = PageTarget::Software(SoftwareRoute {
            engine: BackendKind::FluidSynth,
            instrument: "sf0:band.sf2:0:32".into(),
        });
        pages_mut(&mut song)[0].setup = vec![vec![0xb0, 7, 100], vec![0xb0, 10, 64]];
        assert!(encode(&song).is_ok());

        for selection in [vec![0xc0, 4], vec![0xb0, 0, 1], vec![0xb0, 32, 2]] {
            pages_mut(&mut song)[0].setup = vec![selection];
            assert!(encode(&song)
                .unwrap_err()
                .to_string()
                .contains("route-owned bank/program"));
        }
    }

    #[test]
    fn schedule_rejects_out_of_range_start_without_zero_time_loop() {
        let song = Song::new(&config());
        assert!(schedule(&song, &config(), song.order.len(), 0).is_err());
        assert!(schedule(&song, &config(), 0, song.patterns[&0].rows.len()).is_err());
    }

    #[test]
    fn midi_clock_keeps_twenty_four_ppqn_for_non_divisor_row_grids() {
        let mut cfg = config();
        cfg.send_transport = true;
        cfg.steps_per_beat = 5;
        let mut song = Song::new(&cfg);
        song.patterns.get_mut(&0).unwrap().rows.truncate(5);
        let clocks = schedule(&song, &cfg, 0, 0)
            .unwrap()
            .into_iter()
            .filter(|message| message.bytes == [0xf8])
            .collect::<Vec<_>>();
        // Two enabled page targets share one configured destination, so clock
        // is de-duplicated and sent exactly 24 times per quarter note.
        assert_eq!(clocks.len(), 24);
        assert_eq!(clocks.first().unwrap().at, Duration::ZERO);
        assert!(clocks.last().unwrap().at < Duration::from_millis(500));
    }

    #[test]
    fn stopped_lane_cleanup_follows_a_later_pattern_target() {
        let second = PageTarget::Midi("second".into());
        let owners = BTreeMap::from([(
            (second.clone(), 5, 62),
            BTreeSet::from([NoteOwner::Lane(0)]),
        )]);
        assert_eq!(planned_note_cleanup(&owners), [(second, vec![0x85, 62, 0])]);
    }

    #[test]
    fn song_decoder_rejects_oversized_duplicate_and_non_binary_fields() {
        let encoded = encode(&Song::new(&config())).unwrap();
        assert!(decode(&encoded.replace("pattern=0|64|", "pattern=0|257|")).is_err());
        assert!(decode(&encoded.replace("steps=4\n", "steps=4\nsteps=4\n")).is_err());
        assert!(decode(&encoded.replace("|MELODY|1|", "|MELODY|yes|")).is_err());

        let duplicate_pattern = encoded.replace(
            "pattern=0|64|12000|4|sixteenth|50\n",
            "pattern=0|64|12000|4|sixteenth|50\npattern=0|64|12000|4|sixteenth|50\n",
        );
        assert!(decode(&duplicate_pattern).is_err());
    }

    #[test]
    fn in_memory_song_limits_apply_before_save_or_schedule() {
        let mut song = Song::new(&config());
        song.name = "bad\nname".into();
        assert!(encode(&song).is_err());

        song.name = "bounded".into();
        song.order = vec![0; MAX_ARRANGEMENT_STEPS + 1];
        assert!(schedule(&song, &config(), 0, 0).is_err());

        let mut invalid_config = config();
        invalid_config.channels.clear();
        assert!(diagnostic(&invalid_config).is_err());
    }

    #[test]
    fn version_zero_page_setup_migrates_to_four_identical_columns() {
        let legacy = "SHSYNTH-SONG 0\nname=legacy\nsteps=4\ngate=80\norder=0\npattern=0|1|120|4\npattern_page=0|0|MELODY|1|3|4|5|6|96|0|configured\npattern_lane=0|0|0|L1|1\npattern_lane=0|0|1|L2|1\npattern_lane=0|0|2|L3|1\npattern_lane=0|0|3|L4|1\n";
        let song = decode(legacy).unwrap();
        let page = &song.patterns[&0].pages[0];
        assert_eq!(
            page.columns,
            [ColumnSetup {
                channel: 2,
                bank_msb: 4,
                bank_lsb: 5,
                program: 6,
            }; LANES_PER_PAGE]
        );
        assert!(song.insert_rack.order.is_empty());
        assert!(encode(&song).unwrap().starts_with("SHSYNTH-SONG 17\n"));
    }

    #[test]
    fn version_one_project_migrates_to_an_empty_insert_rack() {
        let current = encode(&Song::new(&config())).unwrap();
        let legacy = without_v5_profile_fields(&current)
            .replacen("SHSYNTH-SONG 17", "SHSYNTH-SONG 1", 1)
            .replace("|default|default|default|default\n", "|1|0|0|0\n")
            .replace("|default\n", "|configured\n")
            .lines()
            .filter(|line| !line.starts_with("insert_rack=") && !line.starts_with("aux_routing="))
            .collect::<Vec<_>>()
            .join("\n");
        let migrated = decode(&legacy).unwrap();
        assert!(migrated.insert_rack.order.is_empty());
        assert!(encode(&migrated).unwrap().starts_with("SHSYNTH-SONG 17\n"));
    }

    #[test]
    fn version_two_project_migrates_to_empty_aux_routing() {
        let current = encode(&Song::new(&config())).unwrap();
        let old = without_v5_profile_fields(&current)
            .replacen("SHSYNTH-SONG 17", "SHSYNTH-SONG 2", 1)
            .replace("|default|default|default|default\n", "|1|0|0|0\n")
            .replace("|default\n", "|configured\n")
            .lines()
            .filter(|line| !line.starts_with("aux_routing="))
            .collect::<Vec<_>>()
            .join("\n");
        let migrated = decode(&old).unwrap();
        assert!(migrated.aux_routing.buses.is_empty());
        assert!(migrated.aux_routing.sends.is_empty());
    }

    #[test]
    fn portable_route_is_explicit_and_never_serializes_channel_zero() {
        let cfg = config();
        let song = Song::new(&cfg);
        let encoded = encode(&song).unwrap();
        assert!(encoded.starts_with("SHSYNTH-SONG 17\n"));
        assert!(encoded.contains("|default|-|manual|1|1\n"));
        assert!(encoded.contains("|default|default|default|default\n"));
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, song);
        assert_eq!(decoded.patterns[&0].pages[0].target, PageTarget::Default);
        assert_eq!(
            decoded.patterns[&0].pages[0].runtime_channel(0, &cfg),
            cfg.channels[0]
        );
    }

    #[test]
    fn version_three_routes_migrate_without_becoming_portable() {
        let current = encode(&Song::new(&config())).unwrap();
        let legacy = without_v5_profile_fields(&current)
            .replacen("SHSYNTH-SONG 17", "SHSYNTH-SONG 3", 1)
            .replace("|default|default|default|default\n", "|7|0|0|0\n")
            .replace("|default\n", "|configured\n");
        let migrated = decode(&legacy).unwrap();
        for page in &migrated.patterns[&0].pages {
            assert_eq!(page.target, PageTarget::ConfiguredExternal);
            assert!(page.columns.iter().all(|column| column.channel == 6));
        }

        let invalid_portable = legacy.replace("|configured\n", "|default\n");
        assert!(decode(&invalid_portable)
            .unwrap_err()
            .to_string()
            .contains("format 4"));
    }

    #[test]
    fn version_four_synth_route_loads_and_upgrades_without_incidental_engine_state() {
        let mut song = Song::new(&config());
        pages_mut(&mut song)[0].target = PageTarget::Synthv1("Legacy Lead".into());
        let legacy = without_v5_profile_fields(&encode(&song).unwrap()).replacen(
            "SHSYNTH-SONG 17",
            "SHSYNTH-SONG 4",
            1,
        );
        let mut loaded = decode(&legacy).unwrap();
        assert_eq!(
            pages(&loaded)[0].target,
            PageTarget::Synthv1("Legacy Lead".into())
        );
        assert_eq!(upgrade_legacy_synth_routes(&mut loaded, "Other Sound"), 1);
        assert_eq!(
            pages(&loaded)[0].target,
            PageTarget::Software(SoftwareRoute::synthv1("Legacy Lead"))
        );
    }

    #[test]
    fn offline_exact_midi_route_stays_offline_then_reconnects_without_rewriting() {
        let mut cfg = config();
        cfg.enabled = true;
        cfg.output_match = "Machine default".into();
        let preferred = PageTarget::Midi("Touring rack".into());
        let default_only = vec!["Machine default port".to_owned()];
        let offline = resolve_midi_route(&cfg, &preferred, &default_only, true);
        assert!(matches!(offline.choice, MidiRouteChoice::Unavailable(_)));
        assert_eq!(offline.notice, None);
        assert_eq!(preferred, PageTarget::Midi("Touring rack".into()));

        let restored_names = vec!["Machine default port".into(), "Touring rack".into()];
        let restored = resolve_midi_route(&cfg, &preferred, &restored_names, true);
        assert_eq!(restored.choice, MidiRouteChoice::Hardware(1));
        assert_eq!(restored.notice, None);

        let ambiguous_names = vec!["Touring rack 20:0".into(), "Touring rack 21:0".into()];
        let ambiguous = resolve_midi_route(&cfg, &preferred, &ambiguous_names, true);
        assert!(matches!(
            ambiguous.choice,
            MidiRouteChoice::Unavailable(ref error) if error.contains("ambiguous")
        ));
        assert_eq!(preferred, PageTarget::Midi("Touring rack".into()));
    }

    #[test]
    fn explicit_midi_never_falls_into_the_pattern_software_synth() {
        let mut cfg = config();
        cfg.enabled = true;
        cfg.output_match = "missing default".into();
        let exact = PageTarget::Midi("missing preferred".into());
        let internal = resolve_midi_route(&cfg, &exact, &[], true);
        assert!(matches!(internal.choice, MidiRouteChoice::Unavailable(_)));
        assert_eq!(internal.notice, None);

        let none = resolve_midi_route(&cfg, &exact, &[], false);
        assert!(matches!(none.choice, MidiRouteChoice::Unavailable(_)));

        let configured_auto = resolve_midi_route(&cfg, &PageTarget::Default, &[], true);
        assert!(matches!(
            configured_auto.choice,
            MidiRouteChoice::Unavailable(_)
        ));

        cfg.enabled = false;
        let portable = resolve_midi_route(&cfg, &PageTarget::Default, &[], true);
        assert_eq!(portable.choice, MidiRouteChoice::Instrument);
        assert_eq!(portable.notice, None);
    }

    #[test]
    fn four_columns_schedule_distinct_channels_and_master_programs() {
        let mut cfg = config();
        cfg.program_changes = true;
        let mut song = Song::new(&cfg);
        song.patterns.get_mut(&0).unwrap().pages[1].enabled = false;
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.pages[0].target = PageTarget::ConfiguredExternal;
        pattern.rows.truncate(1);
        for column in 0..LANES_PER_PAGE {
            pattern.pages[0].columns[column] = ColumnSetup {
                channel: column as u8,
                program: 10 + column as u8,
                ..ColumnSetup::default()
            };
            pattern.rows[0][column].note = Note::On(60 + column as u8);
        }
        let messages = schedule(&song, &cfg, 0, 0).unwrap();
        for column in 0..LANES_PER_PAGE {
            assert!(messages
                .iter()
                .any(|message| message.bytes == [0xc0 | column as u8, 10 + column as u8]));
            assert!(messages
                .iter()
                .any(|message| message.bytes == [0x90 | column as u8, 60 + column as u8, 96]));
        }
    }

    #[test]
    fn raw_midi_accepts_every_channel_and_program_without_a_profile() {
        let cfg = config();
        for channel in 0..=15 {
            for program in [0, 127] {
                let mut song = Song::new(&cfg);
                let pattern = song.patterns.get_mut(&0).unwrap();
                pattern.pages[1].enabled = false;
                pattern.pages[0].target = PageTarget::Midi("Raw DIN output".into());
                pattern.pages[0].device_profile = None;
                for column in &mut pattern.pages[0].columns {
                    column.channel = channel;
                    column.program = program;
                }
                pattern.rows.truncate(1);
                pattern.rows[0][0].note = Note::On(60);
                let messages = schedule(&song, &cfg, 0, 0).unwrap();
                assert!(messages
                    .iter()
                    .any(|message| message.bytes == [0xc0 | channel, program]));
                assert!(messages
                    .iter()
                    .any(|message| message.bytes == [0x90 | channel, 60, 96]));
            }
        }
    }

    #[test]
    fn software_route_round_trips_engine_and_instrument_as_stable_identities() {
        for backend in BackendKind::ALL {
            let mut song = Song::new(&config());
            let route = SoftwareRoute {
                engine: backend,
                instrument: "Pads/Glass Horizon".into(),
            };
            pages_mut(&mut song)[0].target = PageTarget::Software(route.clone());
            let encoded = encode(&song).unwrap();
            assert!(encoded.contains(&format!(
                "software:{}:Pads/Glass Horizon",
                backend.label().to_ascii_lowercase()
            )));
            let decoded = decode(&encoded).unwrap();
            assert_eq!(pages(&decoded)[0].target, PageTarget::Software(route));
        }
    }

    #[test]
    fn shared_channel_requires_compatible_master_instruments() {
        let mut song = Song::new(&config());
        song.patterns.get_mut(&0).unwrap().pages[1].enabled = false;
        let page = &mut song.patterns.get_mut(&0).unwrap().pages[0];
        page.target = PageTarget::ConfiguredExternal;
        page.columns = [ColumnSetup {
            channel: 3,
            program: 9,
            ..ColumnSetup::default()
        }; LANES_PER_PAGE];
        assert!(song.validate().is_ok());
        song.patterns.get_mut(&0).unwrap().pages[0]
            .column_mut(2)
            .program = 10;
        assert!(song
            .validate()
            .unwrap_err()
            .to_string()
            .contains("conflicting"));
    }

    #[test]
    fn unused_pattern_deletion_never_rewrites_arrangement() {
        let mut song = Song::new(&config());
        let referenced_snapshot = song.clone();
        assert!(song
            .delete_unused_pattern(0)
            .unwrap_err()
            .to_string()
            .contains("1 arrangement"));
        assert_eq!(song, referenced_snapshot);
        let setup = song.patterns[&0].clone();
        let orphan = song.append_pattern(setup).unwrap();
        song.order.pop();
        let order = song.order.clone();
        song.delete_unused_pattern(orphan).unwrap();
        assert_eq!(song.order, order);
        assert!(!song.patterns.contains_key(&orphan));
    }

    #[test]
    fn transpose_is_atomic_and_never_changes_percussion_pages() {
        let mut song = Song::new(&config());
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.rows[0][0].note = Note::On(60);
        pattern.rows[1][1].note = Note::On(127);
        pattern.rows[0][LANES_PER_PAGE].note = Note::On(36);
        let before = pattern.clone();
        assert!(pattern.transpose_melodic(1).is_err());
        assert_eq!(pattern, &before);

        pattern.rows[1][1].note = Note::On(72);
        assert_eq!(pattern.transpose_melodic(12).unwrap(), 2);
        assert_eq!(pattern.rows[0][0].note, Note::On(72));
        assert_eq!(pattern.rows[1][1].note, Note::On(84));
        assert_eq!(pattern.rows[0][LANES_PER_PAGE].note, Note::On(36));
    }

    #[test]
    fn project_rename_preserves_source_on_invalid_name_or_collision() {
        let base = std::env::temp_dir().join(format!("shr-rename-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let mut first = Song::new(&config());
        first.name = "first".into();
        save(&base, &first, false).unwrap();
        let mut taken = Song::new(&config());
        taken.name = "taken".into();
        save(&base, &taken, false).unwrap();
        assert!(rename_project(&base, "first", "taken").is_err());
        assert!(base.join("first.shsong").exists());
        assert!(rename_project(&base, "first", "bad\nname").is_err());
        assert!(base.join("first.shsong").exists());
        let (renamed, path) = rename_project(&base, "first", "My Bass Project").unwrap();
        assert_eq!(renamed.name, "My Bass Project");
        assert_eq!(path.file_name().unwrap(), "My-Bass-Project.shsong");
        assert!(!base.join("first.shsong").exists());
        let _ = fs::remove_dir_all(base);
    }
}
