//! Bounded, non-writing Arrangement templates.
//!
//! The assistant owns only an inspectable list of existing Pattern references.
//! `Song` remains the transaction owner for APPEND and REPLACE.

use crate::sequencer::Song;
use anyhow::{bail, Result};

pub const AABA_STEPS: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Section {
    A,
    B,
}

impl Section {
    pub const fn label(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftReference {
    pub section: Section,
    pub pattern: Option<u16>,
    pub rows: Option<usize>,
    pub issue: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArrangementDraft {
    pub references: Vec<DraftReference>,
    pub total_rows: Option<usize>,
}

impl ArrangementDraft {
    pub fn aaba(song: &Song, a: u16, b: Option<u16>) -> Self {
        let requested = [
            (Section::A, Some(a)),
            (Section::A, Some(a)),
            (Section::B, b),
            (Section::A, Some(a)),
        ];
        let references = requested
            .into_iter()
            .map(|(section, pattern)| match pattern {
                Some(pattern) => match song.arrangement_pattern_rows(pattern) {
                    Ok(rows) => DraftReference {
                        section,
                        pattern: Some(pattern),
                        rows: Some(rows),
                        issue: None,
                    },
                    Err(error) => DraftReference {
                        section,
                        pattern: Some(pattern),
                        rows: None,
                        issue: Some(error.to_string()),
                    },
                },
                None => DraftReference {
                    section,
                    pattern: None,
                    rows: None,
                    issue: Some("B Pattern is not selected".into()),
                },
            })
            .collect::<Vec<_>>();
        let total_rows = references.iter().try_fold(0usize, |total, reference| {
            total.checked_add(reference.rows?)
        });
        Self {
            references,
            total_rows,
        }
    }

    pub fn pattern_references(&self) -> Result<Vec<u16>> {
        if self.references.len() != AABA_STEPS {
            bail!("A A B A draft must contain exactly {AABA_STEPS} steps");
        }
        self.references
            .iter()
            .map(|reference| {
                if let Some(issue) = reference.issue.as_deref() {
                    bail!(issue.to_owned());
                }
                reference
                    .pattern
                    .ok_or_else(|| anyhow::anyhow!("Pattern is missing"))
            })
            .collect()
    }

    pub fn first_issue(&self) -> Option<&str> {
        self.references
            .iter()
            .find_map(|reference| reference.issue.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RuntimeConfig;
    use crate::sequencer::{Pattern, MAX_ARRANGEMENT_STEPS};

    fn song_with_two_patterns() -> Song {
        let config = RuntimeConfig::default().external_midi;
        let mut song = Song::new(&config);
        let pattern = Pattern::empty_like_setup(24, &song.patterns[&0]);
        song.append_pattern(pattern).unwrap();
        song.order = vec![0];
        song
    }

    #[test]
    fn aaba_draft_lists_every_reference_and_exact_total_without_writing() {
        let song = song_with_two_patterns();
        let before = song.clone();

        let draft = ArrangementDraft::aaba(&song, 0, Some(1));

        assert_eq!(song, before);
        assert_eq!(draft.references.len(), AABA_STEPS);
        assert_eq!(
            draft
                .references
                .iter()
                .map(|reference| (reference.section, reference.pattern, reference.rows))
                .collect::<Vec<_>>(),
            [
                (Section::A, Some(0), Some(64)),
                (Section::A, Some(0), Some(64)),
                (Section::B, Some(1), Some(24)),
                (Section::A, Some(0), Some(64)),
            ]
        );
        assert_eq!(draft.total_rows, Some(216));
        assert_eq!(draft.pattern_references().unwrap(), [0, 0, 1, 0]);
    }

    #[test]
    fn omitted_missing_and_invalid_patterns_stay_visible_and_cannot_commit() {
        let mut song = song_with_two_patterns();
        let unselected = ArrangementDraft::aaba(&song, 0, None);
        assert_eq!(unselected.total_rows, None);
        assert_eq!(unselected.references[2].pattern, None);
        assert!(unselected.first_issue().unwrap().contains("not selected"));
        assert!(unselected.pattern_references().is_err());

        let missing = ArrangementDraft::aaba(&song, 0, Some(99));
        assert_eq!(missing.references[2].pattern, Some(99));
        assert!(missing.first_issue().unwrap().contains("missing"));
        assert!(missing.pattern_references().is_err());

        song.patterns.get_mut(&1).unwrap().rows.clear();
        let invalid = ArrangementDraft::aaba(&song, 0, Some(1));
        assert!(invalid.first_issue().unwrap().contains("invalid"));
        assert!(invalid.pattern_references().is_err());
    }

    #[test]
    fn song_append_and_replace_are_atomic_order_only_transactions() {
        let mut song = song_with_two_patterns();
        let patterns = song.patterns.clone();

        let first = song.append_arrangement(&[0, 0, 1, 0]).unwrap();
        assert_eq!(first, 1);
        assert_eq!(song.order, [0, 0, 0, 1, 0]);
        assert_eq!(song.patterns, patterns);

        song.replace_arrangement(vec![0, 0, 1, 0]).unwrap();
        assert_eq!(song.order, [0, 0, 1, 0]);
        assert_eq!(song.patterns, patterns);
    }

    #[test]
    fn missing_and_bounds_failures_leave_the_exact_order_untouched() {
        let mut song = song_with_two_patterns();
        let before = song.order.clone();
        assert!(song.append_arrangement(&[99]).is_err());
        assert_eq!(song.order, before);
        assert!(song.replace_arrangement(vec![0, 99]).is_err());
        assert_eq!(song.order, before);

        song.order = vec![0; MAX_ARRANGEMENT_STEPS - 2];
        let before = song.order.clone();
        assert!(song.append_arrangement(&[0, 0, 1, 0]).is_err());
        assert_eq!(song.order, before);

        let mut invalid_project = song_with_two_patterns();
        let mut unrelated = Pattern::empty_like_setup(16, &invalid_project.patterns[&0]);
        unrelated.rows.clear();
        invalid_project.patterns.insert(2, unrelated);
        let before = invalid_project.order.clone();
        assert!(invalid_project.append_arrangement(&[0]).is_err());
        assert_eq!(invalid_project.order, before);
        assert!(invalid_project.replace_arrangement(vec![0]).is_err());
        assert_eq!(invalid_project.order, before);
    }
}
