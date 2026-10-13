//! Оболочка Tauri: команды для интерфейса, настройки и профили на диске.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use engine::audio::PresetInfo;
use engine::model::Profile;
use engine::{presets, Command, Engine, EngineConfig, PadChoice, Settings, Status};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

struct Paths {
    settings: PathBuf,
    profiles: PathBuf,
    recordings: PathBuf,
}

struct AppState {
    engine: Engine,
    settings: Mutex<Settings>,
    profile: Mutex<Profile>,
    paths: Paths,
}

type CmdResult<T> = Result<T, String>;

impl AppState {
    fn update_settings(&self, f: impl FnOnce(&mut Settings)) {
        let mut s = self.settings.lock().unwrap();
        f(&mut s);
        if let Ok(json) = serde_json::to_string_pretty(&*s) {
            let _ = std::fs::write(&self.paths.settings, json);
        }
    }
}

// ---------- профили ----------

#[derive(Serialize)]
struct ProfileEntry {
    name: String,
    builtin: bool,
}

fn builtin_profiles() -> Vec<Profile> {
    vec![presets::classic()]
}

fn user_profiles(dir: &Path) -> Vec<Profile> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<Profile> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|s| Profile::from_json(&s).ok())
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

fn find_profile(dir: &Path, name: &str) -> Option<Profile> {
    user_profiles(dir).into_iter().chain(builtin_profiles()).find(|p| p.name == name)
}

fn file_name_for(name: &str) -> String {
    let safe: String = name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    format!("{}.json", safe.trim())
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
fn set_midi_out(state: State<AppState>, port: Option<String>) {
    state.update_settings(|s| s.midi_out = port.clone());
    state.engine.send(Command::SetMidiOut(port));
}

#[tauri::command]
fn list_profiles(state: State<AppState>) -> Vec<ProfileEntry> {
    let user = user_profiles(&state.paths.profiles);
    let mut v: Vec<ProfileEntry> = builtin_profiles()
        .into_iter()
        .filter(|b| !user.iter().any(|u| u.name == b.name))
        .map(|p| ProfileEntry { name: p.name, builtin: true })
        .collect();
    v.extend(user.into_iter().map(|p| ProfileEntry { name: p.name, builtin: false }));
    v
}

#[tauri::command]
fn get_profile(state: State<AppState>) -> Profile {
    state.profile.lock().unwrap().clone()
}

#[tauri::command]
fn select_profile(state: State<AppState>, name: String) -> CmdResult<()> {
    let profile = find_profile(&state.paths.profiles, &name).ok_or(format!("профиль «{name}» не найден"))?;
    *state.profile.lock().unwrap() = profile.clone();
    state.update_settings(|s| s.profile = Some(name));
    state.engine.send(Command::LoadProfile(Box::new(profile)));
    Ok(())
}

/// Сохраняет профиль в папку пользователя и, если он текущий, сразу применяет.
#[tauri::command]
fn save_profile(state: State<AppState>, profile: Profile) -> CmdResult<()> {
    profile.validate().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&state.paths.profiles).map_err(|e| e.to_string())?;
    let path = state.paths.profiles.join(file_name_for(&profile.name));
    std::fs::write(&path, profile.to_json()).map_err(|e| e.to_string())?;
    let mut current = state.profile.lock().unwrap();
    if current.name == profile.name {
        *current = profile.clone();
        state.engine.send(Command::LoadProfile(Box::new(profile)));
    }
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
    let dir = &state.paths.recordings;
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
    let paths = Paths { settings: config_dir.join("settings.json"), profiles: data_dir.join("profiles"), recordings };

    let settings: Settings = std::fs::read_to_string(&paths.settings)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let profile = settings
        .profile
        .as_deref()
        .and_then(|name| find_profile(&paths.profiles, name))
        .unwrap_or_else(presets::classic);

    let handle = app.handle().clone();
    let engine = Engine::start(
        EngineConfig {
            profile: profile.clone(),
            settings: settings.clone(),
            soundfont_dirs: vec![data_dir.join("sounds")],
            recordings_dir: paths.recordings.clone(),
        },
        move |status| {
            let _ = handle.emit("status", status);
        },
    );
    app.manage(AppState { engine, settings: Mutex::new(settings), profile: Mutex::new(profile), paths });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(setup)
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_presets,
            get_settings,
            list_midi_ports,
            set_pad_choice,
            set_synth_enabled,
            set_midi_out,
            list_profiles,
            get_profile,
            select_profile,
            save_profile,
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
