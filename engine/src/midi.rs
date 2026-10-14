//! MIDI-сообщения и запись в стандартный .mid-файл.

use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidiMsg {
    pub bytes: [u8; 3],
    pub len: u8,
}

impl MidiMsg {
    fn three(status: u8, d1: u8, d2: u8) -> Self {
        Self { bytes: [status, d1 & 0x7F, d2 & 0x7F], len: 3 }
    }

    pub fn note_on(ch: u8, note: u8, vel: u8) -> Self {
        Self::three(0x90 | ch, note, vel)
    }

    pub fn note_off(ch: u8, note: u8) -> Self {
        Self::three(0x80 | ch, note, 0)
    }

    pub fn cc(ch: u8, cc: u8, val: u8) -> Self {
        Self::three(0xB0 | ch, cc, val)
    }

    pub fn program(ch: u8, program: u8) -> Self {
        Self { bytes: [0xC0 | ch, program & 0x7F, 0], len: 2 }
    }

    /// Channel Pressure (aftertouch на весь канал).
    pub fn pressure(ch: u8, value: u8) -> Self {
        Self { bytes: [0xD0 | ch, value & 0x7F, 0], len: 2 }
    }

    /// Однобайтовое системное сообщение реального времени (Clock, Start, Stop…).
    pub fn realtime(status: u8) -> Self {
        Self { bytes: [status, 0, 0], len: 1 }
    }

    /// `value`: 0..16383, центр 8192.
    pub fn bend(ch: u8, value: u16) -> Self {
        Self::three(0xE0 | ch, (value & 0x7F) as u8, (value >> 7) as u8)
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    pub fn status(&self) -> u8 {
        self.bytes[0] & 0xF0
    }

    pub fn channel(&self) -> u8 {
        self.bytes[0] & 0x0F
    }

    /// Системное сообщение (0xF0..0xFF) — не канальное.
    pub fn is_system(&self) -> bool {
        self.bytes[0] >= 0xF0
    }
}

/// Сообщения, задающие диапазон подтяжки через RPN 0.
pub fn bend_range(ch: u8, semitones: u8) -> [MidiMsg; 4] {
    [MidiMsg::cc(ch, 101, 0), MidiMsg::cc(ch, 100, 0), MidiMsg::cc(ch, 6, semitones), MidiMsg::cc(ch, 38, 0)]
}

/// Выбор банка: CC0 (старший байт) и CC32 (младший).
pub fn bank_select(ch: u8, bank: u16) -> [MidiMsg; 2] {
    [MidiMsg::cc(ch, 0, (bank >> 7) as u8), MidiMsg::cc(ch, 32, (bank & 0x7F) as u8)]
}

/// Запись сыгранного: события со временем от `t0`. Если шёл метроном, `t0` — начало такта,
/// и ноты в файле ложатся на сетку DAW.
pub struct Recorder {
    t0: Instant,
    events: Vec<(f64, MidiMsg)>,
}

impl Recorder {
    /// `initial` — состояние на момент старта (инструменты, громкость),
    /// чтобы файл звучал так же, как в программе.
    pub fn start_at(t0: Instant, initial: impl IntoIterator<Item = MidiMsg>) -> Self {
        Self { t0, events: initial.into_iter().map(|m| (0.0, m)).collect() }
    }

    pub fn start(initial: impl IntoIterator<Item = MidiMsg>) -> Self {
        Self::start_at(Instant::now(), initial)
    }

    pub fn add_at(&mut self, at: Instant, msg: MidiMsg) {
        self.events.push((at.saturating_duration_since(self.t0).as_secs_f64(), msg));
    }

    pub fn add(&mut self, msg: MidiMsg) {
        self.add_at(Instant::now(), msg);
    }

    pub fn elapsed(&self) -> f64 {
        self.t0.elapsed().as_secs_f64()
    }

    pub fn has_notes(&self) -> bool {
        self.events.iter().any(|(_, m)| m.is_note_on())
    }

    /// Дописывает note-off для `sounding`, чтобы в файле не осталось зависших нот.
    pub fn finish(mut self, sounding: impl IntoIterator<Item = (u8, u8)>) -> Vec<(f64, MidiMsg)> {
        let t = self.elapsed();
        for (ch, note) in sounding {
            self.events.push((t, MidiMsg::note_off(ch, note)));
        }
        self.events
    }
}

impl MidiMsg {
    pub fn is_note_on(&self) -> bool {
        self.status() == 0x90 && self.bytes[2] > 0
    }

    pub fn is_note_off(&self) -> bool {
        self.status() == 0x80 || (self.status() == 0x90 && self.bytes[2] == 0)
    }
}

pub const PPQ: u16 = 480;

/// Квантование нот при сохранении: шаг сетки в тиках при PPQ 480.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quantize {
    #[default]
    Off,
    Quarter,
    Eighth,
    Sixteenth,
    EighthTriplet,
}

impl Quantize {
    pub fn grid(self) -> Option<u32> {
        match self {
            Quantize::Off => None,
            Quantize::Quarter => Some(PPQ as u32),
            Quantize::Eighth => Some(PPQ as u32 / 2),
            Quantize::Sixteenth => Some(PPQ as u32 / 4),
            Quantize::EighthTriplet => Some(PPQ as u32 / 3),
        }
    }
}

pub struct SmfOptions<'a> {
    pub bpm: f64,
    pub beats_per_bar: u8,
    pub quantize: Quantize,
    /// Названия дорожек по каналам (имена партий).
    pub track_names: &'a [(u8, String)],
}

fn varlen(mut n: u32, out: &mut Vec<u8>) {
    let mut buf = vec![(n & 0x7F) as u8];
    n >>= 7;
    while n > 0 {
        buf.push((n & 0x7F) as u8 | 0x80);
        n >>= 7;
    }
    out.extend(buf.iter().rev());
}

fn meta(out: &mut Vec<u8>, kind: u8, data: &[u8]) {
    varlen(0, out);
    out.extend([0xFF, kind]);
    varlen(data.len() as u32, out);
    out.extend(data);
}

/// Сдвигает начало каждой ноты на ближайшую линию сетки; конец сдвигается вместе с ней,
/// так что длительность сохраняется (но нота не короче четверти шага).
fn quantize_notes(events: &mut [(u32, MidiMsg)], grid: u32) {
    let n = events.len();
    for i in 0..n {
        if !events[i].1.is_note_on() {
            continue;
        }
        let (tick, msg) = events[i];
        let snapped = ((tick as f64 / grid as f64).round() as u32) * grid;
        let delta = snapped as i64 - tick as i64;
        events[i].0 = snapped;
        let off = (i + 1..n).find(|&j| {
            let m = events[j].1;
            m.is_note_off() && m.channel() == msg.channel() && m.bytes[1] == msg.bytes[1] && events[j].0 >= tick
        });
        if let Some(j) = off {
            let moved = (events[j].0 as i64 + delta).max(0) as u32;
            events[j].0 = moved.max(snapped + grid / 4);
        }
    }
    events.sort_by_key(|(t, m)| (*t, m.is_note_on()));
}

fn track_bytes(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend(b"MTrk");
    out.extend((body.len() as u32).to_be_bytes());
    out.extend(body);
    out
}

/// Стандартный MIDI-файл формата 1: дорожка темпа и по дорожке на каждый канал.
pub fn smf_bytes(events: &[(f64, MidiMsg)], opts: &SmfOptions) -> Vec<u8> {
    let ticks_per_sec = PPQ as f64 * opts.bpm / 60.0;
    let mut ticked: Vec<(u32, MidiMsg)> =
        events.iter().map(|(t, m)| ((t.max(0.0) * ticks_per_sec).round() as u32, *m)).collect();
    ticked.sort_by_key(|(t, _)| *t);
    if let Some(grid) = opts.quantize.grid() {
        quantize_notes(&mut ticked, grid);
    }

    // Дорожка 0: название, темп, размер
    let mut conductor = Vec::new();
    meta(&mut conductor, 0x03, "MusicGamepad".as_bytes());
    meta(&mut conductor, 0x51, &((60_000_000.0 / opts.bpm).round() as u32).to_be_bytes()[1..]);
    meta(&mut conductor, 0x58, &[opts.beats_per_bar.max(1), 2, 24, 8]);
    varlen(0, &mut conductor);
    conductor.extend([0xFF, 0x2F, 0x00]);

    let mut channels: Vec<u8> = ticked.iter().map(|(_, m)| m.channel()).collect();
    channels.sort_unstable();
    channels.dedup();

    let mut out = Vec::new();
    out.extend(b"MThd");
    out.extend(6u32.to_be_bytes());
    out.extend(1u16.to_be_bytes());
    out.extend((channels.len() as u16 + 1).to_be_bytes());
    out.extend(PPQ.to_be_bytes());
    out.extend(track_bytes(&conductor));

    for ch in channels {
        let mut body = Vec::new();
        let name = opts
            .track_names
            .iter()
            .find(|(c, _)| *c == ch)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| if ch == 9 { "Ударные".into() } else { format!("Канал {}", ch + 1) });
        meta(&mut body, 0x03, name.as_bytes());
        let mut last = 0u32;
        for (tick, msg) in ticked.iter().filter(|(_, m)| m.channel() == ch) {
            varlen(tick - last, &mut body);
            body.extend(msg.as_slice());
            last = *tick;
        }
        varlen(0, &mut body);
        body.extend([0xFF, 0x2F, 0x00]);
        out.extend(track_bytes(&body));
    }
    out
}

pub fn write_smf(path: &Path, events: &[(f64, MidiMsg)], opts: &SmfOptions) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, smf_bytes(events, opts))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(quantize: Quantize) -> SmfOptions<'static> {
        SmfOptions { bpm: 120.0, beats_per_bar: 4, quantize, track_names: &[] }
    }

    #[test]
    fn varlen_encoding() {
        let enc = |n| {
            let mut v = Vec::new();
            varlen(n, &mut v);
            v
        };
        assert_eq!(enc(0), [0x00]);
        assert_eq!(enc(0x7F), [0x7F]);
        assert_eq!(enc(0x80), [0x81, 0x00]);
        assert_eq!(enc(0x0FFF_FFFF), [0xFF, 0xFF, 0xFF, 0x7F]);
    }

    #[test]
    fn smf_type1_has_track_per_channel() {
        let ev = [
            (0.0, MidiMsg::note_on(0, 60, 100)),
            (0.5, MidiMsg::note_off(0, 60)),
            (0.25, MidiMsg::note_on(9, 36, 100)),
            (0.3, MidiMsg::note_off(9, 36)),
        ];
        let bytes = smf_bytes(&ev, &opts(Quantize::Off));
        assert_eq!(&bytes[..4], b"MThd");
        assert_eq!(u16::from_be_bytes([bytes[8], bytes[9]]), 1, "формат 1");
        assert_eq!(u16::from_be_bytes([bytes[10], bytes[11]]), 3, "темп + 2 канала");
        // полсекунды при 120 BPM и 480 PPQ = 480 тиков = varlen [0x83, 0x60]
        let has = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
        assert!(has(&[0x83, 0x60, 0x80, 60, 0]));
        assert!(has("Ударные".as_bytes()));
        assert!(has(&[0xFF, 0x58, 4, 4, 2, 24, 8]), "размер 4/4");
        assert!(has(&[0xFF, 0x51, 3, 0x07, 0xA1, 0x20]), "темп 500000 мкс = 120 BPM");
    }

    #[test]
    fn quantize_snaps_start_and_keeps_length() {
        // 1/8 при 480 PPQ = 240 тиков; нота в 250 тиков длиной 100 → 240..340
        let mut ev = vec![(250, MidiMsg::note_on(0, 60, 100)), (350, MidiMsg::note_off(0, 60))];
        quantize_notes(&mut ev, 240);
        assert_eq!(ev, [(240, MidiMsg::note_on(0, 60, 100)), (340, MidiMsg::note_off(0, 60))]);
        // очень короткая нота растягивается до четверти шага
        let mut ev = vec![(470, MidiMsg::note_on(0, 62, 100)), (475, MidiMsg::note_off(0, 62))];
        quantize_notes(&mut ev, 240);
        assert_eq!(ev[0].0, 480);
        assert_eq!(ev[1].0, 540);
    }
}
