use crate::sequencer::{
    AutomationLane, AutomationPoint, AutomationTarget, Cell, Lane, LoopSettings, Page, PageTarget,
    Pattern, MAX_PROJECT_AUTOMATION_POINTS, MAX_PROJECT_CELLS,
};
use std::collections::VecDeque;
use std::mem::size_of;

pub const MAX_PATTERN_HISTORY_STATES: usize = 32;
pub const MAX_PATTERN_HISTORY_WEIGHT: usize = MAX_PROJECT_CELLS * size_of::<Cell>() * 2
    + MAX_PROJECT_AUTOMATION_POINTS * size_of::<AutomationPoint>() * 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatternHistoryState<C> {
    pub pattern_id: u16,
    pub pattern: Pattern,
    pub edit_context: C,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatternSnapshot<C> {
    pub pattern_id: u16,
    pub pattern: Pattern,
    pub edit_context: C,
    pub label: String,
}

#[derive(Clone, Debug)]
struct HistoryEntry<C> {
    state: PatternHistoryState<C>,
    weight: usize,
}

#[derive(Clone, Debug)]
pub struct PatternHistory<C, G> {
    undo: VecDeque<HistoryEntry<C>>,
    redo: VecDeque<HistoryEntry<C>>,
    snapshot: Option<PatternSnapshot<C>>,
    coalescing: Option<(u16, G)>,
    weight: usize,
}

impl<C, G> Default for PatternHistory<C, G> {
    fn default() -> Self {
        Self {
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            snapshot: None,
            coalescing: None,
            weight: 0,
        }
    }
}

impl<C: Clone, G: Clone + Eq> PatternHistory<C, G> {
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.snapshot = None;
        self.coalescing = None;
        self.weight = 0;
    }

    pub fn break_coalescing(&mut self) {
        self.coalescing = None;
    }

    pub fn record_mutation(
        &mut self,
        opening: PatternHistoryState<C>,
        current: &Pattern,
        gesture: Option<G>,
    ) -> bool {
        if opening.pattern == *current {
            return false;
        }

        self.clear_redo();
        let continuing = gesture.as_ref().is_some_and(|gesture| {
            self.coalescing.as_ref().is_some_and(|(pattern_id, prior)| {
                *pattern_id == opening.pattern_id && prior == gesture
            })
        });
        if !continuing {
            self.push_undo(opening);
        }
        self.coalescing = gesture.map(|gesture| (opening.pattern_id, gesture));
        self.prune();
        true
    }

    pub fn next_undo(&self) -> Option<&PatternHistoryState<C>> {
        self.undo.back().map(|entry| &entry.state)
    }

    pub fn next_redo(&self) -> Option<&PatternHistoryState<C>> {
        self.redo.back().map(|entry| &entry.state)
    }

    pub fn prepare_undo(&self) -> Option<PatternHistoryState<C>> {
        self.next_undo().cloned()
    }

    pub fn prepare_redo(&self) -> Option<PatternHistoryState<C>> {
        self.next_redo().cloned()
    }

    pub fn commit_undo(&mut self, mut current: PatternHistoryState<C>) {
        let Some(entry) = self.undo.pop_back() else {
            return;
        };
        self.weight = self.weight.saturating_sub(entry.weight);
        current.label.clone_from(&entry.state.label);
        self.push_redo(current);
        self.break_coalescing();
        self.prune();
    }

    pub fn commit_redo(&mut self, mut current: PatternHistoryState<C>) {
        let Some(entry) = self.redo.pop_back() else {
            return;
        };
        self.weight = self.weight.saturating_sub(entry.weight);
        current.label.clone_from(&entry.state.label);
        self.push_undo(current);
        self.break_coalescing();
        self.prune();
    }

    pub fn capture_snapshot(&mut self, state: PatternHistoryState<C>) {
        self.snapshot = Some(PatternSnapshot {
            pattern_id: state.pattern_id,
            pattern: state.pattern,
            edit_context: state.edit_context,
            label: state.label,
        });
        self.break_coalescing();
    }

    pub fn snapshot(&self) -> Option<&PatternSnapshot<C>> {
        self.snapshot.as_ref()
    }

    pub fn prepare_recall(&self) -> Option<PatternSnapshot<C>> {
        self.snapshot.clone()
    }

    pub fn commit_recall(&mut self, mut current: PatternHistoryState<C>) {
        current.label = "RECALL".into();
        self.clear_redo();
        self.push_undo(current);
        self.break_coalescing();
        self.prune();
    }

    #[cfg(test)]
    pub fn depths(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }

    #[cfg(test)]
    pub fn retained_weight(&self) -> usize {
        self.weight
    }

    fn clear_redo(&mut self) {
        for entry in self.redo.drain(..) {
            self.weight = self.weight.saturating_sub(entry.weight);
        }
    }

    fn push_undo(&mut self, state: PatternHistoryState<C>) {
        let weight = pattern_structural_weight(&state.pattern);
        self.weight = self.weight.saturating_add(weight);
        self.undo.push_back(HistoryEntry { state, weight });
    }

    fn push_redo(&mut self, state: PatternHistoryState<C>) {
        let weight = pattern_structural_weight(&state.pattern);
        self.weight = self.weight.saturating_add(weight);
        self.redo.push_back(HistoryEntry { state, weight });
    }

    fn prune(&mut self) {
        while self.undo.len() + self.redo.len() > MAX_PATTERN_HISTORY_STATES
            || self.weight > MAX_PATTERN_HISTORY_WEIGHT
        {
            let removed = self.undo.pop_front().or_else(|| self.redo.pop_front());
            let Some(removed) = removed else {
                break;
            };
            self.weight = self.weight.saturating_sub(removed.weight);
        }
    }
}

pub fn pattern_structural_weight(pattern: &Pattern) -> usize {
    let cells = pattern.rows.iter().map(Vec::len).sum::<usize>();
    let points = pattern
        .automation
        .iter()
        .map(|lane| lane.points.len())
        .sum::<usize>();
    let page_structure = pattern.pages.len().saturating_mul(size_of::<Page>())
        + pattern
            .pages
            .iter()
            .map(|page| {
                page.name.len()
                    + page.lanes.len().saturating_mul(size_of::<Lane>())
                    + page.lanes.iter().map(|lane| lane.name.len()).sum::<usize>()
                    + page.setup.len()
                    + page.setup.iter().map(Vec::len).sum::<usize>()
                    + page.device_profile.as_ref().map_or(0, String::len)
                    + page_target_weight(&page.target)
            })
            .sum::<usize>();
    let automation_structure = pattern
        .automation
        .iter()
        .map(|lane| size_of::<AutomationLane>() + automation_target_weight(&lane.target))
        .sum::<usize>();
    let loop_paths = pattern
        .audio_loops
        .iter()
        .flatten()
        .map(|settings| size_of::<LoopSettings>() + settings.file.len())
        .sum::<usize>();
    cells
        .saturating_mul(size_of::<Cell>())
        .saturating_add(pattern.rows.len().saturating_mul(size_of::<Vec<Cell>>()))
        .saturating_add(points.saturating_mul(size_of::<AutomationPoint>()))
        .saturating_add(page_structure)
        .saturating_add(automation_structure)
        .saturating_add(loop_paths)
        .max(1)
}

fn page_target_weight(target: &PageTarget) -> usize {
    match target {
        PageTarget::Synthv1(name) | PageTarget::InternalDrums(name) | PageTarget::Midi(name) => {
            name.len()
        }
        PageTarget::Software(route) => route.instrument.len(),
        PageTarget::Default | PageTarget::ActiveInstrument | PageTarget::ConfiguredExternal => 0,
    }
}

fn automation_target_weight(target: &AutomationTarget) -> usize {
    match target {
        AutomationTarget::Instrument {
            engine, control, ..
        } => engine.len() + control.len(),
        AutomationTarget::Effect { parameter, .. } => parameter.len(),
        AutomationTarget::MidiCc { .. } | AutomationTarget::EffectBypass { .. } => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequencer::Song;

    fn state(pattern: &Pattern, label: &str) -> PatternHistoryState<usize> {
        PatternHistoryState {
            pattern_id: 0,
            pattern: pattern.clone(),
            edit_context: 7,
            label: label.into(),
        }
    }

    #[test]
    fn no_op_does_not_create_history() {
        let pattern = Song::new(&crate::config::ExternalMidiConfig::default())
            .patterns
            .remove(&0)
            .unwrap();
        let mut history = PatternHistory::<usize, usize>::default();
        assert!(!history.record_mutation(state(&pattern, "CELL"), &pattern, None));
        assert_eq!(history.depths(), (0, 0));
    }

    #[test]
    fn new_edit_after_undo_clears_redo() {
        let mut pattern = Song::new(&crate::config::ExternalMidiConfig::default())
            .patterns
            .remove(&0)
            .unwrap();
        let mut history = PatternHistory::<usize, usize>::default();
        let opening = state(&pattern, "CELL");
        pattern.rows[0][0].note = crate::sequencer::Note::On(60);
        assert!(history.record_mutation(opening, &pattern, None));
        let target = history.prepare_undo().unwrap();
        history.commit_undo(state(&pattern, &target.label));
        assert_eq!(history.depths(), (0, 1));
        let opening = state(&target.pattern, "TEMPO");
        pattern = target.pattern;
        pattern.tempo = crate::tempo::Bpm::from_whole(121).unwrap();
        assert!(history.record_mutation(opening, &pattern, None));
        assert_eq!(history.depths(), (1, 0));
    }

    #[test]
    fn one_gesture_coalesces_and_capacity_is_bounded() {
        let mut pattern = Song::new(&crate::config::ExternalMidiConfig::default())
            .patterns
            .remove(&0)
            .unwrap();
        let mut history = PatternHistory::<usize, usize>::default();
        for value in 1..=40 {
            let opening = state(&pattern, "TEMPO");
            pattern.rows[0][0].velocity = Some(value);
            history.record_mutation(opening, &pattern, Some(1));
        }
        assert_eq!(history.depths(), (1, 0));
        history.break_coalescing();
        for value in 1..=40 {
            let opening = state(&pattern, "CELL");
            pattern.rows[0][0].program = Some(value);
            history.record_mutation(opening, &pattern, None);
        }
        assert_eq!(history.depths().0, MAX_PATTERN_HISTORY_STATES);
        assert!(history.retained_weight() <= MAX_PATTERN_HISTORY_WEIGHT);
    }
}
