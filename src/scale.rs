//! Deterministic scale-constrained note mapping for FT2 N00B mode.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaleKind {
    Major,
    NaturalMinor,
}

impl ScaleKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Major => "MAJOR",
            Self::NaturalMinor => "MINOR",
        }
    }

    const fn intervals(self) -> &'static [u8] {
        match self {
            Self::Major => &[0, 2, 4, 5, 7, 9, 11],
            Self::NaturalMinor => &[0, 2, 3, 5, 7, 8, 10],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Scale {
    pub root: u8,
    pub kind: ScaleKind,
}

impl Default for Scale {
    fn default() -> Self {
        Self {
            root: 0,
            kind: ScaleKind::Major,
        }
    }
}

impl Scale {
    pub fn contains(self, note: u8) -> bool {
        let pitch = (12 + i16::from(note % 12) - i16::from(self.root % 12)) % 12;
        self.kind.intervals().contains(&(pitch as u8))
    }

    /// Maps to the nearest in-scale MIDI note. Exact distance ties choose the
    /// lower note, which keeps the result stable at every octave boundary.
    pub fn map(self, note: u8) -> u8 {
        if self.contains(note) {
            return note;
        }
        for distance in 1..=127u8 {
            if let Some(lower) = note.checked_sub(distance) {
                if self.contains(lower) {
                    return lower;
                }
            }
            if let Some(upper) = note.checked_add(distance).filter(|value| *value <= 127) {
                if self.contains(upper) {
                    return upper;
                }
            }
        }
        note
    }

    #[cfg(test)]
    pub fn notes(self) -> Vec<u8> {
        (0..=127).filter(|note| self.contains(*note)).collect()
    }
}

pub fn note_name(pitch_class: u8) -> &'static str {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    NAMES[usize::from(pitch_class % 12)]
}

/// Owns the output generated for each source channel/note. Stacks preserve
/// repeated note-ons; draining is used by mode changes, stop, panic, and exit.
#[derive(Clone, Debug)]
pub struct NoteLifecycle<T> {
    active: Vec<Vec<T>>,
}

impl<T> Default for NoteLifecycle<T> {
    fn default() -> Self {
        Self {
            active: (0..16 * 128).map(|_| Vec::new()).collect(),
        }
    }
}

impl<T> NoteLifecycle<T> {
    fn index(channel: u8, note: u8) -> usize {
        usize::from(channel.min(15)) * 128 + usize::from(note.min(127))
    }

    pub fn note_on(&mut self, channel: u8, note: u8, output: T) {
        self.active[Self::index(channel, note)].push(output);
    }

    pub fn note_off(&mut self, channel: u8, note: u8) -> Option<T> {
        self.active[Self::index(channel, note)].pop()
    }

    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.active.iter_mut().flat_map(|notes| notes.drain(..))
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.active.iter().map(Vec::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_generation_covers_midi_boundaries() {
        let d_sharp_minor = Scale {
            root: 3,
            kind: ScaleKind::NaturalMinor,
        };
        assert_eq!(
            d_sharp_minor
                .notes()
                .into_iter()
                .filter(|note| (60..72).contains(note))
                .collect::<Vec<_>>(),
            vec![61, 63, 65, 66, 68, 70, 71]
        );
        assert!(!d_sharp_minor.contains(0));
        assert!(!d_sharp_minor.contains(127));
        assert_eq!(d_sharp_minor.map(127), 126);
    }

    #[test]
    fn nearest_mapping_prefers_lower_on_ties_and_clamps_edges() {
        let c_major = Scale::default();
        assert_eq!(c_major.map(61), 60);
        assert_eq!(c_major.map(63), 62);
        assert_eq!(c_major.map(0), 0);
        assert_eq!(c_major.map(127), 127);
        let e_major = Scale {
            root: 4,
            kind: ScaleKind::Major,
        };
        assert_eq!(e_major.map(0), 1);
        assert_eq!(e_major.map(127), 126);
    }

    #[test]
    fn lifecycle_pairs_channels_repeats_velocity_zero_and_all_notes_off() {
        let mut notes = NoteLifecycle::default();
        notes.note_on(0, 61, 60);
        notes.note_on(0, 61, 60);
        notes.note_on(1, 61, 61);
        assert_eq!(notes.note_off(0, 61), Some(60));
        assert_eq!(notes.note_off(1, 61), Some(61));
        // A velocity-zero note-on calls the same note_off path.
        assert_eq!(notes.note_off(0, 61), Some(60));
        assert_eq!(notes.note_off(0, 61), None);
        notes.note_on(2, 63, 62);
        notes.note_on(2, 64, 64);
        assert_eq!(notes.drain().collect::<Vec<_>>(), vec![62, 64]);
        assert_eq!(notes.len(), 0);
    }

    #[test]
    fn mode_transition_drain_releases_original_mapping() {
        let minor = Scale {
            root: 3,
            kind: ScaleKind::NaturalMinor,
        };
        let mut notes = NoteLifecycle::default();
        notes.note_on(4, 64, minor.map(64));
        let major = Scale::default();
        assert_ne!(minor.map(64), major.map(64));
        assert_eq!(notes.drain().collect::<Vec<_>>(), vec![minor.map(64)]);
    }
}
