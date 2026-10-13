//! MIDI-сообщения и запись в стандартный .mid-файл.

use std::path::Path;
use std::time::Instant;

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
}

/// Сообщения, задающие диапазон подтяжки через RPN 0.
pub fn bend_range(ch: u8, semitones: u8) -> [MidiMsg; 4] {
    [MidiMsg::cc(ch, 101, 0), MidiMsg::cc(ch, 100, 0), MidiMsg::cc(ch, 6, semitones), MidiMsg::cc(ch, 38, 0)]
}

/// Выбор банка: CC0 (старший байт) и CC32 (младший).
pub fn bank_select(ch: u8, bank: u16) -> [MidiMsg; 2] {
    [MidiMsg::cc(ch, 0, (bank >> 7) as u8), MidiMsg::cc(ch, 32, (bank & 0x7F) as u8)]
}

/// Запись сыгранного: события с временем от начала.
pub struct Recorder {
    t0: Instant,
    events: Vec<(f64, MidiMsg)>,
}

impl Recorder {
    /// `initial` — состояние на момент старта (инструменты, громкость),
    /// чтобы файл звучал так же, как в программе.
    pub fn start(initial: impl IntoIterator<Item = MidiMsg>) -> Self {
        Self { t0: Instant::now(), events: initial.into_iter().map(|m| (0.0, m)).collect() }
    }

    pub fn add(&mut self, msg: MidiMsg) {
        self.events.push((self.t0.elapsed().as_secs_f64(), msg));
    }

    pub fn has_notes(&self) -> bool {
        self.events.iter().any(|(_, m)| m.status() == 0x90 && m.bytes[2] > 0)
    }

    /// Дописывает note-off для `sounding`, чтобы в файле не осталось зависших нот.
    pub fn finish(mut self, sounding: impl IntoIterator<Item = (u8, u8)>) -> Vec<(f64, MidiMsg)> {
        let t = self.t0.elapsed().as_secs_f64();
        for (ch, note) in sounding {
            self.events.push((t, MidiMsg::note_off(ch, note)));
        }
        self.events
    }
}

const PPQ: u16 = 480;
const BPM: u32 = 120;

fn varlen(mut n: u32, out: &mut Vec<u8>) {
    let mut buf = vec![(n & 0x7F) as u8];
    n >>= 7;
    while n > 0 {
        buf.push((n & 0x7F) as u8 | 0x80);
        n >>= 7;
    }
    out.extend(buf.iter().rev());
}

/// Стандартный MIDI-файл формата 0: одна дорожка, каналы внутри неё.
pub fn smf_bytes(events: &[(f64, MidiMsg)]) -> Vec<u8> {
    let ticks_per_sec = PPQ as f64 * BPM as f64 / 60.0;
    let mut track = Vec::new();
    varlen(0, &mut track);
    track.extend([0xFF, 0x51, 0x03]);
    track.extend(&(60_000_000 / BPM).to_be_bytes()[1..]);
    let mut last = 0u32;
    for (t, msg) in events {
        let tick = ((t * ticks_per_sec).round() as u32).max(last);
        varlen(tick - last, &mut track);
        track.extend(msg.as_slice());
        last = tick;
    }
    varlen(0, &mut track);
    track.extend([0xFF, 0x2F, 0x00]);

    let mut out = Vec::with_capacity(track.len() + 22);
    out.extend(b"MThd");
    out.extend(6u32.to_be_bytes());
    out.extend(0u16.to_be_bytes());
    out.extend(1u16.to_be_bytes());
    out.extend(PPQ.to_be_bytes());
    out.extend(b"MTrk");
    out.extend((track.len() as u32).to_be_bytes());
    out.extend(track);
    out
}

pub fn write_smf(path: &Path, events: &[(f64, MidiMsg)]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, smf_bytes(events))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn smf_has_header_and_note() {
        let bytes = smf_bytes(&[(0.0, MidiMsg::note_on(0, 60, 100)), (0.5, MidiMsg::note_off(0, 60))]);
        assert_eq!(&bytes[..4], b"MThd");
        assert_eq!(&bytes[14..18], b"MTrk");
        // полсекунды при 120 BPM и 480 PPQ = 480 тиков = varlen [0x83, 0x60]
        assert_eq!(&bytes[bytes.len() - 9..], [0x83, 0x60, 0x80, 60, 0, 0x00, 0xFF, 0x2F, 0x00]);
    }
}
