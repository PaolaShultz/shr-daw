//! Transient Live Patterns and tracker-lane performance state.
//!
//! This module deliberately owns no MIDI or JACK endpoint. The sequencer owns
//! activation and note cleanup; the UI owns selection and confirmation.

use crate::sequencer::{Cell, Note, Pattern, Song, LANES_PER_PAGE};
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LaunchQuantization {
    Pattern,
    #[default]
    Bar,
}

impl LaunchQuantization {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pattern => "PATTERN",
            Self::Bar => "BAR",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LaneShape {
    pub muted: bool,
    /// Percentage of the stored/inherited velocity.
    pub velocity_percent: u16,
    /// Percentage of the stored/inherited gate.
    pub gate_percent: u16,
    /// Signed semitone offset.
    pub transpose: i8,
}

impl Default for LaneShape {
    fn default() -> Self {
        Self {
            muted: false,
            velocity_percent: 100,
            gate_percent: 100,
            transpose: 0,
        }
    }
}

impl LaneShape {
    pub const MIN_SCALE: u16 = 10;
    pub const MAX_SCALE: u16 = 200;
    pub const MIN_TRANSPOSE: i8 = -48;
    pub const MAX_TRANSPOSE: i8 = 48;

    pub fn set_velocity(&mut self, percent: i32) {
        self.velocity_percent =
            percent.clamp(i32::from(Self::MIN_SCALE), i32::from(Self::MAX_SCALE)) as u16;
    }

    pub fn set_gate(&mut self, percent: i32) {
        self.gate_percent =
            percent.clamp(i32::from(Self::MIN_SCALE), i32::from(Self::MAX_SCALE)) as u16;
    }

    pub fn set_transpose(&mut self, semitones: i16) {
        self.transpose = semitones.clamp(
            i16::from(Self::MIN_TRANSPOSE),
            i16::from(Self::MAX_TRANSPOSE),
        ) as i8;
    }

    pub fn velocity(self, stored: u8) -> u8 {
        ((u32::from(stored) * u32::from(self.velocity_percent) + 50) / 100).clamp(1, 127) as u8
    }

    pub fn gate(self, stored: u8) -> u8 {
        ((u32::from(stored) * u32::from(self.gate_percent) + 50) / 100).clamp(1, 100) as u8
    }

    pub fn note(self, stored: u8) -> u8 {
        (i16::from(stored) + i16::from(self.transpose)).clamp(0, 127) as u8
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueuedPattern {
    pub pattern: u16,
    pub quantization: LaunchQuantization,
    pub retrigger: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivatedPattern {
    pub serial: u64,
    pub pattern: u16,
    pub retrigger: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum CaptureState {
    #[default]
    Off,
    Recording(Vec<u16>),
    Confirm(Vec<u16>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureCommit {
    Append,
    Replace,
}

#[derive(Clone, Debug, Default)]
pub struct LivePatternPerformance {
    selected: Option<u16>,
    current: Option<u16>,
    queued: Option<QueuedPattern>,
    quantization: LaunchQuantization,
    activation_serial: u64,
    capture: CaptureState,
    lane_shapes: BTreeMap<(u16, usize), [LaneShape; LANES_PER_PAGE]>,
}

impl LivePatternPerformance {
    pub fn reset_for_project(&mut self, song: &Song) {
        let first = song.patterns.keys().next().copied();
        *self = Self {
            selected: first,
            current: None,
            ..Self::default()
        };
    }

    pub fn selected(&self) -> Option<u16> {
        self.selected
    }

    pub fn current(&self) -> Option<u16> {
        self.current
    }

    pub fn queued(&self) -> Option<QueuedPattern> {
        self.queued
    }

    pub fn quantization(&self) -> LaunchQuantization {
        self.quantization
    }

    pub fn set_quantization(&mut self, quantization: LaunchQuantization) {
        self.quantization = quantization;
    }

    #[cfg(test)]
    pub fn select(&mut self, song: &Song, pattern: u16) -> Result<()> {
        if !song.patterns.contains_key(&pattern) {
            bail!("Pattern {pattern:02} is missing");
        }
        self.selected = Some(pattern);
        Ok(())
    }

    pub fn browse(&mut self, song: &Song, direction: i8) {
        let patterns = song.patterns.keys().copied().collect::<Vec<_>>();
        if patterns.is_empty() {
            self.selected = None;
            return;
        }
        let current = self
            .selected
            .and_then(|selected| patterns.iter().position(|pattern| *pattern == selected))
            .unwrap_or(0);
        let next = if direction < 0 {
            (current + patterns.len() - 1) % patterns.len()
        } else {
            (current + 1) % patterns.len()
        };
        self.selected = Some(patterns[next]);
    }

    pub fn group(&self, song: &Song) -> Vec<u16> {
        let patterns = song.patterns.keys().copied().collect::<Vec<_>>();
        let selected = self.selected.unwrap_or_default();
        let index = patterns
            .iter()
            .position(|pattern| *pattern == selected)
            .unwrap_or_default();
        patterns.into_iter().skip(index / 4 * 4).take(4).collect()
    }

    pub fn queue_selected(&mut self) -> Result<QueuedPattern> {
        let queued = QueuedPattern {
            pattern: self.selected.context("no Pattern selected")?,
            quantization: self.quantization,
            retrigger: false,
        };
        self.queued = Some(queued);
        Ok(queued)
    }

    pub fn queue_retrigger(&mut self) -> Result<QueuedPattern> {
        let queued = QueuedPattern {
            pattern: self.current.context("no Pattern is playing")?,
            quantization: self.quantization,
            retrigger: true,
        };
        self.queued = Some(queued);
        Ok(queued)
    }

    pub fn cancel_queue(&mut self) -> bool {
        self.queued.take().is_some()
    }

    /// Commit only after the sequencer has validated the target and reached
    /// the requested boundary.
    pub fn activate(&mut self, pattern: u16, retrigger: bool) -> ActivatedPattern {
        self.current = Some(pattern);
        self.queued = None;
        self.activation_serial = self.activation_serial.wrapping_add(1);
        if let CaptureState::Recording(launches) = &mut self.capture {
            launches.push(pattern);
        }
        ActivatedPattern {
            serial: self.activation_serial,
            pattern,
            retrigger,
        }
    }

    pub fn fail_queued(&mut self) {
        self.queued = None;
    }

    pub fn start_capture(&mut self) {
        self.capture = CaptureState::Recording(Vec::new());
    }

    pub fn request_capture_commit(&mut self) -> Result<()> {
        let CaptureState::Recording(launches) = &self.capture else {
            bail!("Live Pattern capture is not recording");
        };
        self.capture = CaptureState::Confirm(launches.clone());
        Ok(())
    }

    pub fn cancel_capture(&mut self) {
        self.capture = CaptureState::Off;
    }

    pub fn capture(&self) -> &CaptureState {
        &self.capture
    }

    pub fn confirm_capture(&mut self, song: &mut Song, commit: CaptureCommit) -> Result<usize> {
        let CaptureState::Confirm(launches) = &self.capture else {
            bail!("Live Pattern capture is not awaiting confirmation");
        };
        if launches
            .iter()
            .any(|pattern| !song.patterns.contains_key(pattern))
        {
            bail!("captured Pattern is no longer available");
        }
        let count = launches.len();
        match commit {
            CaptureCommit::Append => {
                let total = song.order.len().saturating_add(count);
                if total > 4096 {
                    bail!("captured Arrangement would exceed 4096 steps");
                }
                song.order.extend(launches.iter().copied());
            }
            CaptureCommit::Replace => {
                if launches.is_empty() {
                    bail!("cannot replace Arrangement with an empty capture");
                }
                song.order.clone_from(launches);
            }
        }
        self.capture = CaptureState::Off;
        Ok(count)
    }

    pub fn shapes(&self, pattern: u16, page: usize) -> [LaneShape; LANES_PER_PAGE] {
        self.lane_shapes
            .get(&(pattern, page))
            .copied()
            .unwrap_or([LaneShape::default(); LANES_PER_PAGE])
    }

    pub fn shape_mut(&mut self, pattern: u16, page: usize, lane: usize) -> &mut LaneShape {
        &mut self
            .lane_shapes
            .entry((pattern, page))
            .or_insert([LaneShape::default(); LANES_PER_PAGE])[lane.min(LANES_PER_PAGE - 1)]
    }

    pub fn shaped_pattern(
        &self,
        pattern_number: u16,
        pattern: &Pattern,
        inherited_gate: u8,
    ) -> Pattern {
        let mut shaped = pattern.clone();
        for (page_index, page) in shaped.pages.iter_mut().enumerate() {
            let shapes = self.shapes(pattern_number, page_index);
            for (lane, shape) in shapes.iter().enumerate() {
                if shape.muted {
                    page.lanes[lane].enabled = false;
                }
            }
            for row in &mut shaped.rows {
                for (lane, shape) in shapes.iter().copied().enumerate() {
                    let cell_index = page_index * LANES_PER_PAGE + lane;
                    let Some(cell) = row.get_mut(cell_index) else {
                        continue;
                    };
                    apply_shape(cell, page.velocity, inherited_gate, shape);
                }
            }
        }
        shaped
    }
}

fn apply_shape(cell: &mut Cell, page_velocity: u8, inherited_gate: u8, shape: LaneShape) {
    if let Note::On(note) = cell.note {
        cell.note = Note::On(shape.note(note));
        cell.velocity = Some(shape.velocity(cell.velocity.unwrap_or(page_velocity)));
        cell.gate = Some(shape.gate(cell.gate.unwrap_or(inherited_gate)));
    }
}

pub fn is_launch_boundary(
    quantization: LaunchQuantization,
    next_row: usize,
    pattern_rows: usize,
    steps_per_beat: u8,
    meter: u8,
) -> bool {
    if next_row >= pattern_rows {
        return true;
    }
    match quantization {
        LaunchQuantization::Pattern => false,
        LaunchQuantization::Bar => {
            let bar_rows = usize::from(steps_per_beat.max(1)) * usize::from(meter.max(1));
            next_row > 0 && next_row.is_multiple_of(bar_rows)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RuntimeConfig;

    fn song_with_patterns(count: u16) -> Song {
        let config = RuntimeConfig::default().external_midi;
        let mut song = Song::new(&config);
        for _ in 1..count {
            let pattern = Pattern::empty_like_setup(16, &song.patterns[&0]);
            song.append_pattern(pattern).unwrap();
        }
        song
    }

    #[test]
    fn browsing_and_group_navigation_never_launch() {
        let song = song_with_patterns(6);
        let mut live = LivePatternPerformance::default();
        live.reset_for_project(&song);
        live.browse(&song, 1);
        live.browse(&song, 1);
        assert_eq!(live.selected(), Some(2));
        assert_eq!(live.current(), None);
        assert_eq!(live.queued(), None);
        assert_eq!(live.group(&song), [0, 1, 2, 3]);
        live.select(&song, 5).unwrap();
        assert_eq!(live.group(&song), [4, 5]);
    }

    #[test]
    fn queue_replacement_cancellation_retrigger_and_boundaries_are_explicit() {
        let song = song_with_patterns(3);
        let mut live = LivePatternPerformance::default();
        live.reset_for_project(&song);
        live.set_quantization(LaunchQuantization::Pattern);
        live.select(&song, 1).unwrap();
        assert_eq!(live.queue_selected().unwrap().pattern, 1);
        live.select(&song, 2).unwrap();
        assert_eq!(live.queue_selected().unwrap().pattern, 2);
        assert!(live.cancel_queue());
        assert!(!live.cancel_queue());
        live.activate(1, false);
        assert_eq!(live.queue_retrigger().unwrap().pattern, 1);
        assert!(is_launch_boundary(
            LaunchQuantization::Pattern,
            64,
            64,
            4,
            4
        ));
        assert!(!is_launch_boundary(
            LaunchQuantization::Pattern,
            16,
            64,
            4,
            4
        ));
        assert!(is_launch_boundary(LaunchQuantization::Bar, 16, 64, 4, 4));
    }

    #[test]
    fn capture_records_only_activations_and_requires_confirmation() {
        let mut song = song_with_patterns(3);
        let original = song.order.clone();
        let mut live = LivePatternPerformance::default();
        live.reset_for_project(&song);
        live.start_capture();
        live.select(&song, 2).unwrap();
        live.queue_selected().unwrap();
        live.cancel_queue();
        live.activate(1, false);
        live.fail_queued();
        live.activate(1, true);
        assert_eq!(live.capture(), &CaptureState::Recording(vec![1, 1]));
        assert_eq!(song.order, original);
        live.request_capture_commit().unwrap();
        live.cancel_capture();
        assert_eq!(song.order, original);
        live.start_capture();
        live.activate(2, false);
        live.activate(2, true);
        live.request_capture_commit().unwrap();
        assert_eq!(
            live.confirm_capture(&mut song, CaptureCommit::Replace)
                .unwrap(),
            2
        );
        assert_eq!(song.order, [2, 2]);
        assert_eq!(song.patterns.len(), 3, "references must not clone Patterns");
    }

    #[test]
    fn lane_shaping_is_bounded_deterministic_and_transient() {
        let song = song_with_patterns(1);
        let mut live = LivePatternPerformance::default();
        live.reset_for_project(&song);
        let shape = live.shape_mut(0, 0, 0);
        shape.set_velocity(999);
        shape.set_gate(-1);
        shape.set_transpose(90);
        assert_eq!(shape.velocity(127), 127);
        assert_eq!(shape.gate(80), 8);
        assert_eq!(shape.note(100), 127);

        let mut pattern = song.patterns[&0].clone();
        pattern.rows[0][0] = Cell {
            note: Note::On(100),
            velocity: Some(80),
            gate: Some(50),
            ..Cell::default()
        };
        let shaped = live.shaped_pattern(0, &pattern, song.gate_percent);
        assert_eq!(shaped.rows[0][0].note, Note::On(127));
        assert_eq!(shaped.rows[0][0].velocity, Some(127));
        assert_eq!(shaped.rows[0][0].gate, Some(5));
        assert_eq!(pattern.rows[0][0].note, Note::On(100));

        let replacement = song_with_patterns(1);
        live.reset_for_project(&replacement);
        assert_eq!(live.shapes(0, 0), [LaneShape::default(); 4]);
    }
}
