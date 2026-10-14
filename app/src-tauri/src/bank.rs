//! Звуковой банк при первом запуске: скачать GeneralUser GS (он не входит в установщик).

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const URL: &str = "https://github.com/mrbumpy409/GeneralUser-GS/raw/main/GeneralUser-GS.sf2";
pub const FILE: &str = "GeneralUser-GS.sf2";

/// Скачать банк в `dir`. `progress(скачано, всего)` — по ходу; `cancel` прерывает.
/// Пишет во временный файл и подменяет только целый и проверенный.
pub fn download(dir: &Path, cancel: &AtomicBool, mut progress: impl FnMut(u64, Option<u64>)) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Не удалось создать папку звуков: {e}"))?;
    let tls = native_tls::TlsConnector::new().map_err(|e| format!("TLS: {e}"))?;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .build();
    let resp = agent.get(URL).call().map_err(|e| format!("Не получилось скачать: {e}"))?;
    let total = resp.header("Content-Length").and_then(|v| v.parse::<u64>().ok());
    let mut reader = resp.into_reader();
    let part = dir.join(format!("{FILE}.part"));
    let result = (|| {
        let mut f = File::create(&part).map_err(|e| format!("Не удалось записать файл: {e}"))?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut done = 0u64;
        let mut reported = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Скачивание отменено".to_string());
            }
            let n = reader.read(&mut buf).map_err(|e| format!("Связь прервалась: {e}"))?;
            if n == 0 {
                break;
            }
            f.write_all(&buf[..n]).map_err(|e| format!("Не удалось записать файл: {e}"))?;
            done += n as u64;
            if done - reported >= 256 * 1024 {
                progress(done, total);
                reported = done;
            }
        }
        f.sync_all().map_err(|e| e.to_string())?;
        progress(done, total);
        if total.is_some_and(|t| t != done) {
            return Err("Файл скачался не полностью — попробуй ещё раз".to_string());
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    if !is_soundfont(&part) {
        let _ = std::fs::remove_file(&part);
        return Err("Скачался не звуковой банк (возможно, сайт поменял адрес)".into());
    }
    let target = dir.join(FILE);
    std::fs::rename(&part, &target).map_err(|e| format!("Не удалось сохранить банк: {e}"))?;
    Ok(target)
}

/// Похоже ли на SoundFont 2: «RIFF….sfbk» и не крошечный.
fn is_soundfont(path: &Path) -> bool {
    let mut head = [0u8; 12];
    let big = std::fs::metadata(path).is_ok_and(|m| m.len() > 1024 * 1024);
    big && File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok() && &head[..4] == b"RIFF" && &head[8..12] == b"sfbk"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Настоящее скачивание (32 МБ) — запускать вручную: cargo test -p musicgamepad -- --ignored
    #[test]
    #[ignore]
    fn downloads_real_bank() {
        let dir = std::env::temp_dir().join(format!("musicgamepad-bank-{}", std::process::id()));
        let mut last = (0, None);
        let path = download(&dir, &AtomicBool::new(false), |d, t| last = (d, t)).unwrap();
        assert!(is_soundfont(&path));
        assert_eq!(Some(last.0), last.1, "скачано столько, сколько обещал сервер");
        let _ = std::fs::remove_dir_all(dir);
    }
}
