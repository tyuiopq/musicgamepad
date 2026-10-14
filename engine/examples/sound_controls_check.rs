//! Проверка звуковых контроллеров GM2 в синтезаторе: яркость, атака, затухание.
//! cargo run -p engine --example sound_controls_check --release

use engine::audio::SoundBank;
use rustysynth::{Synthesizer, SynthesizerSettings};

const RATE: i32 = 48_000;

/// Отрисовать ноту: `hold_ms` держим, затем `tail_ms` после отпускания. Контроллеры — (cc, value).
fn render(bank: &SoundBank, program: i32, ccs: &[(i32, i32)], hold_ms: usize, tail_ms: usize) -> Vec<f32> {
    let mut s = SynthesizerSettings::new(RATE);
    s.enable_reverb_and_chorus = false;
    let mut synth = Synthesizer::new(&bank.font, &s).unwrap();
    synth.process_midi_message(0, 0xC0, program, 0);
    for &(cc, v) in ccs {
        synth.process_midi_message(0, 0xB0, cc, v);
    }
    synth.note_on(0, 60, 110);
    let mut out = Vec::new();
    let mut block = |ms: usize, synth: &mut Synthesizer| {
        let n = RATE as usize * ms / 1000;
        let (mut l, mut r) = (vec![0f32; n], vec![0f32; n]);
        synth.render(&mut l, &mut r);
        out.extend(l);
    };
    block(hold_ms, &mut synth);
    synth.note_off(0, 60);
    block(tail_ms, &mut synth);
    out
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

/// Доля высоких частот: RMS разности соседних сэмплов к RMS сигнала.
fn brightness(x: &[f32]) -> f32 {
    let d: Vec<f32> = x.windows(2).map(|w| w[1] - w[0]).collect();
    rms(&d) / rms(x).max(1e-9)
}

/// Время, за которое огибающая доходит до половины максимума, мс.
fn rise_ms(x: &[f32]) -> f32 {
    let win = 48;
    let env: Vec<f32> = x.chunks(win).map(rms).collect();
    let peak = env.iter().cloned().fold(0.0, f32::max);
    env.iter().position(|&e| e >= peak / 2.0).unwrap_or(0) as f32 * win as f32 / 48.0
}

fn main() {
    let bank = SoundBank::load(&SoundBank::find(None, &[]).expect("нет .sf2")).unwrap();
    let saw = 81; // Lead 2 (sawtooth) — яркий звук, фильтр слышно
    let dark = brightness(&render(&bank, saw, &[(74, 0)], 400, 0));
    let neutral = brightness(&render(&bank, saw, &[], 400, 0));
    let bright = brightness(&render(&bank, saw, &[(74, 127)], 400, 0));
    println!("Яркость (CC74 0 / нейтр. / 127): {dark:.3} / {neutral:.3} / {bright:.3}");

    let strings = 48;
    let fast = rise_ms(&render(&bank, strings, &[(73, 0)], 1500, 0));
    let normal = rise_ms(&render(&bank, strings, &[], 1500, 0));
    let slow = rise_ms(&render(&bank, strings, &[(73, 127)], 3000, 0));
    println!("Атака струнных до 50% (CC73 0 / нейтр. / 127): {fast:.0} / {normal:.0} / {slow:.0} мс");

    let tail = |v: Option<i32>| {
        let ccs: Vec<(i32, i32)> = v.map(|v| vec![(72, v)]).unwrap_or_default();
        let x = render(&bank, 0, &ccs, 300, 600);
        rms(&x[RATE as usize * 600 / 1000..]) / rms(&x[..RATE as usize * 300 / 1000])
    };
    println!(
        "Остаток фортепиано через 0,3 с после отпускания (CC72 0 / нейтр. / 127): {:.3} / {:.3} / {:.3}",
        tail(Some(0)),
        tail(None),
        tail(Some(127))
    );
}
