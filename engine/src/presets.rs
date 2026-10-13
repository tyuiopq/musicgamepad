//! Встроенные профили.

use crate::model::*;

fn button(button: Button) -> Input {
    Input::Button { button }
}

fn note(input: Input, degree: i32) -> Binding {
    Binding { input, action: Action::Note { note: NoteSpec::Degree { degree }, part: 0, velocity: 100 } }
}

fn chord(input: Input, degree: i32) -> Binding {
    Binding {
        input,
        action: Action::Note { note: NoteSpec::Chord { degree, chord: ChordKind::Triad }, part: 0, velocity: 90 },
    }
}

fn modifier(input: Input, effect: ModEffect, latch: Latch) -> Binding {
    Binding { input, action: Action::Modifier { effect, latch } }
}

fn command(input: Input, command: Command) -> Binding {
    Binding { input, action: Action::Command { command } }
}

fn control(axis: Axis, target: ControlTarget, shape: AxisShape) -> Binding {
    Binding { input: Input::Axis { axis }, action: Action::Control { target, shape, part: 0 } }
}

/// Ступени по кнопкам: крестовина — нижние, четыре кнопки — верхние (как в прототипе).
const DEGREE_BUTTONS: [(Button, i32); 8] = [
    (Button::DpadLeft, 0),
    (Button::DpadDown, 1),
    (Button::DpadRight, 2),
    (Button::DpadUp, 3),
    (Button::West, 4),
    (Button::South, 5),
    (Button::East, 6),
    (Button::North, 7),
];

/// Раскладка прототипа + слой аккордов на нажатии левого стика.
pub fn classic() -> Profile {
    let mut base: Vec<Binding> = DEGREE_BUTTONS.iter().map(|&(b, d)| note(button(b), d)).collect();
    base.extend([
        modifier(button(Button::LeftShoulder), ModEffect::Octaves { octaves: -1 }, Latch::Hold),
        modifier(button(Button::RightShoulder), ModEffect::Octaves { octaves: 1 }, Latch::Hold),
        modifier(
            Input::AxisSwitch { axis: Axis::LeftTrigger, direction: Direction::Positive, threshold: 0.5 },
            ModEffect::Semitones { semitones: 1 },
            Latch::Hold,
        ),
        modifier(button(Button::LeftStick), ModEffect::Layer { layer: "chords".into() }, Latch::Toggle),
        command(button(Button::Back), Command::NextInstrument { part: 0 }),
        command(button(Button::Start), Command::ToggleRecord),
        control(
            Axis::RightTrigger,
            ControlTarget::Cc { cc: 11, min: 40, max: 127 },
            AxisShape { deadzone: 0.02, mode: AxisMode::Positive, ..Default::default() },
        ),
        control(Axis::RightX, ControlTarget::PitchBend, AxisShape::default()),
        control(
            Axis::RightY,
            ControlTarget::Cc { cc: 1, min: 0, max: 127 },
            AxisShape { mode: AxisMode::Absolute, ..Default::default() },
        ),
    ]);

    let chords = DEGREE_BUTTONS.iter().map(|&(b, d)| chord(button(b), d)).collect();

    Profile {
        name: "Классика".into(),
        music: Music { root: 0, octave: 4, scale: Scale::Preset { preset: ScalePreset::Major } },
        parts: vec![Part {
            name: "Мелодия".into(),
            channel: 0,
            program: 0,
            bank: 0,
            volume: 100,
            bend_range: 2,
            instruments: vec![0, 11, 24, 40, 56, 73, 79, 80],
        }],
        layers: vec![
            Layer { id: "base".into(), name: "Основной".into(), bindings: base },
            Layer { id: "chords".into(), name: "Аккорды".into(), bindings: chords },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_is_valid_and_roundtrips() {
        let p = classic();
        p.validate().unwrap();
        let back = Profile::from_json(&p.to_json()).unwrap();
        assert_eq!(back, p);
    }
}
