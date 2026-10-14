//! Запись целиком на настоящем движке: отсчёт → ноты (прослушивание партии) → стоп →
//! .mid формата 1 и WAV в фоне. cargo run -p engine --example record_check --release

use std::time::{Duration, Instant};

use engine::transport::TransportSettings;
use engine::{presets, Command, Engine, EngineConfig, Settings};

fn main() {
    let dir = std::env::temp_dir().join(format!("musicgamepad-record-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut profile = presets::classic();
    profile.parts.push(engine::model::Part { name: "Бас".into(), channel: 1, program: 33, ..profile.parts[0].clone() });
    let settings = Settings {
        transport: TransportSettings { bpm: 240.0, count_in: true, export_wav: true, ..Default::default() },
        ..Default::default()
    };
    let engine = Engine::start(
        EngineConfig { profile, settings, soundfont_dirs: Vec::new(), recordings_dir: dir.clone() },
        |_| {},
    );
    std::thread::sleep(Duration::from_millis(500));

    let t = Instant::now();
    engine.send(Command::ToggleRecord);
    std::thread::sleep(Duration::from_millis(100));
    println!("через 0,1 с: {:?}, отсчёт осталось долей {:?}", engine.status().transport.record, engine.status().transport.count_in_left);
    std::thread::sleep(Duration::from_millis(1000)); // такт отсчёта при 240 BPM = 1 с
    println!("через {:.1} с: {:?}", t.elapsed().as_secs_f32(), engine.status().transport.record);
    engine.send(Command::Audition { part: 0 });
    engine.send(Command::Audition { part: 1 });
    std::thread::sleep(Duration::from_millis(1500));
    engine.send(Command::ToggleRecord);

    // ждём WAV
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
        let s = engine.status();
        if s.transport.exporting == 0 && s.transport.last_wav.is_some() {
            break;
        }
    }
    let s = engine.status();
    let mid = s.last_recording.expect("нет .mid");
    let bytes = std::fs::read(&mid).unwrap();
    let format = u16::from_be_bytes([bytes[8], bytes[9]]);
    let tracks = u16::from_be_bytes([bytes[10], bytes[11]]);
    let has = |n: &[u8]| bytes.windows(n.len()).any(|w| w == n);
    println!("MIDI: {} байт, формат {format}, дорожек {tracks}, «Бас» {}, темп 240 {}", bytes.len(), has("Бас".as_bytes()), has(&[0xFF, 0x51, 3, 0x03, 0xD0, 0x90]));
    let wav = s.transport.last_wav.expect("нет WAV");
    let w = std::fs::read(&wav).unwrap();
    let secs = (w.len() - 44) as f64 / (48_000.0 * 4.0);
    let peak = w[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs()).max().unwrap_or(0);
    println!("WAV: {:.1} с, пик {peak} из 32767", secs);
    drop(engine);
    let _ = std::fs::remove_dir_all(&dir);
}
