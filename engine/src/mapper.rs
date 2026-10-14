//! Движок раскладок: события геймпада → MIDI. Без ввода-вывода, поэтому покрыт тестами.

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use serde::Serialize;

use crate::midi::{self, MidiMsg};
use crate::model::*;

/// Нормализованное событие ввода. Стики −1..1 (вверх/вправо — плюс), курки 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputEvent {
    Button { button: Button, down: bool },
    Axis { axis: Axis, value: f32 },
}

/// Что делает вход сейчас (см. `Mapper::preview`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ControlView {
    pub input: Input,
    /// Слой, из которого взято назначение.
    pub layer: String,
    #[serde(flatten)]
    pub role: Role,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Role {
    /// `on` — нота-защёлка сейчас звучит.
    Note { notes: Vec<u8>, chord: bool, latch: Latch, on: bool },
    /// CC-кнопка; `on` — переключатель сейчас включён.
    Cc { cc: u8, step: i8, latch: Latch, on: bool },
    Modifier { effect: ModEffect, latch: Latch, active: bool },
    Command { command: Command },
    Control { target: ControlTarget },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    Midi(MidiMsg),
    /// Команда для движка: запись, метроном, лупер.
    Command(Command),
}

/// Гистерезис «кнопки» на оси: при пороге 0.5 нажатие — от 0.55, отпускание — ниже 0.45.
/// Без этого курок у порога дребезжит.
const SWITCH_HYSTERESIS: f32 = 0.05;
/// Подхват засчитывается, если стик подошёл к прежнему значению ближе этого (из 127).
const PICKUP_WINDOW: i32 = 2;
/// Минимальный шаг подтяжки, который стоит отправлять (из 16384).
const BEND_STEP: i32 = 16;
const BEND_CENTER: i32 = 8192;
const ALL_AXES: [Axis; 6] = [Axis::LeftX, Axis::LeftY, Axis::RightX, Axis::RightY, Axis::LeftTrigger, Axis::RightTrigger];

/// Что включило нажатие. Отпускание снимает ровно это, даже если слои
/// или модификаторы с тех пор поменялись.
#[derive(Clone, Debug)]
enum Held {
    Notes(Vec<(u8, u8)>),
    Modifier(ModEffect),
    /// CC-кнопка «пока зажата»: при отпускании отправить `off`.
    Cc { ch: u8, cc: u8, off: u8 },
    Nothing,
}

#[derive(Clone, Debug)]
struct HeldSwitch {
    held: Held,
    /// Порог, с которым нажалась «кнопка» на оси (у настоящих кнопок не используется).
    threshold: f32,
}

/// Какое назначение сейчас обрабатывает ось.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Route {
    layer: usize,
    binding: usize,
}

/// Ключ последнего отправленного значения контроллера: (канал, номер CC или BEND_KEY).
type ControlKey = (u8, u16);
const BEND_KEY: u16 = 0x100;
const PRESSURE_KEY: u16 = 0x101;

/// Подхват (soft takeover): ось молчит, пока не дойдёт до `target` — последнего отправленного значения.
#[derive(Clone, Copy, Debug)]
struct Pickup {
    target: i32,
    /// Значение оси на прошлом шаге — чтобы заметить, что ось «перескочила» через target.
    last: Option<i32>,
}

pub struct Mapper {
    profile: Profile,
    layer_ids: HashMap<String, usize>,
    held: HashMap<SwitchKey, HeldSwitch>,
    latched: Vec<(SwitchKey, ModEffect)>,
    /// Сколько нажатий держат каждую ноту: (канал, нота) → счётчик.
    sounding: HashMap<(u8, u8), u32>,
    axis_values: HashMap<Axis, f32>,
    routes: HashMap<Axis, Route>,
    controls: HashMap<ControlKey, i32>,
    /// Ноты-защёлки, которые сейчас звучат.
    latched_notes: HashMap<SwitchKey, Vec<(u8, u8)>>,
    /// CC-переключатели, которые сейчас включены: что отправить при выключении.
    latched_cc: HashMap<SwitchKey, (u8, u8, u8)>,
    /// Сглаживание осей: текущее и целевое значение (после формы).
    smooth: HashMap<Axis, (f32, f32)>,
    pickup: HashMap<Axis, Pickup>,
    /// Относительный режим: скорость изменения от оси и накопленное дробное значение.
    rates: HashMap<Axis, f32>,
    rel_values: HashMap<ControlKey, f32>,
    dyn_velocity: Option<u8>,
    sustain: bool,
    out: Vec<Output>,
}

impl Mapper {
    /// Создаёт движок; стартовые сообщения (инструменты, громкость) уже лежат в выходе.
    pub fn new(profile: Profile) -> Self {
        let layer_ids = profile.layers.iter().enumerate().map(|(i, l)| (l.id.clone(), i)).collect();
        let mut m = Self {
            profile,
            layer_ids,
            held: HashMap::new(),
            latched: Vec::new(),
            sounding: HashMap::new(),
            axis_values: HashMap::new(),
            routes: HashMap::new(),
            controls: HashMap::new(),
            latched_notes: HashMap::new(),
            latched_cc: HashMap::new(),
            smooth: HashMap::new(),
            pickup: HashMap::new(),
            rates: HashMap::new(),
            rel_values: HashMap::new(),
            dyn_velocity: None,
            sustain: false,
            out: Vec::new(),
        };
        for msg in m.setup_messages() {
            m.out.push(Output::Midi(msg));
        }
        for axis in ALL_AXES {
            m.route_axis(axis, true);
        }
        m
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Забрать накопленный выход.
    pub fn drain(&mut self) -> std::vec::Drain<'_, Output> {
        self.out.drain(..)
    }

    pub fn handle(&mut self, event: InputEvent) {
        match event {
            InputEvent::Button { button, down } => {
                let key = SwitchKey::Button { button };
                if down {
                    if !self.held.contains_key(&key) {
                        self.press(key, 0.0);
                    }
                } else {
                    self.release(key);
                }
            }
            InputEvent::Axis { axis, value } => {
                self.axis_values.insert(axis, value);
                for direction in [Direction::Positive, Direction::Negative] {
                    let key = SwitchKey::AxisSwitch { axis, direction };
                    let amount = match direction {
                        Direction::Positive => value.max(0.0),
                        Direction::Negative => (-value).max(0.0),
                    };
                    if let Some(h) = self.held.get(&key) {
                        if amount < h.threshold - SWITCH_HYSTERESIS {
                            self.release(key);
                        }
                    } else if let Some(Input::AxisSwitch { threshold, .. }) = self.resolve(key).map(|r| self.binding(r).input)
                        && amount >= (threshold + SWITCH_HYSTERESIS).min(0.98)
                    {
                        self.press(key, threshold);
                    }
                }
                self.route_axis(axis, false);
            }
        }
    }

    /// Отпустить всё, что зажато (например, при смене геймпада). Переключенные модификаторы остаются.
    pub fn release_all(&mut self) {
        let keys: Vec<SwitchKey> = self.held.keys().copied().collect();
        for key in keys {
            self.release(key);
        }
        for axis in ALL_AXES {
            self.axis_values.insert(axis, 0.0);
            self.route_axis(axis, false);
        }
    }

    /// Заглушить всё и сбросить переключенные модификаторы, защёлки и CC-переключатели.
    pub fn panic(&mut self) {
        self.release_all();
        let latched: Vec<ModEffect> = self.latched.drain(..).map(|(_, e)| e).collect();
        for effect in latched {
            self.modifier_changed(&effect);
        }
        let notes: Vec<(u8, u8)> = self.latched_notes.drain().flat_map(|(_, n)| n).collect();
        for (ch, n) in notes {
            self.note_off_counted(ch, n);
        }
        let ccs: Vec<(u8, u8, u8)> = self.latched_cc.drain().map(|(_, v)| v).collect();
        for (ch, cc, off) in ccs {
            self.send_cc(ch, cc, off);
        }
        self.sounding.clear();
        for ch in self.channels() {
            self.emit(MidiMsg::cc(ch, 123, 0));
        }
    }

    // ---------- состояние для интерфейса ----------

    /// Ход времени: сглаживание и относительные контроллеры. Вызывать часто (каждые несколько мс).
    pub fn tick(&mut self, dt: Duration) {
        let dt_s = dt.as_secs_f32();
        let routes: Vec<(Axis, Route)> = self.routes.iter().map(|(a, r)| (*a, *r)).collect();
        for (axis, route) in routes {
            let Action::Control { target, shape, part, relative, .. } = self.binding(route).action.clone() else { continue };
            if relative {
                let rate = self.rates.get(&axis).copied().unwrap_or(0.0);
                if rate != 0.0 {
                    self.nudge_relative(target, part, rate * dt_s);
                }
                continue;
            }
            if let Some(&(cur, goal)) = self.smooth.get(&axis)
                && shape.smoothing > 0.0
                && cur != goal
            {
                let alpha = 1.0 - (-dt_s * 1000.0 / shape.smoothing).exp();
                let mut next = cur + (goal - cur) * alpha;
                if (goal - next).abs() < 0.002 {
                    next = goal;
                }
                self.smooth.insert(axis, (next, goal));
                let raw = self.axis_values.get(&axis).copied().unwrap_or(0.0);
                self.send_shaped(axis, route, next, raw, false);
            }
        }
    }

    /// Нужен ли `tick` прямо сейчас: идёт сглаживание или относительный контроллер отклонён.
    pub fn needs_tick(&self) -> bool {
        self.smooth.values().any(|(cur, goal)| cur != goal) || self.rates.values().any(|r| *r != 0.0)
    }

    /// Суммарный сдвиг от модификаторов, полутоны и ступени — для показа в интерфейсе.
    pub fn offset(&self) -> (i32, i32) {
        let mut semis = 0;
        let mut degrees = 0;
        for e in self.effects() {
            match e {
                ModEffect::Semitones { semitones } => semis += semitones,
                ModEffect::Octaves { octaves } => semis += 12 * octaves,
                ModEffect::Degrees { degrees: d } => degrees += d,
                _ => {}
            }
        }
        (semis, degrees)
    }

    /// Звучащие ноты по возрастанию.
    pub fn sounding_notes(&self) -> Vec<u8> {
        let set: BTreeSet<u8> = self.sounding.keys().map(|&(_, n)| n).collect();
        set.into_iter().collect()
    }

    pub fn sounding(&self) -> Vec<(u8, u8)> {
        self.sounding.keys().copied().collect()
    }

    /// Активные слои, начиная с базового.
    pub fn active_layers(&self) -> Vec<&Layer> {
        self.active_layer_indices().into_iter().map(|i| &self.profile.layers[i]).collect()
    }

    /// Зажатые сейчас входы и переключенные модификаторы.
    pub fn pressed(&self) -> Vec<SwitchKey> {
        let mut v: Vec<SwitchKey> = self.held.keys().copied().collect();
        v.extend(self.latched.iter().map(|(k, _)| *k));
        v
    }

    /// Что сделает каждый назначенный вход прямо сейчас, с учётом слоёв и модификаторов.
    /// По этому интерфейс подписывает кнопки на схеме геймпада.
    pub fn preview(&self) -> Vec<ControlView> {
        self.preview_layers(&self.active_layer_indices())
    }

    /// То же для редактора: слой `layer` поверх базового, без модификаторов.
    /// Назначения, пришедшие из базового слоя, помечены его id.
    pub fn preview_layer(&self, layer: &str) -> Option<Vec<ControlView>> {
        let i = *self.layer_ids.get(layer)?;
        let mut layers = vec![0];
        if i != 0 {
            layers.push(i);
        }
        Some(self.preview_layers(&layers))
    }

    /// `layers` — по возрастанию, верхний перекрывает нижние.
    fn preview_layers(&self, layers: &[usize]) -> Vec<ControlView> {
        let pressed = self.pressed();
        let mut seen: Vec<Result<SwitchKey, Axis>> = Vec::new();
        let mut views = Vec::new();
        for &li in layers.iter().rev() {
            let layer = &self.profile.layers[li];
            for b in &layer.bindings {
                let key = match b.input {
                    Input::Axis { axis } => Err(axis),
                    other => Ok(other.switch_key().expect("кнопочный вход")),
                };
                if seen.contains(&key) {
                    continue;
                }
                seen.push(key);
                let role = match &b.action {
                    Action::Note { note, part, latch, .. } => Role::Note {
                        notes: self.notes_for(note, *part),
                        chord: matches!(note, NoteSpec::Chord { .. }),
                        latch: *latch,
                        on: key.is_ok_and(|k| self.latched_notes.contains_key(&k)),
                    },
                    Action::Cc { cc, step, latch, .. } => Role::Cc {
                        cc: *cc,
                        step: *step,
                        latch: *latch,
                        on: key.is_ok_and(|k| self.latched_cc.contains_key(&k)),
                    },
                    Action::Modifier { effect, latch } => Role::Modifier {
                        effect: effect.clone(),
                        latch: *latch,
                        active: key.is_ok_and(|k| pressed.contains(&k)),
                    },
                    Action::Command { command } => Role::Command { command: command.clone() },
                    Action::Control { target, .. } => Role::Control { target: *target },
                };
                views.push(ControlView { input: b.input, layer: layer.id.clone(), role });
            }
        }
        views
    }

    /// Текущие положения осей.
    pub fn axis_values(&self) -> HashMap<Axis, f32> {
        self.axis_values.clone()
    }

    /// Текущий сдвиг в полутонах от модификаторов.
    pub fn transpose(&self) -> i32 {
        self.effects().map(|e| match e {
            ModEffect::Semitones { semitones } => *semitones,
            ModEffect::Octaves { octaves } => 12 * octaves,
            _ => 0,
        }).sum()
    }

    /// Сообщения, воспроизводящие текущее состояние синтезатора (для записи и перезапуска звука).
    pub fn state_messages(&self) -> Vec<MidiMsg> {
        let mut v = self.setup_messages();
        v.extend(self.control_messages());
        v
    }

    // ---------- нажатия ----------

    fn press(&mut self, key: SwitchKey, threshold: f32) {
        let action = self.resolve(key).map(|r| self.binding(r).action.clone());
        let held = match action {
            Some(Action::Note { note, part, velocity, latch }) => {
                // Защёлка: второе нажатие выключает то, что включило первое
                if latch == Latch::Toggle
                    && let Some(notes) = self.latched_notes.remove(&key)
                {
                    for (ch, n) in notes {
                        self.note_off_counted(ch, n);
                    }
                    self.held.insert(key, HeldSwitch { held: Held::Nothing, threshold });
                    return;
                }
                let ch = self.profile.parts[part].channel;
                let vel = self.velocity(velocity);
                let notes: Vec<(u8, u8)> = self.notes_for(&note, part).into_iter().map(|n| (ch, n)).collect();
                for &(ch, n) in &notes {
                    let count = self.sounding.entry((ch, n)).or_insert(0);
                    *count += 1;
                    if *count == 1 {
                        self.emit(MidiMsg::note_on(ch, n, vel));
                    }
                }
                if latch == Latch::Toggle {
                    self.latched_notes.insert(key, notes);
                    Held::Nothing
                } else {
                    Held::Notes(notes)
                }
            }
            Some(Action::Cc { cc, part, on, off, latch, step }) => {
                let ch = self.profile.parts[part].channel;
                if step != 0 {
                    let cur = self.controls.get(&(ch, cc as u16)).copied().unwrap_or(off as i32);
                    self.send_cc(ch, cc, (cur + step as i32).clamp(0, 127) as u8);
                    Held::Nothing
                } else if latch == Latch::Toggle {
                    if self.latched_cc.remove(&key).is_some() {
                        self.send_cc(ch, cc, off);
                    } else {
                        self.latched_cc.insert(key, (ch, cc, off));
                        self.send_cc(ch, cc, on);
                    }
                    Held::Nothing
                } else {
                    self.send_cc(ch, cc, on);
                    Held::Cc { ch, cc, off }
                }
            }
            Some(Action::Modifier { effect, latch: Latch::Hold }) => {
                self.held.insert(key, HeldSwitch { held: Held::Modifier(effect.clone()), threshold });
                self.modifier_changed(&effect);
                return;
            }
            Some(Action::Modifier { effect, latch: Latch::Toggle }) => {
                if let Some(i) = self.latched.iter().position(|(k, _)| *k == key) {
                    self.latched.remove(i);
                } else {
                    self.latched.push((key, effect.clone()));
                }
                self.modifier_changed(&effect);
                Held::Nothing
            }
            Some(Action::Command { command }) => {
                self.run(command);
                Held::Nothing
            }
            Some(Action::Control { .. }) | None => Held::Nothing,
        };
        self.held.insert(key, HeldSwitch { held, threshold });
    }

    fn release(&mut self, key: SwitchKey) {
        let Some(h) = self.held.remove(&key) else { return };
        match h.held {
            Held::Notes(notes) => {
                for (ch, n) in notes {
                    self.note_off_counted(ch, n);
                }
            }
            Held::Modifier(effect) => self.modifier_changed(&effect),
            Held::Cc { ch, cc, off } => self.send_cc(ch, cc, off),
            Held::Nothing => {}
        }
    }

    /// Note Off с учётом того, что ноту может держать несколько нажатий.
    fn note_off_counted(&mut self, ch: u8, n: u8) {
        if let Some(count) = self.sounding.get_mut(&(ch, n)) {
            *count -= 1;
            if *count == 0 {
                self.sounding.remove(&(ch, n));
                self.emit(MidiMsg::note_off(ch, n));
            }
        }
    }

    fn send_cc(&mut self, ch: u8, cc: u8, value: u8) {
        self.controls.insert((ch, cc as u16), value as i32);
        self.rel_values.remove(&(ch, cc as u16));
        self.emit(MidiMsg::cc(ch, cc, value));
    }

    fn run(&mut self, command: Command) {
        match command {
            Command::NextInstrument { part } => self.step_instrument(part, 1),
            Command::PrevInstrument { part } => self.step_instrument(part, -1),
            Command::SetProgram { part, program, bank } => {
                let p = &mut self.profile.parts[part];
                p.program = program.min(127);
                p.bank = bank;
                let ch = p.channel;
                for msg in midi::bank_select(ch, bank) {
                    self.emit(msg);
                }
                self.emit(MidiMsg::program(ch, program.min(127)));
            }
            Command::Panic => self.panic(),
            other => self.out.push(Output::Command(other)),
        }
    }

    fn step_instrument(&mut self, part: usize, step: i32) {
        let p = &mut self.profile.parts[part];
        if p.instruments.is_empty() {
            return;
        }
        let len = p.instruments.len() as i32;
        let next = match p.instruments.iter().position(|&x| x == p.program) {
            Some(i) => (i as i32 + step).rem_euclid(len),
            None => 0,
        };
        p.program = p.instruments[next as usize];
        let msg = MidiMsg::program(p.channel, p.program);
        self.emit(msg);
    }

    // ---------- модификаторы и слои ----------

    fn effects(&self) -> impl Iterator<Item = &ModEffect> {
        let held = self.held.values().filter_map(|h| match &h.held {
            Held::Modifier(e) => Some(e),
            _ => None,
        });
        held.chain(self.latched.iter().map(|(_, e)| e))
    }

    /// Активные слои в порядке приоритета (последний — главный).
    /// Правило: базовый слой внизу; выше — включённые модификаторами, по порядку в профиле;
    /// ещё выше — слои-комбинации, все части которых включены.
    fn active_layer_indices(&self) -> Vec<usize> {
        let mut set = BTreeSet::from([0]);
        for e in self.effects() {
            if let ModEffect::Layer { layer } = e
                && let Some(&i) = self.layer_ids.get(layer)
            {
                set.insert(i);
            }
        }
        let active_ids: Vec<&str> = set.iter().map(|&i| self.profile.layers[i].id.as_str()).collect();
        let combos: Vec<usize> = self
            .profile
            .layers
            .iter()
            .enumerate()
            .filter(|(_, l)| !l.combo.is_empty() && l.combo.iter().all(|id| active_ids.contains(&id.as_str())))
            .map(|(i, _)| i)
            .collect();
        let mut order: Vec<usize> = set.into_iter().filter(|i| !combos.contains(i)).collect();
        order.extend(combos);
        order
    }

    fn modifier_changed(&mut self, effect: &ModEffect) {
        match effect {
            ModEffect::Layer { .. } => {
                for axis in ALL_AXES {
                    self.route_axis(axis, false);
                }
            }
            ModEffect::Sustain => {
                let on = self.effects().any(|e| *e == ModEffect::Sustain);
                if on != self.sustain {
                    self.sustain = on;
                    for ch in self.channels() {
                        self.emit(MidiMsg::cc(ch, 64, if on { 127 } else { 0 }));
                    }
                }
            }
            _ => {}
        }
    }

    // ---------- поиск назначений ----------

    fn binding(&self, r: Route) -> &Binding {
        &self.profile.layers[r.layer].bindings[r.binding]
    }

    /// Ищет назначение сверху вниз по активным слоям.
    fn find(&self, matches: impl Fn(&Input) -> bool) -> Option<Route> {
        for layer in self.active_layer_indices().into_iter().rev() {
            if let Some(binding) = self.profile.layers[layer].bindings.iter().position(|b| matches(&b.input)) {
                return Some(Route { layer, binding });
            }
        }
        None
    }

    fn resolve(&self, key: SwitchKey) -> Option<Route> {
        self.find(|i| i.switch_key() == Some(key))
    }

    // ---------- ноты ----------

    fn notes_for(&self, spec: &NoteSpec, part: usize) -> Vec<u8> {
        let (mods_semis, degrees) = self.offset();
        let semis = mods_semis + self.profile.parts.get(part).map_or(0, |p| p.transpose);
        let music = &self.profile.music;
        let raw: Vec<i32> = match spec {
            NoteSpec::Degree { degree } => vec![music.degree_note(degree + degrees)],
            NoteSpec::Absolute { note } => vec![*note as i32],
            NoteSpec::Chord { degree, chord, intervals, inversion, open, bass } => {
                let root = music.degree_note(degree + degrees);
                let tones: Vec<i32> = if *chord == ChordKind::Custom {
                    let iv = if intervals.is_empty() { &[0, 4, 7][..] } else { &intervals[..] };
                    iv.iter().map(|i| root + i).collect()
                } else {
                    chord.degrees().iter().map(|d| music.degree_note(degree + degrees + d)).collect()
                };
                crate::theory::voice(tones, *inversion, *open, *bass)
            }
        };
        let mut notes: Vec<u8> = raw.into_iter().map(|n| (n + semis).clamp(0, 127) as u8).collect();
        notes.sort_unstable();
        notes.dedup();
        notes
    }

    fn velocity(&self, base: u8) -> u8 {
        let forced = self.effects().filter_map(|e| match e {
            ModEffect::Velocity { velocity } => Some(*velocity),
            _ => None,
        }).last();
        forced.or(self.dyn_velocity).unwrap_or(base).clamp(1, 127)
    }

    // ---------- плавные контроллеры ----------

    /// Подключает ось к назначению из активных слоёв и отправляет значение.
    /// Если назначение сменилось, старая цель возвращается в нейтраль
    /// (кроме CC с подхватом — там значение остаётся, где было).
    fn route_axis(&mut self, axis: Axis, force: bool) {
        let new = self.find(|i| *i == Input::Axis { axis });
        let old = self.routes.get(&axis).copied();
        if old != new
            && let Some(old) = old
        {
            self.routes.remove(&axis);
            self.smooth.remove(&axis);
            self.rates.remove(&axis);
            self.pickup.remove(&axis);
            let (keep, shape) = match &self.binding(old).action {
                Action::Control { takeover, relative, target, shape, .. } => {
                    (*relative || (*takeover && !matches!(target, ControlTarget::PitchBend)), *shape)
                }
                _ => (true, AxisShape::default()),
            };
            if !keep {
                self.send_shaped(axis, old, shape_value(0.0, shape), 0.0, false);
            }
        }
        let Some(new) = new else { return };
        self.routes.insert(axis, new);
        let changed = old != Some(new);
        if changed
            && let Action::Control { takeover: true, target, part, .. } = &self.binding(new).action
            && let Some(key) = self.control_key(*target, *part)
            && let Some(&last) = self.controls.get(&key)
        {
            self.pickup.insert(axis, Pickup { target: last, last: None });
        }
        let force = (force || changed) && !self.pickup.contains_key(&axis);
        self.update_axis(axis, new, force);
    }

    /// Новое положение оси → значение (сразу, через сглаживание или как скорость).
    fn update_axis(&mut self, axis: Axis, route: Route, force: bool) {
        let Action::Control { target, shape, relative, .. } = self.binding(route).action.clone() else { return };
        let raw = self.axis_values.get(&axis).copied().unwrap_or(0.0);
        let v = shape_value(raw, shape);
        if relative && matches!(target, ControlTarget::Cc { .. } | ControlTarget::Pressure { .. }) {
            self.rates.insert(axis, v);
            return;
        }
        if shape.smoothing > 0.0 && !force {
            let cur = self.smooth.get(&axis).map_or(v, |(c, _)| *c);
            self.smooth.insert(axis, (cur, v));
            return;
        }
        self.smooth.insert(axis, (v, v));
        self.send_shaped(axis, route, v, raw, force);
    }

    fn control_key(&self, target: ControlTarget, part: usize) -> Option<ControlKey> {
        let ch = self.profile.parts.get(part)?.channel;
        match target {
            ControlTarget::PitchBend => Some((ch, BEND_KEY)),
            ControlTarget::Cc { cc, .. } => Some((ch, cc as u16)),
            ControlTarget::Pressure { .. } => Some((ch, PRESSURE_KEY)),
            ControlTarget::Velocity { .. } => None,
        }
    }

    /// Отправить значение `v` (после формы) в цель назначения.
    fn send_shaped(&mut self, axis: Axis, route: Route, v: f32, raw: f32, force: bool) {
        let Action::Control { target, shape, part, .. } = self.binding(route).action.clone() else { return };
        let unipolar = if shape.mode == AxisMode::Bipolar { (v + 1.0) / 2.0 } else { v };
        let val = match target {
            ControlTarget::PitchBend => (BEND_CENTER + (v * 8191.0).round() as i32).clamp(0, 16383),
            ControlTarget::Cc { min, max, .. } | ControlTarget::Pressure { min, max } => lerp(min, max, unipolar),
            ControlTarget::Velocity { min, max } => {
                self.dyn_velocity = (raw != 0.0).then(|| lerp(min, max, unipolar) as u8);
                return;
            }
        };
        // Подхват: молчим, пока ось не дойдёт до прежнего значения (или не перескочит его)
        if let Some(p) = self.pickup.get_mut(&axis) {
            let window = if matches!(target, ControlTarget::PitchBend) { PICKUP_WINDOW * 128 } else { PICKUP_WINDOW };
            let crossed = p.last.is_some_and(|l| (l - p.target).signum() != (val - p.target).signum());
            if (val - p.target).abs() <= window || crossed {
                self.pickup.remove(&axis);
            } else {
                p.last = Some(val);
                return;
            }
        }
        let key = self.control_key(target, part).expect("у цели есть ключ");
        let last = self.controls.get(&key).copied();
        let send = match target {
            ControlTarget::PitchBend => {
                let last = last.unwrap_or(BEND_CENTER);
                let edge = val == BEND_CENTER || val == 0 || val == 16383;
                force || (val != last && ((val - last).abs() >= BEND_STEP || edge))
            }
            _ => force || last != Some(val),
        };
        if send {
            self.controls.insert(key, val);
            self.rel_values.remove(&key);
            self.emit(control_message(key, val));
        }
    }

    /// Относительный режим: сдвинуть значение на `amount` диапазона (1.0 — весь диапазон).
    fn nudge_relative(&mut self, target: ControlTarget, part: usize, amount: f32) {
        let (min, max) = match target {
            ControlTarget::Cc { min, max, .. } | ControlTarget::Pressure { min, max } => (min as f32, max as f32),
            _ => return,
        };
        let Some(key) = self.control_key(target, part) else { return };
        let (lo, hi) = (min.min(max), min.max(max));
        let cur = self
            .rel_values
            .get(&key)
            .copied()
            .or_else(|| self.controls.get(&key).map(|v| *v as f32))
            .unwrap_or(lo);
        let next = (cur + amount * (max - min)).clamp(lo, hi);
        self.rel_values.insert(key, next);
        let val = next.round() as i32;
        if self.controls.get(&key) != Some(&val) {
            self.controls.insert(key, val);
            self.emit(control_message(key, val));
        }
    }

    fn control_messages(&self) -> Vec<MidiMsg> {
        self.controls.iter().map(|(&k, &v)| control_message(k, v)).collect()
    }

    // ---------- служебное ----------

    fn channels(&self) -> Vec<u8> {
        let set: BTreeSet<u8> = self.profile.parts.iter().map(|p| p.channel).collect();
        set.into_iter().collect()
    }

    fn setup_messages(&self) -> Vec<MidiMsg> {
        let mut v = Vec::new();
        for p in &self.profile.parts {
            v.extend(midi::bank_select(p.channel, p.bank));
            v.push(MidiMsg::program(p.channel, p.program));
            v.push(MidiMsg::cc(p.channel, 7, p.volume));
            v.extend(midi::bend_range(p.channel, p.bend_range));
            for (cc, value) in p.sound.controllers() {
                v.push(MidiMsg::cc(p.channel, cc, value));
            }
        }
        v
    }

    /// Можно ли применить `next` без сброса: поменялось только звучание (партии и имя).
    pub fn can_retune(&self, next: &Profile) -> bool {
        let cur = &self.profile;
        cur.layers == next.layers
            && cur.music == next.music
            && cur.parts.len() == next.parts.len()
            && cur.parts.iter().zip(&next.parts).all(|(a, b)| a.channel == b.channel && a.transpose == b.transpose)
    }

    /// Применить новое звучание, не обрывая ноты (см. `can_retune`).
    pub fn retune(&mut self, next: Profile) {
        self.profile = next;
        for msg in self.setup_messages() {
            self.emit(msg);
        }
        // Вернуть то, что сейчас задают оси (настройка звука могла перекрыть, например, яркость)
        for msg in self.control_messages() {
            self.emit(msg);
        }
    }

    fn emit(&mut self, msg: MidiMsg) {
        self.out.push(Output::Midi(msg));
    }
}

fn control_message((ch, key): ControlKey, val: i32) -> MidiMsg {
    match key {
        BEND_KEY => MidiMsg::bend(ch, val as u16),
        PRESSURE_KEY => MidiMsg::pressure(ch, val as u8),
        cc => MidiMsg::cc(ch, cc as u8, val as u8),
    }
}

/// Значение оси после мёртвой зоны, кривой и режима: −1..1 для Bipolar, иначе 0..1.
pub fn shape_value(raw: f32, shape: AxisShape) -> f32 {
    let v = if shape.invert { -raw } else { raw };
    let dz = shape.deadzone.clamp(0.0, 0.95);
    let a = v.abs();
    let a = if a < dz { 0.0 } else { ((a - dz) / (1.0 - dz)).min(1.0) };
    let k = shape.curve.max(0.05);
    let a = match shape.curve_kind {
        CurveKind::Power => a.powf(k),
        CurveKind::S => {
            let p = a.powf(k);
            let q = (1.0 - a).powf(k);
            if p + q > 0.0 { p / (p + q) } else { a }
        }
    };
    let v = a.copysign(v);
    match shape.mode {
        AxisMode::Bipolar => v,
        AxisMode::Absolute => v.abs(),
        AxisMode::Positive => v.max(0.0),
        AxisMode::Negative => (-v).max(0.0),
    }
}

fn lerp(min: u8, max: u8, t: f32) -> i32 {
    (min as f32 + (max as f32 - min as f32) * t.clamp(0.0, 1.0)).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::classic;

    fn midi(m: &mut Mapper) -> Vec<MidiMsg> {
        m.drain().filter_map(|o| if let Output::Midi(x) = o { Some(x) } else { None }).collect()
    }

    fn notes_on(m: &mut Mapper) -> Vec<u8> {
        midi(m).into_iter().filter(|x| x.status() == 0x90).map(|x| x.bytes[1]).collect()
    }

    fn btn(m: &mut Mapper, button: Button, down: bool) {
        m.handle(InputEvent::Button { button, down });
    }

    fn axis(m: &mut Mapper, axis: Axis, value: f32) {
        m.handle(InputEvent::Axis { axis, value });
    }

    #[test]
    fn s_curve_is_soft_at_both_ends() {
        let s = AxisShape { deadzone: 0.0, curve: 2.0, curve_kind: CurveKind::S, mode: AxisMode::Positive, ..Default::default() };
        assert!((shape_value(0.5, s) - 0.5).abs() < 1e-6, "середина остаётся серединой");
        assert!(shape_value(0.1, s) < 0.1 / 2.0, "у начала мягче линейной");
        assert!(shape_value(0.9, s) > 0.9 + 0.05, "у конца быстрее добирает до максимума");
        assert_eq!(shape_value(1.0, s), 1.0);
        assert_eq!(shape_value(0.0, s), 0.0);
        let lin = AxisShape { curve: 1.0, ..s };
        assert!((shape_value(0.3, lin) - 0.3).abs() < 1e-6, "S-кривая с силой 1 — линейная");
    }

    fn fresh() -> Mapper {
        let mut m = Mapper::new(classic());
        m.drain();
        m
    }

    #[test]
    fn setup_sets_instrument_and_quiet_expression() {
        let mut m = Mapper::new(classic());
        let msgs = midi(&mut m);
        assert!(msgs.contains(&MidiMsg::program(0, 0)));
        assert!(msgs.contains(&MidiMsg::cc(0, 11, 40)));
        assert!(msgs.contains(&MidiMsg::bend(0, 8192)));
    }

    #[test]
    fn note_on_and_off() {
        let mut m = fresh();
        btn(&mut m, Button::South, true);
        assert_eq!(midi(&mut m), [MidiMsg::note_on(0, 69, 100)]);
        assert_eq!(m.sounding_notes(), [69]);
        btn(&mut m, Button::South, false);
        assert_eq!(midi(&mut m), [MidiMsg::note_off(0, 69)]);
        assert!(m.sounding_notes().is_empty());
    }

    #[test]
    fn repeated_down_does_not_retrigger() {
        let mut m = fresh();
        btn(&mut m, Button::South, true);
        btn(&mut m, Button::South, true);
        assert_eq!(notes_on(&mut m), [69]);
    }

    #[test]
    fn release_turns_off_the_note_that_was_played() {
        let mut m = fresh();
        btn(&mut m, Button::LeftShoulder, true);
        btn(&mut m, Button::South, true);
        assert_eq!(notes_on(&mut m), [57]);
        btn(&mut m, Button::LeftShoulder, false);
        btn(&mut m, Button::South, false);
        assert_eq!(midi(&mut m), [MidiMsg::note_off(0, 57)]);
    }

    #[test]
    fn trigger_as_modifier_with_hysteresis() {
        let mut m = fresh();
        axis(&mut m, Axis::LeftTrigger, 0.6);
        btn(&mut m, Button::DpadLeft, true);
        assert_eq!(notes_on(&mut m), [61]);
        btn(&mut m, Button::DpadLeft, false);
        axis(&mut m, Axis::LeftTrigger, 0.47);
        assert_eq!(m.transpose(), 1, "у порога модификатор ещё держится");
        axis(&mut m, Axis::LeftTrigger, 0.3);
        assert_eq!(m.transpose(), 0);
    }

    #[test]
    fn toggle_layer_plays_chords() {
        let mut m = fresh();
        btn(&mut m, Button::LeftStick, true);
        btn(&mut m, Button::LeftStick, false);
        btn(&mut m, Button::South, true);
        assert_eq!(notes_on(&mut m), [69, 72, 76]);
        btn(&mut m, Button::South, false);
        btn(&mut m, Button::LeftStick, true);
        btn(&mut m, Button::LeftStick, false);
        m.drain();
        btn(&mut m, Button::South, true);
        assert_eq!(notes_on(&mut m), [69]);
    }

    #[test]
    fn axes_send_controls() {
        let mut m = fresh();
        axis(&mut m, Axis::RightTrigger, 1.0);
        assert_eq!(midi(&mut m), [MidiMsg::cc(0, 11, 127)]);
        axis(&mut m, Axis::RightX, 1.0);
        assert_eq!(midi(&mut m), [MidiMsg::bend(0, 16383)]);
        axis(&mut m, Axis::RightX, 0.05);
        assert_eq!(midi(&mut m), [MidiMsg::bend(0, 8192)], "мёртвая зона возвращает в центр");
        axis(&mut m, Axis::RightY, -1.0);
        assert_eq!(midi(&mut m), [MidiMsg::cc(0, 1, 127)], "вибрато в обе стороны");
    }

    #[test]
    fn commands() {
        let mut m = fresh();
        btn(&mut m, Button::Back, true);
        assert_eq!(midi(&mut m), [MidiMsg::program(0, 11)]);
        btn(&mut m, Button::Start, true);
        assert_eq!(m.drain().collect::<Vec<_>>(), [Output::Command(Command::ToggleRecord)]);
    }

    /// X — модификатор слоя, в котором правый стик управляет яркостью вместо подтяжки.
    fn layered_profile() -> Profile {
        let mut p = classic();
        p.layers[0].bindings.retain(|b| b.input != Input::Button { button: Button::West });
        p.layers[0].bindings.push(Binding {
            input: Input::Button { button: Button::West },
            action: Action::Modifier { effect: ModEffect::Layer { layer: "fx".into() }, latch: Latch::Hold },
        });
        p.layers.push(Layer {
            id: "fx".into(),
            name: "Эффекты".into(),
            bindings: vec![Binding {
                input: Input::Axis { axis: Axis::RightX },
                action: Action::Control {
                    target: ControlTarget::Cc { cc: 74, min: 0, max: 127 },
                    shape: AxisShape { deadzone: 0.0, ..Default::default() },
                    part: 0,
                    takeover: false,
                    relative: false,
                },
            }],
            combo: vec![],
        });
        p.validate().unwrap();
        p
    }

    #[test]
    fn layer_switch_reroutes_axis_and_resets_old_target() {
        let mut m = Mapper::new(layered_profile());
        m.drain();
        axis(&mut m, Axis::RightX, 1.0);
        assert_eq!(midi(&mut m), [MidiMsg::bend(0, 16383)]);
        btn(&mut m, Button::West, true);
        assert_eq!(midi(&mut m), [MidiMsg::bend(0, 8192), MidiMsg::cc(0, 74, 127)]);
        assert_eq!(m.active_layers().len(), 2);
        btn(&mut m, Button::West, false);
        assert_eq!(midi(&mut m), [MidiMsg::cc(0, 74, 64), MidiMsg::bend(0, 16383)]);
    }

    #[test]
    fn shared_note_is_refcounted() {
        let mut p = classic();
        p.layers[0].bindings.push(Binding {
            input: Input::Button { button: Button::Misc1 },
            action: Action::Note { note: NoteSpec::Absolute { note: 69 }, part: 0, velocity: 100, latch: Latch::Hold },
        });
        let mut m = Mapper::new(p);
        m.drain();
        btn(&mut m, Button::South, true);
        btn(&mut m, Button::Misc1, true);
        btn(&mut m, Button::South, false);
        assert_eq!(midi(&mut m), [MidiMsg::note_on(0, 69, 100)], "нота ещё держится второй кнопкой");
        btn(&mut m, Button::Misc1, false);
        assert_eq!(midi(&mut m), [MidiMsg::note_off(0, 69)]);
    }

    #[test]
    fn panic_silences_and_unlatches() {
        let mut m = fresh();
        btn(&mut m, Button::LeftStick, true);
        btn(&mut m, Button::South, true);
        m.drain();
        m.panic();
        let msgs = midi(&mut m);
        assert!(msgs.contains(&MidiMsg::cc(0, 123, 0)));
        assert!(m.sounding_notes().is_empty());
        assert_eq!(m.active_layers().len(), 1);
    }

    #[test]
    fn preview_follows_modifiers_and_layers() {
        let mut m = fresh();
        let south = |m: &Mapper| {
            m.preview().into_iter().find(|v| v.input == Input::Button { button: Button::South }).unwrap()
        };
        let note = |notes: Vec<u8>, chord: bool| Role::Note { notes, chord, latch: Latch::Hold, on: false };
        assert_eq!(south(&m).role, note(vec![69], false));
        btn(&mut m, Button::LeftShoulder, true);
        assert_eq!(south(&m).role, note(vec![57], false));
        let lb = m.preview().into_iter().find(|v| v.input == Input::Button { button: Button::LeftShoulder }).unwrap();
        assert!(matches!(lb.role, Role::Modifier { active: true, .. }));
        btn(&mut m, Button::LeftShoulder, false);
        btn(&mut m, Button::LeftStick, true);
        let v = south(&m);
        assert_eq!(v.layer, "chords");
        assert_eq!(v.role, note(vec![69, 72, 76], true));
        // назначения из нижнего слоя видны, если верхний их не перекрывает
        assert!(m.preview().iter().any(|v| v.input == Input::Axis { axis: Axis::RightX }));
        let json = serde_json::to_value(v).unwrap();
        assert_eq!(json["role"], "note");
        assert_eq!(json["input"]["button"], "south");
    }

    #[test]
    fn preview_layer_for_editor() {
        let m = fresh();
        let chords = m.preview_layer("chords").unwrap();
        let south = chords.iter().find(|v| v.input == Input::Button { button: Button::South }).unwrap();
        assert_eq!(south.layer, "chords");
        // назначения базового слоя видны как унаследованные
        let lb = chords.iter().find(|v| v.input == Input::Button { button: Button::LeftShoulder }).unwrap();
        assert_eq!(lb.layer, "base");
        assert!(m.preview_layer("nope").is_none());
    }

    #[test]
    fn part_transpose_and_sound_setup() {
        let mut p = classic();
        p.parts[0].transpose = -12;
        p.parts[0].sound.brightness = 20;
        let mut m = Mapper::new(p);
        assert!(midi(&mut m).contains(&MidiMsg::cc(0, 74, 20)));
        btn(&mut m, Button::South, true);
        assert_eq!(notes_on(&mut m), [57]);
    }

    #[test]
    fn retune_keeps_sounding_notes() {
        let mut m = fresh();
        btn(&mut m, Button::South, true);
        m.drain();
        let mut next = classic();
        next.parts[0].sound.reverb = 100;
        next.parts[0].program = 40;
        assert!(m.can_retune(&next));
        m.retune(next);
        let msgs = midi(&mut m);
        assert!(msgs.contains(&MidiMsg::cc(0, 91, 100)));
        assert!(msgs.contains(&MidiMsg::program(0, 40)));
        assert!(!msgs.iter().any(|x| x.status() == 0x80 || x.bytes[1] == 123), "ноты не обрываются");
        assert_eq!(m.sounding_notes(), [69]);
        let mut other = classic();
        other.music.root = 2;
        assert!(!m.can_retune(&other), "смена лада — полная перезагрузка");
    }

    /// Профиль «Классика» плюс назначения на свободные кнопки (Misc1 и лопатки).
    fn with(bindings: Vec<Binding>) -> Mapper {
        let mut p = classic();
        p.layers[0].bindings.extend(bindings);
        p.validate().unwrap();
        let mut m = Mapper::new(p);
        m.drain();
        m
    }

    fn bind(button: Button, action: Action) -> Binding {
        Binding { input: Input::Button { button }, action }
    }

    fn axis_bind(axis: Axis, target: ControlTarget, shape: AxisShape, takeover: bool, relative: bool) -> Binding {
        Binding { input: Input::Axis { axis }, action: Action::Control { target, shape, part: 0, takeover, relative } }
    }

    #[test]
    fn hysteresis_is_symmetric_around_threshold() {
        let mut m = fresh();
        axis(&mut m, Axis::LeftTrigger, 0.52);
        assert_eq!(m.transpose(), 0, "0.52 ещё не нажатие (порог 0.5 + 0.05)");
        axis(&mut m, Axis::LeftTrigger, 0.56);
        assert_eq!(m.transpose(), 1);
        axis(&mut m, Axis::LeftTrigger, 0.46);
        assert_eq!(m.transpose(), 1, "0.46 ещё держит");
        axis(&mut m, Axis::LeftTrigger, 0.44);
        assert_eq!(m.transpose(), 0);
    }

    #[test]
    fn latched_note_toggles_and_survives_modifier_change() {
        let mut m = with(vec![bind(
            Button::Misc1,
            Action::Note { note: NoteSpec::Degree { degree: 0 }, part: 0, velocity: 100, latch: Latch::Toggle },
        )]);
        btn(&mut m, Button::Misc1, true);
        btn(&mut m, Button::Misc1, false);
        assert_eq!(midi(&mut m), [MidiMsg::note_on(0, 60, 100)], "отпускание не гасит защёлку");
        // включили октаву вниз — второе нажатие гасит именно C4, а не C3
        btn(&mut m, Button::LeftShoulder, true);
        btn(&mut m, Button::Misc1, true);
        assert_eq!(midi(&mut m), [MidiMsg::note_off(0, 60)]);
        assert!(m.sounding_notes().is_empty());
    }

    #[test]
    fn panic_releases_latched_notes_and_cc() {
        let mut m = with(vec![
            bind(Button::Misc1, Action::Note { note: NoteSpec::Absolute { note: 40 }, part: 0, velocity: 100, latch: Latch::Toggle }),
            bind(Button::LeftPaddle1, Action::Cc { cc: 80, part: 0, on: 127, off: 0, latch: Latch::Toggle, step: 0 }),
        ]);
        btn(&mut m, Button::Misc1, true);
        btn(&mut m, Button::LeftPaddle1, true);
        m.drain();
        m.panic();
        let msgs = midi(&mut m);
        assert!(msgs.contains(&MidiMsg::note_off(0, 40)));
        assert!(msgs.contains(&MidiMsg::cc(0, 80, 0)));
    }

    #[test]
    fn cc_button_momentary_toggle_and_step() {
        let mut m = with(vec![
            bind(Button::LeftPaddle1, Action::Cc { cc: 64, part: 0, on: 127, off: 0, latch: Latch::Hold, step: 0 }),
            bind(Button::LeftPaddle2, Action::Cc { cc: 85, part: 0, on: 100, off: 10, latch: Latch::Toggle, step: 0 }),
            bind(Button::RightPaddle1, Action::Cc { cc: 74, part: 0, on: 127, off: 64, latch: Latch::Hold, step: 8 }),
        ]);
        btn(&mut m, Button::LeftPaddle1, true);
        btn(&mut m, Button::LeftPaddle1, false);
        assert_eq!(midi(&mut m), [MidiMsg::cc(0, 64, 127), MidiMsg::cc(0, 64, 0)]);
        btn(&mut m, Button::LeftPaddle2, true);
        btn(&mut m, Button::LeftPaddle2, false);
        btn(&mut m, Button::LeftPaddle2, true);
        assert_eq!(midi(&mut m), [MidiMsg::cc(0, 85, 100), MidiMsg::cc(0, 85, 10)]);
        // шаг: от «выкл» 64 вверх по 8
        btn(&mut m, Button::RightPaddle1, true);
        btn(&mut m, Button::RightPaddle1, false);
        btn(&mut m, Button::RightPaddle1, true);
        assert_eq!(midi(&mut m), [MidiMsg::cc(0, 74, 72), MidiMsg::cc(0, 74, 80)]);
    }

    #[test]
    fn set_program_and_pressure() {
        let mut m = with(vec![
            bind(Button::Misc1, Action::Command { command: Command::SetProgram { part: 0, program: 40, bank: 0 } }),
            axis_bind(
                Axis::LeftTrigger,
                ControlTarget::Pressure { min: 0, max: 127 },
                AxisShape { deadzone: 0.0, mode: AxisMode::Positive, ..Default::default() },
                false,
                false,
            ),
        ]);
        btn(&mut m, Button::Misc1, true);
        let msgs = midi(&mut m);
        assert_eq!(msgs.last(), Some(&MidiMsg::program(0, 40)));
        assert_eq!(m.profile().parts[0].program, 40);
        axis(&mut m, Axis::LeftTrigger, 1.0);
        assert!(midi(&mut m).contains(&MidiMsg::pressure(0, 127)));
    }

    #[test]
    fn chord_voicing_and_custom_intervals() {
        let chord = |chord: ChordKind, intervals: Vec<i32>, inversion: u8, bass: bool| Action::Note {
            note: NoteSpec::Chord { degree: 0, chord, intervals, inversion, open: false, bass },
            part: 0,
            velocity: 100,
            latch: Latch::Hold,
        };
        let mut m = with(vec![
            bind(Button::Misc1, chord(ChordKind::Triad, vec![], 1, false)),
            bind(Button::LeftPaddle1, chord(ChordKind::Custom, vec![0, 3, 7, 10], 0, true)),
        ]);
        btn(&mut m, Button::Misc1, true);
        assert_eq!(notes_on(&mut m), [64, 67, 72], "до-мажор, первое обращение");
        btn(&mut m, Button::LeftPaddle1, true);
        assert_eq!(notes_on(&mut m), [48, 60, 63, 70], "Cm7 своими интервалами и бас (G4 уже звучит)");
    }

    #[test]
    fn combo_layer_has_priority_over_its_parts() {
        let mut p = classic();
        let lay = |id: &str, combo: Vec<String>, note: u8| Layer {
            id: id.into(),
            name: id.into(),
            bindings: vec![bind(Button::South, Action::Note { note: NoteSpec::Absolute { note }, part: 0, velocity: 100, latch: Latch::Hold })],
            combo,
        };
        p.layers[0].bindings.retain(|b| !matches!(b.input, Input::Button { button: Button::LeftShoulder | Button::RightShoulder }));
        for (button, layer) in [(Button::LeftShoulder, "lb"), (Button::RightShoulder, "rb")] {
            p.layers[0].bindings.push(Binding {
                input: Input::Button { button },
                action: Action::Modifier { effect: ModEffect::Layer { layer: layer.into() }, latch: Latch::Hold },
            });
        }
        // комбинация стоит в списке раньше своих частей — приоритет всё равно у неё
        p.layers.push(lay("both", vec!["lb".into(), "rb".into()], 90));
        p.layers.push(lay("lb", vec![], 80));
        p.layers.push(lay("rb", vec![], 81));
        p.validate().unwrap();
        let mut m = Mapper::new(p);
        m.drain();
        btn(&mut m, Button::LeftShoulder, true);
        btn(&mut m, Button::South, true);
        btn(&mut m, Button::South, false);
        assert_eq!(notes_on(&mut m), [80]);
        btn(&mut m, Button::RightShoulder, true);
        btn(&mut m, Button::South, true);
        assert_eq!(notes_on(&mut m), [90], "LB+RB");
    }

    #[test]
    fn smoothing_moves_gradually() {
        let mut m = with(vec![axis_bind(
            Axis::LeftX,
            ControlTarget::Cc { cc: 20, min: 0, max: 127 },
            AxisShape { deadzone: 0.0, mode: AxisMode::Positive, smoothing: 50.0, ..Default::default() },
            false,
            false,
        )]);
        m.drain();
        axis(&mut m, Axis::LeftX, 1.0);
        assert!(midi(&mut m).is_empty(), "без хода времени значение не прыгает");
        assert!(m.needs_tick());
        m.tick(Duration::from_millis(10));
        let first = midi(&mut m);
        let v = first[0].bytes[2];
        assert!(v > 10 && v < 60, "за 10 мс при сглаживании 50 мс — часть пути: {v}");
        for _ in 0..100 {
            m.tick(Duration::from_millis(10));
        }
        assert_eq!(midi(&mut m).last(), Some(&MidiMsg::cc(0, 20, 127)));
        assert!(!m.needs_tick());
    }

    #[test]
    fn takeover_waits_for_pickup_after_layer_switch() {
        // базовый слой: правый стик Y → CC74; слой «fx» на кнопке X: левый стик X → тот же CC74 с подхватом
        let mut p = classic();
        p.layers[0].bindings.retain(|b| !matches!(b.input, Input::Axis { axis: Axis::RightY } | Input::Button { button: Button::West }));
        p.layers[0].bindings.push(axis_bind(
            Axis::RightY,
            ControlTarget::Cc { cc: 74, min: 0, max: 127 },
            AxisShape { deadzone: 0.0, mode: AxisMode::Positive, ..Default::default() },
            false,
            false,
        ));
        p.layers[0].bindings.push(Binding {
            input: Input::Button { button: Button::West },
            action: Action::Modifier { effect: ModEffect::Layer { layer: "fx".into() }, latch: Latch::Toggle },
        });
        p.layers.push(Layer {
            id: "fx".into(),
            name: "fx".into(),
            bindings: vec![axis_bind(
                Axis::LeftX,
                ControlTarget::Cc { cc: 74, min: 0, max: 127 },
                AxisShape { deadzone: 0.0, mode: AxisMode::Positive, ..Default::default() },
                true,
                false,
            )],
            combo: vec![],
        });
        p.validate().unwrap();
        let mut m = Mapper::new(p);
        axis(&mut m, Axis::RightY, 0.8); // CC74 ≈ 102
        axis(&mut m, Axis::LeftX, 0.1);
        btn(&mut m, Button::West, true); // включаем слой fx
        m.drain();
        axis(&mut m, Axis::LeftX, 0.3);
        assert!(midi(&mut m).is_empty(), "стик ниже прежнего значения — не прыгаем");
        axis(&mut m, Axis::LeftX, 0.81);
        let msgs = midi(&mut m);
        assert_eq!(msgs.len(), 1, "подхватили и пошли: {msgs:?}");
        assert_eq!(msgs[0].bytes[1], 74);
    }

    #[test]
    fn relative_axis_changes_value_over_time() {
        let mut m = with(vec![axis_bind(
            Axis::LeftX,
            ControlTarget::Cc { cc: 21, min: 0, max: 100 },
            AxisShape { deadzone: 0.0, ..Default::default() },
            false,
            true,
        )]);
        axis(&mut m, Axis::LeftX, 1.0);
        assert!(midi(&mut m).is_empty());
        m.tick(Duration::from_millis(500));
        assert_eq!(midi(&mut m), [MidiMsg::cc(0, 21, 50)], "полное отклонение — весь диапазон за секунду");
        axis(&mut m, Axis::LeftX, 0.0);
        m.tick(Duration::from_millis(500));
        assert!(midi(&mut m).is_empty(), "стик в центре — значение стоит");
        axis(&mut m, Axis::LeftX, -1.0);
        m.tick(Duration::from_secs(2));
        assert_eq!(midi(&mut m), [MidiMsg::cc(0, 21, 0)]);
    }

    #[test]
    fn shape_curve_and_modes() {
        let s = AxisShape { deadzone: 0.0, curve: 2.0, invert: false, mode: AxisMode::Bipolar, smoothing: 0.0, curve_kind: CurveKind::Power };
        assert!((shape_value(-0.5, s) + 0.25).abs() < 1e-6);
        let s = AxisShape { mode: AxisMode::Negative, deadzone: 0.0, ..s };
        assert_eq!(shape_value(0.5, s), 0.0);
    }
}
