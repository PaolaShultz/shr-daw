//! Read-only harmony relationships derived from the current Project key.

use crate::chord::NoteNaming;
use crate::scale::{Scale, ScaleKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HarmonyKey {
    pub root: u8,
    pub kind: ScaleKind,
}

impl HarmonyKey {
    fn new(root: u8, kind: ScaleKind) -> Self {
        Self {
            root: root % 12,
            kind,
        }
    }

    pub fn long_label(self, naming: NoteNaming) -> String {
        format!("{} {}", naming.pitch_name(self.root), self.kind.label())
    }

    pub fn compact_label(self, naming: NoteNaming) -> String {
        format!(
            "{}{}",
            naming.pitch_name(self.root),
            if self.kind == ScaleKind::NaturalMinor {
                "m"
            } else {
                ""
            }
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriadQuality {
    Major,
    Minor,
    Diminished,
}

impl TriadQuality {
    const fn suffix(self) -> &'static str {
        match self {
            Self::Major => "",
            Self::Minor => "m",
            Self::Diminished => "dim",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiatonicTriad {
    pub degree: &'static str,
    pub root: u8,
    pub quality: TriadQuality,
}

impl DiatonicTriad {
    pub fn label(self, naming: NoteNaming) -> String {
        format!(
            "{} {}{}",
            self.degree,
            naming.pitch_name(self.root),
            self.quality.suffix()
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Harmony {
    pub current: HarmonyKey,
    pub counter_clockwise: HarmonyKey,
    pub clockwise: HarmonyKey,
    pub relative: HarmonyKey,
    pub parallel: HarmonyKey,
    pub triads: [DiatonicTriad; 7],
}

impl Harmony {
    pub fn from_scale(scale: Scale) -> Self {
        let root = scale.root % 12;
        let current = HarmonyKey::new(root, scale.kind);
        let (relative, parallel, intervals, degrees, qualities) = match scale.kind {
            ScaleKind::Major => (
                HarmonyKey::new(root + 9, ScaleKind::NaturalMinor),
                HarmonyKey::new(root, ScaleKind::NaturalMinor),
                [0, 2, 4, 5, 7, 9, 11],
                ["I", "ii", "iii", "IV", "V", "vi", "vii°"],
                [
                    TriadQuality::Major,
                    TriadQuality::Minor,
                    TriadQuality::Minor,
                    TriadQuality::Major,
                    TriadQuality::Major,
                    TriadQuality::Minor,
                    TriadQuality::Diminished,
                ],
            ),
            ScaleKind::NaturalMinor => (
                HarmonyKey::new(root + 3, ScaleKind::Major),
                HarmonyKey::new(root, ScaleKind::Major),
                [0, 2, 3, 5, 7, 8, 10],
                ["i", "ii°", "III", "iv", "v", "VI", "VII"],
                [
                    TriadQuality::Minor,
                    TriadQuality::Diminished,
                    TriadQuality::Major,
                    TriadQuality::Minor,
                    TriadQuality::Minor,
                    TriadQuality::Major,
                    TriadQuality::Major,
                ],
            ),
        };
        Self {
            current,
            counter_clockwise: HarmonyKey::new(root + 5, scale.kind),
            clockwise: HarmonyKey::new(root + 7, scale.kind),
            relative,
            parallel,
            triads: std::array::from_fn(|index| DiatonicTriad {
                degree: degrees[index],
                root: (root + intervals[index]) % 12,
                quality: qualities[index],
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_every_supported_key_and_mode_without_duplicate_scale_degrees() {
        for root in 0..12 {
            for kind in [ScaleKind::Major, ScaleKind::NaturalMinor] {
                let scale = Scale { root, kind };
                let harmony = Harmony::from_scale(scale);
                assert_eq!(harmony.current, HarmonyKey { root, kind });
                assert_eq!(harmony.counter_clockwise.root, (root + 5) % 12);
                assert_eq!(harmony.clockwise.root, (root + 7) % 12);
                assert_eq!(harmony.counter_clockwise.kind, kind);
                assert_eq!(harmony.clockwise.kind, kind);
                assert_eq!(harmony.parallel.root, root);
                assert_ne!(harmony.parallel.kind, kind);
                assert_eq!(harmony.triads.len(), 7);

                let mut triad_roots = harmony
                    .triads
                    .iter()
                    .map(|triad| triad.root)
                    .collect::<Vec<_>>();
                triad_roots.sort_unstable();
                triad_roots.dedup();
                assert_eq!(triad_roots.len(), 7);
                assert!(harmony
                    .triads
                    .iter()
                    .all(|triad| scale.contains(triad.root)));
            }
        }
    }

    #[test]
    fn c_major_and_a_minor_have_the_expected_relationships_and_triads() {
        let c_major = Harmony::from_scale(Scale::default());
        assert_eq!(c_major.counter_clockwise.root, 5);
        assert_eq!(c_major.clockwise.root, 7);
        assert_eq!(
            c_major.relative,
            HarmonyKey::new(9, ScaleKind::NaturalMinor)
        );
        assert_eq!(
            c_major.parallel,
            HarmonyKey::new(0, ScaleKind::NaturalMinor)
        );
        assert_eq!(
            c_major.triads.map(|triad| triad.label(NoteNaming::English)),
            [
                "I C",
                "ii Dm",
                "iii Em",
                "IV F",
                "V G",
                "vi Am",
                "vii° Bdim"
            ]
        );

        let a_minor = Harmony::from_scale(Scale {
            root: 9,
            kind: ScaleKind::NaturalMinor,
        });
        assert_eq!(a_minor.relative, HarmonyKey::new(0, ScaleKind::Major));
        assert_eq!(a_minor.parallel, HarmonyKey::new(9, ScaleKind::Major));
        assert_eq!(
            a_minor.triads.map(|triad| triad.label(NoteNaming::English)),
            [
                "i Am",
                "ii° Bdim",
                "III C",
                "iv Dm",
                "v Em",
                "VI F",
                "VII G"
            ]
        );
    }

    #[test]
    fn labels_reuse_the_repository_english_and_german_sharp_policy() {
        let english = Harmony::from_scale(Scale {
            root: 10,
            kind: ScaleKind::Major,
        });
        assert_eq!(english.current.long_label(NoteNaming::English), "A# MAJOR");
        assert_eq!(english.current.long_label(NoteNaming::German), "B MAJOR");

        let german_b = Harmony::from_scale(Scale {
            root: 11,
            kind: ScaleKind::NaturalMinor,
        });
        assert_eq!(german_b.current.compact_label(NoteNaming::English), "Bm");
        assert_eq!(german_b.current.compact_label(NoteNaming::German), "Hm");

        let sharp = Harmony::from_scale(Scale {
            root: 1,
            kind: ScaleKind::Major,
        });
        assert_eq!(sharp.current.long_label(NoteNaming::English), "C# MAJOR");
        assert_eq!(sharp.parallel.long_label(NoteNaming::German), "C# MINOR");
    }
}
