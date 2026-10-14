//! Поток движка: геймпад → раскладка → синтезатор / MIDI-порт / запись.
//! Интерфейс общается с ним командами, а состояние получает снимками `Status`.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::calibration::Calibration;
use crate::audio::{AudioMode, PresetInfo, SoundBank, Synth};
use crate::rack::{new_synth, Pad, RackConfig, Route, SampleData};
use rustysynth::SoundFont;
use crate::input::{self, GamepadInput, PadInfo, RawEvent};
use crate::mapper::{ControlView, InputEvent, Mapper, Output};
use crate::midi::{self, MidiMsg, Recorder, SmfOptions};
use crate::midi_in::{ClockEvent, ClockIn};
use crate::midi_out::MidiOut;
use crate::model::{Axis, Command as Action, Direction, Music, PartSource, Profile, SwitchKey};
use crate::transport::{self, Clock, LoopEffects, LoopState, Looper, Metronome, TransportSettings};
use crate::wav;

/// Сколько ждать событий геймпада за один оборот цикла. Нажатия будят поток сразу,
/// это лишь верхняя граница реакции на команды из интерфейса.
const POLL: Duration = Duration::from_millis(8);
/// Не чаще ~60 обновлений состояния в секунду.
const STATUS_INTERVAL: Duration = Duration::from_millis(16);
const AUDIO_RETRY: Duration = Duration::from_secs(2);
/// За сколько до начала цикла лупера отправлять его события в расписание.
const LOOP_LOOKAHEAD: Duration = Duration::from_millis(30);
/// Не чаще одного сообщения в столько на каждый контроллер (CC, подтяжка, давление).
/// Последнее значение не теряется — оно уходит, как только интервал прошёл.
const CONTROL_INTERVAL: Duration = Duration::from_millis(5);
/// Пока идёт сглаживание или относительный контроллер — будить поток так часто.
const TICK_INTERVAL: Duration = Duration::from_millis(4);
/// Сколько последних сообщений хранит MIDI-монитор.
const MONITOR_SIZE: usize = 400;
/// MMC: Record Strobe и Stop.
const MMC_RECORD: [u8; 6] = [0xF0, 0x7F, 0x7F, 0x06, 0x06, 0xF7];
const MMC_STOP: [u8; 6] = [0xF0, 0x7F, 0x7F, 0x06, 0x01, 0xF7];

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
    /// Общая громкость встроенного синтезатора: 1 — как есть.
    pub master_volume: f32,
    pub midi_out: Option<String>,
    pub soundfont: Option<PathBuf>,
    pub pad: PadChoice,
    pub profile: Option<String>,
    pub transport: TransportSettings,
    /// Какой профиль включать для какого семейства геймпадов (xbox, play_station, nintendo, generic).
    pub pad_profiles: HashMap<String, String>,
    /// Оформление интерфейса (движку не нужно, хранится вместе с настройками).
    pub ui: UiSettings,
    /// Инструменты, выбранные кнопкой во время игры, — по профилям: (программа, банк) каждой партии.
    /// Так выбор переживает перезапуск, а сам профиль (в т.ч. встроенный) не меняется.
    pub instruments: HashMap<String, Vec<(u8, u16)>>,
    /// Калибровка осей — по имени геймпада.
    pub calibration: HashMap<String, Calibration>,
    /// MIDI-вход, с которого брать темп и Start/Stop (MIDI Clock от DAW).
    pub clock_in: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    /// Основной акцент (ноты, главные кнопки), «#rrggbb».
    pub accent: Option<String>,
    /// Второй акцент (модификаторы, слои).
    pub secondary: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            synth_enabled: true,
            audio_mode: AudioMode::default(),
            master_volume: 1.0,
            midi_out: None,
            soundfont: None,
            pad: PadChoice::Auto,
            profile: None,
            transport: TransportSettings::default(),
            pad_profiles: HashMap::new(),
            ui: UiSettings::default(),
            instruments: HashMap::new(),
            calibration: HashMap::new(),
            clock_in: None,
        }
    }
}

pub struct EngineConfig {
    pub profile: Profile,
    pub settings: Settings,
    /// Где ещё искать звуковой банк (например, папка данных приложения).
    pub soundfont_dirs: Vec<PathBuf>,
    pub recordings_dir: PathBuf,
    /// Папка библиотеки звуков: относительные пути .sf2 и сэмплов партий — от неё.
    pub library_dir: PathBuf,
}

pub enum Command {
    SetPadChoice(PadChoice),
    /// Откуда принимать MIDI Clock (None — не принимать, темп свой).
    SetClockIn(Option<String>),
    /// Калибровка геймпада с этим именем (None — сбросить).
    SetCalibration { pad: String, calibration: Option<Calibration> },
    SetSynthEnabled(bool),
    SetAudioMode(AudioMode),
    SetMasterVolume(f32),
    /// Сменить звуковой банк (None — найти банк по умолчанию).
    SetSoundFont(Option<PathBuf>),
    /// Сыграть короткую фразу партией, чтобы послушать инструмент.
    Audition { part: usize },
    SetMidiOut(Option<String>),
    LoadProfile(Box<Profile>),
    ToggleRecord,
    Panic,
    SetTransport(TransportSettings),
    /// Метроном, лупер — то же, что кнопки геймпада.
    Transport(Action),
    /// Отправить сообщения (с задержкой в мс) — для MIDI Learn в DAW.
    SendTest(Vec<(u64, MidiMsg)>),
    Shutdown,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PartStatus {
    pub name: String,
    pub channel: u8,
    pub program: u8,
    pub bank: u16,
    pub instrument: String,
    /// Почему не загрузился свой банк или сэмпл партии.
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SynthStatus {
    pub enabled: bool,
    pub soundfont: Option<String>,
    pub device: Option<String>,
    /// Какой режим выбран и какой реально работает (может откатиться на общий).
    pub requested_mode: AudioMode,
    pub master_volume: f32,
    /// Банк выбран пользователем (а не найден по умолчанию).
    pub custom_soundfont: bool,
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
    /// Свои выходы партий: порт и ошибка (если не подключился).
    pub extra: Vec<ExtraOut>,
    pub clock_in: Option<String>,
    pub clock_in_error: Option<String>,
    /// Приходит MIDI Clock — темп задаёт DAW.
    pub clock_external: bool,
    /// DAW играет (был Start/Continue).
    pub clock_playing: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ExtraOut {
    pub port: String,
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
    /// Те же оси до калибровки — для мастера калибровки.
    pub raw_axes: HashMap<Axis, f32>,
    pub transpose: i32,
    /// Сдвиг по ступеням от модификаторов.
    pub degree_shift: i32,
    pub notes: Vec<u8>,
    pub pressed: Vec<SwitchKey>,
    pub parts: Vec<PartStatus>,
    pub synth: SynthStatus,
    pub midi: MidiStatus,
    pub recording: bool,
    pub last_recording: Option<String>,
    pub touched: Option<Touched>,
    pub transport: TransportStatus,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordPhase {
    #[default]
    Idle,
    /// Отсчёт такта перед записью.
    CountIn,
    Recording,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LooperStatus {
    pub state: LoopState,
    pub layers: usize,
    /// Пройденная доля петли 0..1.
    pub progress: f32,
    pub bars: u8,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct TransportStatus {
    pub settings: TransportSettings,
    /// Такт и доля (с единицы), пока идут часы.
    pub bar: Option<u64>,
    pub beat: Option<u8>,
    pub record: RecordPhase,
    /// Сколько долей отсчёта осталось.
    pub count_in_left: Option<u8>,
    pub record_seconds: f64,
    pub looper: LooperStatus,
    /// Сколько WAV сейчас сохраняется.
    pub exporting: usize,
    pub last_wav: Option<String>,
}

/// Откуда отложенное событие — чтобы снимать события петли, не трогая остальные.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Audition,
    Loop,
    /// Нота, сдвинутая живым квантованием к доле.
    Live,
}

/// Повтор подключения MIDI-портов, которые пропали (выдернули интерфейс, закрыли loopMIDI).
const MIDI_RETRY: Duration = Duration::from_secs(2);
/// Шаг темпа от внешних часов меньше этого не считаем сменой темпа (дрожание).
const EXT_BPM_STEP: f32 = 0.3;

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

/// Запись MIDI-монитора: что и когда ушло из программы.
#[derive(Clone, Debug, Serialize)]
pub struct MonitorEntry {
    pub seq: u64,
    /// Мс от запуска движка.
    pub ms: f64,
    /// Ушедшее сообщение (пусто у записей о входах).
    pub bytes: Vec<u8>,
    /// Что пришло с геймпада.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<MonitorInput>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MonitorInput {
    Button { button: crate::model::Button, down: bool },
    Axis { axis: Axis, value: f32 },
}

/// Движения осей в мониторе: не чаще и не мельче этого — иначе стик забьёт весь журнал.
const MONITOR_AXIS_MS: u64 = 60;
const MONITOR_AXIS_STEP: f32 = 0.05;

/// Данные, которые поток движка публикует для интерфейса.
#[derive(Default)]
pub struct Shared {
    pub status: Status,
    pub presets: Vec<PresetInfo>,
    pub monitor: VecDeque<MonitorEntry>,
    monitor_seq: u64,
}

impl Shared {
    fn log(&mut self, ms: f64, bytes: &[u8]) {
        self.push(MonitorEntry { seq: 0, ms, bytes: bytes.to_vec(), input: None });
    }

    fn log_input(&mut self, ms: f64, input: MonitorInput) {
        self.push(MonitorEntry { seq: 0, ms, bytes: Vec::new(), input: Some(input) });
    }

    fn push(&mut self, mut entry: MonitorEntry) {
        self.monitor_seq += 1;
        entry.seq = self.monitor_seq;
        if self.monitor.len() >= MONITOR_SIZE {
            self.monitor.pop_front();
        }
        self.monitor.push_back(entry);
    }
}

/// Ограничение частоты одного контроллера.
struct Throttle {
    last: Instant,
    pending: Option<MidiMsg>,
}

/// Ключ ограничения: (статус, канал, номер CC) — или None, если сообщение не ограничивается.
/// Не ограничиваются ноты, педали (их быстрые нажатия нельзя «склеивать»), банк, RPN и режимы канала.
fn throttle_key(m: &MidiMsg) -> Option<(u8, u8, u8)> {
    match m.status() {
        0xE0 | 0xD0 => Some((m.status(), m.channel(), 0)),
        0xB0 => {
            let cc = m.bytes[1];
            let exempt = matches!(cc, 0 | 6 | 32 | 38 | 64..=69 | 98..=101 | 120..=127);
            (!exempt).then_some((0xB0, m.channel(), cc))
        }
        _ => None,
    }
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

    /// Сообщения MIDI-монитора новее `since`.
    pub fn monitor(&self, since: u64) -> Vec<MonitorEntry> {
        self.shared.lock().unwrap().monitor.iter().filter(|e| e.seq > since).cloned().collect()
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
    soundfont_dirs: Vec<PathBuf>,
    /// Отложенные MIDI-сообщения (прослушивание инструмента, петля лупера).
    scheduled: Vec<(Instant, MidiMsg, Source)>,
    clock: Clock,
    metronome: Metronome,
    record_phase: RecordPhase,
    count_in_at: Instant,
    looper: Looper,
    /// Ноты, включённые петлёй и ещё не выключенные.
    loop_sounding: Vec<(u8, u8)>,
    /// Нажатия Tap Tempo (последние несколько).
    taps: Vec<Instant>,
    wav_tx: Sender<Result<String, String>>,
    wav_rx: Receiver<Result<String, String>>,
    exporting: usize,
    last_wav: Option<String>,
    library_dir: PathBuf,
    /// Загруженные свои банки и сэмплы — чтобы не перечитывать файлы.
    font_cache: HashMap<PathBuf, Arc<SoundFont>>,
    sample_cache: HashMap<PathBuf, Arc<SampleData>>,
    /// Какие источники звука партий применены к стойке (канал, источник).
    applied_sources: Option<Vec<(u8, PartSource)>>,
    part_errors: Vec<Option<String>>,
    throttle: HashMap<(u8, u8, u8), Throttle>,
    started: Instant,
    last_tick: Instant,
    /// MIDI Clock: номер следующего тика (24 на долю) и шли ли часы в прошлый раз.
    clock_next_tick: u64,
    clock_was_running: bool,
    input: Option<GamepadInput>,
    input_error: Option<String>,
    active: Option<u32>,
    mapper: Mapper,
    bank: Option<SoundBank>,
    synth: Option<Synth>,
    synth_error: Option<String>,
    last_audio_try: Instant,
    midi_out: Option<MidiOut>,
    /// Свои выходы партий (по имени порта) и какие каналы куда идут.
    extra_outs: HashMap<String, MidiOut>,
    extra_errors: HashMap<String, String>,
    channel_ports: [Option<String>; 16],
    last_midi_try: Instant,
    clock_in: Option<ClockIn>,
    clock_in_error: Option<String>,
    clock_tx: Sender<(ClockEvent, Instant)>,
    clock_rx: Receiver<(ClockEvent, Instant)>,
    ext: transport::ExternalClock,
    /// Ноты, отложенные живым квантованием: (канал, нота) → (когда прозвучит, когда нажали).
    live_pending: HashMap<(u8, u8), (Instant, Instant)>,
    midi_error: Option<String>,
    recorder: Option<Recorder>,
    last_recording: Option<String>,
    outputs: Vec<Output>,
    /// Скользящее среднее задержки ввода, мс.
    input_ms: Option<f32>,
    /// Последние сырые значения осей активного геймпада.
    raw_axes: HashMap<Axis, f32>,
    /// Когда и с каким значением ось последний раз попала в монитор.
    monitor_axes: HashMap<Axis, (Instant, f32)>,
    /// Калибровка активного геймпада (id геймпада, поправка) — чтобы не искать на каждое движение.
    cal_cache: Option<(u32, Option<Calibration>)>,
    touched: Option<Touched>,
    /// Какие направления осей сейчас отклонены за порог касания.
    axis_touch: Vec<(Axis, Direction)>,
    dirty: bool,
}

impl Runtime {
    fn new(config: EngineConfig, shared: Arc<Mutex<Shared>>) -> Self {
        let (clock_tx, clock_rx) = mpsc::channel();
        // Таймер Windows по умолчанию тикает раз в ~15 мс — метроному и луперу нужна 1 мс
        #[cfg(windows)]
        unsafe {
            windows::Win32::Media::timeBeginPeriod(1);
        }
        let (wav_tx, wav_rx) = mpsc::channel();
        let mut rt = Self {
            shared,
            recordings_dir: config.recordings_dir,
            soundfont_dirs: config.soundfont_dirs.clone(),
            scheduled: Vec::new(),
            clock: Clock::default(),
            metronome: Metronome::default(),
            record_phase: RecordPhase::Idle,
            count_in_at: Instant::now(),
            looper: Looper::default(),
            loop_sounding: Vec::new(),
            taps: Vec::new(),
            wav_tx,
            wav_rx,
            exporting: 0,
            last_wav: None,
            library_dir: config.library_dir.clone(),
            font_cache: HashMap::new(),
            sample_cache: HashMap::new(),
            applied_sources: None,
            part_errors: Vec::new(),
            throttle: HashMap::new(),
            started: Instant::now(),
            last_tick: Instant::now(),
            clock_next_tick: 0,
            clock_was_running: false,
            input: None,
            input_error: None,
            active: None,
            mapper: Mapper::new(config.profile),
            bank: None,
            synth: None,
            synth_error: None,
            last_audio_try: Instant::now(),
            midi_out: None,
            extra_outs: HashMap::new(),
            extra_errors: HashMap::new(),
            channel_ports: Default::default(),
            last_midi_try: Instant::now(),
            clock_in: None,
            clock_in_error: None,
            clock_tx,
            clock_rx,
            ext: transport::ExternalClock::default(),
            live_pending: HashMap::new(),
            midi_error: None,
            recorder: None,
            last_recording: None,
            outputs: Vec::new(),
            input_ms: None,
            raw_axes: HashMap::new(),
            monitor_axes: HashMap::new(),
            cal_cache: None,
            touched: None,
            axis_touch: Vec::new(),
            dirty: true,
            settings: config.settings,
        };

        rt.load_bank();
        if rt.settings.synth_enabled {
            rt.start_synth();
        }
        if let Some(port) = rt.settings.midi_out.clone() {
            rt.connect_midi(&port);
        }
        rt.sync_ports();
        if let Some(port) = rt.settings.clock_in.clone() {
            rt.connect_clock_in(&port);
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
            let timeout = self.next_wait(Instant::now());
            match self.input.as_mut() {
                Some(input) => input.wait(timeout, &mut events),
                None => std::thread::sleep(timeout),
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
            self.check_midi();
            self.poll_clock_in();
            self.tick_mapper();
            self.tick_transport(Instant::now());
            self.run_scheduled();
            self.flush_throttled(Instant::now());
            self.poll_exports();
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
                let event = self.calibrate(pad, event);
                self.log_input(event);
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

    fn calibrate(&mut self, pad: u32, event: InputEvent) -> InputEvent {
        let InputEvent::Axis { axis, value } = event else { return event };
        self.raw_axes.insert(axis, value);
        if self.cal_cache.as_ref().map(|c| c.0) != Some(pad) {
            let name = self.input.as_ref().and_then(|i| i.pads().into_iter().find(|p| p.id == pad)).map(|p| p.name);
            let cal = name.and_then(|n| self.settings.calibration.get(&n).cloned());
            self.cal_cache = Some((pad, cal));
        }
        match &self.cal_cache {
            Some((_, Some(cal))) => InputEvent::Axis { axis, value: cal.apply(axis, value) },
            _ => event,
        }
    }

    fn log_input(&mut self, event: InputEvent) {
        let input = match event {
            InputEvent::Button { button, down } => MonitorInput::Button { button, down },
            InputEvent::Axis { axis, value } => {
                let now = Instant::now();
                if let Some((t, last)) = self.monitor_axes.get(&axis) {
                    let settled = value == 0.0 && *last != 0.0;
                    let moved = (value - last).abs() >= MONITOR_AXIS_STEP;
                    let due = now.duration_since(*t) >= Duration::from_millis(MONITOR_AXIS_MS);
                    if !settled && !(moved && due) {
                        return;
                    }
                }
                self.monitor_axes.insert(axis, (now, value));
                MonitorInput::Axis { axis, value }
            }
        };
        let ms = self.started.elapsed().as_secs_f64() * 1000.0;
        self.shared.lock().unwrap().log_input(ms, input);
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
            Command::SetCalibration { pad, calibration } => {
                match calibration {
                    Some(c) => self.settings.calibration.insert(pad, c),
                    None => self.settings.calibration.remove(&pad),
                };
                self.cal_cache = None;
            }
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
                if self.mapper.can_retune(&profile) {
                    // Поменялось только звучание — не обрываем ноты (ползунки в настройках звука)
                    self.mapper.retune(*profile);
                } else {
                    self.mapper.panic();
                    self.flush();
                    self.mapper = Mapper::new(*profile);
                }
                self.sync_sounds(false);
                self.sync_ports();
                self.flush();
            }
            Command::SetClockIn(port) => {
                self.clock_in = None;
                self.clock_in_error = None;
                self.ext = transport::ExternalClock::default();
                self.settings.clock_in = port.clone();
                if let Some(port) = port {
                    self.connect_clock_in(&port);
                }
            }
            Command::SetMasterVolume(v) => {
                self.settings.master_volume = v;
                if let Some(s) = &self.synth {
                    s.set_gain(v);
                }
            }
            Command::SetSoundFont(path) => {
                self.settings.soundfont = path;
                self.synth = None;
                self.load_bank();
                if self.settings.synth_enabled {
                    self.start_synth();
                }
            }
            Command::Audition { part } => self.audition(part),
            Command::ToggleRecord => self.toggle_record(),
            Command::SetTransport(ts) => self.set_transport(ts),
            Command::Transport(action) => self.transport_action(action),
            Command::SendTest(msgs) => {
                let now = Instant::now();
                for (delay, msg) in msgs {
                    self.scheduled.push((now + Duration::from_millis(delay), msg, Source::Audition));
                }
            }
            Command::Panic => {
                self.drop_live_pending();
                self.mapper.panic();
                self.flush();
            }
            Command::Shutdown => {}
        }
    }

    fn shutdown(&mut self) {
        if self.record_phase == RecordPhase::Recording {
            self.toggle_record();
        }
        #[cfg(windows)]
        unsafe {
            windows::Win32::Media::timeEndPeriod(1);
        }
        self.mapper.panic();
        self.flush();
    }

    // ---------- вывод ----------

    /// Разослать всё, что накопил движок раскладок.
    fn flush(&mut self) {
        self.outputs.extend(self.mapper.drain());
        let outputs = std::mem::take(&mut self.outputs);
        let now = Instant::now();
        for out in &outputs {
            match out {
                Output::Midi(msg) => {
                    if !self.live_quantize(*msg, now) {
                        self.dispatch(*msg);
                        self.looper.capture(now, *msg);
                    }
                }
                Output::Command(action) => self.transport_action(action.clone()),
            }
        }
        self.outputs = outputs;
        self.outputs.clear();
    }

    /// Живое квантование: Note On переносится на ближайшую долю сетки (если она впереди),
    /// его Note Off — на столько же позже, чтобы длительность нажатия сохранилась.
    /// true — сообщение отложено.
    fn live_quantize(&mut self, msg: MidiMsg, now: Instant) -> bool {
        let Some(grid) = self.settings.transport.live_quantize.grid() else { return false };
        if !self.clock.running() || !(msg.is_note_on() || msg.is_note_off()) {
            return false;
        }
        let key = (msg.channel(), msg.bytes[1]);
        if msg.is_note_on() {
            let Some(at) = self.clock.snap(now, grid, &self.settings.transport) else { return false };
            if at <= now + Duration::from_millis(2) {
                return false;
            }
            self.live_pending.insert(key, (at, now));
            self.scheduled.push((at, msg, Source::Live));
            return true;
        }
        // Note On ещё ждёт своей доли — Note Off сдвигаем следом
        let Some((at, pressed)) = self.live_pending.remove(&key) else { return false };
        let held = now.duration_since(pressed).max(Duration::from_millis(30));
        self.scheduled.push((at + held, msg, Source::Live));
        true
    }

    /// Снять отложенные квантованием ноты (паника): их Note On ещё не ушли.
    fn drop_live_pending(&mut self) {
        self.scheduled.retain(|(_, _, s)| *s != Source::Live);
        self.live_pending.clear();
    }

    /// Отправить сообщение: с ограничением частоты для контроллеров.
    fn dispatch(&mut self, msg: MidiMsg) {
        if let Some(key) = throttle_key(&msg) {
            let now = Instant::now();
            match self.throttle.get_mut(&key) {
                Some(t) if now.duration_since(t.last) < CONTROL_INTERVAL => {
                    t.pending = Some(msg);
                    return;
                }
                _ => {
                    self.throttle.insert(key, Throttle { last: now, pending: None });
                }
            }
        }
        self.deliver(msg);
    }

    /// Отправить задержанные контроллеры, чей интервал прошёл.
    fn flush_throttled(&mut self, now: Instant) {
        let due: Vec<MidiMsg> = self
            .throttle
            .values_mut()
            .filter(|t| t.pending.is_some() && now.duration_since(t.last) >= CONTROL_INTERVAL)
            .filter_map(|t| {
                t.last = now;
                t.pending.take()
            })
            .collect();
        for msg in due {
            self.deliver(msg);
        }
    }

    /// Только в MIDI-выход (часы, MMC) — синтезатору и записи это не нужно.
    fn send_out(&mut self, msg: MidiMsg) {
        if msg.bytes[0] != 0xF8 {
            let ms = self.started.elapsed().as_secs_f64() * 1000.0;
            self.shared.lock().unwrap().log(ms, msg.as_slice());
        }
        if let Some(out) = &mut self.midi_out {
            let _ = out.send(msg);
        }
    }

    fn send_mmc(&mut self, bytes: &[u8]) {
        if !self.settings.transport.send_mmc {
            return;
        }
        let ms = self.started.elapsed().as_secs_f64() * 1000.0;
        self.shared.lock().unwrap().log(ms, bytes);
        if let Some(out) = &mut self.midi_out {
            let _ = out.send_raw(bytes);
        }
    }

    fn tick_mapper(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick);
        self.last_tick = now;
        if self.mapper.needs_tick() {
            self.mapper.tick(dt);
            self.flush();
        }
    }

    fn deliver(&mut self, msg: MidiMsg) {
        let ms = self.started.elapsed().as_secs_f64() * 1000.0;
        self.shared.lock().unwrap().log(ms, msg.as_slice());
        if self.settings.synth_enabled
            && let Some(synth) = &mut self.synth
        {
            synth.send(msg);
        }
        let port = if msg.is_system() { None } else { self.channel_ports[msg.channel() as usize & 15].as_ref() };
        match port {
            Some(port) => {
                if let Some(out) = self.extra_outs.get_mut(port)
                    && let Err(e) = out.send(msg)
                {
                    self.extra_errors.insert(port.clone(), e.to_string());
                    self.extra_outs.remove(&port.clone());
                    self.dirty = true;
                }
            }
            None => {
                if let Some(out) = &mut self.midi_out
                    && let Err(e) = out.send(msg)
                {
                    self.midi_error = Some(e.to_string());
                    self.midi_out = None;
                    self.dirty = true;
                }
            }
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

    /// Подключить свои выходы партий из профиля и отключить ненужные.
    fn sync_ports(&mut self) {
        let mut ports: [Option<String>; 16] = Default::default();
        for p in &self.mapper.profile().parts {
            if let Some(port) = p.port.as_ref().filter(|x| !x.is_empty()) {
                ports[p.channel as usize & 15] = Some(port.clone());
            }
        }
        let wanted: Vec<String> = ports.iter().flatten().cloned().collect();
        let silence = self.all_notes_off();
        let stale: Vec<String> = self.extra_outs.keys().filter(|k| !wanted.contains(k)).cloned().collect();
        for port in stale {
            if let Some(mut out) = self.extra_outs.remove(&port) {
                for m in &silence {
                    let _ = out.send(*m);
                }
            }
        }
        self.extra_errors.retain(|k, _| wanted.contains(k));
        self.channel_ports = ports;
        for port in wanted {
            if !self.extra_outs.contains_key(&port) {
                self.connect_extra(&port);
            }
        }
        self.dirty = true;
    }

    fn connect_extra(&mut self, port: &str) {
        match MidiOut::connect(port) {
            Ok(mut out) => {
                let state = self.mapper.state_messages();
                for m in state.into_iter().filter(|m| self.channel_ports[m.channel() as usize & 15].as_deref() == Some(port)) {
                    let _ = out.send(m);
                }
                self.extra_outs.insert(port.to_string(), out);
                self.extra_errors.remove(port);
            }
            Err(e) => {
                self.extra_errors.insert(port.to_string(), e.to_string());
            }
        }
    }

    /// Переподключить пропавшие MIDI-порты (раз в пару секунд).
    fn check_midi(&mut self) {
        if self.last_midi_try.elapsed() < MIDI_RETRY {
            return;
        }
        self.last_midi_try = Instant::now();
        if self.midi_out.is_none()
            && let Some(port) = self.settings.midi_out.clone()
        {
            self.connect_midi(&port);
            if self.midi_out.is_some() {
                self.midi_error = None;
                self.dirty = true;
            }
        }
        let missing: Vec<String> = self.extra_errors.keys().cloned().collect();
        for port in missing {
            self.connect_extra(&port);
            self.dirty = true;
        }
        if self.clock_in.is_none()
            && let Some(port) = self.settings.clock_in.clone()
        {
            self.connect_clock_in(&port);
            self.dirty = true;
        }
    }

    fn connect_clock_in(&mut self, port: &str) {
        match ClockIn::connect(port, self.clock_tx.clone()) {
            Ok(c) => {
                self.clock_in = Some(c);
                self.clock_in_error = None;
            }
            Err(e) => self.clock_in_error = Some(e.to_string()),
        }
    }

    /// Внешние часы: Start/Stop ведут наши часы, тики — темп и позицию доли.
    fn poll_clock_in(&mut self) {
        while let Ok((event, t)) = self.clock_rx.try_recv() {
            self.dirty = true;
            match event {
                ClockEvent::Start => {
                    self.ext.start();
                    self.clock.stop();
                }
                ClockEvent::Continue => self.ext.resume(),
                ClockEvent::Stop => self.ext.stop(),
                ClockEvent::Tick => {
                    let count = self.ext.count;
                    if let Some(bpm) = self.ext.tick(t)
                        && (bpm - self.settings.transport.bpm).abs() >= EXT_BPM_STEP
                    {
                        let mut ts = self.settings.transport.clone();
                        ts.bpm = (bpm * 10.0).round() / 10.0;
                        self.set_transport(ts);
                    }
                    if !self.ext.playing {
                        continue;
                    }
                    let ts = self.settings.transport.clone();
                    if !self.clock.running() {
                        // Первый тик после Start — начало доли
                        self.clock.start(t);
                        self.clock.align(t, count as f64 / 24.0, &ts);
                        self.metronome.sync(&self.clock, t, &ts);
                    } else if count.is_multiple_of(24) {
                        // На каждой доле подстраиваемся под DAW, чтобы не разъезжаться
                        self.clock.align(t, count as f64 / 24.0, &ts);
                    }
                }
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
        match Synth::start(bank, self.settings.audio_mode, self.settings.master_volume) {
            Ok(synth) => {
                self.synth = Some(synth);
                self.synth_error = None;
                self.sync_sounds(true);
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

    fn resolve(&self, file: &str) -> PathBuf {
        let p = PathBuf::from(file);
        if p.is_absolute() { p } else { self.library_dir.join(p) }
    }

    fn load_font(&mut self, file: &str) -> Result<Arc<SoundFont>, String> {
        let path = self.resolve(file);
        if let Some(f) = self.font_cache.get(&path) {
            return Ok(f.clone());
        }
        let font = SoundBank::load(&path).map(|b| b.font).map_err(|e| format!("банк «{file}»: {e}"))?;
        self.font_cache.insert(path, font.clone());
        Ok(font)
    }

    fn load_sample(&mut self, file: &str) -> Result<Arc<SampleData>, String> {
        let path = self.resolve(file);
        if let Some(s) = self.sample_cache.get(&path) {
            return Ok(s.clone());
        }
        let data = Arc::new(SampleData::load_wav(&path).map_err(|e| format!("сэмпл «{file}»: {e}"))?);
        self.sample_cache.insert(path, data.clone());
        Ok(data)
    }

    /// Стойка под текущие партии: свои банки и сэмплы. Ошибки — в `part_errors`, такая партия
    /// играет основным банком.
    fn rack_config(&mut self, rate: u32) -> RackConfig {
        let parts = self.mapper.profile().parts.clone();
        let mut routes = [Route::Main; 16];
        let mut extra = Vec::new();
        let mut font_index: Vec<PathBuf> = Vec::new();
        let mut pads = HashMap::new();
        self.part_errors = vec![None; parts.len()];
        for (i, p) in parts.iter().enumerate() {
            let ch = p.channel as usize & 0x0F;
            match &p.source {
                PartSource::Default => {}
                PartSource::SoundFont { file } => match self.load_font(file) {
                    Ok(font) => {
                        let path = self.resolve(file);
                        let idx = match font_index.iter().position(|x| *x == path) {
                            Some(idx) => idx,
                            None => match new_synth(&font, rate) {
                                Ok(s) => {
                                    extra.push(s);
                                    font_index.push(path);
                                    extra.len() - 1
                                }
                                Err(e) => {
                                    self.part_errors[i] = Some(e.to_string());
                                    continue;
                                }
                            },
                        };
                        routes[ch] = Route::Font(idx);
                    }
                    Err(e) => self.part_errors[i] = Some(e),
                },
                PartSource::Samples { pads: list } => {
                    routes[ch] = Route::Sampler;
                    let mut errors = Vec::new();
                    for pad in list {
                        match self.load_sample(&pad.file) {
                            Ok(data) => {
                                let gain = 10f32.powf(pad.gain_db / 20.0);
                                pads.insert((p.channel, pad.note), Pad { data, gain, pitch: pad.pitch as f32, choke: pad.choke });
                            }
                            Err(e) => errors.push(e),
                        }
                    }
                    if !errors.is_empty() {
                        self.part_errors[i] = Some(errors.join("; "));
                    }
                }
            }
        }
        RackConfig { routes, extra, pads }
    }

    /// Применить источники звука партий к стойке (если они поменялись или `force`).
    fn sync_sounds(&mut self, force: bool) {
        let sig: Vec<(u8, PartSource)> = self.mapper.profile().parts.iter().map(|p| (p.channel, p.source.clone())).collect();
        if !force && self.applied_sources.as_ref() == Some(&sig) {
            return;
        }
        let Some(rate) = self.synth.as_ref().map(|s| s.sample_rate) else { return };
        let cfg = self.rack_config(rate);
        if let Some(synth) = &mut self.synth {
            synth.configure(cfg);
        }
        self.applied_sources = Some(sig);
        self.restore_synth_state();
        self.dirty = true;
    }

    /// Найти и загрузить звуковой банк по настройкам. При ошибке прежний банк остаётся.
    fn load_bank(&mut self) {
        let explicit = self.settings.soundfont.clone();
        match SoundBank::find(explicit.as_deref(), &self.soundfont_dirs) {
            Some(path) => match SoundBank::load(&path) {
                Ok(bank) => {
                    self.shared.lock().unwrap().presets = bank.presets.clone();
                    self.bank = Some(bank);
                    self.synth_error = None;
                }
                Err(e) => self.synth_error = Some(format!("Звуковой банк не загрузился: {e}")),
            },
            None => self.synth_error = Some("Звуковой банк (.sf2) не найден".into()),
        }
        if let Some(path) = explicit.filter(|p| !p.is_file()) {
            self.synth_error = Some(format!("Файл банка не найден: {}", path.display()));
        }
    }

    /// Короткая фраза партией: арпеджио по ладу, для ударных — простой бит.
    fn audition(&mut self, part: usize) {
        let profile = self.mapper.profile();
        let Some(p) = profile.parts.get(part) else { return };
        let notes: Vec<u8> = if let PartSource::Samples { pads } = &p.source {
            pads.iter().map(|x| x.note).take(8).collect()
        } else if p.channel == 9 {
            vec![36, 42, 38, 42, 36, 36, 38, 46]
        } else {
            [0, 2, 4, 7, 4, 2, 0]
                .iter()
                .map(|&d| (profile.music.degree_note(d) + p.transpose).clamp(0, 127) as u8)
                .collect()
        };
        let ch = p.channel;
        let now = Instant::now();
        for (i, &n) in notes.iter().enumerate() {
            let on = now + Duration::from_millis(180 * i as u64);
            self.scheduled.push((on, MidiMsg::note_on(ch, n, 100), Source::Audition));
            self.scheduled.push((on + Duration::from_millis(160), MidiMsg::note_off(ch, n), Source::Audition));
        }
    }

    fn run_scheduled(&mut self) {
        if self.scheduled.is_empty() {
            return;
        }
        let now = Instant::now();
        let (mut due, later): (Vec<_>, Vec<_>) = self.scheduled.drain(..).partition(|(t, _, _)| *t <= now);
        self.scheduled = later;
        due.sort_by_key(|(t, msg, _)| (*t, msg.is_note_on()));
        for (_, msg, source) in due {
            if source == Source::Live {
                if msg.is_note_on() {
                    let key = (msg.channel(), msg.bytes[1]);
                    // Нота прозвучала — её Note Off дальше идёт без задержки
                    self.live_pending.remove(&key);
                }
                self.looper.capture(now, msg);
            }
            if source == Source::Loop {
                let key = (msg.channel(), msg.bytes[1]);
                if msg.is_note_on() {
                    self.loop_sounding.push(key);
                } else if msg.is_note_off() {
                    self.loop_sounding.retain(|k| *k != key);
                }
            }
            self.dispatch(msg);
        }
    }

    // ---------- метроном, отсчёт, лупер ----------

    /// Сколько можно спать: до ближайшего щелчка, события или дела лупера, но не дольше POLL.
    fn next_wait(&self, now: Instant) -> Duration {
        let ts = &self.settings.transport;
        let mut deadline = now + POLL;
        let mut consider = |t: Option<Instant>| {
            if let Some(t) = t {
                deadline = deadline.min(t);
            }
        };
        if self.clock_audible() {
            consider(self.metronome.next_time(&self.clock, ts));
        }
        if self.record_phase == RecordPhase::CountIn {
            consider(Some(self.count_in_at));
        }
        consider(self.scheduled.iter().map(|(t, _, _)| *t).min());
        consider(self.looper.next_deadline(LOOP_LOOKAHEAD));
        consider(self.throttle.values().filter(|t| t.pending.is_some()).map(|t| t.last + CONTROL_INTERVAL).min());
        if self.mapper.needs_tick() {
            consider(Some(now + TICK_INTERVAL));
        }
        if ts.send_clock && self.clock.running() {
            consider(self.clock_tick_time(self.clock_next_tick));
        }
        deadline.saturating_duration_since(now)
    }

    fn clock_audible(&self) -> bool {
        self.settings.transport.metronome || self.record_phase == RecordPhase::CountIn
    }

    fn tick_transport(&mut self, now: Instant) {
        if self.record_phase == RecordPhase::CountIn && now >= self.count_in_at {
            self.recorder = Some(Recorder::start_at(self.count_in_at, self.mapper.state_messages()));
            self.record_phase = RecordPhase::Recording;
            self.send_mmc(&MMC_RECORD);
            self.dirty = true;
        }
        self.tick_midi_clock(now);

        let fx = self.looper.tick(now, LOOP_LOOKAHEAD);
        self.apply_loop(fx);

        if self.clock.running() {
            let ts = self.settings.transport.clone();
            let clicks = self.metronome.due(&self.clock, now, &ts);
            if self.clock_audible()
                && self.settings.synth_enabled
                && let Some(synth) = &mut self.synth
            {
                for accent in clicks {
                    synth.send(transport::click(accent, ts.metronome_volume));
                }
            }
            self.dirty = true;
            // Часы больше никому не нужны — останавливаем
            if !self.clock_audible() && self.looper.state == LoopState::Empty && !self.ext.playing {
                self.clock.stop();
            }
        }
    }

    fn clock_tick_time(&self, tick: u64) -> Option<Instant> {
        let ts = &self.settings.transport;
        let origin = self.clock.beat_time(0, ts)?;
        Some(origin + ts.beat_len().mul_f64(tick as f64 / 24.0))
    }

    /// MIDI Clock для DAW: Start при запуске часов, 24 тика на долю, Stop при остановке.
    fn tick_midi_clock(&mut self, now: Instant) {
        let running = self.clock.running();
        // Темп пришёл от DAW — обратно часы не шлём, иначе получится петля
        if !self.settings.transport.send_clock || self.clock_in.is_some() {
            self.clock_was_running = running;
            return;
        }
        if running && !self.clock_was_running {
            self.send_out(MidiMsg::realtime(0xFA));
            let beats = self.clock.beats(now, &self.settings.transport).unwrap_or(0.0);
            self.clock_next_tick = (beats * 24.0).ceil() as u64;
        }
        if !running && self.clock_was_running {
            self.send_out(MidiMsg::realtime(0xFC));
        }
        self.clock_was_running = running;
        while running && self.clock_tick_time(self.clock_next_tick).is_some_and(|t| t <= now) {
            self.send_out(MidiMsg::realtime(0xF8));
            self.clock_next_tick += 1;
        }
    }

    /// Запустить часы, если стоят (доля 0 — сейчас).
    fn ensure_clock(&mut self, now: Instant) -> bool {
        if self.clock.running() {
            return false;
        }
        self.clock.start(now);
        self.metronome.sync(&self.clock, now, &self.settings.transport);
        true
    }

    fn set_transport(&mut self, ts: TransportSettings) {
        let now = Instant::now();
        let old = std::mem::replace(&mut self.settings.transport, ts);
        let new = self.settings.transport.clone();
        if (old.bpm != new.bpm || old.beats_per_bar != new.beats_per_bar) && self.clock.running() {
            self.clock.retempo(now, &old, &new);
            self.metronome.sync(&self.clock, now, &new);
        }
        if new.metronome && !old.metronome && !self.ensure_clock(now) {
            self.metronome.sync(&self.clock, now, &new);
        }
    }

    fn transport_action(&mut self, action: Action) {
        let now = Instant::now();
        self.dirty = true;
        match action {
            Action::ToggleRecord => self.toggle_record(),
            Action::ToggleMetronome => {
                let mut ts = self.settings.transport.clone();
                ts.metronome = !ts.metronome;
                self.set_transport(ts);
            }
            Action::LoopRecord => {
                self.ensure_clock(now);
                let ts = self.settings.transport.clone();
                let bar_start = self.clock.bar_start(now, &ts).unwrap_or(now);
                let next_bar = self.clock.next_bar(now, &ts).unwrap_or(now);
                let length = ts.bar_len() * ts.loop_bars.max(1) as u32;
                let fx = self.looper.press(now, bar_start, next_bar, length);
                self.apply_loop(fx);
            }
            Action::LoopUndo => {
                let fx = self.looper.undo(now);
                self.apply_loop(fx);
            }
            Action::LoopClear => {
                let fx = self.looper.clear();
                self.apply_loop(fx);
            }
            Action::TapTempo => {
                if let Some(bpm) = transport::tap_tempo(&mut self.taps, now) {
                    let mut ts = self.settings.transport.clone();
                    ts.bpm = bpm;
                    self.set_transport(ts);
                }
            }
            Action::Panic | Action::NextInstrument { .. } | Action::PrevInstrument { .. } | Action::SetProgram { .. } => {}
        }
    }

    fn apply_loop(&mut self, fx: LoopEffects) {
        if fx.silence {
            self.scheduled.retain(|(_, _, s)| *s != Source::Loop);
            for (ch, note) in std::mem::take(&mut self.loop_sounding) {
                self.dispatch(MidiMsg::note_off(ch, note));
            }
            self.dirty = true;
        }
        for (t, msg) in fx.schedule {
            self.scheduled.push((t, msg, Source::Loop));
        }
    }

    fn poll_exports(&mut self) {
        while let Ok(result) = self.wav_rx.try_recv() {
            self.exporting = self.exporting.saturating_sub(1);
            match result {
                Ok(path) => self.last_wav = Some(path),
                Err(e) => self.midi_error = Some(format!("WAV не сохранился: {e}")),
            }
            self.dirty = true;
        }
    }

    // ---------- запись ----------

    /// Запись: старт (с отсчётом такта, если включён) → стоп и сохранение.
    fn toggle_record(&mut self) {
        self.dirty = true;
        let now = Instant::now();
        let ts = self.settings.transport.clone();
        match self.record_phase {
            RecordPhase::Idle if ts.count_in => {
                let fresh = self.ensure_clock(now);
                self.count_in_at = if fresh { now + ts.bar_len() } else { self.clock.next_bar(now, &ts).unwrap_or(now) };
                self.record_phase = RecordPhase::CountIn;
            }
            RecordPhase::Idle => {
                // Если идут часы — начало записи на начале такта, чтобы ноты легли на сетку
                let t0 = self.clock.bar_start(now, &ts).unwrap_or(now);
                self.recorder = Some(Recorder::start_at(t0, self.mapper.state_messages()));
                self.record_phase = RecordPhase::Recording;
                self.send_mmc(&MMC_RECORD);
            }
            RecordPhase::CountIn => self.record_phase = RecordPhase::Idle,
            RecordPhase::Recording => {
                self.record_phase = RecordPhase::Idle;
                self.send_mmc(&MMC_STOP);
                let Some(rec) = self.recorder.take() else { return };
                if !rec.has_notes() {
                    self.last_recording = None;
                    return;
                }
                let mut sounding = self.mapper.sounding();
                sounding.extend(self.loop_sounding.iter().copied());
                let events = rec.finish(sounding);
                self.save_recording(events);
            }
        }
    }

    fn save_recording(&mut self, events: Vec<(f64, MidiMsg)>) {
        let ts = &self.settings.transport;
        let names: Vec<(u8, String)> = self.mapper.profile().parts.iter().map(|p| (p.channel, p.name.clone())).collect();
        let opts = SmfOptions { bpm: ts.bpm as f64, beats_per_bar: ts.beats_per_bar, quantize: ts.quantize, track_names: &names };
        let name = chrono::Local::now().format("melody_%Y-%m-%d_%H-%M-%S.mid").to_string();
        let path = self.recordings_dir.join(name);
        match midi::write_smf(&path, &events, &opts) {
            Ok(()) => self.last_recording = Some(path.display().to_string()),
            Err(e) => {
                self.midi_error = Some(format!("Не удалось сохранить запись: {e}"));
                return;
            }
        }
        // WAV считается в фоне: синтезатор проигрывает запись заново, это может занять секунды
        if ts.export_wav
            && let Some(font) = self.bank.as_ref().map(|b| b.font.clone())
        {
            // Стойка собирается здесь (файлы уже в кеше), считается в фоне
            let cfg = self.rack_config(wav::SAMPLE_RATE);
            let rack = match wav::rack(&font, cfg) {
                Ok(r) => r,
                Err(e) => {
                    self.midi_error = Some(format!("WAV: {e}"));
                    return;
                }
            };
            let gain = self.settings.master_volume;
            let wav_path = path.with_extension("wav");
            let tx = self.wav_tx.clone();
            self.exporting += 1;
            std::thread::spawn(move || {
                let result = wav::export(rack, &events, gain, &wav_path)
                    .map(|()| wav_path.display().to_string())
                    .map_err(|e| e.to_string());
                let _ = tx.send(result);
            });
        }
    }

    // ---------- состояние ----------

    fn status(&self) -> Status {
        let profile = self.mapper.profile();
        let parts = profile
            .parts
            .iter()
            .enumerate()
            .map(|(i, p)| PartStatus {
                name: p.name.clone(),
                channel: p.channel,
                program: p.program,
                bank: p.bank,
                instrument: match &p.source {
                    PartSource::Samples { pads } => format!("Сэмплы ({})", pads.len()),
                    PartSource::SoundFont { file } => {
                        let font = self.font_cache.get(&self.resolve(file));
                        let name = font.and_then(|f| {
                            f.get_presets()
                                .iter()
                                .find(|x| x.get_bank_number() == p.bank as i32 && x.get_patch_number() == p.program as i32)
                                .map(|x| x.get_name().trim().to_string())
                        });
                        name.unwrap_or_else(|| format!("Программа {}", p.program + 1))
                    }
                    PartSource::Default => self
                        .bank
                        .as_ref()
                        .and_then(|b| b.preset_name(p.bank, p.program))
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("Программа {}", p.program + 1)),
                },
                error: self.part_errors.get(i).cloned().flatten(),
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
            raw_axes: self.raw_axes.clone(),
            layers: self.mapper.active_layers().iter().map(|l| l.name.clone()).collect(),
            transpose: self.mapper.transpose(),
            degree_shift: self.mapper.offset().1,
            notes: self.mapper.sounding_notes(),
            pressed: self.mapper.pressed(),
            parts,
            synth: SynthStatus {
                enabled: self.settings.synth_enabled,
                soundfont: self.bank.as_ref().map(|b| b.path.display().to_string()),
                device: self.synth.as_ref().map(|s| s.device.clone()),
                requested_mode: self.settings.audio_mode,
                master_volume: self.settings.master_volume,
                custom_soundfont: self.settings.soundfont.is_some(),
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
            midi: MidiStatus {
                port: self.midi_out.as_ref().map(|m| m.port.clone()),
                error: self.midi_error.clone(),
                extra: {
                    let mut ports: Vec<String> = self.channel_ports.iter().flatten().cloned().collect();
                    ports.sort();
                    ports.dedup();
                    ports.into_iter().map(|port| ExtraOut { error: self.extra_errors.get(&port).cloned(), port }).collect()
                },
                clock_in: self.clock_in.as_ref().map(|c| c.port.clone()),
                clock_in_error: self.clock_in_error.clone(),
                clock_external: self.ext.alive(Instant::now()),
                clock_playing: self.ext.playing,
            },
            recording: self.record_phase == RecordPhase::Recording,
            last_recording: self.last_recording.clone(),
            touched: self.touched.clone(),
            transport: self.transport_status(),
        }
    }

    fn transport_status(&self) -> TransportStatus {
        let now = Instant::now();
        let ts = &self.settings.transport;
        let bpb = ts.beats_per_bar.max(1) as u64;
        let beats = self.clock.beats(now, ts).map(|b| b.floor() as u64);
        let count_in_left = (self.record_phase == RecordPhase::CountIn).then(|| {
            let left = self.count_in_at.saturating_duration_since(now).as_secs_f64() / ts.beat_len().as_secs_f64();
            left.ceil() as u8
        });
        TransportStatus {
            settings: ts.clone(),
            bar: beats.map(|b| b / bpb + 1),
            beat: beats.map(|b| (b % bpb + 1) as u8),
            record: self.record_phase,
            count_in_left,
            record_seconds: self.recorder.as_ref().map_or(0.0, |r| r.elapsed()),
            looper: LooperStatus {
                state: self.looper.state,
                layers: self.looper.layers(),
                progress: self.looper.progress(now),
                bars: ts.loop_bars,
            },
            exporting: self.exporting,
            last_wav: self.last_wav.clone(),
        }
    }
}
