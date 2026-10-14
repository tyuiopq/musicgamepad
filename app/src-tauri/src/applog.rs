//! Журнал ошибок: тихо пишет в файл в папке данных программы (интерфейс его не показывает).
//! Туда попадают паники, ошибки движка и команд, ошибки интерфейса — чтобы было что прислать,
//! если «что-то не работает».

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Больше этого журнал переезжает в `.old.log` и начинается заново.
const MAX_BYTES: u64 = 512 * 1024;

static LOG: OnceLock<Mutex<PathBuf>> = OnceLock::new();

/// Указать папку журнала и перехватить паники.
pub fn init(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    let _ = LOG.set(Mutex::new(dir.join("musicgamepad.log")));
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current().name().unwrap_or("?").to_string();
        let trace = std::backtrace::Backtrace::force_capture();
        write("PANIC", &format!("поток «{thread}»: {info}\n{trace}"));
        default(info);
    }));
    write(
        "INFO",
        &format!("запуск MusicGamepad {} ({} {})", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH),
    );
}

pub fn dir() -> Option<PathBuf> {
    LOG.get().and_then(|p| p.lock().ok()?.parent().map(Path::to_path_buf))
}

pub fn write(level: &str, message: &str) {
    let Some(lock) = LOG.get() else { return };
    let Ok(path) = lock.lock() else { return };
    if std::fs::metadata(&*path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(&*path, path.with_extension("old.log"));
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&*path) {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(f, "{now} [{level}] {message}");
    }
}

/// Пишет ошибку, только когда она появилась или сменилась (статус приходит до 60 раз в секунду).
#[derive(Default)]
pub struct Changes {
    last: std::collections::HashMap<String, String>,
}

impl Changes {
    pub fn track(&mut self, source: &str, error: Option<&str>) {
        match error {
            Some(e) if self.last.get(source).map(String::as_str) != Some(e) => {
                write("ERROR", &format!("{source}: {e}"));
                self.last.insert(source.to_string(), e.to_string());
            }
            None => {
                self.last.remove(source);
            }
            _ => {}
        }
    }
}
