//! Монитор и ограничение частоты: 100 значений CC за 20 мс → в выход уходит меньше,
//! но последнее значение (127) доходит. cargo run -p engine --example monitor_check --release

use std::time::Duration;

use engine::midi::MidiMsg;
use engine::{presets, Command, Engine, EngineConfig, Settings};

fn main() {
    let engine = Engine::start(
        EngineConfig {
            profile: presets::classic(),
            settings: Settings { synth_enabled: false, ..Default::default() },
            soundfont_dirs: Vec::new(),
            recordings_dir: std::env::temp_dir(),
            library_dir: std::env::temp_dir(),
        },
        |_| {},
    );
    std::thread::sleep(Duration::from_millis(300));
    let since = engine.monitor(0).last().map_or(0, |e| e.seq);
    let msgs: Vec<(u64, MidiMsg)> = (0..=100u64).map(|i| (i / 5, MidiMsg::cc(0, 74, (i * 127 / 100) as u8))).collect();
    engine.send(Command::SendTest(msgs));
    engine.send(Command::SendTest(vec![(30, MidiMsg::note_on(0, 60, 100)), (60, MidiMsg::note_off(0, 60))]));
    std::thread::sleep(Duration::from_millis(200));
    let log = engine.monitor(since);
    let cc: Vec<&engine::engine::MonitorEntry> = log.iter().filter(|e| e.bytes[0] == 0xB0 && e.bytes[1] == 74).collect();
    println!(
        "CC74: отправлено 101, ушло {}, последнее значение {}; ноты в мониторе: {}",
        cc.len(),
        cc.last().map_or(0, |e| e.bytes[2]),
        log.iter().filter(|e| e.bytes[0] & 0xF0 == 0x90 || e.bytes[0] & 0xF0 == 0x80).count()
    );
}
