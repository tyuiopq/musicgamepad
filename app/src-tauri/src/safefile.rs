//! Сохранение, которое не портит файл при сбое: запись во временный файл, сброс на диск,
//! подмена. Прежняя версия остаётся рядом как `.bak` — её читаем, если основной файл повреждён.

use std::io::Write;
use std::path::{Path, PathBuf};

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

pub fn backup_path(path: &Path) -> PathBuf {
    sibling(path, ".bak")
}

pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = sibling(path, ".tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    if path.exists() {
        let _ = std::fs::copy(path, backup_path(path));
    }
    // На Windows rename заменяет существующий файл (MoveFileEx с REPLACE_EXISTING)
    std::fs::rename(&tmp, path)
}

/// Прочитать и разобрать файл; если он повреждён или пропал — резервную копию.
pub fn read_parsed<T>(path: &Path, parse: impl Fn(&str) -> Option<T>) -> Option<T> {
    let main = std::fs::read_to_string(path).ok().and_then(|s| parse(&s));
    main.or_else(|| std::fs::read_to_string(backup_path(path)).ok().and_then(|s| parse(&s)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_backup_when_main_is_corrupt() {
        let dir = std::env::temp_dir().join(format!("musicgamepad-safe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        write_atomic(&path, b"{\"v\":1}").unwrap();
        write_atomic(&path, b"{\"v\":2}").unwrap();
        assert_eq!(std::fs::read_to_string(backup_path(&path)).unwrap(), "{\"v\":1}");
        // «сбой» посреди записи: основной файл обрезан
        std::fs::write(&path, b"{\"v\":").unwrap();
        let v = read_parsed(&path, |s| serde_json::from_str::<serde_json::Value>(s).ok());
        assert_eq!(v.unwrap()["v"], 1);
        assert!(!sibling(&path, ".tmp").exists(), "временный файл не остаётся");
        let _ = std::fs::remove_dir_all(dir);
    }
}
