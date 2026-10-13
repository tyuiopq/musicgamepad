//! Поток движка: геймпад → раскладка → синтезатор / MIDI-порт / запись.
//! Интерфейс общается с ним командами, а состояние получает снимками `Status`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::audio::{AudioMode, PresetInfo, SoundBank, Synth};
use crate::input::{self, GamepadInput, PadInfo, RawEvent};
use crate::mapper::{ControlView, InputEvent, Mapper, Output};
use crate::midi::{self, MidiMsg, Recorder};
use crate::midi_out::MidiOut;
use crate::model::{Axis, Direction, Music, Profile, SwitchKey};

/// Сколько ждать событий геймпада за один оборот цикла. Нажатия будят поток сразу,
/// это лишь верхняя граница реакции на команды из интерфейса.
const POLL: Duration = Duration::from_millis(8);
/// Не чаще ~60 обновлений состояния в секунду.
const STATUS_INTERVAL: Duration = Duration::from_millis(16);
const AUDIO_RETRY: Duration = Duration::from_secs(2);

/// Каким геймпадом играть.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum PadChoice {
    /// Тот, на котором нажали кнопку последним.
    #[default]
    Auto,
    /// Только этот (по названию — переживает переподключение).
    Fixed { name: String },
}

/// Настройки, которые приложение сохраняет между запусками.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub synth_enabled: bool,
    pub audio_mode: AudioMode,
    pub midi_out: Option<String>,
    pub soundfont: Option<PathBuf>,
    pub pad: PadChoice,
    pub profile: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            synth_enabled: true,
            audio_mode: AudioMode::default(),
            midi_out: None,
            soundfont: None,
            pad: PadChoice::Auto,
            profile: None,
        }
    }
}

pub struct EngineConfig {
    pub profile: Profile,
    pub settings: Settings,
    /// Где ещё искать звуковой банк (например, папка данных приложения).
    pub soundfont_dirs: Vec<PathBuf>,
    pub recordings_dir: PathBuf,
}

pub enum Command {
    SetPadChoice(PadChoice),
    SetSynthEnabled(bool),
    SetAudioMode(AudioMode),
    SetMidiOut(Option<String>),
    LoadProfile(Box<Profile>),
    ToggleRecord,
    Panic,
    Shutdown,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PartStatus {
    pub name: String,
    pub channel: u8,
    pub program: u8,
    pub instrument: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SynthStatus {
    pub enabled: bool,
    pub soundfont: Option<String>,
    pub device: Option<String>,
    /// Какой режим выбран и какой реально работает (может откатиться на общий).
    pub requested_mode: AudioMode,
    pub mode: Option<AudioMode>,
    /// Почему не включился эксклюзивный режим.
    pub fallback: Option<String>,
    pub error: Option<String>,
    pub latency: Option<Latency>,
}

/// Оценка задержки «нажал → услышал», мс. Время передачи самого геймпада сюда не входит.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Latency {
    /// От получения нажатия системой до его обработки движком (среднее по последним).
    pub input: Option<f32>,
    /// Ожидание ближайшего аудиоблока: в среднем полпериода.
    pub block: f32,
    /// От синтеза до динамиков.
    pub output: f32,
    pub total: f32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct MidiStatus {
    pub port: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Status {
    pub pads: Vec<PadInfo>,
    pub active_pad: Option<u32>,
    pub pad_choice: PadChoice,
    pub input_error: Option<String>,
    pub profile: String,
    pub music: Option<Music>,
    pub layers: Vec<String>,
    /// Что делает каждый назначенный вход прямо сейчас.
    pub controls: Vec<ControlView>,
    /// Положения стиков и курков.
    pub axes: HashMap<Axis, f32>,
    pub transpose: i32,
    pub notes: Vec<u8>,
    pub pressed: Vec<SwitchKey>,
    pub parts: Vec<PartStatus>,
    pub synth: SynthStatus,
    pub midi: MidiStatus,
    pub recording: bool,
    pub last_recording: Option<String>,
    pub touched: Option<Touched>,
}

/// Последний контрол, которого коснулись на геймпаде (для «назначить с геймпада»).
/// `seq` растёт с каждым касанием, так интерфейс отличает новое касание от старого.
#[derive(Clone, Debug, Serialize)]
pub struct Touched {
    pub key: SwitchKey,
    pub seq: u64,
}

/// Отклонение оси, которое считается касанием, и порог, ниже которого ось «отпущена».
const TOUCH_ON: f32 = 0.6;
const TOUCH_OFF: f32 = 0.3;

/// Данные, которые поток движка публикует для интерфейса.
#[derive(Default)]
pub struct Shared {
    pub status: Status,
    pub presets: Vec<PresetInfo>,
}

pub struct Engine {
    tx: Sender<Command>,
    shared: Arc<Mutex<Shared>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Engine {
    /// Запускает поток движка. `on_status` вызывается из него при каждом изменении состояния.
    pub fn start(config: EngineConfig, on_status: impl FnMut(&Status) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let shared_thread = shared.clone();
        let thread = std::thread::Builder::new()
            .name("engine".into())
            .spawn(move || Runtime::new(config, shared_thread).run(rx, on_status))
            .expect("не удалось запустить поток движка");
        Self { tx, shared, thread: Mutex::new(Some(thread)) }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }

    pub fn status(&self) -> Status {
        self.shared.lock().unwrap().status.clone()
    }

    pub fn presets(&self) -> Vec<PresetInfo> {
        self.shared.lock().unwrap().presets.clone()
    }

    /// Остановить движок и дождаться: доиграть note-off, сохранить идущую запись.
    pub fn shutdown(&self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(t) = self.thread.lock().unwrap().take() {
            let _ = t.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Runtime {
    shared: Arc<Mutex<Shared>>,
    settings: Settings,
    recordings_dir: PathBuf,
    input: Option<GamepadInput>,
    input_error: Option<String>,
    active: Option<u32>,
    mapper: Mapper,
    bank: Option<SoundBank>,
    synth: Option<Synth>,
    synth_error: Option<String>,
    last_audio_try: Instant,
    midi_out: Option<MidiOut>,
    midi_error: Option<String>,
    recorder: Option<Recorder>,
    last_recording: Option<String>,
    outputs: Vec<Output>,
    /// Скользящее среднее задержки ввода, мс.
    input_ms: Option<f32>,
    touched: Option<Touched>,
    /// Какие направления осей сейчас отклонены за порог касания.
    axis_touch: Vec<(Axis, Direction)>,
    dirty: bool,
}

impl Runtime {
    fn new(config: EngineConfig, shared: Arc<Mutex<Shared>>) -> Self {
        let mut rt = Self {
            shared,
            recordings_dir: config.recordings_dir,
            input: None,
            input_error: None,
            active: None,
            mapper: Mapper::new(config.profile),
            bank: None,
            synth: None,
            synth_error: None,
            last_audio_try: Instant::now(),
            midi_out: None,
            midi_error: None,
            recorder: None,
            last_recording: None,
            outputs: Vec::new(),
            input_ms: None,
            touched: None,
            axis_touch: Vec::new(),
            dirty: true,
            settings: config.settings,
        };

        match SoundBank::find(rt.settings.soundfont.as_deref(), &config.soundfont_dirs) {
            Some(path) => match SoundBank::load(&path) {
                Ok(bank) => rt.bank = Some(bank),
                Err(e) => rt.synth_error = Some(format!("Звуковой банк не загрузился: {e}")),
            },
            None => rt.synth_error = Some("Звуковой банк (.sf2) не найден".into()),
        }
        if let Some(bank) = &rt.bank {
            rt.shared.lock().unwrap().presets = bank.presets.clone();
        }
        if rt.settings.synth_enabled {
            rt.start_synth();
        }
        if let Some(port) = rt.settings.midi_out.clone() {
            rt.connect_midi(&port);
        }
        match GamepadInput::new() {
            Ok(input) => rt.input = Some(input),
            Err(e) => rt.input_error = Some(e.to_string()),
        }
        rt.flush();
        rt
    }

    fn run(mut self, rx: Receiver<Command>, mut on_status: impl FnMut(&Status)) {
        let mut events = Vec::new();
        let mut last_status = Instant::now() - STATUS_INTERVAL;
        loop {
            match self.input.as_mut() {
                Some(input) => input.wait(POLL, &mut events),
                None => std::thread::sleep(POLL),
            }
            for event in events.drain(..) {
                self.on_raw(event);
            }
            while let Ok(cmd) = rx.try_recv() {
                if matches!(cmd, Command::Shutdown) {
                    self.shutdown();
                    return;
                }
                self.on_command(cmd);
            }
            self.check_audio();
            if self.dirty && last_status.elapsed() >= STATUS_INTERVAL {
                let status = self.status();
                on_status(&status);
                self.shared.lock().unwrap().status = status;
                self.dirty = false;
                last_status = Instant::now();
            }
        }
    }

    // ---------- геймпады ----------

    fn on_raw(&mut self, event: RawEvent) {
        self.dirty = true;
        match event {
            RawEvent::Added(_) => {
                if self.active.is_none() {
                    self.active = self.pick_pad();
                }
            }
            RawEvent::Removed(id) => {
                if self.active == Some(id) {
                    self.mapper.release_all();
                    self.flush();
                    self.active = self.pick_pad();
                }
            }
            RawEvent::Input { pad, event, t_ns } => {
                if Some(pad) != self.active {
                    let pressed = matches!(event, InputEvent::Button { down: true, .. });
                    if self.settings.pad != PadChoice::Auto || !pressed {
                        return;
                    }
                    self.mapper.release_all();
                    self.active = Some(pad);
                }
                self.track_touch(event);
                if matches!(event, InputEvent::Button { down: true, .. }) {
                    let ms = input::ticks_ns().saturating_sub(t_ns) as f32 / 1e6;
                    self.input_ms = Some(self.input_ms.map_or(ms, |avg| avg * 0.8 + ms * 0.2));
                }
                self.mapper.handle(event);
                self.flush();
            }
        }
    }

    fn track_touch(&mut self, event: InputEvent) {
        let key = match event {
            InputEvent::Button { button, down: true } => Some(SwitchKey::Button { button }),
            InputEvent::Button { .. } => None,
            InputEvent::Axis { axis, value } => {
                let mut hit = None;
                for direction in [Direction::Positive, Direction::Negative] {
                    let amount = if direction == Direction::Positive { value } else { -value };
                    let pos = self.axis_touch.iter().position(|t| *t == (axis, direction));
                    match pos {
                        None if amount >= TOUCH_ON => {
                            self.axis_touch.push((axis, direction));
                            hit = Some(SwitchKey::AxisSwitch { axis, direction });
                        }
                        Some(i) if amount < TOUCH_OFF => {
                            self.axis_touch.remove(i);
                        }
                        _ => {}
                    }
                }
                hit
            }
        };
        if let Some(key) = key {
            let seq = self.touched.as_ref().map_or(1, |t| t.seq + 1);
            self.touched = Some(Touched { key, seq });
        }
    }

    /// Какой геймпад сделать активным, если текущий пропал или выбор изменился.
    fn pick_pad(&self) -> Option<u32> {
        let pads = self.input.as_ref()?.pads();
        match &self.settings.pad {
            PadChoice::Auto => pads.first().map(|p| p.id),
            PadChoice::Fixed { name } => pads.iter().find(|p| &p.name == name).map(|p| p.id),
        }
    }

    // ---------- команды ----------

    fn on_command(&mut self, cmd: Command) {
        self.dirty = true;
        match cmd {
            Command::SetPadChoice(choice) => {
                self.settings.pad = choice;
                let next = match &self.settings.pad {
                    PadChoice::Auto => self.active.or_else(|| self.pick_pad()),
                    PadChoice::Fixed { .. } => self.pick_pad(),
                };
                if next != self.active {
                    self.mapper.release_all();
                    self.flush();
                    self.active = next;
                }
            }
            Command::SetSynthEnabled(on) => {
                self.settings.synth_enabled = on;
                // Выключенный синтезатор отпускает звуковое устройство (важно в эксклюзивном режиме)
                self.synth = None;
                self.synth_error = None;
                if on {
                    self.start_synth();
                }
            }
            Command::SetAudioMode(mode) => {
                self.settings.audio_mode = mode;
                if self.settings.synth_enabled {
                    self.synth = None;
                    self.start_synth();
                }
            }
            Command::SetMidiOut(port) => {
                self.silence_midi_out();
                self.midi_out = None;
                self.midi_error = None;
                self.settings.midi_out = port.clone();
                if let Some(port) = port {
                    self.connect_midi(&port);
                }
            }
            Command::LoadProfile(profile) => {
                self.mapper.panic();
                self.flush();
                self.mapper = Mapper::new(*profile);
                self.flush();
            }
            Command::ToggleRecord => self.toggle_record(),
            Command::Panic => {
                self.mapper.panic();
                self.flush();
            }
            Command::Shutdown => {}
        }
    }

    fn shutdown(&mut self) {
        if self.recorder.is_some() {
            self.toggle_record();
        }
        self.mapper.panic();
        self.flush();
    }

    // ---------- вывод ----------

    /// Разослать всё, что накопил движок раскладок.
    fn flush(&mut self) {
        self.outputs.extend(self.mapper.drain());
        let outputs = std::mem::take(&mut self.outputs);
        for out in &outputs {
            match *out {
                Output::Midi(msg) => self.dispatch(msg),
                Output::ToggleRecord => self.toggle_record(),
            }
        }
        self.outputs = outputs;
        self.outputs.clear();
    }

    fn dispatch(&mut self, msg: MidiMsg) {
        if self.settings.synth_enabled
            && let Some(synth) = &mut self.synth
        {
            synth.send(msg);
        }
        if let Some(out) = &mut self.midi_out
            && let Err(e) = out.send(msg)
        {
            self.midi_error = Some(e.to_string());
            self.midi_out = None;
            self.dirty = true;
        }
        if let Some(rec) = &mut self.recorder {
            rec.add(msg);
        }
    }

    fn all_notes_off(&self) -> Vec<MidiMsg> {
        (0..16).map(|ch| MidiMsg::cc(ch, 123, 0)).collect()
    }

    fn silence_midi_out(&mut self) {
        let msgs = self.all_notes_off();
        if let Some(out) = &mut self.midi_out {
            for m in msgs {
                let _ = out.send(m);
            }
        }
    }

    fn restore_synth_state(&mut self) {
        let msgs = self.mapper.state_messages();
        if let Some(synth) = &mut self.synth {
            msgs.into_iter().for_each(|m| synth.send(m));
        }
    }

    fn connect_midi(&mut self, port: &str) {
        match MidiOut::connect(port) {
            Ok(mut out) => {
                for m in self.mapper.state_messages() {
                    let _ = out.send(m);
                }
                self.midi_out = Some(out);
            }
            Err(e) => self.midi_error = Some(e.to_string()),
        }
    }

    fn start_synth(&mut self) {
        let Some(bank) = &self.bank else { return };
        self.last_audio_try = Instant::now();
        match Synth::start(bank, self.settings.audio_mode) {
            Ok(synth) => {
                self.synth = Some(synth);
                self.synth_error = None;
                self.restore_synth_state();
            }
            Err(e) => self.synth_error = Some(format!("Звук: {e}")),
        }
    }

    /// Если аудиоустройство отвалилось — пересоздать поток, пробуя раз в пару секунд.
    fn check_audio(&mut self) {
        if self.synth.as_ref().is_some_and(Synth::failed) {
            self.synth = None;
            self.synth_error = Some("Аудиоустройство отключилось, переподключаюсь…".into());
            self.dirty = true;
        }
        if self.settings.synth_enabled
            && self.synth.is_none()
            && self.bank.is_some()
            && self.last_audio_try.elapsed() >= AUDIO_RETRY
        {
            self.start_synth();
            self.dirty = true;
        }
    }

    // ---------- запись ----------

    fn toggle_record(&mut self) {
        self.dirty = true;
        match self.recorder.take() {
            None => self.recorder = Some(Recorder::start(self.mapper.state_messages())),
            Some(rec) => {
                if !rec.has_notes() {
                    self.last_recording = None;
                    return;
                }
                let events = rec.finish(self.mapper.sounding());
                let name = chrono::Local::now().format("melody_%Y-%m-%d_%H-%M-%S.mid").to_string();
                let path = self.recordings_dir.join(name);
                match midi::write_smf(&path, &events) {
                    Ok(()) => self.last_recording = Some(path.display().to_string()),
                    Err(e) => self.midi_error = Some(format!("Не удалось сохранить запись: {e}")),
                }
            }
        }
    }

    // ---------- состояние ----------

    fn status(&self) -> Status {
        let profile = self.mapper.profile();
        let parts = profile
            .parts
            .iter()
            .map(|p| PartStatus {
                name: p.name.clone(),
                channel: p.channel,
                program: p.program,
                instrument: self
                    .bank
                    .as_ref()
                    .and_then(|b| b.preset_name(p.bank, p.program))
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("Программа {}", p.program + 1)),
            })
            .collect();
        Status {
            pads: self.input.as_ref().map(GamepadInput::pads).unwrap_or_default(),
            active_pad: self.active,
            pad_choice: self.settings.pad.clone(),
            input_error: self.input_error.clone(),
            profile: profile.name.clone(),
            music: Some(profile.music.clone()),
            controls: self.mapper.preview(),
            axes: self.mapper.axis_values(),
            layers: self.mapper.active_layers().iter().map(|l| l.name.clone()).collect(),
            transpose: self.mapper.transpose(),
            notes: self.mapper.sounding_notes(),
            pressed: self.mapper.pressed(),
            parts,
            synth: SynthStatus {
                enabled: self.settings.synth_enabled,
                soundfont: self.bank.as_ref().map(|b| b.path.display().to_string()),
                device: self.synth.as_ref().map(|s| s.device.clone()),
                requested_mode: self.settings.audio_mode,
                mode: self.synth.as_ref().map(|s| s.mode),
                fallback: self.synth.as_ref().and_then(|s| s.fallback.clone()),
                error: self.synth_error.clone(),
                latency: self.synth.as_ref().and_then(|s| {
                    let output = s.output_latency_ms()?;
                    let block = s.period_ms() / 2.0;
                    let total = self.input_ms.unwrap_or(0.0) + block + output;
                    Some(Latency { input: self.input_ms, block, output, total })
                }),
            },
            midi: MidiStatus { port: self.midi_out.as_ref().map(|m| m.port.clone()), error: self.midi_error.clone() },
            recording: self.recorder.is_some(),
            last_recording: self.last_recording.clone(),
            touched: self.touched.clone(),
        }
    }
}
