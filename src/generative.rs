//! Offline deterministic Pattern generators.
//!
//! Drafting always clones the source Pattern. Playback never calls this module;
//! Apply stores the resulting ordinary Cells through the existing owners.

use crate::sequencer::{Cell, Command, Note, Pattern, StepCondition, LANES_PER_PAGE};
use anyhow::{bail, Context, Result};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Tool {
    #[default]
    Euclidean,
    Accumulator,
    Mutation,
    Fill,
}

impl Tool {
    pub const ALL: [Self; 4] = [
        Self::Euclidean,
        Self::Accumulator,
        Self::Mutation,
        Self::Fill,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Euclidean => "EUCLIDEAN",
            Self::Accumulator => "ACCUMULATOR",
            Self::Mutation => "MUTATION",
            Self::Fill => "FILL",
        }
    }

    pub const fn uses_seed(self) -> bool {
        matches!(self, Self::Mutation | Self::Fill)
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
}

impl Recipe {
    pub fn bounded_for(
        pattern: &Pattern,
        tool: Tool,
        page: usize,
        lane: usize,
        start_row: usize,
        retained_seed: u64,
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
        let length = remaining.min(16) as u16;
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
            },
            offset: match tool {
                Tool::Accumulator => 2,
                _ => 0,
            },
            phase: 0,
            density: 50,
            seed: retained_seed,
            collision: CollisionPolicy::EmptyOnly,
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
        self.length = self.length.clamp(1, remaining.min(256) as u16);
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
        }
        self.density = self.density.min(100);
        Ok(())
    }

    pub fn end_row(self) -> usize {
        self.start_row + usize::from(self.length)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Report {
    pub candidates: usize,
    pub affected: usize,
    pub replacements: usize,
    pub collisions: usize,
    pub protected: usize,
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
    }
    Ok(Draft {
        recipe,
        pattern: draft,
        report,
        affected_rows,
    })
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
        affected_rows.push(row);
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
        affected_rows.push(row);
        return;
    }
    let replaceable = matches!(existing.note, Note::On(_)) && existing.command == Command::None;
    if recipe.collision == CollisionPolicy::ReplaceNotes && replaceable {
        draft.rows[row][lane] = candidate;
        report.affected += 1;
        report.replacements += 1;
        affected_rows.push(row);
    } else {
        report.collisions += 1;
        report.protected += usize::from(!replaceable);
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
        Recipe::bounded_for(pattern, tool, 0, 0, 0, 41).unwrap()
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
}
