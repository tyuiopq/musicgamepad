//! Экспорт записи в WAV: события заново проигрываются синтезатором без вывода на устройство.

use std::path::Path;
use std::sync::Arc;

use rustysynth::SoundFont;

use crate::midi::MidiMsg;
use crate::rack::{new_synth, Rack, RackConfig};

pub const SAMPLE_RATE: u32 = 48_000;
const CHUNK: usize = 4096;
/// Хвост после последнего события: дописываем, пока не затихнет, но не дольше.
const MAX_TAIL_S: f64 = 6.0;
const SILENCE: f32 = 1e-4;
/// Пик после нормализации: −1 дБ.
const TARGET_PEAK: f32 = 0.891;

/// Отрисовать события (время в секундах) в стерео f32, нормализовав громкость.
/// `gain` нужен только для полной тишины (тогда нормализовать нечего).
pub fn render(mut synth: Rack, events: &[(f64, MidiMsg)], gain: f32) -> anyhow::Result<(Vec<f32>, Vec<f32>)> {
    let mut sorted: Vec<&(f64, MidiMsg)> = events.iter().collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));

    let (mut left, mut right) = (Vec::new(), Vec::new());
    let (mut l, mut r) = (vec![0f32; CHUNK], vec![0f32; CHUNK]);
    let mut render_to = |frame: usize, synth: &mut Rack, left: &mut Vec<f32>, right: &mut Vec<f32>| {
        while left.len() < frame {
            let n = (frame - left.len()).min(CHUNK);
            synth.render(&mut l[..n], &mut r[..n]);
            left.extend_from_slice(&l[..n]);
            right.extend_from_slice(&r[..n]);
        }
    };

    for (t, msg) in sorted {
        render_to((t.max(0.0) * SAMPLE_RATE as f64) as usize, &mut synth, &mut left, &mut right);
        synth.process(*msg);
    }

    // Хвост: пока звучит (реверберация, затухание), но не дольше MAX_TAIL_S
    let max_tail = (MAX_TAIL_S * SAMPLE_RATE as f64) as usize;
    let tail_start = left.len();
    loop {
        let before = left.len();
        render_to(before + CHUNK, &mut synth, &mut left, &mut right);
        let quiet = left[before..].iter().chain(&right[before..]).all(|s| s.abs() < SILENCE);
        if (quiet && before > tail_start) || left.len() - tail_start >= max_tail {
            break;
        }
    }

    // Нормализация: самый громкий момент — около −1 дБ, чтобы файл не был тихим
    let peak = left.iter().chain(&right).fold(0f32, |m, s| m.max(s.abs()));
    let scale = if peak > SILENCE { TARGET_PEAK / peak } else { gain };
    left.iter_mut().chain(right.iter_mut()).for_each(|s| *s *= scale);
    Ok((left, right))
}

/// 16-битный стерео WAV.
pub fn wav_bytes(left: &[f32], right: &[f32]) -> Vec<u8> {
    let frames = left.len().min(right.len());
    let data_len = (frames * 4) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend(b"RIFF");
    out.extend((36 + data_len).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes()); // PCM
    out.extend(2u16.to_le_bytes()); // стерео
    out.extend(SAMPLE_RATE.to_le_bytes());
    out.extend((SAMPLE_RATE * 4).to_le_bytes());
    out.extend(4u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(data_len.to_le_bytes());
    for i in 0..frames {
        for s in [left[i], right[i]] {
            out.extend(((s.clamp(-1.0, 1.0) * 32_767.0) as i16).to_le_bytes());
        }
    }
    out
}

/// Стойка для экспорта: основной синтезатор на частоте WAV плюс конфигурация партий.
pub fn rack(font: &Arc<SoundFont>, cfg: RackConfig) -> anyhow::Result<Rack> {
    let mut rack = Rack::new(new_synth(font, SAMPLE_RATE)?, SAMPLE_RATE);
    rack.apply(cfg);
    Ok(rack)
}

pub fn export(rack: Rack, events: &[(f64, MidiMsg)], gain: f32, path: &Path) -> anyhow::Result<()> {
    let (l, r) = render(rack, events, gain)?;
    std::fs::write(path, wav_bytes(&l, &r))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header() {
        let b = wav_bytes(&[0.0, 1.0], &[0.0, -1.0]);
        assert_eq!(&b[..4], b"RIFF");
        assert_eq!(&b[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(b[24..28].try_into().unwrap()), SAMPLE_RATE);
        assert_eq!(u32::from_le_bytes(b[40..44].try_into().unwrap()), 8, "2 кадра × 4 байта");
        assert_eq!(&b[48..52], &[0xFF, 0x7F, 0x01, 0x80]);
    }
}
