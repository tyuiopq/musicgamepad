//! Встроенные профили.

use crate::model::*;

fn button(button: Button) -> Input {
    Input::Button { button }
}

fn note(input: Input, degree: i32) -> Binding {
    Binding { input, action: Action::Note { note: NoteSpec::Degree { degree }, part: 0, velocity: 100, latch: Latch::Hold, length_ms: 0, velocity_from: VelocitySource::Fixed } }
}

fn chord(input: Input, degree: i32) -> Binding {
    Binding {
        input,
        action: Action::Note {
            note: NoteSpec::Chord { degree, chord: ChordKind::Triad, intervals: vec![], inversion: 0, open: false, bass: false },
            part: 0,
            velocity: 90,
            latch: Latch::Hold, length_ms: 0, velocity_from: VelocitySource::Fixed,
        },
    }
}

fn modifier(input: Input, effect: ModEffect, latch: Latch) -> Binding {
    Binding { input, action: Action::Modifier { effect, latch } }
}

fn command(input: Input, command: Command) -> Binding {
    Binding { input, action: Action::Command { command } }
}

fn control(axis: Axis, target: ControlTarget, shape: AxisShape) -> Binding {
    Binding { input: Input::Axis { axis }, action: Action::Control { target, shape, part: 0, takeover: false, relative: false } }
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
            transpose: 0,
            sound: Sound::default(),
            source: PartSource::Default,
        }],
        layers: vec![
            Layer { id: "base".into(), name: "Основной".into(), bindings: base, combo: vec![], transpose: 0 },
            Layer { id: "chords".into(), name: "Аккорды".into(), bindings: chords, combo: vec![], transpose: 0 },
        ],
        meta: ProfileMeta {
            category: "builtin".into(),
            target: "Встроенный звук".into(),
            description: "Раскладка прототипа: 8 нот до мажора на крестовине и кнопках, октавы на плечах, полутон на LT, громкость на RT, подтяжка и вибрато на правом стике. Нажатие левого стика — аккорды, Back — следующий инструмент, Start — запись.".into(),
            setup: vec![],
        },
    }
}

// ---------- библиотека профилей ----------

fn absolute(input: Input, note: u8, velocity: u8) -> Binding {
    Binding { input, action: Action::Note { note: NoteSpec::Absolute { note }, part: 0, velocity, latch: Latch::Hold, length_ms: 0, velocity_from: VelocitySource::Fixed } }
}

fn control_takeover(axis: Axis, target: ControlTarget, shape: AxisShape) -> Binding {
    Binding { input: Input::Axis { axis }, action: Action::Control { target, shape, part: 0, takeover: true, relative: false } }
}

fn cc_button(input: Input, cc: u8, latch: Latch) -> Binding {
    Binding { input, action: Action::Cc { cc, part: 0, on: 127, off: 0, latch, step: 0 } }
}

fn trigger_switch(axis: Axis) -> Input {
    Input::AxisSwitch { axis, direction: Direction::Positive, threshold: 0.5 }
}

fn trigger(mode: AxisMode) -> AxisShape {
    AxisShape { deadzone: 0.02, mode, ..Default::default() }
}

fn part(name: &str, channel: u8, program: u8, bank: u16, instruments: Vec<u8>) -> Part {
    Part {
        name: name.into(),
        channel,
        program,
        bank,
        volume: 100,
        bend_range: 2,
        instruments,
        transpose: 0,
        sound: Sound::default(),
        source: PartSource::Default,
    }
}

fn base_layer(bindings: Vec<Binding>) -> Layer {
    Layer { id: "base".into(), name: "Основной".into(), bindings, combo: vec![], transpose: 0 }
}

fn step(text: &str) -> SetupStep {
    SetupStep { text: text.into(), learn: None }
}

fn learn_step(text: &str, input: Input) -> SetupStep {
    SetupStep { text: text.into(), learn: Some(input) }
}

/// 8 нот гаммы на крестовине и кнопках, как в прототипе.
fn scale_notes() -> Vec<Binding> {
    DEGREE_BUTTONS.iter().map(|&(b, d)| note(button(b), d)).collect()
}

/// Свободные номера CC (85–87 в стандарте MIDI не заняты) — для Play/Record/… через MIDI Map в DAW.
const CC_PLAY: u8 = 85;
const CC_RECORD: u8 = 86;
const CC_EXTRA: u8 = 87;

fn loopmidi_steps() -> Vec<SetupStep> {
    vec![
        step("Установи бесплатную программу loopMIDI и создай в ней порт (например, «loopMIDI Port»). Это «провод» от MusicGamepad к DAW."),
        step("Здесь, в «Настройках»: выбери MIDI-выход «loopMIDI Port» и выключи «Встроенный синтезатор», чтобы звук не двоился."),
    ]
}

/// Импровизация: минорная пентатоника от «ля», гитара с перегрузом.
pub fn pentatonic() -> Profile {
    let mut base = scale_notes();
    base.extend([
        modifier(button(Button::LeftShoulder), ModEffect::Octaves { octaves: -1 }, Latch::Hold),
        modifier(button(Button::RightShoulder), ModEffect::Octaves { octaves: 1 }, Latch::Hold),
        modifier(trigger_switch(Axis::LeftTrigger), ModEffect::Sustain, Latch::Hold),
        command(button(Button::Back), Command::NextInstrument { part: 0 }),
        command(button(Button::Start), Command::ToggleRecord),
        control(Axis::RightTrigger, ControlTarget::Cc { cc: 11, min: 50, max: 127 }, trigger(AxisMode::Positive)),
        control(Axis::RightX, ControlTarget::PitchBend, AxisShape::default()),
        control(Axis::RightY, ControlTarget::Cc { cc: 1, min: 0, max: 127 }, AxisShape { mode: AxisMode::Absolute, ..Default::default() }),
    ]);
    Profile {
        name: "Импровизация: пентатоника".into(),
        music: Music { root: 9, octave: 3, scale: Scale::Preset { preset: ScalePreset::MinorPentatonic } },
        parts: vec![part("Гитара", 0, 29, 0, vec![29, 30, 26, 81])],
        layers: vec![base_layer(base)],
        meta: ProfileMeta {
            category: "builtin".into(),
            target: "Встроенный звук".into(),
            description: "Минорная пентатоника от «ля»: любая кнопка звучит «в тему» — удобно импровизировать под блюз и рок. LT — педаль, подтяжка на правом стике.".into(),
            setup: vec![],
        },
    }
}

/// Барабаны на встроенном звуке: стандартная раскладка General MIDI.
pub fn drums() -> Profile {
    let base = vec![
        absolute(button(Button::South), 36, 110),
        absolute(button(Button::East), 38, 110),
        absolute(button(Button::West), 42, 100),
        absolute(button(Button::North), 46, 100),
        absolute(button(Button::DpadRight), 39, 110),
        absolute(button(Button::DpadUp), 50, 105),
        absolute(button(Button::DpadLeft), 47, 105),
        absolute(button(Button::DpadDown), 43, 105),
        absolute(button(Button::LeftShoulder), 49, 110),
        absolute(button(Button::RightShoulder), 51, 100),
        absolute(trigger_switch(Axis::LeftTrigger), 44, 90),
        absolute(button(Button::RightStick), 37, 100),
        command(button(Button::Back), Command::NextInstrument { part: 0 }),
        command(button(Button::Start), Command::ToggleRecord),
        control(Axis::RightTrigger, ControlTarget::Velocity { min: 50, max: 127 }, trigger(AxisMode::Positive)),
    ];
    Profile {
        name: "Барабаны".into(),
        music: Music { root: 0, octave: 4, scale: Scale::Preset { preset: ScalePreset::Major } },
        // Ударные установки GM/GS: Standard, Room, Power, Electronic, TR-808, Jazz, Brush
        parts: vec![part("Ударные", 9, 0, 128, vec![0, 8, 16, 24, 25, 32, 40])],
        layers: vec![base_layer(base)],
        meta: ProfileMeta {
            category: "builtin".into(),
            target: "Встроенный звук".into(),
            description: "A — бочка, B — малый, X/Y — хэт закрытый/открытый, крестовина — томы и хлопок, плечи — тарелки, LT — хэт педалью. Чем сильнее зажат RT, тем громче удары. Back — другая установка.".into(),
            setup: vec![],
        },
    }
}

/// Общая часть клавишных профилей для DAW.
fn daw_keys_bindings() -> Vec<Binding> {
    let mut base = scale_notes();
    base.extend([
        modifier(button(Button::LeftShoulder), ModEffect::Octaves { octaves: -1 }, Latch::Hold),
        modifier(button(Button::RightShoulder), ModEffect::Octaves { octaves: 1 }, Latch::Hold),
        modifier(trigger_switch(Axis::LeftTrigger), ModEffect::Sustain, Latch::Hold),
        modifier(button(Button::RightStick), ModEffect::Layer { layer: "chords".into() }, Latch::Toggle),
        control(Axis::RightTrigger, ControlTarget::Velocity { min: 40, max: 127 }, trigger(AxisMode::Positive)),
        control(Axis::RightX, ControlTarget::PitchBend, AxisShape::default()),
        control(Axis::RightY, ControlTarget::Cc { cc: 1, min: 0, max: 127 }, AxisShape { mode: AxisMode::Absolute, ..Default::default() }),
        control_takeover(Axis::LeftX, ControlTarget::Cc { cc: 74, min: 0, max: 127 }, AxisShape { smoothing: 20.0, ..Default::default() }),
        control_takeover(Axis::LeftY, ControlTarget::Cc { cc: 71, min: 0, max: 127 }, AxisShape { smoothing: 20.0, ..Default::default() }),
        cc_button(button(Button::Back), CC_PLAY, Latch::Hold),
        cc_button(button(Button::Start), CC_RECORD, Latch::Hold),
        cc_button(button(Button::LeftStick), CC_EXTRA, Latch::Toggle),
    ]);
    base
}

fn chords_layer() -> Layer {
    Layer {
        id: "chords".into(),
        name: "Аккорды".into(),
        bindings: DEGREE_BUTTONS.iter().map(|&(b, d)| chord(button(b), d)).collect(),
        combo: vec![],
        transpose: 0,
    }
}

const KEYS_DESCRIPTION: &str = "Ноты — крестовина и кнопки, октавы — плечи, LT — педаль, RT — сила нажатия. Правый стик: подтяжка и Mod Wheel, его нажатие — аккорды. Левый стик — две ручки (CC74, CC71) для MIDI Learn, с подхватом. Back/Start — CC 85/86 для Play и Record, нажатие левого стика — CC 87.";

pub fn ableton_keys() -> Profile {
    let mut setup = loopmidi_steps();
    setup.extend([
        step("В Ableton: Options → Preferences → Link, Tempo & MIDI. У входа «loopMIDI Port» включи Track и Remote."),
        step("Создай MIDI-трек с инструментом и включи у него запись (Arm) — уже можно играть. В Ableton средняя «до» называется C3: это та же нота, что C4 здесь."),
        learn_step("Play: нажми Ctrl+M, кликни кнопку Play в Ableton и нажми Back на геймпаде.", button(Button::Back)),
        learn_step("Record: в том же режиме кликни Record и нажми Start.", button(Button::Start)),
        learn_step("Ручки: в режиме Ctrl+M кликни макрос или ручку инструмента и наклони левый стик влево-вправо…", Input::Axis { axis: Axis::LeftX }),
        learn_step("…а вторую ручку — наклоном вверх-вниз. Ctrl+M ещё раз — выйти из режима.", Input::Axis { axis: Axis::LeftY }),
        step("По желанию — темп: в «Запись → ⚙» включи «MIDI Clock в DAW», а в Ableton у входа loopMIDI — Sync. Тогда метроном здесь ведёт темп Ableton."),
    ]);
    Profile {
        name: "Ableton Live: клавиши".into(),
        music: Music { root: 0, octave: 4, scale: Scale::Preset { preset: ScalePreset::Major } },
        parts: vec![part("В DAW", 0, 0, 0, vec![])],
        layers: vec![base_layer(daw_keys_bindings()), chords_layer()],
        meta: ProfileMeta {
            category: "daw".into(),
            target: "Ableton Live".into(),
            description: KEYS_DESCRIPTION.into(),
            setup,
        },
    }
}

pub fn fl_keys() -> Profile {
    let mut setup = loopmidi_steps();
    setup.extend([
        step("В FL Studio: Options → MIDI settings. В списке Input выбери «loopMIDI Port» и включи Enable."),
        step("Выбери канал с инструментом в Channel Rack — играй. В FL средняя «до» называется C5: это та же нота, что C4 здесь."),
        learn_step("Play: правый клик по кнопке Play → Link to controller… → нажми Back.", button(Button::Back)),
        learn_step("Record: правый клик по Record → Link to controller… → нажми Start.", button(Button::Start)),
        learn_step("Ручки: правый клик по ручке → Link to controller… → наклони левый стик влево-вправо…", Input::Axis { axis: Axis::LeftX }),
        learn_step("…и для второй ручки — вверх-вниз.", Input::Axis { axis: Axis::LeftY }),
    ]);
    Profile {
        name: "FL Studio: клавиши".into(),
        music: Music { root: 0, octave: 4, scale: Scale::Preset { preset: ScalePreset::Major } },
        parts: vec![part("В DAW", 0, 0, 0, vec![])],
        layers: vec![base_layer(daw_keys_bindings()), chords_layer()],
        meta: ProfileMeta { category: "daw".into(), target: "FL Studio".into(), description: KEYS_DESCRIPTION.into(), setup },
    }
}

pub fn ableton_drums() -> Profile {
    // Пэды Drum Rack — ноты 36…51 (в Ableton C1…D#2), снизу слева
    let pads = [
        (button(Button::DpadLeft), 36),
        (button(Button::DpadDown), 37),
        (button(Button::DpadRight), 38),
        (button(Button::DpadUp), 39),
        (button(Button::West), 40),
        (button(Button::South), 41),
        (button(Button::East), 42),
        (button(Button::North), 43),
        (button(Button::LeftShoulder), 44),
        (button(Button::RightShoulder), 45),
        (trigger_switch(Axis::LeftTrigger), 46),
        (button(Button::LeftStick), 47),
        (button(Button::RightStick), 48),
    ];
    let mut base: Vec<Binding> = pads.iter().map(|&(input, n)| absolute(input, n, 110)).collect();
    base.extend([
        control(Axis::RightTrigger, ControlTarget::Velocity { min: 40, max: 127 }, trigger(AxisMode::Positive)),
        control_takeover(Axis::RightX, ControlTarget::Cc { cc: 74, min: 0, max: 127 }, AxisShape { smoothing: 20.0, ..Default::default() }),
        control_takeover(Axis::RightY, ControlTarget::Cc { cc: 71, min: 0, max: 127 }, AxisShape { smoothing: 20.0, ..Default::default() }),
        cc_button(button(Button::Back), CC_PLAY, Latch::Hold),
        cc_button(button(Button::Start), CC_RECORD, Latch::Hold),
    ]);
    let mut setup = loopmidi_steps();
    setup.extend([
        step("В Ableton: Options → Preferences → Link, Tempo & MIDI. У входа «loopMIDI Port» включи Track и Remote."),
        step("Положи на MIDI-трек Drum Rack с китом и включи запись трека (Arm). Нижний ряд пэдов — крестовина, второй — кнопки, дальше плечи, LT и нажатия стиков."),
        learn_step("Play: Ctrl+M, клик по Play, нажми Back.", button(Button::Back)),
        learn_step("Record: в том же режиме клик по Record, нажми Start.", button(Button::Start)),
        learn_step("Ручки (например, фильтр на дорожке): в Ctrl+M кликни ручку и наклони правый стик влево-вправо…", Input::Axis { axis: Axis::RightX }),
        learn_step("…или вверх-вниз.", Input::Axis { axis: Axis::RightY }),
    ]);
    Profile {
        name: "Ableton Live: Drum Rack".into(),
        music: Music { root: 0, octave: 4, scale: Scale::Preset { preset: ScalePreset::Major } },
        // Канал 10: если оставить встроенный звук, он тоже сыграет барабаны
        parts: vec![part("Drum Rack", 9, 0, 128, vec![])],
        layers: vec![base_layer(base)],
        meta: ProfileMeta {
            category: "daw".into(),
            target: "Ableton Live".into(),
            description: "13 пэдов Drum Rack (ноты 36–48, в Ableton C1…C2). Сила удара — RT, правый стик — две ручки для MIDI Learn. Back/Start — CC 85/86 для Play и Record.".into(),
            setup,
        },
    }
}

fn synth_setup() -> Vec<SetupStep> {
    let mut setup = loopmidi_steps();
    setup.extend([
        step("В DAW выбери вход «loopMIDI Port» на треке с синтезатором и включи у трека запись/мониторинг."),
        step("Подтяжка, Mod Wheel (CC1) и Aftertouch работают сами, если их использует пресет. Диапазон подтяжки в синте должен совпадать с профилем (здесь ±2 полутона)."),
        learn_step("Ручки синта: правый клик по ручке → MIDI Learn (в Vital — «Learn MIDI Assignment»), затем наклони правый стик вверх-вниз…", Input::Axis { axis: Axis::RightY }),
        learn_step("…вторая ручка — левый стик влево-вправо…", Input::Axis { axis: Axis::LeftX }),
        learn_step("…третья — левый стик вверх-вниз.", Input::Axis { axis: Axis::LeftY }),
    ]);
    setup
}

pub fn synth_lead() -> Profile {
    let mut base = scale_notes();
    base.extend([
        modifier(button(Button::LeftShoulder), ModEffect::Octaves { octaves: -1 }, Latch::Hold),
        modifier(button(Button::RightShoulder), ModEffect::Octaves { octaves: 1 }, Latch::Hold),
        control(Axis::LeftTrigger, ControlTarget::Pressure { min: 0, max: 127 }, trigger(AxisMode::Positive)),
        control(Axis::RightTrigger, ControlTarget::Cc { cc: 1, min: 0, max: 127 }, trigger(AxisMode::Positive)),
        control(Axis::RightX, ControlTarget::PitchBend, AxisShape::default()),
        control_takeover(Axis::RightY, ControlTarget::Cc { cc: 74, min: 0, max: 127 }, AxisShape { smoothing: 25.0, ..Default::default() }),
        control_takeover(Axis::LeftX, ControlTarget::Cc { cc: 71, min: 0, max: 127 }, AxisShape { smoothing: 25.0, ..Default::default() }),
        control_takeover(Axis::LeftY, ControlTarget::Cc { cc: 16, min: 0, max: 127 }, AxisShape { smoothing: 25.0, ..Default::default() }),
        cc_button(button(Button::Back), CC_PLAY, Latch::Hold),
        cc_button(button(Button::Start), CC_RECORD, Latch::Hold),
    ]);
    Profile {
        name: "Синтезатор: лид".into(),
        music: Music { root: 0, octave: 4, scale: Scale::Preset { preset: ScalePreset::NaturalMinor } },
        parts: vec![part("Лид", 0, 81, 0, vec![80, 81, 84, 87])],
        layers: vec![base_layer(base)],
        meta: ProfileMeta {
            category: "synth".into(),
            target: "Serum, Vital и другие VST".into(),
            description: "Соло-партия в до миноре. LT — Aftertouch, RT — Mod Wheel, правый стик: подтяжка и CC74 (фильтр после MIDI Learn), левый стик — ещё две ручки (CC71, CC16). Все ручки с подхватом и сглаживанием.".into(),
            setup: synth_setup(),
        },
    }
}

pub fn synth_bass() -> Profile {
    let mut base = scale_notes();
    base.extend([
        modifier(button(Button::LeftShoulder), ModEffect::Octaves { octaves: -1 }, Latch::Hold),
        modifier(button(Button::RightShoulder), ModEffect::Octaves { octaves: 1 }, Latch::Hold),
        control(Axis::LeftTrigger, ControlTarget::Velocity { min: 60, max: 127 }, trigger(AxisMode::Positive)),
        control_takeover(Axis::RightTrigger, ControlTarget::Cc { cc: 74, min: 20, max: 127 }, trigger(AxisMode::Positive)),
        control(Axis::RightX, ControlTarget::PitchBend, AxisShape::default()),
        control_takeover(Axis::RightY, ControlTarget::Cc { cc: 1, min: 0, max: 127 }, AxisShape { mode: AxisMode::Absolute, smoothing: 20.0, ..Default::default() }),
        control_takeover(Axis::LeftX, ControlTarget::Cc { cc: 71, min: 0, max: 127 }, AxisShape { smoothing: 25.0, ..Default::default() }),
        control_takeover(Axis::LeftY, ControlTarget::Cc { cc: 16, min: 0, max: 127 }, AxisShape { smoothing: 25.0, ..Default::default() }),
        cc_button(button(Button::Back), CC_PLAY, Latch::Hold),
        cc_button(button(Button::Start), CC_RECORD, Latch::Hold),
    ]);
    let mut setup = synth_setup();
    setup[4] = learn_step(
        "Фильтр: правый клик по ручке среза → MIDI Learn, затем нажми RT. Чем сильнее курок — тем ярче бас.",
        Input::Axis { axis: Axis::RightTrigger },
    );
    Profile {
        name: "Синтезатор: бас".into(),
        music: Music { root: 0, octave: 2, scale: Scale::Preset { preset: ScalePreset::NaturalMinor } },
        parts: vec![part("Бас", 0, 38, 0, vec![38, 39, 33, 34])],
        layers: vec![base_layer(base)],
        meta: ProfileMeta {
            category: "synth".into(),
            target: "Serum, Vital и другие VST".into(),
            description: "Бас в до миноре, вторая октава. RT — фильтр (CC74), LT — сила нажатия, правый стик — подтяжка и Mod Wheel, левый стик — две ручки (CC71, CC16).".into(),
            setup,
        },
    }
}

fn chord_of(input: Input, degree: i32, chord: ChordKind) -> Binding {
    Binding {
        input,
        action: Action::Note {
            note: NoteSpec::Chord { degree, chord, intervals: vec![], inversion: 0, open: false, bass: true },
            part: 0,
            velocity: 96,
            latch: Latch::Hold,
            length_ms: 0,
            velocity_from: VelocitySource::Fixed,
        },
    }
}

/// Аккомпанемент: аккорд на каждой кнопке, с LT — септаккорды.
pub fn chords() -> Profile {
    let mut base: Vec<Binding> = DEGREE_BUTTONS.iter().map(|&(b, d)| chord_of(button(b), d, ChordKind::Triad)).collect();
    base.extend([
        modifier(button(Button::LeftShoulder), ModEffect::Octaves { octaves: -1 }, Latch::Hold),
        modifier(button(Button::RightShoulder), ModEffect::Sustain, Latch::Hold),
        modifier(trigger_switch(Axis::LeftTrigger), ModEffect::Layer { layer: "sevenths".into() }, Latch::Hold),
        control(Axis::RightTrigger, ControlTarget::Velocity { min: 45, max: 127 }, trigger(AxisMode::Positive)),
        control(Axis::RightX, ControlTarget::PitchBend, AxisShape::default()),
        control(Axis::RightY, ControlTarget::Cc { cc: 1, min: 0, max: 127 }, AxisShape { mode: AxisMode::Absolute, ..Default::default() }),
        command(button(Button::Back), Command::NextInstrument { part: 0 }),
        command(button(Button::Start), Command::ToggleRecord),
    ]);
    let sevenths = Layer {
        id: "sevenths".into(),
        name: "Септаккорды".into(),
        bindings: DEGREE_BUTTONS.iter().map(|&(b, d)| chord_of(button(b), d, ChordKind::Seventh)).collect(),
        combo: vec![],
        transpose: 0,
    };
    Profile {
        name: "Аккорды".into(),
        music: Music { root: 0, octave: 4, scale: Scale::Preset { preset: ScalePreset::Major } },
        parts: vec![part("Аккорды", 0, 0, 0, vec![0, 4, 48, 19])],
        layers: vec![base_layer(base), sevenths],
        meta: ProfileMeta {
            category: "builtin".into(),
            target: String::new(),
            description: "Аккомпанемент: на крестовине и кнопках — трезвучия всех ступеней лада с басом, пока зажат LT — септаккорды. LB — октава ниже, RB — педаль, RT — сила нажатия. Back — другой инструмент (фортепиано, электропиано, струнные, орган).".into(),
            setup: vec![],
        },
    }
}

/// Номера CC пульта DAW: свободные в стандарте MIDI (102–119 не заняты).
const DAW_CC: [(Button, u8); 8] = [
    (Button::South, 102),
    (Button::East, 103),
    (Button::West, 104),
    (Button::North, 105),
    (Button::DpadUp, 106),
    (Button::DpadDown, 107),
    (Button::DpadLeft, 108),
    (Button::DpadRight, 109),
];

fn relative(axis: Axis, cc: u8) -> Binding {
    Binding {
        input: Input::Axis { axis },
        action: Action::Control {
            target: ControlTarget::Cc { cc, min: 0, max: 127 },
            shape: AxisShape { deadzone: 0.2, ..Default::default() },
            part: 0,
            takeover: false,
            relative: true,
        },
    }
}

/// Пульт для DAW: транспорт, навигация и ручки — без нот.
pub fn daw_control() -> Profile {
    let mut base: Vec<Binding> = DAW_CC.iter().map(|&(b, cc)| cc_button(button(b), cc, Latch::Hold)).collect();
    base.extend([
        cc_button(button(Button::Back), CC_PLAY, Latch::Hold),
        cc_button(button(Button::Start), CC_RECORD, Latch::Hold),
        cc_button(button(Button::RightStick), CC_EXTRA, Latch::Hold),
        command(button(Button::LeftStick), Command::TapTempo),
        relative(Axis::LeftY, 20),
        relative(Axis::LeftX, 21),
        relative(Axis::RightY, 22),
        relative(Axis::RightX, 23),
        control_takeover(Axis::LeftTrigger, ControlTarget::Cc { cc: 24, min: 0, max: 127 }, trigger(AxisMode::Positive)),
        control_takeover(Axis::RightTrigger, ControlTarget::Cc { cc: 25, min: 0, max: 127 }, trigger(AxisMode::Positive)),
    ]);
    let mut setup = loopmidi_steps();
    setup.extend([
        step("В DAW включи у входа «loopMIDI Port» управление (в Ableton — Remote, в FL — «Generic controller»). Дальше каждую кнопку привязываем через MIDI Learn: в Ableton — Ctrl+M, кликнуть элемент, нажать кнопку геймпада."),
        learn_step("Play — Back.", button(Button::Back)),
        learn_step("Record — Start.", button(Button::Start)),
        learn_step("Stop — A (Cross).", button(Button::South)),
        learn_step("Петля (Loop) — B (Circle).", button(Button::East)),
        learn_step("Метроном DAW — X (Square).", button(Button::West)),
        learn_step("Overdub или Undo — Y (Triangle).", button(Button::North)),
        learn_step("Крестовина ↑↓ — выбор сцены или клипа, ←→ — выбор трека. Начни с ↑.", button(Button::DpadUp)),
        learn_step("Громкость выбранного трека — левый стик вверх-вниз: отклонение крутит значение, отпустил — осталось.", Input::Axis { axis: Axis::LeftY }),
        learn_step("Панорама — левый стик влево-вправо.", Input::Axis { axis: Axis::LeftX }),
        learn_step("Посылы или макросы — правый стик и курки. Нажатие левого стика — Tap Tempo для метронома здесь (с «MIDI Clock в DAW» темп уйдёт и в DAW).", Input::Axis { axis: Axis::RightY }),
    ]);
    Profile {
        name: "Управление DAW".into(),
        music: Music { root: 0, octave: 4, scale: Scale::Preset { preset: ScalePreset::Major } },
        parts: vec![part("В DAW", 0, 0, 0, vec![])],
        layers: vec![base_layer(base)],
        meta: ProfileMeta {
            category: "daw".into(),
            target: "Ableton Live, FL Studio, Reaper…".into(),
            description: "Геймпад как пульт: Play/Record/Stop, петля и метроном на кнопках, навигация по трекам и сценам на крестовине, громкость, панорама и посылы — на стиках (относительный режим, значение остаётся, где оставил), курки — две ручки. Нажатие левого стика — Tap Tempo.".into(),
            setup,
        },
    }
}

/// Все встроенные профили в порядке показа.
pub fn library() -> Vec<Profile> {
    vec![
        classic(),
        chords(),
        pentatonic(),
        drums(),
        ableton_keys(),
        ableton_drums(),
        fl_keys(),
        daw_control(),
        synth_lead(),
        synth_bass(),
    ]
}

#[cfg(test)]
mod library_tests {
    use super::*;

    #[test]
    fn library_profiles_are_valid_and_unique() {
        let lib = library();
        for p in &lib {
            p.validate().unwrap_or_else(|e| panic!("{}: {e}", p.name));
            assert_eq!(Profile::from_json(&p.to_json()).unwrap(), *p, "{} переживает JSON", p.name);
            assert!(!p.meta.category.is_empty() && !p.meta.description.is_empty(), "{} описан", p.name);
            for s in &p.meta.setup {
                if let Some(input) = s.learn {
                    let bound = p.layers[0].bindings.iter().any(|b| b.input == input);
                    assert!(bound, "{}: шаг «{}» ссылается на неназначенный вход", p.name, s.text);
                }
            }
        }
        let mut names: Vec<&str> = lib.iter().map(|p| p.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), lib.len(), "имена не повторяются");
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

/// Профиль в том виде, в каком его пишет редактор интерфейса (app/src/lib/profileEdit.ts).
#[cfg(test)]
mod editor_json {
    use crate::model::Profile;

    #[test]
    fn editor_output_is_accepted() {
        let json = r#"{
          "name": "Из редактора",
          "music": { "root": 9, "octave": 3, "scale": { "type": "preset", "preset": "minor_pentatonic" } },
          "parts": [{ "name": "Мелодия", "channel": 0, "program": 0, "bank": 0, "volume": 100, "bend_range": 2, "instruments": [0, 40] }],
          "layers": [
            { "id": "base", "name": "Основной", "bindings": [
              { "input": { "kind": "button", "button": "south" },
                "action": { "kind": "note", "note": { "type": "degree", "degree": 0 }, "part": 0, "velocity": 100 } },
              { "input": { "kind": "button", "button": "east" },
                "action": { "kind": "note", "note": { "type": "absolute", "note": 36 }, "part": 0, "velocity": 127 } },
              { "input": { "kind": "button", "button": "west" },
                "action": { "kind": "note", "note": { "type": "chord", "degree": 2, "chord": "seventh" }, "part": 0, "velocity": 90 } },
              { "input": { "kind": "button", "button": "north" },
                "action": { "kind": "modifier", "effect": { "type": "layer", "layer": "layer1" }, "latch": "toggle" } },
              { "input": { "kind": "axis_switch", "axis": "left_trigger", "direction": "positive", "threshold": 0.35 },
                "action": { "kind": "modifier", "effect": { "type": "sustain" }, "latch": "hold" } },
              { "input": { "kind": "axis_switch", "axis": "left_y", "direction": "negative", "threshold": 0.5 },
                "action": { "kind": "command", "command": { "type": "toggle_record" } } },
              { "input": { "kind": "axis", "axis": "right_trigger" },
                "action": { "kind": "control", "target": { "type": "velocity", "min": 30, "max": 127 },
                            "shape": { "deadzone": 0.02, "curve": 1.4, "invert": false, "mode": "positive" }, "part": 0 } },
              { "input": { "kind": "axis", "axis": "left_x" },
                "action": { "kind": "control", "target": { "type": "cc", "cc": 74, "min": 0, "max": 127 },
                            "shape": { "deadzone": 0.15, "curve": 1, "invert": true, "mode": "bipolar" }, "part": 0 } }
            ] },
            { "id": "layer1", "name": "Слой 1", "bindings": [
              { "input": { "kind": "button", "button": "south" },
                "action": { "kind": "modifier", "effect": { "type": "velocity", "velocity": 60 }, "latch": "hold" } }
            ] }
          ]
        }"#;
        let p = Profile::from_json(json).expect("профиль из редактора должен приниматься");
        assert_eq!(p.layers[0].bindings.len(), 8);
    }

    #[test]
    fn editor_output_with_pro_features_is_accepted() {
        let json = r#"{
          "name": "Про",
          "music": { "root": 0, "octave": 4, "scale": { "type": "preset", "preset": "major" } },
          "parts": [{ "name": "Мелодия", "channel": 0, "program": 0, "bank": 0, "volume": 100, "bend_range": 2, "instruments": [],
                      "transpose": 0, "sound": { "pan": 64, "reverb": 40, "chorus": 0, "attack": 64, "release": 64, "brightness": 64, "resonance": 64 } }],
          "layers": [
            { "id": "base", "name": "Основной", "bindings": [
              { "input": { "kind": "button", "button": "south" },
                "action": { "kind": "note", "note": { "type": "chord", "degree": 0, "chord": "custom", "intervals": [0, 3, 7, 10], "inversion": 1, "open": true, "bass": true },
                            "part": 0, "velocity": 90, "latch": "toggle" } },
              { "input": { "kind": "button", "button": "east" },
                "action": { "kind": "cc", "cc": 64, "part": 0, "on": 127, "off": 0, "latch": "hold", "step": 0 } },
              { "input": { "kind": "button", "button": "west" },
                "action": { "kind": "command", "command": { "type": "set_program", "part": 0, "program": 40, "bank": 0 } } },
              { "input": { "kind": "button", "button": "left_shoulder" },
                "action": { "kind": "modifier", "effect": { "type": "layer", "layer": "a" }, "latch": "hold" } },
              { "input": { "kind": "button", "button": "right_shoulder" },
                "action": { "kind": "modifier", "effect": { "type": "layer", "layer": "b" }, "latch": "hold" } },
              { "input": { "kind": "axis", "axis": "left_trigger" },
                "action": { "kind": "control", "target": { "type": "pressure", "min": 0, "max": 127 },
                            "shape": { "deadzone": 0.02, "curve": 1, "invert": false, "mode": "positive", "smoothing": 30 },
                            "part": 0, "takeover": true, "relative": false } }
            ] },
            { "id": "a", "name": "A", "bindings": [] },
            { "id": "b", "name": "B", "bindings": [] },
            { "id": "ab", "name": "A+B", "bindings": [], "combo": ["a", "b"] }
          ]
        }"#;
        let p = Profile::from_json(json).expect("профиль из редактора должен приниматься");
        assert_eq!(p.layers[3].combo, ["a", "b"]);
    }
}
