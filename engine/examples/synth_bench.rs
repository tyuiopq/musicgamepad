//! Успевает ли синтезатор в эксклюзивный период (~3 мс = 144 кадра при 48 кГц).
//! cargo run -p engine --example synth_bench --release

use std::time::Instant;

use engine::audio::SoundBank;
use rustysynth::{Synthesizer, SynthesizerSettings};

fn main() {
    let bank = SoundBank::load(&SoundBank::find(None, &[]).expect("нет .sf2")).unwrap();
    let mut settings = SynthesizerSettings::new(48_000);
    settings.block_size = 64;
    let mut synth = Synthesizer::new(&bank.font, &settings).unwrap();
    // Худший случай: 4 партии с длинными звуками, по 6 нот, педаль
    for (ch, program) in [(0, 0), (1, 48), (2, 19), (3, 89)] {
        synth.process_midi_message(ch, 0xC0, program, 0);
        synth.process_midi_message(ch, 0xB0, 64, 127);
        for n in [48, 52, 55, 60, 64, 67] {
            synth.note_on(ch, n, 110);
        }
    }
    let frames = 144;
    let (mut l, mut r) = (vec![0f32; frames], vec![0f32; frames]);
    let mut worst = 0f64;
    let mut total = 0f64;
    let runs = 2000;
    for _ in 0..runs {
        let t = Instant::now();
        synth.render(&mut l, &mut r);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        worst = worst.max(ms);
        total += ms;
    }
    let period = frames as f64 / 48.0;
    println!(
        "24 ноты, 4 инструмента: в среднем {:.3} мс, худший {:.3} мс на блок из {frames} кадров (период {period:.1} мс) — загрузка {:.0}%",
        total / runs as f64,
        worst,
        100.0 * total / runs as f64 / period
    );
}
