//! Оболочка Tauri: команды для интерфейса, настройки и профили на диске.

mod profiles;

use std::path::PathBuf;
use std::sync::Mutex;

use engine::audio::PresetInfo;
use engine::mapper::{ControlView, Mapper};
use engine::model::{Command as Action, Profile};
use engine::transport::TransportSettings;
use serde::Serialize;
use engine::{presets, AudioMode, Command, Engine, EngineConfig, PadChoice, Settings, Status};
use profiles::{validated, ProfileEntry, ProfileStore};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

struct AppState {
    engine: Engine,
    settings: Mutex<Settings>,
    /// Профиль, который сейчас играет (может быть несохранённым черновиком редактора).
    profile: Mutex<Profile>,
    profiles: ProfileStore,
    settings_path: PathBuf,
    recordings: PathBuf,
}

type CmdResult<T> = Result<T, String>;

impl AppState {
    fn update_settings(&self, f: impl FnOnce(&mut Settings)) {
        let mut s = self.settings.lock().unwrap();
        f(&mut s);
        if let Ok(json) = serde_json::to_string_pretty(&*s) {
            let _ = std::fs::write(&self.settings_path, json);
        }
    }

    /// Сделать профиль текущим: играть им и запомнить выбор.
    fn activate(&self, profile: Profile) {
        let name = profile.name.clone();
        *self.profile.lock().unwrap() = profile.clone();
        self.update_settings(|s| s.profile = Some(name));
        self.engine.send(Command::LoadProfile(Box::new(profile)));
    }
}

// ---------- команды ----------

#[tauri::command]
fn get_status(state: State<AppState>) -> Status {
    state.engine.status()
}

#[tauri::command]
fn get_presets(state: State<AppState>) -> Vec<PresetInfo> {
    state.engine.presets()
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn list_midi_ports() -> Vec<String> {
    engine::midi_out::list_ports()
}

#[tauri::command]
fn set_pad_choice(state: State<AppState>, choice: PadChoice) {
    state.update_settings(|s| s.pad = choice.clone());
    state.engine.send(Command::SetPadChoice(choice));
}

#[tauri::command]
fn set_synth_enabled(state: State<AppState>, enabled: bool) {
    state.update_settings(|s| s.synth_enabled = enabled);
    state.engine.send(Command::SetSynthEnabled(enabled));
}

#[tauri::command]
fn set_audio_mode(state: State<AppState>, mode: AudioMode) {
    state.update_settings(|s| s.audio_mode = mode);
    state.engine.send(Command::SetAudioMode(mode));
}

#[tauri::command]
fn set_master_volume(state: State<AppState>, volume: f32) {
    let volume = volume.clamp(0.0, 2.0);
    state.update_settings(|s| s.master_volume = volume);
    state.engine.send(Command::SetMasterVolume(volume));
}

/// Выбрать свой звуковой банк (.sf2). Возвращает путь или None, если окно закрыли.
/// Асинхронная: системное окно выбора файла нельзя открывать из главного потока команд.
#[tauri::command]
async fn choose_soundfont(app: AppHandle) -> Option<String> {
    let file = app.dialog().file().add_filter("Звуковой банк SoundFont", &["sf2"]).blocking_pick_file()?;
    let path = file.into_path().ok()?;
    let state = app.state::<AppState>();
    state.update_settings(|s| s.soundfont = Some(path.clone()));
    state.engine.send(Command::SetSoundFont(Some(path.clone())));
    Some(path.display().to_string())
}

/// Вернуться к банку по умолчанию.
#[tauri::command]
fn reset_soundfont(state: State<AppState>) {
    state.update_settings(|s| s.soundfont = None);
    state.engine.send(Command::SetSoundFont(None));
}

/// Послушать инструмент партии короткой фразой.
#[tauri::command]
fn audition(state: State<AppState>, part: usize) {
    state.engine.send(Command::Audition { part });
}

#[tauri::command]
fn set_midi_out(state: State<AppState>, port: Option<String>) {
    state.update_settings(|s| s.midi_out = port.clone());
    state.engine.send(Command::SetMidiOut(port));
}

#[tauri::command]
fn list_profiles(state: State<AppState>) -> Vec<ProfileEntry> {
    state.profiles.list()
}

#[tauri::command]
fn get_profile(state: State<AppState>) -> Profile {
    state.profile.lock().unwrap().clone()
}

#[tauri::command]
fn select_profile(state: State<AppState>, name: String) -> CmdResult<()> {
    let profile = state.profiles.find(&name).ok_or(format!("профиль «{name}» не найден"))?;
    state.activate(profile);
    Ok(())
}

/// Что делает каждая кнопка в слое `layer` профиля — для схемы в редакторе.
#[tauri::command]
fn preview_profile(profile: Profile, layer: String) -> CmdResult<Vec<ControlView>> {
    validated(&profile)?;
    Mapper::new(profile).preview_layer(&layer).ok_or(format!("слой «{layer}» не найден"))
}

/// Играть с профилем, не сохраняя его (правки в редакторе слышны сразу).
#[tauri::command]
fn apply_profile(state: State<AppState>, profile: Profile) -> CmdResult<()> {
    validated(&profile)?;
    *state.profile.lock().unwrap() = profile.clone();
    state.engine.send(Command::LoadProfile(Box::new(profile)));
    Ok(())
}

/// Сохраняет профиль и делает его текущим. `previous` — имя, под которым он был сохранён.
#[tauri::command]
fn save_profile(state: State<AppState>, profile: Profile, previous: Option<String>) -> CmdResult<()> {
    state.profiles.save(&profile, previous.as_deref())?;
    state.activate(profile);
    Ok(())
}

/// Сохранить профиль в файл, чтобы поделиться. Возвращает путь или None, если окно закрыли.
#[tauri::command]
async fn export_profile(app: AppHandle, name: String) -> CmdResult<Option<String>> {
    let state = app.state::<AppState>();
    let profile = state.profiles.find(&name).ok_or(format!("профиль «{name}» не найден"))?;
    let safe: String = name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) { '_' } else { c }).collect();
    let Some(file) = app
        .dialog()
        .file()
        .add_filter("Профиль MusicGamepad", &["json"])
        .set_file_name(format!("{safe}.json"))
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, profile.to_json()).map_err(|e| e.to_string())?;
    Ok(Some(path.display().to_string()))
}

/// Загрузить профиль из файла и сделать текущим. Возвращает его имя (при совпадении — «Имя (2)»).
#[tauri::command]
async fn import_profile(app: AppHandle) -> CmdResult<Option<String>> {
    let Some(file) = app.dialog().file().add_filter("Профиль MusicGamepad", &["json"]).blocking_pick_file() else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut profile = Profile::from_json(&text).map_err(|e| format!("это не профиль MusicGamepad: {e}"))?;
    let state = app.state::<AppState>();
    profile.name = state.profiles.free_name(&profile.name);
    state.profiles.save(&profile, None)?;
    let name = profile.name.clone();
    state.activate(profile);
    Ok(Some(name))
}

/// Профиль, который включается, когда играешь геймпадом этого семейства (None — не менять).
#[tauri::command]
fn set_pad_profile(state: State<AppState>, family: String, profile: Option<String>) {
    state.update_settings(|s| match profile {
        Some(name) => {
            s.pad_profiles.insert(family, name);
        }
        None => {
            s.pad_profiles.remove(&family);
        }
    });
}

/// Сменился тип активного геймпада — включить назначенный ему профиль.
fn auto_profile(app: &AppHandle, family: &str) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let Some(name) = state.settings.lock().unwrap().pad_profiles.get(family).cloned() else { return };
    if state.profile.lock().unwrap().name == name {
        return;
    }
    if let Some(profile) = state.profiles.find(&name) {
        state.activate(profile);
    }
}

/// Удаляет профиль пользователя (для изменённого встроенного — возвращает заводской).
/// Если он был текущим, текущим становится встроенный с тем же именем или «Классика».
#[tauri::command]
fn delete_profile(state: State<AppState>, name: String) -> CmdResult<String> {
    state.profiles.delete(&name)?;
    let current = state.profile.lock().unwrap().name.clone();
    if current == name {
        let next = state.profiles.find(&name).unwrap_or_else(presets::classic);
        let next_name = next.name.clone();
        state.activate(next);
        return Ok(next_name);
    }
    Ok(current)
}

#[tauri::command]
fn set_transport(state: State<AppState>, settings: TransportSettings) {
    state.update_settings(|s| s.transport = settings.clone());
    state.engine.send(Command::SetTransport(settings));
}

/// Метроном и лупер из окна — то же, что кнопки геймпада.
#[tauri::command]
fn transport_action(state: State<AppState>, action: Action) -> CmdResult<()> {
    match action {
        Action::ToggleMetronome | Action::LoopRecord | Action::LoopUndo | Action::LoopClear | Action::ToggleRecord => {
            // Метроном переключили — запомнить
            if matches!(action, Action::ToggleMetronome) {
                state.update_settings(|s| s.transport.metronome = !s.transport.metronome);
            }
            state.engine.send(Command::Transport(action));
            Ok(())
        }
        _ => Err("эта команда не для транспорта".into()),
    }
}

#[derive(Serialize)]
struct RecordingEntry {
    name: String,
    mid: Option<String>,
    wav: Option<String>,
    /// Время изменения, «ГГГГ-ММ-ДД ЧЧ:ММ».
    modified: String,
    bytes: u64,
}

/// Записи в папке: .mid и .wav с одним именем — одна запись. Новые сверху.
#[tauri::command]
fn list_recordings(state: State<AppState>) -> Vec<RecordingEntry> {
    let Ok(dir) = std::fs::read_dir(&state.recordings) else { return Vec::new() };
    let mut map: std::collections::BTreeMap<String, RecordingEntry> = Default::default();
    let mut times: std::collections::HashMap<String, std::time::SystemTime> = Default::default();
    for entry in dir.filter_map(|e| e.ok()) {
        let path = entry.path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()).map(str::to_lowercase) else { continue };
        if ext != "mid" && ext != "wav" {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else { continue };
        let meta = entry.metadata().ok();
        let e = map.entry(stem.clone()).or_insert(RecordingEntry {
            name: stem.clone(),
            mid: None,
            wav: None,
            modified: String::new(),
            bytes: 0,
        });
        let p = Some(path.display().to_string());
        if ext == "mid" { e.mid = p } else { e.wav = p }
        e.bytes += meta.as_ref().map_or(0, |m| m.len());
        if let Some(t) = meta.and_then(|m| m.modified().ok()) {
            let newest = times.entry(stem).or_insert(t);
            *newest = (*newest).max(t);
        }
    }
    let mut v: Vec<RecordingEntry> = map
        .into_values()
        .map(|mut e| {
            if let Some(t) = times.get(&e.name) {
                e.modified = chrono::DateTime::<chrono::Local>::from(*t).format("%Y-%m-%d %H:%M").to_string();
            }
            e
        })
        .collect();
    v.sort_by(|a, b| b.modified.cmp(&a.modified).then(b.name.cmp(&a.name)));
    v
}

/// Файл записи — только из папки записей.
fn recording_path(state: &AppState, path: &str) -> CmdResult<PathBuf> {
    let p = PathBuf::from(path);
    let dir = state.recordings.canonicalize().map_err(|e| e.to_string())?;
    let full = p.canonicalize().map_err(|e| e.to_string())?;
    if full.parent() != Some(dir.as_path()) {
        return Err("файл не из папки записей".into());
    }
    Ok(full)
}

#[tauri::command]
fn open_recording(app: AppHandle, state: State<AppState>, path: String) -> CmdResult<()> {
    let full = recording_path(&state, &path)?;
    app.opener().open_path(full.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_recording(state: State<AppState>, name: String) -> CmdResult<()> {
    for ext in ["mid", "wav"] {
        let p = state.recordings.join(format!("{name}.{ext}"));
        if p.exists() {
            let full = recording_path(&state, &p.to_string_lossy())?;
            std::fs::remove_file(full).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Последние сообщения MIDI-монитора (новее `since`).
#[tauri::command]
fn midi_monitor(state: State<AppState>, since: u64) -> Vec<engine::engine::MonitorEntry> {
    state.engine.monitor(since)
}

/// Отправить пробные сообщения для MIDI Learn в DAW: (задержка мс, байты).
#[tauri::command]
fn send_test(state: State<AppState>, messages: Vec<(u64, Vec<u8>)>) -> CmdResult<()> {
    let mut out = Vec::new();
    for (delay, bytes) in messages {
        let ok = (2..=3).contains(&bytes.len()) && (0x80..0xF0).contains(&bytes[0]) && bytes[1..].iter().all(|b| *b < 0x80);
        if !ok {
            return Err(format!("неверное MIDI-сообщение: {bytes:?}"));
        }
        let mut b = [0u8; 3];
        b[..bytes.len()].copy_from_slice(&bytes);
        out.push((delay.min(5000), engine::midi::MidiMsg { bytes: b, len: bytes.len() as u8 }));
    }
    state.engine.send(Command::SendTest(out));
    Ok(())
}

#[tauri::command]
fn toggle_record(state: State<AppState>) {
    state.engine.send(Command::ToggleRecord);
}

#[tauri::command]
fn panic(state: State<AppState>) {
    state.engine.send(Command::Panic);
}

#[tauri::command]
fn open_recordings(app: AppHandle, state: State<AppState>) -> CmdResult<()> {
    let dir = &state.recordings;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

// ---------- запуск ----------

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let path = app.path();
    let config_dir = path.app_config_dir()?;
    let data_dir = path.app_data_dir()?;
    let recordings = path.document_dir().map(|d| d.join("MusicGamepad")).unwrap_or_else(|_| data_dir.join("recordings"));
    std::fs::create_dir_all(&config_dir)?;
    let settings_path = config_dir.join("settings.json");
    let profiles = ProfileStore::new(data_dir.join("profiles"));

    let settings: Settings = std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let profile = settings.profile.as_deref().and_then(|name| profiles.find(name)).unwrap_or_else(presets::classic);

    let handle = app.handle().clone();
    let engine = Engine::start(
        EngineConfig {
            profile: profile.clone(),
            settings: settings.clone(),
            soundfont_dirs: vec![data_dir.join("sounds")],
            recordings_dir: recordings.clone(),
        },
        {
            let mut last_family: Option<String> = None;
            move |status: &Status| {
                let _ = handle.emit("status", status);
                let family = status
                    .active_pad
                    .and_then(|id| status.pads.iter().find(|p| p.id == id))
                    .and_then(|p| serde_json::to_value(p.family).ok())
                    .and_then(|v| v.as_str().map(str::to_string));
                // До готовности состояния приложения смену не засчитываем — проверим в следующий раз
                if family != last_family && handle.try_state::<AppState>().is_some() {
                    if let Some(f) = &family {
                        auto_profile(&handle, f);
                    }
                    last_family = family;
                }
            }
        },
    );
    app.manage(AppState {
        engine,
        settings: Mutex::new(settings),
        profile: Mutex::new(profile),
        profiles,
        settings_path,
        recordings,
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(setup)
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_presets,
            get_settings,
            list_midi_ports,
            set_pad_choice,
            set_synth_enabled,
            set_audio_mode,
            set_midi_out,
            set_master_volume,
            choose_soundfont,
            reset_soundfont,
            audition,
            list_profiles,
            get_profile,
            select_profile,
            preview_profile,
            apply_profile,
            save_profile,
            delete_profile,
            export_profile,
            import_profile,
            set_pad_profile,
            toggle_record,
            midi_monitor,
            send_test,
            set_transport,
            transport_action,
            list_recordings,
            open_recording,
            delete_recording,
            panic,
            open_recordings,
        ])
        .build(tauri::generate_context!())
        .expect("ошибка запуска приложения")
        .run(|app, event| {
            // Движок останавливаем явно: доиграть ноты, сохранить идущую запись
            if let tauri::RunEvent::Exit = event
                && let Some(state) = app.try_state::<AppState>()
            {
                state.engine.shutdown();
            }
        });
}
