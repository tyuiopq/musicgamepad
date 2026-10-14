//! Стойка на настоящем движке: партия-сэмплер (свои WAV), партия со своим .sf2,
//! партия с несуществующим файлом (ошибка, а не падение), запись в WAV.
//! cargo run -p engine --example sampler_check --release

use std::time::{Duration, Instant};

use engine::model::{Part, PartSource, SamplePad};
use engine::transport::TransportSettings;
use engine::{presets, Command, Engine, EngineConfig, Settings};

fn write_tone(path: &std::path::Path, freq: f32, secs: f32) {
    let spec = hound::WavSpec { channels: 1, sample_rate: 44_100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..(44_100.0 * secs) as usize {
        let t = i as f32 / 44_100.0;
        let env = (1.0 - t / secs).max(0.0);
        w.write_sample((((t * freq * std::f32::consts::TAU).sin()) * env * 20_000.0) as i16).unwrap();
    }
    w.finalize().unwrap();
}

fn main() {
    let dir = std::env::temp_dir().join(format!("musicgamepad-rack-{}", std::process::id()));
    let lib = dir.join("sounds");
    std::fs::create_dir_all(lib.join("samples")).unwrap();
    write_tone(&lib.join("samples/kick.wav"), 60.0, 0.4);
    write_tone(&lib.join("samples/hat.wav"), 4000.0, 0.1);
    let sf2 = engine::audio::SoundBank::find(None, &[]).expect("нет .sf2");

    let mut profile = presets::classic();
    let base = profile.parts[0].clone();
    profile.parts.push(Part {
        name: "Кит".into(),
        channel: 9,
        source: PartSource::Samples {
            pads: vec![
                SamplePad { note: 36, file: "samples/kick.wav".into(), gain_db: 0.0, pitch: 0, choke: 0 },
                SamplePad { note: 42, file: "samples/hat.wav".into(), gain_db: -6.0, pitch: 0, choke: 1 },
            ],
        },
        ..base.clone()
    });
    profile.parts.push(Part { name: "Свой банк".into(), channel: 2, program: 48, source: PartSource::SoundFont { file: sf2.display().to_string() }, ..base.clone() });
    profile.parts.push(Part { name: "Битый".into(), channel: 3, source: PartSource::SoundFont { file: "нет-такого.sf2".into() }, ..base });

    let settings = Settings { transport: TransportSettings { count_in: false, export_wav: true, ..Default::default() }, ..Default::default() };
    let engine = Engine::start(
        EngineConfig { profile, settings, soundfont_dirs: Vec::new(), recordings_dir: dir.clone(), library_dir: lib },
        |_| {},
    );
    std::thread::sleep(Duration::from_millis(1200));
    let s = engine.status();
    for p in &s.parts {
        println!("партия «{}»: {} {}", p.name, p.instrument, p.error.as_deref().map_or(String::new(), |e| format!("— ОШИБКА: {e}")));
    }

    engine.send(Command::ToggleRecord);
    engine.send(Command::Audition { part: 1 });
    engine.send(Command::Audition { part: 2 });
    std::thread::sleep(Duration::from_millis(1500));
    engine.send(Command::ToggleRecord);
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && engine.status().transport.last_wav.is_none() {
        std::thread::sleep(Duration::from_millis(200));
    }
    let wav = engine.status().transport.last_wav.expect("нет WAV");
    let w = std::fs::read(&wav).unwrap();
    let peak = w[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs()).max().unwrap_or(0);
    println!("WAV через стойку: {:.1} с, пик {peak}", (w.len() - 44) as f64 / 192_000.0);
    drop(engine);
    let _ = std::fs::remove_dir_all(&dir);
}
