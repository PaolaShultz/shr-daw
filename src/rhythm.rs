//! Deterministic Pattern groove transforms. Runtime playback never randomizes
//! these shapes: Apply stores the exact resulting Cell values.

use crate::sequencer::{Note, Pattern, LANES_PER_PAGE, MAX_CELL_NUDGE};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GroovePreset {
    #[default]
    SnareLate,
    HatsEarly,
    Alternating,
    EndDrag,
    EndPush,
}

impl GroovePreset {
    pub const ALL: [Self; 5] = [
        Self::SnareLate,
        Self::HatsEarly,
        Self::Alternating,
        Self::EndDrag,
        Self::EndPush,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::SnareLate => "SNARE LATE",
            Self::HatsEarly => "HATS EARLY",
            Self::Alternating => "ALT PUSH/PULL",
            Self::EndDrag => "END DRAG",
            Self::EndPush => "END PUSH",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GrooveScope {
    #[default]
    Cell,
    Lane,
    Page,
    Pattern,
}

impl GrooveScope {
    pub const ALL: [Self; 4] = [Self::Cell, Self::Lane, Self::Page, Self::Pattern];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Cell => "CELL",
            Self::Lane => "LANE",
            Self::Page => "PAGE",
            Self::Pattern => "PATTERN",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GrooveSelection {
    pub row: usize,
    pub page: usize,
    pub lane: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GrooveReport {
    pub matched: usize,
    pub changed: usize,
}

pub fn preview(
    pattern: &Pattern,
    preset: GroovePreset,
    scope: GrooveScope,
    selection: GrooveSelection,
) -> usize {
    selected_hits(pattern, preset, scope, selection).len()
}

pub fn apply(
    pattern: &mut Pattern,
    preset: GroovePreset,
    scope: GrooveScope,
    selection: GrooveSelection,
    strength: u8,
) -> GrooveReport {
    let strength = strength.min(100);
    let hits = selected_hits(pattern, preset, scope, selection);
    let matched = hits.len();
    let rows = pattern.rows.len();
    let mut changed = 0;
    for (sequence, (row, lane)) in hits.into_iter().enumerate() {
        let cell = &mut pattern.rows[row][lane];
        let old = *cell;
        let (full_nudge, full_velocity_delta) = match preset {
            GroovePreset::SnareLate => (12, 8),
            GroovePreset::HatsEarly => (-12, 0),
            GroovePreset::Alternating if sequence.is_multiple_of(2) => (-12, 6),
            GroovePreset::Alternating => (12, -6),
            GroovePreset::EndDrag => {
                let position = phrase_position(row, rows);
                (
                    (24.0 * position).round() as i16,
                    (-6.0 * position).round() as i16,
                )
            }
            GroovePreset::EndPush => {
                let position = phrase_position(row, rows);
                (
                    (-24.0 * position).round() as i16,
                    (6.0 * position).round() as i16,
                )
            }
        };
        let scaled_nudge = scale_signed(full_nudge, strength);
        let minimum = if row == 0 { 0 } else { -MAX_CELL_NUDGE };
        let maximum = MAX_CELL_NUDGE;
        cell.nudge = i16::from(cell.nudge)
            .saturating_add(scaled_nudge)
            .clamp(i16::from(minimum), i16::from(maximum)) as i8;
        if let Some(velocity) = cell.velocity.as_mut() {
            let delta = scale_signed(full_velocity_delta, strength);
            *velocity = (i16::from(*velocity) + delta).clamp(1, 127) as u8;
        }
        changed += usize::from(*cell != old);
    }
    GrooveReport { matched, changed }
}

fn scale_signed(value: i16, strength: u8) -> i16 {
    let numerator = i32::from(value) * i32::from(strength);
    if numerator < 0 {
        ((numerator - 50) / 100) as i16
    } else {
        ((numerator + 50) / 100) as i16
    }
}

fn phrase_position(row: usize, rows: usize) -> f64 {
    if rows <= 1 {
        0.0
    } else {
        row as f64 / (rows - 1) as f64
    }
}

fn selected_hits(
    pattern: &Pattern,
    preset: GroovePreset,
    scope: GrooveScope,
    selection: GrooveSelection,
) -> Vec<(usize, usize)> {
    let selected_lane = selection.page * LANES_PER_PAGE + selection.lane;
    pattern
        .rows
        .iter()
        .enumerate()
        .flat_map(|(row, cells)| {
            cells.iter().enumerate().filter_map(move |(lane, cell)| {
                let in_scope = match scope {
                    GrooveScope::Cell => row == selection.row && lane == selected_lane,
                    GrooveScope::Lane => lane == selected_lane,
                    GrooveScope::Page => lane / LANES_PER_PAGE == selection.page,
                    GrooveScope::Pattern => true,
                };
                let Note::On(note) = cell.note else {
                    return None;
                };
                let matches_preset = match preset {
                    GroovePreset::SnareLate => matches!(note, 38 | 40),
                    GroovePreset::HatsEarly => matches!(note, 42 | 44 | 46 | 51 | 53 | 59),
                    GroovePreset::Alternating | GroovePreset::EndDrag | GroovePreset::EndPush => {
                        true
                    }
                };
                (in_scope && matches_preset).then_some((row, lane))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequencer::{Cell, Page};
    use crate::tempo::Bpm;

    fn fixture() -> Pattern {
        let mut pattern = Pattern::new(4, Bpm::DEFAULT, 4, vec![Page::new("DRUMS", 9, true, 0)]);
        for (row, note) in [36, 38, 42, 38].into_iter().enumerate() {
            pattern.rows[row][0] = Cell {
                note: Note::On(note),
                velocity: Some(96),
                ..Cell::default()
            };
        }
        pattern
    }

    #[test]
    fn presets_are_deterministic_and_boundary_safe() {
        let source = fixture();
        for preset in GroovePreset::ALL {
            let mut first = source.clone();
            let mut second = source.clone();
            let selection = GrooveSelection {
                row: 0,
                page: 0,
                lane: 0,
            };
            assert_eq!(
                apply(&mut first, preset, GrooveScope::Pattern, selection, 100),
                apply(&mut second, preset, GrooveScope::Pattern, selection, 100)
            );
            assert_eq!(first, second);
            assert!(first.rows[0].iter().all(|cell| cell.nudge >= 0));
            assert!(first
                .rows
                .iter()
                .flatten()
                .all(|cell| (-MAX_CELL_NUDGE..=MAX_CELL_NUDGE).contains(&cell.nudge)));
        }
    }

    #[test]
    fn zero_strength_and_empty_scope_are_noops() {
        let source = fixture();
        let mut pattern = source.clone();
        let report = apply(
            &mut pattern,
            GroovePreset::SnareLate,
            GrooveScope::Cell,
            GrooveSelection {
                row: 0,
                page: 0,
                lane: 3,
            },
            0,
        );
        assert_eq!(report, GrooveReport::default());
        assert_eq!(pattern, source);
    }
}
