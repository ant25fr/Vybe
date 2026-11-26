//! Musical scales and the chords built on them, shared by the piano roll and
//! the MIDI effects.

use serde::{Deserialize, Serialize};

pub const ROOT_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ScaleKind {
    #[default]
    Major,
    Minor,
    HarmonicMinor,
    MelodicMinor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
    MajorPentatonic,
    MinorPentatonic,
    Blues,
}

impl ScaleKind {
    pub const ALL: [ScaleKind; 12] = [
        ScaleKind::Major,
        ScaleKind::Minor,
        ScaleKind::HarmonicMinor,
        ScaleKind::MelodicMinor,
        ScaleKind::Dorian,
        ScaleKind::Phrygian,
        ScaleKind::Lydian,
        ScaleKind::Mixolydian,
        ScaleKind::Locrian,
        ScaleKind::MajorPentatonic,
        ScaleKind::MinorPentatonic,
        ScaleKind::Blues,
    ];

    /// Semitones above the root of each degree, rising.
    pub fn intervals(self) -> &'static [u8] {
        match self {
            Self::Major => &[0, 2, 4, 5, 7, 9, 11],
            Self::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Self::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Self::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            Self::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Self::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Self::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Self::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Self::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            Self::MajorPentatonic => &[0, 2, 4, 7, 9],
            Self::MinorPentatonic => &[0, 3, 5, 7, 10],
            Self::Blues => &[0, 3, 5, 6, 7, 10],
        }
    }
}

impl std::fmt::Display for ScaleKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Major => "Major",
            Self::Minor => "Minor",
            Self::HarmonicMinor => "Harmonic minor",
            Self::MelodicMinor => "Melodic minor",
            Self::Dorian => "Dorian",
            Self::Phrygian => "Phrygian",
            Self::Lydian => "Lydian",
            Self::Mixolydian => "Mixolydian",
            Self::Locrian => "Locrian",
            Self::MajorPentatonic => "Major pentatonic",
            Self::MinorPentatonic => "Minor pentatonic",
            Self::Blues => "Blues",
        })
    }
}

/// A scale on a root pitch class, 0 being C.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Scale {
    pub root: u8,
    pub kind: ScaleKind,
}

impl Scale {
    /// Semitones of `pitch` above the root at or below it.
    fn degree_offset(self, pitch: u8) -> u8 {
        (pitch % 12 + 12 - self.root % 12) % 12
    }

    pub fn contains(self, pitch: u8) -> bool {
        self.kind.intervals().contains(&self.degree_offset(pitch))
    }

    pub fn is_root(self, pitch: u8) -> bool {
        self.degree_offset(pitch) == 0
    }

    /// The nearest pitch of the scale, the lower one when two are as near.
    pub fn snap(self, pitch: u8) -> u8 {
        let pitch = pitch.min(127);
        (0..=6)
            .flat_map(|distance| [pitch.checked_sub(distance), pitch.checked_add(distance)])
            .flatten()
            .find(|&candidate| candidate <= 127 && self.contains(candidate))
            .unwrap_or(pitch)
    }

    /// `pitch` moved by `degrees` of the scale, down when negative; it stays
    /// put at either end of the keyboard.
    pub fn shift(self, pitch: u8, degrees: i32) -> u8 {
        let mut pitch = self.snap(pitch);
        for _ in 0..degrees.unsigned_abs() {
            let next = if degrees > 0 {
                (pitch.saturating_add(1)..=127).find(|&p| p > pitch && self.contains(p))
            } else {
                (0..pitch).rev().find(|&p| self.contains(p))
            };
            pitch = next.unwrap_or(pitch);
        }
        pitch
    }

    /// `steps` degrees above `pitch`, which is snapped first; `None` past the
    /// top of the keyboard.
    pub fn step_up(self, pitch: u8, steps: usize) -> Option<u8> {
        let mut pitch = self.snap(pitch);
        for _ in 0..steps {
            pitch = (pitch.checked_add(1)?..=127).find(|&p| self.contains(p))?;
        }
        Some(pitch)
    }
}

/// The notes one played note stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ChordKind {
    #[default]
    Off,
    Octave,
    Fifth,
    Major,
    Minor,
    Sus4,
    Seventh,
    /// Thirds stacked on the scale; major without a scale.
    ScaleTriad,
    ScaleSeventh,
}

/// The most notes a chord holds.
pub const CHORD_NOTES: usize = 4;

impl ChordKind {
    pub const ALL: [ChordKind; 9] = [
        ChordKind::Off,
        ChordKind::Octave,
        ChordKind::Fifth,
        ChordKind::Major,
        ChordKind::Minor,
        ChordKind::Sus4,
        ChordKind::Seventh,
        ChordKind::ScaleTriad,
        ChordKind::ScaleSeventh,
    ];

    /// The chord on `pitch`, the played note first. Notes above the keyboard
    /// are left out.
    pub fn notes(self, pitch: u8, scale: Option<Scale>) -> ([u8; CHORD_NOTES], usize) {
        let mut notes = [pitch; CHORD_NOTES];
        let mut count = 1;
        let mut add = |note: Option<u8>| {
            if let Some(note) = note.filter(|&note| note <= 127) {
                if !notes[..count].contains(&note) {
                    notes[count] = note;
                    count += 1;
                }
            }
        };
        let fixed: &[u8] = match (self, scale) {
            (Self::Off, _) => &[],
            (Self::Octave, _) => &[12],
            (Self::Fifth, _) => &[7],
            (Self::Major, _) | (Self::ScaleTriad, None) => &[4, 7],
            (Self::Minor, _) => &[3, 7],
            (Self::Sus4, _) => &[5, 7],
            (Self::Seventh, _) | (Self::ScaleSeventh, None) => &[4, 7, 10],
            (Self::ScaleTriad, Some(scale)) => {
                add(scale.step_up(pitch, 2));
                add(scale.step_up(pitch, 4));
                &[]
            }
            (Self::ScaleSeventh, Some(scale)) => {
                add(scale.step_up(pitch, 2));
                add(scale.step_up(pitch, 4));
                add(scale.step_up(pitch, 6));
                &[]
            }
        };
        for &interval in fixed {
            add(pitch.checked_add(interval));
        }
        (notes, count)
    }
}

impl std::fmt::Display for ChordKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Off => "No chord",
            Self::Octave => "Octave",
            Self::Fifth => "Fifth",
            Self::Major => "Major",
            Self::Minor => "Minor",
            Self::Sus4 => "Sus4",
            Self::Seventh => "7th",
            Self::ScaleTriad => "Scale triad",
            Self::ScaleSeventh => "Scale 7th",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C_MAJOR: Scale = Scale {
        root: 0,
        kind: ScaleKind::Major,
    };

    #[test]
    fn pitches_snap_to_the_nearest_note_of_the_scale() {
        assert_eq!(C_MAJOR.snap(60), 60);
        // C sharp is as near C as D: the lower wins.
        assert_eq!(C_MAJOR.snap(61), 60);
        assert_eq!(C_MAJOR.snap(66), 65);
        assert_eq!(C_MAJOR.snap(127), 127);
    }

    #[test]
    fn scale_chords_follow_the_degree() {
        let chord = |pitch, kind: ChordKind| {
            let (notes, count) = kind.notes(pitch, Some(C_MAJOR));
            notes[..count].to_vec()
        };
        assert_eq!(chord(60, ChordKind::ScaleTriad), [60, 64, 67]);
        // D is a minor chord in C major.
        assert_eq!(chord(62, ChordKind::ScaleTriad), [62, 65, 69]);
        assert_eq!(chord(67, ChordKind::ScaleSeventh), [67, 71, 74, 77]);
        assert_eq!(chord(120, ChordKind::Seventh), [120, 124, 127]);
    }
}
