//! Профиль: что делает каждый контрол геймпада. Хранится в JSON.
//!
//! Устройство раскладки:
//! - профиль состоит из слоёв; слой 0 — базовый и активен всегда;
//! - остальные слои включаются модификаторами и перекрывают нижние
//!   (если в верхнем слое контрол не назначен, работает назначение из нижнего);
//! - каждое назначение (`Binding`) связывает вход (`Input`) с действием (`Action`).

use serde::{Deserialize, Serialize};

/// Кнопки по положению на геймпаде (как в SDL): South — нижняя из четырёх
/// (A у Xbox, ✕ у PlayStation, B у Nintendo), East — правая и т.д.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    South,
    East,
    West,
    North,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    LeftShoulder,
    RightShoulder,
    LeftStick,
    RightStick,
    Back,
    Start,
    Guide,
    Touchpad,
    Misc1,
    LeftPaddle1,
    LeftPaddle2,
    RightPaddle1,
    RightPaddle2,
}

/// Оси. Стики: −1..1, вверх и вправо — плюс. Курки: 0..1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    LeftX,
    LeftY,
    RightX,
    RightY,
    LeftTrigger,
    RightTrigger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Positive,
    Negative,
}

/// Вход назначения.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Input {
    /// Обычная кнопка.
    Button { button: Button },
    /// Ось как кнопка: «нажата», когда отклонение в сторону `direction` ≥ `threshold`.
    /// Так курок или стик становятся модификатором или нотой.
    AxisSwitch { axis: Axis, direction: Direction, threshold: f32 },
    /// Ось как плавный контроллер (только для `Action::Control`).
    Axis { axis: Axis },
}

/// Ключ «кнопочного» входа — по нему отслеживаем, что сейчас зажато.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SwitchKey {
    Button { button: Button },
    AxisSwitch { axis: Axis, direction: Direction },
}

impl Input {
    pub fn switch_key(&self) -> Option<SwitchKey> {
        match *self {
            Input::Button { button } => Some(SwitchKey::Button { button }),
            Input::AxisSwitch { axis, direction, .. } => Some(SwitchKey::AxisSwitch { axis, direction }),
            Input::Axis { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub input: Input,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// Сыграть ноту или аккорд: пока вход зажат или до повторного нажатия (защёлка).
    Note {
        note: NoteSpec,
        #[serde(default)]
        part: usize,
        #[serde(default = "default_velocity")]
        velocity: u8,
        #[serde(default)]
        latch: Latch,
        /// Длительность, мс: нота звучит столько, сколько задано, независимо от кнопки (0 — пока зажата).
        #[serde(default, skip_serializing_if = "is_zero")]
        length_ms: u32,
        /// Откуда брать силу нажатия.
        #[serde(default, skip_serializing_if = "VelocitySource::is_fixed")]
        velocity_from: VelocitySource,
    },
    /// CC от кнопки: `on` при нажатии и `off` при отпускании (или переключателем),
    /// либо шаг `step` к текущему значению при каждом нажатии (относительный режим).
    /// Через MIDI Map в DAW так вешаются Play, Record, Mute и т.п.
    Cc {
        cc: u8,
        #[serde(default)]
        part: usize,
        #[serde(default = "default_cc_on")]
        on: u8,
        #[serde(default)]
        off: u8,
        #[serde(default)]
        latch: Latch,
        /// Не 0 — относительный режим: прибавить к текущему значению.
        #[serde(default)]
        step: i8,
    },
    /// Изменить поведение остальных кнопок, пока вход зажат (или до повторного нажатия).
    Modifier {
        effect: ModEffect,
        #[serde(default)]
        latch: Latch,
    },
    /// Разовое действие по нажатию.
    Command { command: Command },
    /// Плавный контроллер от оси.
    Control {
        target: ControlTarget,
        #[serde(default)]
        shape: AxisShape,
        #[serde(default)]
        part: usize,
        /// Подхват (soft takeover): после смены слоя не прыгать к положению стика,
        /// а ждать, пока стик «догонит» последнее отправленное значение.
        #[serde(default)]
        takeover: bool,
        /// Относительный режим: отклонение задаёт скорость изменения, а не значение.
        #[serde(default)]
        relative: bool,
    },
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// Сила нажатия ноты: постоянная (`velocity`) или от отклонения оси —
/// от `min` при едва тронутой оси до `velocity` при оси до упора.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VelocitySource {
    #[default]
    Fixed,
    /// Конкретная ось (курок, стик).
    Axis { axis: Axis, min: u8 },
    /// Ось, которую тронули последней.
    LastAxis { min: u8 },
}

impl VelocitySource {
    pub fn is_fixed(&self) -> bool {
        *self == VelocitySource::Fixed
    }
}

fn default_velocity() -> u8 {
    100
}

fn default_cc_on() -> u8 {
    127
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NoteSpec {
    /// Ступень лада от тоники: 0 — тоника, 7 — тоника октавой выше (в семиступенном ладу).
    Degree { degree: i32 },
    /// Конкретная MIDI-нота, лад и тоника не влияют (удобно для ударных).
    Absolute { note: u8 },
    /// Аккорд от ступени `degree`.
    Chord {
        degree: i32,
        chord: ChordKind,
        /// Для `ChordKind::Custom`: интервалы от основного тона в полутонах, например [0, 4, 7, 11].
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        intervals: Vec<i32>,
        /// Обращение: сколько нижних звуков поднять на октаву.
        #[serde(default)]
        inversion: u8,
        /// Широкое расположение: второй снизу звук на октаву выше.
        #[serde(default)]
        open: bool,
        /// Добавить основной тон октавой ниже.
        #[serde(default)]
        bass: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChordKind {
    /// Трезвучие: ступени 1-3-5.
    Triad,
    /// Септаккорд: 1-3-5-7.
    Seventh,
    /// Нонаккорд: 1-3-5-7-9.
    Ninth,
    /// С задержанием: 1-2-5.
    Sus2,
    /// С задержанием: 1-4-5.
    Sus4,
    /// Секстаккорд: 1-3-5-6.
    Sixth,
    /// «Пауэр-аккорд»: 1-5-8.
    Power,
    /// Свои интервалы в полутонах (не зависят от лада).
    Custom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModEffect {
    /// Сдвиг на полутоны (диез/бемоль).
    Semitones { semitones: i32 },
    Octaves { octaves: i32 },
    /// Сдвиг по ступеням лада.
    Degrees { degrees: i32 },
    /// Включить слой раскладки.
    Layer { layer: String },
    /// Фиксированная сила нажатия для новых нот.
    Velocity { velocity: u8 },
    /// Педаль: ноты тянутся после отпускания.
    Sustain,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Latch {
    /// Действует, пока зажат.
    #[default]
    Hold,
    /// Нажал — включил, нажал ещё раз — выключил.
    Toggle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    NextInstrument {
        #[serde(default)]
        part: usize,
    },
    PrevInstrument {
        #[serde(default)]
        part: usize,
    },
    ToggleRecord,
    /// Заглушить всё.
    Panic,
    ToggleMetronome,
    /// Сменить инструмент партии на конкретный (Program Change).
    SetProgram {
        #[serde(default)]
        part: usize,
        program: u8,
        #[serde(default)]
        bank: u16,
    },
    /// Лупер: начать петлю / наложить слой / закончить слой.
    LoopRecord,
    /// Лупер: отменить последний слой.
    LoopUndo,
    /// Лупер: очистить.
    LoopClear,
    /// Темп по нажатиям: постучи 3–4 раза в нужном темпе.
    TapTempo,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlTarget {
    /// Подтяжка высоты. Диапазон задаётся в `Part::bend_range`.
    PitchBend,
    /// Любой MIDI CC: 1 — модуляция (вибрато), 11 — экспрессия (громкость), 74 — яркость…
    Cc { cc: u8, min: u8, max: u8 },
    /// Сила нажатия для следующих нот.
    Velocity { min: u8, max: u8 },
    /// Давление на канал (Channel Pressure / aftertouch).
    Pressure { min: u8, max: u8 },
}

/// Как значение оси превращается в выход.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AxisShape {
    /// Мёртвая зона у нуля (против дрейфа стиков), 0..1.
    pub deadzone: f32,
    /// Сила кривой: 1 — линейно, >1 — мягче у центра, <1 — резче.
    pub curve: f32,
    /// Вид кривой: степенная или S-образная.
    pub curve_kind: CurveKind,
    pub invert: bool,
    pub mode: AxisMode,
    /// Сглаживание, мс: убирает дрожание стика (0 — без сглаживания).
    pub smoothing: f32,
}

impl Default for AxisShape {
    fn default() -> Self {
        Self { deadzone: 0.15, curve: 1.0, curve_kind: CurveKind::Power, invert: false, mode: AxisMode::Bipolar, smoothing: 0.0 }
    }
}

/// Форма кривой отклика оси.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurveKind {
    /// a^k: k>1 — экспонента (мягко у центра), k<1 — логарифм (резко у центра).
    #[default]
    Power,
    /// a^k / (a^k + (1−a)^k): мягко у центра и у края, быстро в середине хода.
    S,
}

/// Выход формы: `Bipolar` даёт −1..1, остальные режимы 0..1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisMode {
    /// Обе стороны: для стика влево-вправо (подтяжка вверх и вниз).
    #[default]
    Bipolar,
    /// Отклонение в любую сторону: для вибрато «стик от центра».
    Absolute,
    /// Только плюс: курок, стик вверх.
    Positive,
    /// Только минус: стик вниз.
    Negative,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub bindings: Vec<Binding>,
    /// Слой-комбинация: включается сам, когда включены все перечисленные слои
    /// (например, «LB+RB»). Такой слой важнее слоёв, из которых состоит.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub combo: Vec<String>,
    /// Сдвиг нот этого слоя, полутоны (вдобавок к сдвигу партии).
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub transpose: i32,
}

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

/// Звуковая партия: свой MIDI-канал и инструмент. Несколько партий звучат одновременно.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub name: String,
    /// 0..15 (9 — ударные по стандарту GM).
    pub channel: u8,
    pub program: u8,
    #[serde(default)]
    pub bank: u16,
    #[serde(default = "default_volume")]
    pub volume: u8,
    /// Диапазон подтяжки в полутонах.
    #[serde(default = "default_bend_range")]
    pub bend_range: u8,
    /// Инструменты, по которым ходят команды Next/PrevInstrument (номера GM).
    #[serde(default)]
    pub instruments: Vec<u8>,
    /// Сдвиг всех нот партии в полутонах.
    #[serde(default)]
    pub transpose: i32,
    #[serde(default)]
    pub sound: Sound,
    /// Откуда звук во встроенном синтезаторе (в MIDI-выход это не влияет).
    #[serde(default, skip_serializing_if = "PartSource::is_default")]
    pub source: PartSource,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PartSource {
    /// Основной звуковой банк программы.
    #[default]
    Default,
    /// Свой банк .sf2: путь относительно папки библиотеки звуков (или полный).
    SoundFont { file: String },
    /// One-shot сэмплы на ноты (барабанный кит из своих WAV).
    Samples { pads: Vec<SamplePad> },
}

impl PartSource {
    pub fn is_default(&self) -> bool {
        *self == PartSource::Default
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SamplePad {
    pub note: u8,
    /// WAV: путь относительно папки библиотеки звуков (или полный).
    pub file: String,
    #[serde(default)]
    pub gain_db: f32,
    /// Сдвиг высоты, полутоны.
    #[serde(default)]
    pub pitch: i32,
    /// Группа глушения (0 — нет): новый звук группы глушит прежние.
    #[serde(default)]
    pub choke: u8,
}

/// Звучание партии — стандартные MIDI-контроллеры, их понимают и встроенный синтезатор, и DAW.
/// 64 у атаки, затухания, яркости и резонанса — «как задумано в банке».
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sound {
    /// CC10: 0 — лево, 64 — центр, 127 — право.
    pub pan: u8,
    /// CC91: посыл на реверберацию.
    pub reverb: u8,
    /// CC93: посыл на хорус.
    pub chorus: u8,
    /// CC73: время атаки.
    pub attack: u8,
    /// CC72: время затухания после отпускания.
    pub release: u8,
    /// CC74: яркость (частота среза фильтра).
    pub brightness: u8,
    /// CC71: резонанс фильтра.
    pub resonance: u8,
}

impl Default for Sound {
    fn default() -> Self {
        Self { pan: 64, reverb: 40, chorus: 0, attack: 64, release: 64, brightness: 64, resonance: 64 }
    }
}

impl Sound {
    /// (номер CC, значение) для отправки синтезатору.
    pub fn controllers(&self) -> [(u8, u8); 7] {
        [
            (10, self.pan),
            (91, self.reverb),
            (93, self.chorus),
            (73, self.attack),
            (72, self.release),
            (74, self.brightness),
            (71, self.resonance),
        ]
    }
}

fn default_volume() -> u8 {
    100
}

fn default_bend_range() -> u8 {
    2
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Music {
    /// Тоника: 0 — до, 1 — до-диез, … 11 — си.
    pub root: u8,
    /// Октава тоники для ступени 0 (4 — первая октава, до = MIDI 60).
    pub octave: i32,
    pub scale: Scale,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Scale {
    Preset { preset: ScalePreset },
    /// Свой лад: интервалы от тоники в полутонах, например [0, 3, 5, 7, 10].
    Custom { steps: Vec<u8> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScalePreset {
    Major,
    NaturalMinor,
    HarmonicMinor,
    MelodicMinor,
    MajorPentatonic,
    MinorPentatonic,
    Blues,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
    Chromatic,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub music: Music,
    pub parts: Vec<Part>,
    pub layers: Vec<Layer>,
    #[serde(default, skip_serializing_if = "ProfileMeta::is_empty")]
    pub meta: ProfileMeta,
}

/// Описание профиля для библиотеки: раздел, для чего он и как подключить.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileMeta {
    /// Раздел библиотеки: builtin (встроенный звук), daw, synth. Пусто — «Мои».
    pub category: String,
    /// Для чего профиль: «Ableton Live», «Встроенный звук»…
    pub target: String,
    pub description: String,
    /// Шаги подключения (для DAW и синтезаторов).
    pub setup: Vec<SetupStep>,
}

impl ProfileMeta {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Шаг настройки. Если указан `learn` — рядом кнопка «Отправить для MIDI Learn» этого входа.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SetupStep {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub learn: Option<Input>,
}

impl Profile {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("профиль всегда сериализуется")
    }

    pub fn from_json(s: &str) -> anyhow::Result<Self> {
        let p: Profile = serde_json::from_str(s)?;
        p.validate()?;
        Ok(p)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.parts.is_empty(), "в профиле нет ни одной партии");
        anyhow::ensure!(!self.layers.is_empty(), "в профиле нет базового слоя");
        for p in &self.parts {
            anyhow::ensure!(p.channel < 16, "канал партии «{}» вне 0..15", p.name);
        }
        for layer in &self.layers {
            for id in &layer.combo {
                anyhow::ensure!(*id != layer.id, "слой «{}» не может включаться сам собой", layer.name);
                anyhow::ensure!(self.layers.iter().any(|l| &l.id == id), "комбинация слоя «{}» ссылается на несуществующий слой", layer.name);
            }
            for b in &layer.bindings {
                let part = match &b.action {
                    Action::Note { part, .. } | Action::Control { part, .. } | Action::Cc { part, .. } => Some(*part),
                    Action::Command {
                        command:
                            Command::NextInstrument { part } | Command::PrevInstrument { part } | Command::SetProgram { part, .. },
                    } => Some(*part),
                    _ => None,
                };
                if let Some(part) = part {
                    anyhow::ensure!(part < self.parts.len(), "слой «{}» ссылается на несуществующую партию {part}", layer.name);
                }
                if let Action::Modifier { effect: ModEffect::Layer { layer: id }, .. } = &b.action {
                    anyhow::ensure!(self.layers.iter().any(|l| &l.id == id), "модификатор ссылается на несуществующий слой «{id}»");
                }
                if let Action::Cc { cc, on, off, .. } = &b.action {
                    anyhow::ensure!(*cc < 128 && *on < 128 && *off < 128, "CC и значения — 0..127");
                }
                let is_axis = matches!(b.input, Input::Axis { .. });
                let is_control = matches!(b.action, Action::Control { .. });
                anyhow::ensure!(is_axis == is_control, "в слое «{}» плавный контроллер можно назначить только на ось, и наоборот", layer.name);
            }
        }
        Ok(())
    }
}
