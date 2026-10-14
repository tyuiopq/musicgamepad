//! Оболочка Tauri: команды для интерфейса, настройки и профили на диске.

mod profiles;

use std::path::PathBuf;
use std::sync::Mutex;

use engine::audio::PresetInfo;
use engine::mapper::{ControlView, Mapper};
use engine::model::Profile;
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
        move |status| {
            let _ = handle.emit("status", status);
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
            toggle_record,
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
