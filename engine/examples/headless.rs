//! Движок без окна: печатает состояние. Удобно для отладки.
//! cargo run -p engine --example headless --release

use std::time::Duration;

use engine::{presets, Engine, EngineConfig, Settings};

fn main() {
    let engine = Engine::start(
        EngineConfig {
            profile: presets::classic(),
            settings: Settings::default(),
            soundfont_dirs: Vec::new(),
            recordings_dir: std::env::temp_dir().join("musicgamepad"),
        },
        |status| {
            let notes: Vec<String> = status.notes.iter().map(|&n| engine::theory::note_name(n)).collect();
            println!(
                "pads={:?} active={:?} layers={:?} notes={:?} rec={}",
                status.pads.iter().map(|p| format!("{} [{}]", p.name, p.model)).collect::<Vec<_>>(),
                status.active_pad,
                status.layers,
                notes,
                status.recording,
            );
        },
    );
    std::thread::sleep(Duration::from_millis(1500));
    let s = engine.status();
    println!("\nсинтезатор: {:?}", s.synth);
    println!("инструмент: {:?}", s.parts.iter().map(|p| &p.instrument).collect::<Vec<_>>());
    println!("пресетов в банке: {}", engine.presets().len());
    let secs: u64 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(0);
    std::thread::sleep(Duration::from_secs(secs));
}
