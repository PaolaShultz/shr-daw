//! Offline deterministic Pattern generators.
//!
//! Drafting always clones the source Pattern. Playback never calls this module;
//! Apply stores the resulting ordinary Cells through the existing owners.

use crate::scale::Scale;
use crate::sequencer::{Cell, Command, Note, Pattern, StepCondition, LANES_PER_PAGE};
use anyhow::{bail, Context, Result};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Tool {
    #[default]
    Euclidean,
    Accumulator,
    Mutation,
    Fill,
    Arpeggio,
    Chord,
    Harmonizer,
}

impl Tool {
    pub const ALL: [Self; 7] = [
        Self::Euclidean,
        Self::Accumulator,
        Self::Mutation,
        Self::Fill,
        Self::Arpeggio,
        Self::Chord,
        Self::Harmonizer,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Euclidean => "EUCLIDEAN",
            Self::Accumulator => "ACCUMULATOR",
            Self::Mutation => "MUTATION",
            Self::Fill => "FILL",
            Self::Arpeggio => "ARPEGGIO",
            Self::Chord => "CHORD",
            Self::Harmonizer => "HARMONIZER",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ArpeggioOrder {
    #[default]
    Up,
    Down,
    UpDown,
    AsLane,
}

impl ArpeggioOrder {
    pub const ALL: [Self; 4] = [Self::Up, Self::Down, Self::UpDown, Self::AsLane];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Up => "UP",
            Self::Down => "DOWN",
            Self::UpDown => "UP/DOWN",
            Self::AsLane => "AS LANE",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RowRate {
    #[default]
    One,
    Two,
    Four,
    Eight,
}

impl RowRate {
    pub const ALL: [Self; 4] = [Self::One, Self::Two, Self::Four, Self::Eight];

    pub const fn rows(self) -> usize {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Four => 4,
            Self::Eight => 8,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::One => "1 ROW",
            Self::Two => "2 ROWS",
            Self::Four => "4 ROWS",
            Self::Eight => "8 ROWS",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ChordVoicing {
    #[default]
    Close,
    Open,
}

impl ChordVoicing {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Close => "CLOSE",
            Self::Open => "OPEN",
        }
    }
}

pub fn chord_quality_label(scale: Scale, degree: u8) -> &'static str {
    let index = degree.saturating_sub(1).min(6) as usize;
    match scale.kind {
        crate::scale::ScaleKind::Major => ["MAJ", "MIN", "MIN", "MAJ", "MAJ", "MIN", "DIM"][index],
        crate::scale::ScaleKind::NaturalMinor => {
            ["MIN", "DIM", "MAJ", "MIN", "MIN", "MAJ", "MAJ"][index]
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HarmonyInterval {
    #[default]
    Third,
    Fifth,
}

impl HarmonyInterval {
    pub const fn steps(self) -> usize {
        match self {
            Self::Third => 2,
            Self::Fifth => 4,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Third => "THIRD",
            Self::Fifth => "FIFTH",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HarmonyVoice {
    #[default]
    Above,
    Below,
}

impl HarmonyVoice {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Above => "ABOVE",
            Self::Below => "BELOW",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OutOfScalePolicy {
    #[default]
    Refuse,
    Skip,
}

impl OutOfScalePolicy {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Refuse => "REFUSE",
            Self::Skip => "SKIP",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CollisionPolicy {
    #[default]
    EmptyOnly,
    ReplaceNotes,
}

impl CollisionPolicy {
    pub const fn label(self) -> &'static str {
        match self {
            Self::EmptyOnly => "EMPTY ONLY",
            Self::ReplaceNotes => "REPLACE NOTE",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Recipe {
    pub tool: Tool,
    pub page: usize,
    pub lane: usize,
    pub start_row: usize,
    pub length: u16,
    /// Euclidean/Fill hits, Accumulator wrap span, Mutation pitch range.
    pub amount: u16,
    /// Accumulator signed increment. Other tools keep this zero.
    pub offset: i8,
    /// Euclidean/Fill rotation or Accumulator initial phase.
    pub phase: u16,
    pub density: u8,
    pub seed: u64,
    pub collision: CollisionPolicy,
    pub scale: Scale,
    pub arpeggio_order: ArpeggioOrder,
    pub arpeggio_octaves: u8,
    pub row_rate: RowRate,
    pub gate: u8,
    pub chord_degree: u8,
    pub chord_inversion: u8,
    pub chord_voicing: ChordVoicing,
    pub harmony_interval: HarmonyInterval,
    pub harmony_voice: HarmonyVoice,
    pub harmony_target_lane: u8,
    pub out_of_scale: OutOfScalePolicy,
}

impl Recipe {
    pub fn bounded_for(
        pattern: &Pattern,
        tool: Tool,
        page: usize,
        lane: usize,
        start_row: usize,
        retained_seed: u64,
        scale: Scale,
    ) -> Result<Self> {
        let remaining = pattern
            .rows
            .len()
            .checked_sub(start_row)
            .filter(|remaining| *remaining > 0)
            .context("generator cursor is outside the Pattern")?;
        if page >= pattern.pages.len() || lane >= LANES_PER_PAGE {
            bail!("generator lane is outside the Pattern");
        }
        let length = match tool {
            Tool::Arpeggio | Tool::Chord => 1,
            _ => remaining.min(16) as u16,
        };
        Ok(Self {
            tool,
            page,
            lane,
            start_row,
            length,
            amount: match tool {
                Tool::Euclidean | Tool::Fill => (length / 4).max(1),
                Tool::Accumulator => 12,
                Tool::Mutation => 2,
                Tool::Arpeggio | Tool::Chord | Tool::Harmonizer => 1,
            },
            offset: match tool {
                Tool::Accumulator => 2,
                _ => 0,
            },
            phase: 0,
            density: 50,
            seed: retained_seed,
            collision: CollisionPolicy::EmptyOnly,
            scale,
            arpeggio_order: ArpeggioOrder::Up,
            arpeggio_octaves: 1,
            row_rate: RowRate::One,
            gate: 75,
            chord_degree: 1,
            chord_inversion: 0,
            chord_voicing: ChordVoicing::Close,
            harmony_interval: HarmonyInterval::Third,
            harmony_voice: HarmonyVoice::Above,
            harmony_target_lane: if lane + 1 < LANES_PER_PAGE {
                (lane + 1) as u8
            } else {
                lane.saturating_sub(1) as u8
            },
            out_of_scale: OutOfScalePolicy::Refuse,
        })
    }

    pub fn normalize(&mut self, pattern: &Pattern) -> Result<()> {
        if self.page >= pattern.pages.len() || self.lane >= LANES_PER_PAGE {
            bail!("generator lane is outside the Pattern");
        }
        let remaining = pattern
            .rows
            .len()
            .checked_sub(self.start_row)
            .filter(|remaining| *remaining > 0)
            .context("generator row span is outside the Pattern")?;
        self.length = match self.tool {
            Tool::Arpeggio | Tool::Chord => self.length.clamp(1, 8),
            _ => self.length.clamp(1, remaining.min(256) as u16),
        };
        match self.tool {
            Tool::Euclidean | Tool::Fill => {
                self.amount = self.amount.min(self.length);
                self.offset = 0;
                self.phase %= self.length;
            }
            Tool::Accumulator => {
                self.amount = self.amount.clamp(1, 48);
                self.offset = self.offset.clamp(-12, 12);
                self.phase %= self.length;
            }
            Tool::Mutation => {
                self.amount = self.amount.clamp(1, 12);
                self.offset = 0;
                self.phase = 0;
            }
            Tool::Arpeggio | Tool::Chord | Tool::Harmonizer => {
                self.amount = 1;
                self.offset = 0;
                self.phase = 0;
            }
        }
        self.density = self.density.min(100);
        self.scale.root %= 12;
        self.arpeggio_octaves = self.arpeggio_octaves.clamp(1, 3);
        self.gate = match self.gate {
            0..=25 => 25,
            26..=50 => 50,
            51..=75 => 75,
            _ => 100,
        };
        self.chord_degree = self.chord_degree.clamp(1, 7);
        self.chord_inversion = self.chord_inversion.min(2);
        self.harmony_target_lane = self
            .harmony_target_lane
            .min((LANES_PER_PAGE.saturating_sub(1)) as u8);
        Ok(())
    }

    pub fn end_row(self) -> usize {
        self.start_row + usize::from(self.length)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Report {
    pub source_cells: usize,
    pub candidates: usize,
    pub affected: usize,
    pub replacements: usize,
    pub collisions: usize,
    pub protected: usize,
    pub out_of_scale: usize,
    pub range_refusals: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Draft {
    pub recipe: Recipe,
    pub pattern: Pattern,
    pub report: Report,
    pub affected_rows: Vec<usize>,
}

pub fn build(pattern: &Pattern, mut recipe: Recipe) -> Result<Draft> {
    recipe.normalize(pattern)?;
    let global_lane = recipe.page * LANES_PER_PAGE + recipe.lane;
    let mut draft = pattern.clone();
    let mut report = Report::default();
    let mut affected_rows = Vec::new();
    match recipe.tool {
        Tool::Euclidean => {
            let template = trigger_template(pattern, recipe, global_lane)?;
            for step in 0..usize::from(recipe.length) {
                if euclidean_hit(
                    step,
                    usize::from(recipe.length),
                    usize::from(recipe.amount),
                    rotation(recipe),
                ) {
                    propose(
                        &mut draft,
                        recipe,
                        recipe.start_row + step,
                        global_lane,
                        template,
                        &mut report,
                        &mut affected_rows,
                    );
                }
            }
        }
        Tool::Accumulator => {
            let template = trigger_template(pattern, recipe, global_lane)?;
            let Note::On(source_note) = template.note else {
                unreachable!("trigger template always contains a note")
            };
            for step in 0..usize::from(recipe.length) {
                let mut candidate = template;
                candidate.note = Note::On(accumulator_note(source_note, recipe, step));
                propose(
                    &mut draft,
                    recipe,
                    recipe.start_row + step,
                    global_lane,
                    candidate,
                    &mut report,
                    &mut affected_rows,
                );
            }
        }
        Tool::Mutation => mutate(
            pattern,
            &mut draft,
            recipe,
            global_lane,
            &mut report,
            &mut affected_rows,
        ),
        Tool::Fill => {
            if !pattern.pages[recipe.page].percussion {
                bail!("controlled Fill requires a percussion page");
            }
            let template = trigger_template(pattern, recipe, global_lane)?;
            let hits = usize::from(recipe.amount).min(usize::from(recipe.length));
            let mut selected = (0..usize::from(recipe.length))
                .map(|step| {
                    (
                        mixed(recipe.seed, recipe.start_row + step, global_lane, 0),
                        step,
                    )
                })
                .collect::<Vec<_>>();
            selected.sort_unstable();
            let mut selected = selected
                .into_iter()
                .take(hits)
                .map(|(_, step)| (step + rotation(recipe)) % usize::from(recipe.length))
                .collect::<Vec<_>>();
            selected.sort_unstable();
            let base_velocity = template
                .velocity
                .unwrap_or(pattern.pages[recipe.page].velocity);
            for (rank, step) in selected.into_iter().enumerate() {
                let mut candidate = template;
                candidate.velocity = Some(fill_velocity(base_velocity, rank, hits));
                candidate.condition = StepCondition::Fill;
                propose(
                    &mut draft,
                    recipe,
                    recipe.start_row + step,
                    global_lane,
                    candidate,
                    &mut report,
                    &mut affected_rows,
                );
            }
        }
        Tool::Arpeggio => {
            if pattern.pages[recipe.page].percussion {
                bail!("arpeggio requires a melodic page");
            }
            let family = arpeggio_family(pattern, recipe)?;
            report.source_cells = family.source_cells;
            let family = family.cells;
            let total = family
                .len()
                .checked_mul(usize::from(recipe.length))
                .context("arpeggio repetition count is too large")?;
            let final_offset = total
                .checked_mul(recipe.row_rate.rows())
                .context("arpeggio row placement is too large")?;
            if recipe.start_row.saturating_add(final_offset) >= pattern.rows.len() {
                bail!(
                    "arpeggio Pattern-end refusal: {total} step(s) at {} from row {}",
                    recipe.row_rate.label(),
                    recipe.start_row
                );
            }
            for index in 0..total {
                let row = recipe.start_row + (index + 1) * recipe.row_rate.rows();
                propose(
                    &mut draft,
                    recipe,
                    row,
                    global_lane,
                    family[index % family.len()],
                    &mut report,
                    &mut affected_rows,
                );
            }
        }
        Tool::Chord => {
            if pattern.pages[recipe.page].percussion {
                bail!("chord generator requires a melodic page");
            }
            if recipe.lane + 2 >= LANES_PER_PAGE {
                bail!("chord needs the selected lane plus two following lanes");
            }
            let notes = chord_notes(pattern, recipe, global_lane)?;
            let repetitions = usize::from(recipe.length);
            let final_offset = repetitions
                .saturating_sub(1)
                .checked_mul(recipe.row_rate.rows())
                .context("chord row placement is too large")?;
            if recipe.start_row.saturating_add(final_offset) >= pattern.rows.len() {
                bail!(
                    "chord Pattern-end refusal: {repetitions} chord(s) at {} from row {}",
                    recipe.row_rate.label(),
                    recipe.start_row
                );
            }
            for repetition in 0..repetitions {
                let row = recipe.start_row + repetition * recipe.row_rate.rows();
                for (voice, note) in notes.into_iter().enumerate() {
                    let candidate = Cell {
                        note: Note::On(note),
                        velocity: Some(pattern.pages[recipe.page].velocity),
                        ..Cell::default()
                    };
                    propose(
                        &mut draft,
                        recipe,
                        row,
                        global_lane + voice,
                        candidate,
                        &mut report,
                        &mut affected_rows,
                    );
                }
            }
        }
        Tool::Harmonizer => harmonize(
            pattern,
            &mut draft,
            recipe,
            global_lane,
            &mut report,
            &mut affected_rows,
        )?,
    }
    Ok(Draft {
        recipe,
        pattern: draft,
        report,
        affected_rows,
    })
}

struct ArpeggioFamily {
    source_cells: usize,
    cells: Vec<Cell>,
}

fn arpeggio_family(pattern: &Pattern, recipe: Recipe) -> Result<ArpeggioFamily> {
    let page_start = recipe.page * LANES_PER_PAGE;
    let source_row = pattern
        .rows
        .get(recipe.start_row)
        .context("arpeggio source row is outside the Pattern")?;
    let mut sources = Vec::new();
    for lane in 0..LANES_PER_PAGE {
        let source = source_row[page_start + lane];
        let Note::On(note) = source.note else {
            continue;
        };
        if sources
            .iter()
            .any(|(existing, _): &(u8, Cell)| *existing == note)
        {
            continue;
        }
        sources.push((note, source));
    }
    if sources.is_empty() {
        bail!("arpeggio source row needs an existing note or chord");
    }
    let source_cells = sources.len();
    let mut expanded = Vec::new();
    let mut range_refusals = 0usize;
    for octave in 0..recipe.arpeggio_octaves {
        for (note, source) in &sources {
            let shifted = u16::from(*note) + u16::from(octave) * 12;
            if shifted > 127 {
                range_refusals += 1;
                continue;
            }
            let mut candidate = *source;
            candidate.note = Note::On(shifted as u8);
            candidate.velocity = Some(
                source
                    .velocity
                    .unwrap_or(pattern.pages[recipe.page].velocity),
            );
            candidate.gate = Some(recipe.gate);
            candidate.command = Command::None;
            candidate.nudge = 0;
            candidate.probability = 100;
            candidate.condition = StepCondition::Always;
            expanded.push(candidate);
        }
    }
    if range_refusals > 0 {
        bail!("arpeggio MIDI range refusal: {range_refusals} note(s)");
    }
    let cells = match recipe.arpeggio_order {
        ArpeggioOrder::AsLane => expanded,
        ArpeggioOrder::Up => {
            expanded.sort_by_key(|cell| match cell.note {
                Note::On(note) => note,
                _ => 0,
            });
            expanded
        }
        ArpeggioOrder::Down => {
            expanded.sort_by_key(|cell| {
                std::cmp::Reverse(match cell.note {
                    Note::On(note) => note,
                    _ => 0,
                })
            });
            expanded
        }
        ArpeggioOrder::UpDown => {
            expanded.sort_by_key(|cell| match cell.note {
                Note::On(note) => note,
                _ => 0,
            });
            let mut reflected = expanded.clone();
            if expanded.len() > 2 {
                reflected.extend(expanded[1..expanded.len() - 1].iter().rev().copied());
            }
            reflected
        }
    };
    Ok(ArpeggioFamily {
        source_cells,
        cells,
    })
}

fn chord_notes(pattern: &Pattern, recipe: Recipe, global_lane: usize) -> Result<[u8; 3]> {
    let register = match pattern.rows[recipe.start_row][global_lane].note {
        Note::On(note) => u16::from(note / 12) * 12,
        Note::Empty | Note::Off => 60,
    };
    let intervals = recipe.scale.kind.intervals();
    let degree = usize::from(recipe.chord_degree - 1);
    let mut absolute = [0u16; 3];
    for (voice, degree_offset) in [0usize, 2, 4].into_iter().enumerate() {
        let scale_degree = degree + degree_offset;
        let octave = scale_degree / intervals.len();
        absolute[voice] = register
            + u16::from(recipe.scale.root)
            + u16::from(intervals[scale_degree % intervals.len()])
            + u16::try_from(octave).unwrap_or(u16::MAX).saturating_mul(12);
    }
    for voice in 0..usize::from(recipe.chord_inversion) {
        absolute[voice] = absolute[voice].saturating_add(12);
    }
    absolute.sort_unstable();
    if recipe.chord_voicing == ChordVoicing::Open {
        absolute[1] = absolute[1].saturating_add(12);
        absolute.sort_unstable();
    }
    let range_refusals = absolute.iter().filter(|note| **note > 127).count();
    if range_refusals > 0 {
        bail!("chord MIDI range refusal: {range_refusals} voice(s)");
    }
    Ok(absolute.map(|note| note as u8))
}

fn harmonize(
    source: &Pattern,
    draft: &mut Pattern,
    recipe: Recipe,
    global_lane: usize,
    report: &mut Report,
    affected_rows: &mut Vec<usize>,
) -> Result<()> {
    if source.pages[recipe.page].percussion {
        bail!("harmonizer requires a melodic page");
    }
    let target_lane = recipe.page * LANES_PER_PAGE + usize::from(recipe.harmony_target_lane);
    if target_lane == global_lane {
        bail!("harmonizer target lane must differ from the source lane");
    }
    let mut proposals = Vec::new();
    let mut out_of_scale = 0usize;
    let mut range_refusals = 0usize;
    for row in recipe.start_row..recipe.end_row() {
        let original = source.rows[row][global_lane];
        match original.note {
            Note::On(note) if !recipe.scale.contains(note) => out_of_scale += 1,
            Note::On(note) => match diatonic_shift(
                note,
                recipe.scale,
                recipe.harmony_interval.steps(),
                recipe.harmony_voice,
            ) {
                Some(changed) => {
                    let mut candidate = original;
                    candidate.note = Note::On(changed);
                    proposals.push((row, candidate));
                    report.source_cells += 1;
                }
                None => range_refusals += 1,
            },
            Note::Off => {
                proposals.push((row, original));
                report.source_cells += 1;
            }
            Note::Empty => {}
        }
    }
    if out_of_scale > 0 && recipe.out_of_scale == OutOfScalePolicy::Refuse {
        bail!("harmonizer out-of-scale refusal: {out_of_scale} note(s)");
    }
    if range_refusals > 0 {
        bail!("harmonizer MIDI range refusal: {range_refusals} note(s)");
    }
    report.out_of_scale = out_of_scale;
    report.range_refusals = range_refusals;
    for (row, candidate) in proposals {
        propose(
            draft,
            recipe,
            row,
            target_lane,
            candidate,
            report,
            affected_rows,
        );
    }
    Ok(())
}

fn diatonic_shift(note: u8, scale: Scale, steps: usize, voice: HarmonyVoice) -> Option<u8> {
    let direction = if voice == HarmonyVoice::Above { 1 } else { -1 };
    let mut shifted = i16::from(note);
    for _ in 0..steps {
        loop {
            shifted += direction;
            if !(0..=127).contains(&shifted) {
                return None;
            }
            if scale.contains(shifted as u8) {
                break;
            }
        }
    }
    Some(shifted as u8)
}

fn trigger_template(pattern: &Pattern, recipe: Recipe, global_lane: usize) -> Result<Cell> {
    let source = *pattern
        .rows
        .get(recipe.start_row)
        .and_then(|row| row.get(global_lane))
        .context("generator source cell is outside the Pattern")?;
    let Note::On(note) = source.note else {
        bail!("generator source cell needs a note trigger");
    };
    Ok(Cell {
        note: Note::On(note),
        velocity: Some(
            source
                .velocity
                .unwrap_or(pattern.pages[recipe.page].velocity),
        ),
        program: source.program,
        gate: source.gate,
        command: Command::None,
        nudge: 0,
        probability: 100,
        condition: StepCondition::Always,
    })
}

fn euclidean_hit(step: usize, length: usize, pulses: usize, rotation: usize) -> bool {
    if pulses == 0 {
        return false;
    }
    if pulses >= length {
        return true;
    }
    let source_step = (step + length - rotation % length) % length;
    (source_step * pulses) % length < pulses
}

fn accumulator_note(source: u8, recipe: Recipe, step: usize) -> u8 {
    let span = i32::from(recipe.amount);
    let increment = i32::from(recipe.offset);
    if increment == 0 {
        return source;
    }
    let source = i32::from(source);
    let (lower, upper) = if increment > 0 {
        (source, (source + span).min(127))
    } else {
        ((source - span).max(0), source)
    };
    let width = upper - lower + 1;
    let phase = i32::from(recipe.phase) + i32::try_from(step).unwrap_or(i32::MAX);
    (lower + (source - lower + phase.saturating_mul(increment)).rem_euclid(width)) as u8
}

fn mutate(
    source: &Pattern,
    draft: &mut Pattern,
    recipe: Recipe,
    global_lane: usize,
    report: &mut Report,
    affected_rows: &mut Vec<usize>,
) {
    for row in recipe.start_row..recipe.end_row() {
        let original = source.rows[row][global_lane];
        let Note::On(note) = original.note else {
            report.protected += usize::from(original != Cell::default());
            continue;
        };
        if original.command != Command::None {
            report.protected += 1;
            continue;
        }
        if mixed(recipe.seed, row, global_lane, 0) % 100 >= u64::from(recipe.density) {
            continue;
        }
        let range = i16::try_from(recipe.amount).unwrap_or(12);
        let candidates = (-range..=range)
            .filter(|delta| *delta != 0)
            .filter_map(|delta| {
                let changed = i16::from(note) + delta;
                (0..=127).contains(&changed).then_some(changed as u8)
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            report.protected += 1;
            continue;
        }
        report.candidates += 1;
        let choice = mixed(recipe.seed, row, global_lane, 1) as usize % candidates.len();
        let mut candidate = original;
        candidate.note = Note::On(candidates[choice]);
        draft.rows[row][global_lane] = candidate;
        report.affected += 1;
        report.replacements += 1;
        push_affected_row(affected_rows, row);
    }
}

fn propose(
    draft: &mut Pattern,
    recipe: Recipe,
    row: usize,
    lane: usize,
    candidate: Cell,
    report: &mut Report,
    affected_rows: &mut Vec<usize>,
) {
    report.candidates += 1;
    let existing = draft.rows[row][lane];
    if existing == candidate {
        return;
    }
    if existing == Cell::default() {
        draft.rows[row][lane] = candidate;
        report.affected += 1;
        push_affected_row(affected_rows, row);
        return;
    }
    let replaceable = matches!(existing.note, Note::On(_)) && existing.command == Command::None;
    if recipe.collision == CollisionPolicy::ReplaceNotes && replaceable {
        draft.rows[row][lane] = candidate;
        report.affected += 1;
        report.replacements += 1;
        push_affected_row(affected_rows, row);
    } else {
        report.collisions += 1;
        report.protected += usize::from(!replaceable);
    }
}

fn push_affected_row(affected_rows: &mut Vec<usize>, row: usize) {
    if affected_rows.last() != Some(&row) {
        affected_rows.push(row);
    }
}

fn rotation(recipe: Recipe) -> usize {
    usize::from(recipe.phase) % usize::from(recipe.length)
}

fn fill_velocity(base: u8, rank: usize, hits: usize) -> u8 {
    if hits <= 1 {
        return base.max(1);
    }
    let rise = usize::from(127 - base) * rank / (hits - 1);
    base.saturating_add(rise as u8).max(1)
}

fn mixed(seed: u64, row: usize, lane: usize, stream: u64) -> u64 {
    splitmix64(
        seed ^ (row as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (lane as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
            ^ stream.wrapping_mul(0x94D0_49BB_1331_11EB),
    )
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequencer::{Page, SwingDivision};
    use crate::tempo::Bpm;

    fn fixture(rows: usize, percussion: bool) -> Pattern {
        let mut pattern = Pattern::new(
            rows,
            Bpm::DEFAULT,
            4,
            vec![Page::new(
                "GEN",
                if percussion { 9 } else { 0 },
                percussion,
                0,
            )],
        );
        pattern.rows[0][0] = Cell {
            note: Note::On(if percussion { 38 } else { 60 }),
            velocity: Some(80),
            gate: Some(75),
            ..Cell::default()
        };
        pattern
    }

    fn recipe(pattern: &Pattern, tool: Tool) -> Recipe {
        Recipe::bounded_for(pattern, tool, 0, 0, 0, 41, Scale::default()).unwrap()
    }

    #[test]
    fn euclidean_length_pulses_rotation_bounds_and_placement_are_exact() {
        for length in [1, 2, 7, 16, 255, 256] {
            let pattern = fixture(length, false);
            for pulses in [0, 1, length / 2, length] {
                let mut settings = recipe(&pattern, Tool::Euclidean);
                settings.length = length as u16;
                settings.amount = pulses as u16;
                settings.phase = length.saturating_sub(1) as u16;
                settings.collision = CollisionPolicy::ReplaceNotes;
                let first = build(&pattern, settings).unwrap();
                let second = build(&pattern, settings).unwrap();
                assert_eq!(first, second);
                assert_eq!(first.report.candidates, pulses);
                assert_eq!(first.affected_rows.len(), first.report.affected);
                assert!(first.affected_rows.iter().all(|row| *row < length));
            }
        }
        let pattern = fixture(8, false);
        let mut unrotated = recipe(&pattern, Tool::Euclidean);
        unrotated.length = 8;
        unrotated.amount = 3;
        unrotated.collision = CollisionPolicy::ReplaceNotes;
        let mut rotated = unrotated;
        rotated.phase = 1;
        assert_eq!(build(&pattern, unrotated).unwrap().affected_rows, [3, 6]);
        assert_eq!(build(&pattern, rotated).unwrap().affected_rows, [1, 4, 7]);
    }

    #[test]
    fn accumulator_resets_wraps_and_repeats_inside_midi_bounds() {
        let mut pattern = fixture(16, false);
        pattern.rows[0][0].note = Note::On(124);
        let mut settings = recipe(&pattern, Tool::Accumulator);
        settings.length = 8;
        settings.amount = 12;
        settings.offset = 5;
        settings.phase = 2;
        settings.collision = CollisionPolicy::ReplaceNotes;
        let first = build(&pattern, settings).unwrap();
        assert_eq!(first, build(&pattern, settings).unwrap());
        let notes = first.pattern.rows[..8]
            .iter()
            .map(|row| row[0].note)
            .collect::<Vec<_>>();
        assert_eq!(
            notes,
            [126, 127, 124, 125, 126, 127, 124, 125].map(Note::On)
        );
        let mut negative_pattern = fixture(16, false);
        negative_pattern.rows[0][0].note = Note::On(3);
        settings.offset = -5;
        let negative = build(&negative_pattern, settings).unwrap();
        assert_eq!(
            negative.pattern.rows[..8]
                .iter()
                .map(|row| row[0].note)
                .collect::<Vec<_>>(),
            [1, 0, 3, 2, 1, 0, 3, 2].map(Note::On)
        );
        settings.offset = 0;
        let reset = build(&negative_pattern, settings).unwrap();
        assert!(reset.pattern.rows[..8]
            .iter()
            .all(|row| row[0].note == Note::On(3)));
    }

    #[test]
    fn mutation_seed_density_scope_and_protected_cells_are_stable() {
        let mut pattern = fixture(16, false);
        for row in 1..16 {
            pattern.rows[row][0] = Cell {
                note: Note::On(60 + row as u8 % 8),
                velocity: Some(70 + row as u8),
                probability: 80,
                condition: StepCondition::Previous,
                ..Cell::default()
            };
        }
        pattern.rows[4][0].command = Command::Retrigger(2);
        pattern.rows[5][0].note = Note::Off;
        pattern.rows[6][1].note = Note::On(90);
        let mut settings = recipe(&pattern, Tool::Mutation);
        settings.length = 16;
        settings.amount = 2;
        settings.density = 100;
        let first = build(&pattern, settings).unwrap();
        assert_eq!(first, build(&pattern, settings).unwrap());
        assert_eq!(first.pattern.rows[4][0], pattern.rows[4][0]);
        assert_eq!(first.pattern.rows[5][0], pattern.rows[5][0]);
        assert_eq!(first.pattern.rows[6][1], pattern.rows[6][1]);
        assert!(first.report.protected >= 2);
        for row in first.affected_rows {
            assert_eq!(
                first.pattern.rows[row][0].velocity,
                pattern.rows[row][0].velocity
            );
            assert_eq!(
                first.pattern.rows[row][0].condition,
                pattern.rows[row][0].condition
            );
        }
        settings.seed += 1;
        assert_ne!(build(&pattern, settings).unwrap().pattern, first.pattern);
        settings.seed -= 1;
        settings.density = 50;
        let intermediate = build(&pattern, settings).unwrap();
        assert_eq!(intermediate, build(&pattern, settings).unwrap());
        assert!(intermediate.report.affected > 0);
        assert!(intermediate.report.affected < first.report.affected);
        settings.density = 0;
        assert_eq!(build(&pattern, settings).unwrap().report.affected, 0);
    }

    #[test]
    fn controlled_fill_is_exact_seeded_fill_only_and_bounded() {
        let pattern = fixture(16, true);
        let mut settings = recipe(&pattern, Tool::Fill);
        settings.length = 16;
        settings.amount = 6;
        settings.collision = CollisionPolicy::ReplaceNotes;
        let first = build(&pattern, settings).unwrap();
        assert_eq!(first.report.candidates, 6);
        assert_eq!(first, build(&pattern, settings).unwrap());
        for row in &first.affected_rows {
            assert_eq!(first.pattern.rows[*row][0].condition, StepCondition::Fill);
            assert!(first.pattern.rows[*row][0]
                .velocity
                .is_some_and(|value| value <= 127));
        }
        settings.seed += 1;
        assert_ne!(
            first.affected_rows,
            build(&pattern, settings).unwrap().affected_rows
        );
        settings.seed -= 1;
        settings.phase = 1;
        let rotated = build(&pattern, settings).unwrap();
        let mut expected = first
            .affected_rows
            .iter()
            .map(|row| (row + 1) % 16)
            .collect::<Vec<_>>();
        expected.sort_unstable();
        assert_eq!(rotated.affected_rows, expected);
        let melodic = fixture(16, false);
        assert!(build(&melodic, settings).is_err());
    }

    #[test]
    fn reports_replacements_collisions_protection_and_noop_drafts_exactly() {
        let mut pattern = fixture(8, false);
        pattern.rows[1][0] = Cell {
            note: Note::On(64),
            ..Cell::default()
        };
        pattern.rows[2][0] = Cell {
            note: Note::Off,
            ..Cell::default()
        };
        pattern.rows[3][0] = Cell {
            command: Command::Tempo(Bpm::DEFAULT),
            ..Cell::default()
        };
        let mut settings = recipe(&pattern, Tool::Accumulator);
        settings.length = 4;
        settings.amount = 12;
        settings.offset = 2;
        settings.phase = 2;
        let empty_only = build(&pattern, settings).unwrap();
        assert_eq!(empty_only.report.collisions, 4);
        assert_eq!(empty_only.report.protected, 2);
        assert_eq!(empty_only.pattern, pattern);
        settings.collision = CollisionPolicy::ReplaceNotes;
        let replace = build(&pattern, settings).unwrap();
        assert_eq!(replace.report.replacements, 2);
        assert_eq!(replace.report.collisions, 2);
        assert_eq!(replace.report.protected, 2);
        assert_eq!(replace.report.affected, 2);
        assert_eq!(pattern.swing_division, SwingDivision::Sixteenth);
    }

    #[test]
    fn arpeggio_extracts_explicit_chord_orders_octaves_rates_gates_and_repeats() {
        let mut pattern = fixture(48, false);
        pattern.rows[0][0] = Cell {
            note: Note::On(67),
            velocity: Some(91),
            program: Some(4),
            gate: Some(20),
            command: Command::Retrigger(2),
            nudge: 12,
            probability: 50,
            condition: StepCondition::Previous,
        };
        pattern.rows[0][1] = Cell {
            note: Note::On(60),
            velocity: Some(72),
            ..Cell::default()
        };
        pattern.rows[0][2] = Cell {
            note: Note::On(64),
            velocity: Some(83),
            ..Cell::default()
        };
        pattern.rows[0][3] = Cell {
            note: Note::On(60),
            velocity: Some(120),
            ..Cell::default()
        };
        let mut settings = recipe(&pattern, Tool::Arpeggio);
        settings.length = 2;
        settings.gate = 50;
        settings.collision = CollisionPolicy::ReplaceNotes;
        let up = build(&pattern, settings).unwrap();
        assert_eq!(up, build(&pattern, settings).unwrap());
        assert_eq!(up.report.source_cells, 3);
        assert_eq!(up.report.affected, 6);
        assert_eq!(up.affected_rows, [1, 2, 3, 4, 5, 6]);
        assert_eq!(
            up.pattern.rows[1..=6]
                .iter()
                .map(|row| row[0].note)
                .collect::<Vec<_>>(),
            [60, 64, 67, 60, 64, 67].map(Note::On)
        );
        for row in 1..=6 {
            let cell = up.pattern.rows[row][0];
            assert_eq!(cell.gate, Some(50));
            assert_eq!(cell.command, Command::None);
            assert_eq!(cell.nudge, 0);
            assert_eq!(cell.probability, 100);
            assert_eq!(cell.condition, StepCondition::Always);
        }

        settings.arpeggio_order = ArpeggioOrder::AsLane;
        settings.length = 1;
        let as_lane = build(&pattern, settings).unwrap();
        assert_eq!(
            as_lane.pattern.rows[1..=3]
                .iter()
                .map(|row| row[0].note)
                .collect::<Vec<_>>(),
            [67, 60, 64].map(Note::On)
        );
        settings.arpeggio_order = ArpeggioOrder::Down;
        let down = build(&pattern, settings).unwrap();
        assert_eq!(
            down.pattern.rows[1..=3]
                .iter()
                .map(|row| row[0].note)
                .collect::<Vec<_>>(),
            [67, 64, 60].map(Note::On)
        );
        settings.arpeggio_order = ArpeggioOrder::UpDown;
        settings.arpeggio_octaves = 2;
        settings.row_rate = RowRate::Two;
        let reflected = build(&pattern, settings).unwrap();
        assert_eq!(
            (1..=10)
                .map(|index| reflected.pattern.rows[index * 2][0].note)
                .collect::<Vec<_>>(),
            [60, 64, 67, 72, 76, 79, 76, 72, 67, 64].map(Note::On)
        );
    }

    #[test]
    fn arpeggio_refuses_missing_source_midi_range_and_partial_pattern_end() {
        let mut empty = fixture(8, false);
        empty.rows[0][0] = Cell::default();
        let settings = recipe(&empty, Tool::Arpeggio);
        assert!(build(&empty, settings)
            .unwrap_err()
            .to_string()
            .contains("existing note or chord"));

        let mut high = fixture(16, false);
        high.rows[0][0].note = Note::On(120);
        let mut settings = recipe(&high, Tool::Arpeggio);
        settings.arpeggio_octaves = 2;
        assert_eq!(
            build(&high, settings).unwrap_err().to_string(),
            "arpeggio MIDI range refusal: 1 note(s)"
        );

        let short = fixture(4, false);
        let mut settings = recipe(&short, Tool::Arpeggio);
        settings.length = 2;
        settings.row_rate = RowRate::Two;
        assert!(build(&short, settings)
            .unwrap_err()
            .to_string()
            .contains("Pattern-end refusal"));
    }

    #[test]
    fn chord_degrees_quality_inversions_voicing_lanes_and_placement_are_exact() {
        let pattern = fixture(32, false);
        let expected_major = [
            [60, 64, 67],
            [62, 65, 69],
            [64, 67, 71],
            [65, 69, 72],
            [67, 71, 74],
            [69, 72, 76],
            [71, 74, 77],
        ];
        let expected_minor = [
            [60, 63, 67],
            [62, 65, 68],
            [63, 67, 70],
            [65, 68, 72],
            [67, 70, 74],
            [68, 72, 75],
            [70, 74, 77],
        ];
        for (kind, expected) in [
            (crate::scale::ScaleKind::Major, expected_major),
            (crate::scale::ScaleKind::NaturalMinor, expected_minor),
        ] {
            for degree in 1..=7 {
                let mut settings = recipe(&pattern, Tool::Chord);
                settings.scale.kind = kind;
                settings.chord_degree = degree;
                settings.collision = CollisionPolicy::ReplaceNotes;
                let draft = build(&pattern, settings).unwrap();
                assert_eq!(draft, build(&pattern, settings).unwrap());
                assert_eq!(
                    draft.pattern.rows[0][..3]
                        .iter()
                        .map(|cell| match cell.note {
                            Note::On(note) => note,
                            _ => 255,
                        })
                        .collect::<Vec<_>>(),
                    expected[usize::from(degree - 1)]
                );
                assert_eq!(
                    chord_quality_label(settings.scale, degree),
                    match expected[usize::from(degree - 1)] {
                        [root, third, fifth] if third - root == 4 && fifth - root == 7 => "MAJ",
                        [root, third, fifth] if third - root == 3 && fifth - root == 7 => "MIN",
                        _ => "DIM",
                    }
                );
            }
        }

        let mut settings = recipe(&pattern, Tool::Chord);
        settings.collision = CollisionPolicy::ReplaceNotes;
        settings.chord_inversion = 1;
        assert_eq!(chord_notes(&pattern, settings, 0).unwrap(), [64, 67, 72]);
        settings.chord_inversion = 2;
        assert_eq!(chord_notes(&pattern, settings, 0).unwrap(), [67, 72, 76]);
        settings.chord_inversion = 0;
        settings.chord_voicing = ChordVoicing::Open;
        assert_eq!(chord_notes(&pattern, settings, 0).unwrap(), [60, 67, 76]);
        settings.chord_voicing = ChordVoicing::Close;
        settings.length = 3;
        settings.row_rate = RowRate::Two;
        let placed = build(&pattern, settings).unwrap();
        assert_eq!(placed.affected_rows, [0, 2, 4]);
        assert_eq!(placed.report.candidates, 9);

        settings.lane = 2;
        assert!(build(&pattern, settings)
            .unwrap_err()
            .to_string()
            .contains("two following lanes"));
    }

    #[test]
    fn chord_refuses_midi_and_pattern_bounds_before_returning_a_partial_draft() {
        let mut pattern = fixture(4, false);
        pattern.rows[0][0].note = Note::On(120);
        let mut settings = recipe(&pattern, Tool::Chord);
        settings.chord_inversion = 2;
        assert!(build(&pattern, settings)
            .unwrap_err()
            .to_string()
            .contains("MIDI range refusal"));

        settings.chord_inversion = 0;
        settings.length = 3;
        settings.row_rate = RowRate::Two;
        assert!(build(&pattern, settings)
            .unwrap_err()
            .to_string()
            .contains("Pattern-end refusal"));
    }

    #[test]
    fn harmonizer_scale_interval_voice_policy_range_and_fields_are_exact() {
        let mut pattern = fixture(8, false);
        pattern.rows[0][0] = Cell {
            note: Note::On(60),
            velocity: Some(71),
            program: Some(3),
            gate: Some(62),
            command: Command::Retrigger(3),
            nudge: -17,
            probability: 63,
            condition: StepCondition::Previous,
        };
        pattern.rows[1][0] = Cell {
            note: Note::On(63),
            velocity: Some(88),
            ..Cell::default()
        };
        pattern.rows[2][0] = Cell {
            note: Note::Off,
            ..Cell::default()
        };
        pattern.rows[3][0] = Cell {
            note: Note::On(71),
            ..Cell::default()
        };
        let mut settings = recipe(&pattern, Tool::Harmonizer);
        settings.length = 4;
        assert_eq!(
            build(&pattern, settings).unwrap_err().to_string(),
            "harmonizer out-of-scale refusal: 1 note(s)"
        );
        settings.out_of_scale = OutOfScalePolicy::Skip;
        let above_third = build(&pattern, settings).unwrap();
        assert_eq!(above_third, build(&pattern, settings).unwrap());
        assert_eq!(above_third.report.out_of_scale, 1);
        assert_eq!(above_third.pattern.rows[0][1].note, Note::On(64));
        let mut expected = pattern.rows[0][0];
        expected.note = Note::On(64);
        assert_eq!(above_third.pattern.rows[0][1], expected);
        assert_eq!(above_third.pattern.rows[2][1].note, Note::Off);
        assert_eq!(above_third.pattern.rows[1][1], Cell::default());
        assert_eq!(pattern.rows[0][0].note, Note::On(60));

        settings.harmony_interval = HarmonyInterval::Fifth;
        assert_eq!(
            build(&pattern, settings).unwrap().pattern.rows[0][1].note,
            Note::On(67)
        );
        settings.harmony_voice = HarmonyVoice::Below;
        settings.harmony_interval = HarmonyInterval::Third;
        assert_eq!(
            build(&pattern, settings).unwrap().pattern.rows[0][1].note,
            Note::On(57)
        );
        settings.harmony_target_lane = 0;
        assert!(build(&pattern, settings)
            .unwrap_err()
            .to_string()
            .contains("must differ"));

        let mut low = fixture(2, false);
        low.rows[0][0].note = Note::On(0);
        let mut low_settings = recipe(&low, Tool::Harmonizer);
        low_settings.harmony_voice = HarmonyVoice::Below;
        assert_eq!(
            build(&low, low_settings).unwrap_err().to_string(),
            "harmonizer MIDI range refusal: 1 note(s)"
        );
    }
}
