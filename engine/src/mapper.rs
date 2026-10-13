//! Движок раскладок: события геймпада → MIDI. Без ввода-вывода, поэтому покрыт тестами.

use std::collections::{BTreeSet, HashMap};

use crate::midi::{self, MidiMsg};
use crate::model::*;

/// Нормализованное событие ввода. Стики −1..1 (вверх/вправо — плюс), курки 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputEvent {
    Button { button: Button, down: bool },
    Axis { axis: Axis, value: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    Midi(MidiMsg),
    ToggleRecord,
}

/// Насколько ось должна вернуться ниже порога, чтобы «кнопка» на ней отпустилась.
/// Без этого курок у порога дребезжит.
const SWITCH_HYSTERESIS: f32 = 0.05;
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
                        && amount >= threshold
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

    /// Заглушить всё и сбросить переключенные модификаторы.
    pub fn panic(&mut self) {
        self.release_all();
        let latched: Vec<ModEffect> = self.latched.drain(..).map(|(_, e)| e).collect();
        for effect in latched {
            self.modifier_changed(&effect);
        }
        self.sounding.clear();
        for ch in self.channels() {
            self.emit(MidiMsg::cc(ch, 123, 0));
        }
    }

    // ---------- состояние для интерфейса ----------

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
        for (&(ch, key), &val) in &self.controls {
            v.push(if key == BEND_KEY { MidiMsg::bend(ch, val as u16) } else { MidiMsg::cc(ch, key as u8, val as u8) });
        }
        v
    }

    // ---------- нажатия ----------

    fn press(&mut self, key: SwitchKey, threshold: f32) {
        let action = self.resolve(key).map(|r| self.binding(r).action.clone());
        let held = match action {
            Some(Action::Note { note, part, velocity }) => {
                let ch = self.profile.parts[part].channel;
                let vel = self.velocity(velocity);
                let notes: Vec<(u8, u8)> = self.notes_for(note).into_iter().map(|n| (ch, n)).collect();
                for &(ch, n) in &notes {
                    let count = self.sounding.entry((ch, n)).or_insert(0);
                    *count += 1;
                    if *count == 1 {
                        self.emit(MidiMsg::note_on(ch, n, vel));
                    }
                }
                Held::Notes(notes)
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
                    if let Some(count) = self.sounding.get_mut(&(ch, n)) {
                        *count -= 1;
                        if *count == 0 {
                            self.sounding.remove(&(ch, n));
                            self.emit(MidiMsg::note_off(ch, n));
                        }
                    }
                }
            }
            Held::Modifier(effect) => self.modifier_changed(&effect),
            Held::Nothing => {}
        }
    }

    fn run(&mut self, command: Command) {
        match command {
            Command::NextInstrument { part } => self.step_instrument(part, 1),
            Command::PrevInstrument { part } => self.step_instrument(part, -1),
            Command::ToggleRecord => self.out.push(Output::ToggleRecord),
            Command::Panic => self.panic(),
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

    fn active_layer_indices(&self) -> Vec<usize> {
        let mut set = BTreeSet::from([0]);
        for e in self.effects() {
            if let ModEffect::Layer { layer } = e
                && let Some(&i) = self.layer_ids.get(layer)
            {
                set.insert(i);
            }
        }
        set.into_iter().collect()
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

    fn notes_for(&self, spec: NoteSpec) -> Vec<u8> {
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
        let music = &self.profile.music;
        let raw: Vec<i32> = match spec {
            NoteSpec::Degree { degree } => vec![music.degree_note(degree + degrees)],
            NoteSpec::Absolute { note } => vec![note as i32],
            NoteSpec::Chord { degree, chord } => {
                chord.degrees().iter().map(|d| music.degree_note(degree + degrees + d)).collect()
            }
        };
        let mut notes: Vec<u8> = raw.into_iter().map(|n| (n + semis).clamp(0, 127) as u8).collect();
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
    /// Если назначение сменилось, старая цель возвращается в нейтраль.
    fn route_axis(&mut self, axis: Axis, force: bool) {
        let new = self.find(|i| *i == Input::Axis { axis });
        let old = self.routes.get(&axis).copied();
        if old != new
            && let Some(old) = old
        {
            self.apply_control(old, 0.0, false);
            self.routes.remove(&axis);
        }
        if let Some(new) = new {
            self.routes.insert(axis, new);
            let value = self.axis_values.get(&axis).copied().unwrap_or(0.0);
            self.apply_control(new, value, force || old != Some(new));
        }
    }

    fn apply_control(&mut self, route: Route, raw: f32, force: bool) {
        let Action::Control { target, shape, part } = self.binding(route).action else { return };
        let v = shape_value(raw, shape);
        let unipolar = if shape.mode == AxisMode::Bipolar { (v + 1.0) / 2.0 } else { v };
        let ch = self.profile.parts[part].channel;
        match target {
            ControlTarget::PitchBend => {
                let val = (BEND_CENTER + (v * 8191.0).round() as i32).clamp(0, 16383);
                let last = self.controls.get(&(ch, BEND_KEY)).copied().unwrap_or(BEND_CENTER);
                let edge = val == BEND_CENTER || val == 0 || val == 16383;
                if force || (val != last && ((val - last).abs() >= BEND_STEP || edge)) {
                    self.controls.insert((ch, BEND_KEY), val);
                    self.emit(MidiMsg::bend(ch, val as u16));
                }
            }
            ControlTarget::Cc { cc, min, max } => {
                let val = lerp(min, max, unipolar);
                if force || self.controls.get(&(ch, cc as u16)) != Some(&val) {
                    self.controls.insert((ch, cc as u16), val);
                    self.emit(MidiMsg::cc(ch, cc, val as u8));
                }
            }
            ControlTarget::Velocity { min, max } => {
                self.dyn_velocity = (raw != 0.0).then(|| lerp(min, max, unipolar) as u8);
            }
        }
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
        }
        v
    }

    fn emit(&mut self, msg: MidiMsg) {
        self.out.push(Output::Midi(msg));
    }
}

/// Значение оси после мёртвой зоны, кривой и режима: −1..1 для Bipolar, иначе 0..1.
pub fn shape_value(raw: f32, shape: AxisShape) -> f32 {
    let v = if shape.invert { -raw } else { raw };
    let dz = shape.deadzone.clamp(0.0, 0.95);
    let a = v.abs();
    let a = if a < dz { 0.0 } else { ((a - dz) / (1.0 - dz)).min(1.0) };
    let v = a.powf(shape.curve.max(0.05)).copysign(v);
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
        assert_eq!(m.drain().collect::<Vec<_>>(), [Output::ToggleRecord]);
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
                },
            }],
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
            action: Action::Note { note: NoteSpec::Absolute { note: 69 }, part: 0, velocity: 100 },
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
    fn shape_curve_and_modes() {
        let s = AxisShape { deadzone: 0.0, curve: 2.0, invert: false, mode: AxisMode::Bipolar };
        assert!((shape_value(-0.5, s) + 0.25).abs() < 1e-6);
        let s = AxisShape { mode: AxisMode::Negative, deadzone: 0.0, ..s };
        assert_eq!(shape_value(0.5, s), 0.0);
    }
}
