//! Лады, ступени и аккорды.

use crate::model::{ChordKind, Music, Scale, ScalePreset};

pub const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

pub fn note_name(note: u8) -> String {
    format!("{}{}", NOTE_NAMES[note as usize % 12], note as i32 / 12 - 1)
}

impl ScalePreset {
    pub fn steps(self) -> &'static [u8] {
        match self {
            ScalePreset::Major => &[0, 2, 4, 5, 7, 9, 11],
            ScalePreset::NaturalMinor => &[0, 2, 3, 5, 7, 8, 10],
            ScalePreset::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            ScalePreset::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            ScalePreset::MajorPentatonic => &[0, 2, 4, 7, 9],
            ScalePreset::MinorPentatonic => &[0, 3, 5, 7, 10],
            ScalePreset::Blues => &[0, 3, 5, 6, 7, 10],
            ScalePreset::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            ScalePreset::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            ScalePreset::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            ScalePreset::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            ScalePreset::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            ScalePreset::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        }
    }
}

impl Scale {
    /// Интервалы лада; пустой или кривой свой лад заменяется мажором.
    pub fn steps(&self) -> Vec<i32> {
        let steps: &[u8] = match self {
            Scale::Preset { preset } => preset.steps(),
            Scale::Custom { steps } => steps,
        };
        let mut v: Vec<i32> = steps.iter().map(|&s| s as i32 % 12).collect();
        v.sort_unstable();
        v.dedup();
        if v.is_empty() {
            return ScalePreset::Major.steps().iter().map(|&s| s as i32).collect();
        }
        v
    }
}

impl Music {
    /// MIDI-нота тоники для ступени 0.
    pub fn tonic(&self) -> i32 {
        (self.octave + 1) * 12 + self.root as i32
    }

    /// Нота ступени лада. Ступени за пределами лада переходят в соседние октавы:
    /// в мажоре ступень 7 — тоника октавой выше, −1 — си октавой ниже.
    pub fn degree_note(&self, degree: i32) -> i32 {
        let steps = self.scale.steps();
        let n = steps.len() as i32;
        self.tonic() + steps[degree.rem_euclid(n) as usize] + 12 * degree.div_euclid(n)
    }
}

impl ChordKind {
    /// Ступени аккорда относительно основной.
    pub fn degrees(self) -> &'static [i32] {
        match self {
            ChordKind::Triad => &[0, 2, 4],
            ChordKind::Seventh => &[0, 2, 4, 6],
            ChordKind::Power => &[0, 4, 7],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn music(preset: ScalePreset) -> Music {
        Music { root: 0, octave: 4, scale: Scale::Preset { preset } }
    }

    #[test]
    fn major_degrees() {
        let m = music(ScalePreset::Major);
        let notes: Vec<i32> = (0..8).map(|d| m.degree_note(d)).collect();
        assert_eq!(notes, [60, 62, 64, 65, 67, 69, 71, 72]);
        assert_eq!(m.degree_note(-1), 59);
        assert_eq!(m.degree_note(14), 84);
    }

    #[test]
    fn pentatonic_wraps_every_five() {
        let m = music(ScalePreset::MinorPentatonic);
        assert_eq!(m.degree_note(5), 72);
        assert_eq!(m.degree_note(6), 75);
    }

    #[test]
    fn custom_scale_is_sanitized() {
        let s = Scale::Custom { steps: vec![7, 0, 12, 3] };
        assert_eq!(s.steps(), [0, 3, 7]);
        assert_eq!(Scale::Custom { steps: vec![] }.steps().len(), 7);
    }

    #[test]
    fn names() {
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(69), "A4");
    }
}
