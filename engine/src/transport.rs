//! Время музыки: темп и такты (часы), метроном, отсчёт перед записью, лупер.
//! Всё принимает `now` явно — так логику можно проверить тестами без ожидания.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::midi::{MidiMsg, Quantize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TransportSettings {
    pub bpm: f32,
    /// Долей в такте (знаменатель — четверть).
    pub beats_per_bar: u8,
    pub metronome: bool,
    pub metronome_volume: f32,
    /// Такт отсчёта перед записью.
    pub count_in: bool,
    pub quantize: Quantize,
    /// Длина петли лупера в тактах.
    pub loop_bars: u8,
    /// Сохранять запись ещё и в WAV.
    pub export_wav: bool,
    /// Слать в MIDI-выход MIDI Clock и Start/Stop, пока идут часы (синхронизация DAW).
    pub send_clock: bool,
    /// Слать MMC (Record/Stop) при записи — для DAW, которые его понимают.
    pub send_mmc: bool,
}

impl Default for TransportSettings {
    fn default() -> Self {
        Self {
            bpm: 100.0,
            beats_per_bar: 4,
            metronome: false,
            metronome_volume: 0.6,
            count_in: true,
            quantize: Quantize::Off,
            loop_bars: 2,
            export_wav: false,
            send_clock: false,
            send_mmc: false,
        }
    }
}

impl TransportSettings {
    pub fn beat_len(&self) -> Duration {
        Duration::from_secs_f64(60.0 / self.bpm.clamp(20.0, 300.0) as f64)
    }

    pub fn bar_len(&self) -> Duration {
        self.beat_len() * self.beats_per_bar.max(1) as u32
    }
}

/// Часы: доля 0 приходится на `origin`. Остановлены — `origin` нет.
#[derive(Default)]
pub struct Clock {
    origin: Option<Instant>,
}

impl Clock {
    pub fn running(&self) -> bool {
        self.origin.is_some()
    }

    pub fn start(&mut self, now: Instant) {
        if self.origin.is_none() {
            self.origin = Some(now);
        }
    }

    pub fn stop(&mut self) {
        self.origin = None;
    }

    /// Позиция в долях (дробная).
    pub fn beats(&self, now: Instant, s: &TransportSettings) -> Option<f64> {
        let origin = self.origin?;
        Some(now.saturating_duration_since(origin).as_secs_f64() / s.beat_len().as_secs_f64())
    }

    /// Момент доли с номером `beat`.
    pub fn beat_time(&self, beat: u64, s: &TransportSettings) -> Option<Instant> {
        Some(self.origin? + s.beat_len().mul_f64(beat as f64))
    }

    /// Начало такта, в котором находится `now`.
    pub fn bar_start(&self, now: Instant, s: &TransportSettings) -> Option<Instant> {
        let bpb = s.beats_per_bar.max(1) as u64;
        let beat = self.beats(now, s)?.floor() as u64;
        self.beat_time(beat - beat % bpb, s)
    }

    /// Начало следующего такта.
    pub fn next_bar(&self, now: Instant, s: &TransportSettings) -> Option<Instant> {
        Some(self.bar_start(now, s)? + s.bar_len())
    }

    /// Сменить темп, не сдвигая текущую позицию.
    pub fn retempo(&mut self, now: Instant, old: &TransportSettings, new: &TransportSettings) {
        if let Some(pos) = self.beats(now, old) {
            self.origin = Some(now - new.beat_len().mul_f64(pos));
        }
    }
}

/// Щелчки метронома: какую долю щёлкнуть следующей.
#[derive(Default)]
pub struct Metronome {
    next_beat: u64,
}

impl Metronome {
    /// Сбросить так, чтобы следующим прозвучал первый удар не раньше `now`.
    pub fn sync(&mut self, clock: &Clock, now: Instant, s: &TransportSettings) {
        self.next_beat = clock.beats(now, s).map_or(0, |b| b.ceil() as u64);
    }

    /// Щелчки, которые пора сыграть: (акцент на первую долю такта). Опоздавшие больше чем
    /// на 30 мс пропускаются — лучше тишина, чем щелчок мимо доли.
    pub fn due(&mut self, clock: &Clock, now: Instant, s: &TransportSettings) -> Vec<bool> {
        let mut out = Vec::new();
        while let Some(t) = clock.beat_time(self.next_beat, s) {
            if t > now {
                break;
            }
            if now.duration_since(t) < Duration::from_millis(30) {
                out.push(self.next_beat.is_multiple_of(s.beats_per_bar.max(1) as u64));
            }
            self.next_beat += 1;
        }
        out
    }

    pub fn next_time(&self, clock: &Clock, s: &TransportSettings) -> Option<Instant> {
        clock.beat_time(self.next_beat, s)
    }
}

/// Внутреннее сообщение «щелчок метронома» для аудиопотока (в MIDI и запись не попадает).
pub fn click(accent: bool, volume: f32) -> MidiMsg {
    let level = (volume.clamp(0.0, 1.0) * if accent { 127.0 } else { 90.0 }) as u8;
    MidiMsg { bytes: [CLICK_STATUS, level, accent as u8], len: 3 }
}

pub const CLICK_STATUS: u8 = 0xFD;

// ---------- лупер ----------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopState {
    #[default]
    Empty,
    /// Ждёт начала такта, чтобы начать первую запись.
    Armed,
    /// Первая запись: задаёт петлю.
    Recording,
    Playing,
    /// Дописывает новый слой поверх.
    Overdub,
}

pub struct Looper {
    pub state: LoopState,
    /// Начало петли (цикл 0).
    start: Instant,
    length: Duration,
    /// Слои: события с позицией в секундах от начала петли.
    layers: Vec<Vec<(f64, MidiMsg)>>,
    /// Текущая запись: абсолютное время.
    take: Vec<(Instant, MidiMsg)>,
    /// До какого цикла включительно события уже отправлены в расписание.
    scheduled_cycle: i64,
}

impl Default for Looper {
    fn default() -> Self {
        Self {
            state: LoopState::Empty,
            start: Instant::now(),
            length: Duration::from_secs(1),
            layers: Vec::new(),
            take: Vec::new(),
            scheduled_cycle: 0,
        }
    }
}

/// Что лупер просит сделать после команды или по времени.
#[derive(Debug, Default, PartialEq)]
pub struct LoopEffects {
    /// Снять запланированные события петли и заглушить её ноты.
    pub silence: bool,
    /// События, которые нужно сыграть в указанное время.
    pub schedule: Vec<(Instant, MidiMsg)>,
}

impl Looper {
    pub fn layers(&self) -> usize {
        self.layers.len()
    }

    pub fn length(&self) -> Duration {
        self.length
    }

    /// Доля пройденного цикла 0..1.
    pub fn progress(&self, now: Instant) -> f32 {
        if matches!(self.state, LoopState::Empty | LoopState::Armed) || now < self.start {
            return 0.0;
        }
        let t = now.duration_since(self.start).as_secs_f64();
        ((t % self.length.as_secs_f64()) / self.length.as_secs_f64()) as f32
    }

    /// Кнопка «Луп»: начать петлю → дописать слой → закончить слой.
    /// `bar_start` — начало текущего такта, `next_bar` — следующего (часы уже идут).
    pub fn press(&mut self, now: Instant, bar_start: Instant, next_bar: Instant, length: Duration) -> LoopEffects {
        match self.state {
            LoopState::Empty => {
                self.length = length;
                self.take.clear();
                // Опоздал меньше чем на долю такта — считаем, что начал вовремя
                let late = now.duration_since(bar_start);
                if late < length / 16 {
                    self.start = bar_start;
                    self.state = LoopState::Recording;
                } else {
                    self.start = next_bar;
                    self.state = LoopState::Armed;
                }
                self.scheduled_cycle = 0;
            }
            LoopState::Armed => self.state = LoopState::Empty,
            LoopState::Recording => {
                // Досрочно: петля всё равно той длины, что задана
            }
            LoopState::Playing => {
                self.take.clear();
                self.state = LoopState::Overdub;
            }
            LoopState::Overdub => {
                self.commit(now);
                self.state = LoopState::Playing;
            }
        }
        LoopEffects::default()
    }

    /// Отменить: текущий дубль или последний слой.
    pub fn undo(&mut self, now: Instant) -> LoopEffects {
        match self.state {
            LoopState::Overdub => {
                self.take.clear();
                self.state = LoopState::Playing;
                LoopEffects::default()
            }
            LoopState::Playing => {
                self.layers.pop();
                if self.layers.is_empty() {
                    self.state = LoopState::Empty;
                    return LoopEffects { silence: true, schedule: Vec::new() };
                }
                // Снять запланированное и доиграть текущий цикл оставшимися слоями
                let cycle = (now.saturating_duration_since(self.start).as_secs_f64() / self.length.as_secs_f64()).floor();
                let cycle_start = self.start + self.length.mul_f64(cycle);
                let mut schedule = Vec::new();
                for layer in &self.layers {
                    for &(pos, msg) in layer {
                        let t = cycle_start + Duration::from_secs_f64(pos);
                        if t > now {
                            schedule.push((t, msg));
                        }
                    }
                }
                self.scheduled_cycle = cycle as i64;
                LoopEffects { silence: true, schedule }
            }
            LoopState::Armed | LoopState::Recording => {
                self.clear();
                LoopEffects { silence: true, schedule: Vec::new() }
            }
            LoopState::Empty => LoopEffects::default(),
        }
    }

    pub fn clear(&mut self) -> LoopEffects {
        let was = self.state != LoopState::Empty;
        self.state = LoopState::Empty;
        self.layers.clear();
        self.take.clear();
        LoopEffects { silence: was, schedule: Vec::new() }
    }

    /// Живое событие игрока: попадает в текущую запись.
    pub fn capture(&mut self, now: Instant, msg: MidiMsg) {
        if matches!(self.state, LoopState::Recording | LoopState::Overdub) && now >= self.start {
            self.take.push((now, msg));
        }
    }

    /// Ход времени: начать отложенную запись, закончить первую, запланировать следующий цикл.
    pub fn tick(&mut self, now: Instant, lookahead: Duration) -> LoopEffects {
        let mut fx = LoopEffects::default();
        if self.state == LoopState::Armed && now >= self.start {
            self.state = LoopState::Recording;
        }
        if self.state == LoopState::Recording && now >= self.start + self.length {
            self.commit(self.start + self.length);
            self.state = LoopState::Playing;
            self.scheduled_cycle = 0;
        }
        if matches!(self.state, LoopState::Playing | LoopState::Overdub) {
            // Пора запланировать следующий цикл?
            let next = self.scheduled_cycle + 1;
            let cycle_start = self.start + self.length * next as u32;
            if now + lookahead >= cycle_start {
                for layer in &self.layers {
                    for &(pos, msg) in layer {
                        fx.schedule.push((cycle_start + Duration::from_secs_f64(pos), msg));
                    }
                }
                self.scheduled_cycle = next;
            }
        }
        fx
    }

    /// Ближайший момент, когда луперу нужно внимание (для точного сна потока).
    pub fn next_deadline(&self, lookahead: Duration) -> Option<Instant> {
        match self.state {
            LoopState::Armed => Some(self.start),
            LoopState::Recording => Some(self.start + self.length),
            LoopState::Playing | LoopState::Overdub => {
                Some((self.start + self.length * (self.scheduled_cycle + 1) as u32).checked_sub(lookahead).unwrap_or(self.start))
            }
            LoopState::Empty => None,
        }
    }

    /// Превратить дубль в слой: позиции по модулю длины петли, без «осиротевших» note-off.
    fn commit(&mut self, end: Instant) {
        let len = self.length.as_secs_f64();
        let mut open: Vec<(u8, u8)> = Vec::new();
        let mut layer = Vec::new();
        for &(t, msg) in &self.take {
            if t > end {
                continue;
            }
            let key = (msg.channel(), msg.bytes[1]);
            if msg.is_note_on() {
                open.push(key);
            } else if msg.is_note_off() {
                let Some(i) = open.iter().position(|k| *k == key) else { continue };
                open.remove(i);
            }
            let pos = t.saturating_duration_since(self.start).as_secs_f64() % len;
            layer.push((pos, msg));
        }
        // Ноты, которые ещё держатся, закрываем в конце дубля
        let end_pos = (end.saturating_duration_since(self.start).as_secs_f64() % len).max(0.0);
        let end_pos = if end_pos == 0.0 { len - 0.001 } else { end_pos };
        for (ch, note) in open {
            layer.push((end_pos, MidiMsg::note_off(ch, note)));
        }
        self.take.clear();
        if layer.iter().any(|(_, m)| m.is_note_on()) {
            self.layers.push(layer);
        }
    }
}

/// Нажатия дальше этого друг от друга — новая серия.
const TAP_GAP: Duration = Duration::from_millis(2500);

/// Tap Tempo: запомнить нажатие и, если их уже три и больше, вернуть темп по средним промежуткам.
pub fn tap_tempo(taps: &mut Vec<Instant>, now: Instant) -> Option<f32> {
    if taps.last().is_some_and(|&t| now.duration_since(t) > TAP_GAP) {
        taps.clear();
    }
    taps.push(now);
    if taps.len() > 5 {
        taps.remove(0);
    }
    if taps.len() < 3 {
        return None;
    }
    let span = now.duration_since(taps[0]).as_secs_f32() / (taps.len() - 1) as f32;
    Some((60.0 / span).round().clamp(30.0, 300.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_tempo_averages_and_restarts() {
        let t0 = Instant::now();
        let mut taps = Vec::new();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        assert_eq!(tap_tempo(&mut taps, at(0)), None);
        assert_eq!(tap_tempo(&mut taps, at(500)), None);
        assert_eq!(tap_tempo(&mut taps, at(1000)), Some(120.0));
        assert_eq!(tap_tempo(&mut taps, at(1660)), Some(108.0));
        assert_eq!(tap_tempo(&mut taps, at(9000)), None, "долгая пауза — новая серия");
    }

    fn s() -> TransportSettings {
        TransportSettings { bpm: 120.0, beats_per_bar: 4, ..Default::default() }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn clock_bars() {
        let t0 = Instant::now();
        let mut c = Clock::default();
        c.start(t0);
        let st = s();
        assert_eq!(c.beats(t0 + ms(1250), &st).unwrap(), 2.5);
        assert_eq!(c.bar_start(t0 + ms(2100), &st).unwrap(), t0 + ms(2000));
        assert_eq!(c.next_bar(t0 + ms(2100), &st).unwrap(), t0 + ms(4000));
        let fast = TransportSettings { bpm: 240.0, ..st.clone() };
        c.retempo(t0 + ms(1000), &st, &fast);
        assert!((c.beats(t0 + ms(1000), &fast).unwrap() - 2.0).abs() < 1e-9, "позиция не сдвинулась");
    }

    #[test]
    fn metronome_accents_and_skips_late() {
        let t0 = Instant::now();
        let mut c = Clock::default();
        c.start(t0);
        let st = s();
        let mut m = Metronome::default();
        assert_eq!(m.due(&c, t0, &st), [true]);
        assert!(m.due(&c, t0 + ms(400), &st).is_empty());
        assert_eq!(m.due(&c, t0 + ms(510), &st), [false]);
        // поток проспал три доли — старые щелчки не играем
        assert_eq!(m.due(&c, t0 + ms(2010), &st), [true]);
        assert_eq!(m.next_time(&c, &st).unwrap(), t0 + ms(2500));
    }

    #[test]
    fn looper_records_plays_and_overdubs() {
        let t0 = Instant::now();
        let len = ms(2000);
        let mut l = Looper::default();
        l.press(t0, t0, t0 + len, len);
        assert_eq!(l.state, LoopState::Recording);
        l.capture(t0 + ms(100), MidiMsg::note_on(0, 60, 100));
        l.capture(t0 + ms(300), MidiMsg::note_off(0, 60));
        l.capture(t0 + ms(1900), MidiMsg::note_on(0, 64, 100)); // держится через границу
        let fx = l.tick(t0 + ms(1990), ms(20));
        assert!(fx.schedule.is_empty(), "петля ещё пишется");
        let fx = l.tick(t0 + ms(2000), ms(20));
        assert_eq!(l.state, LoopState::Playing);
        assert_eq!(l.layers(), 1);
        // запланирован цикл 1: нота 60 в 2.1 с, недозакрытая 64 закрыта в конце петли
        assert!(fx.schedule.contains(&(t0 + ms(2100), MidiMsg::note_on(0, 60, 100))));
        assert!(fx.schedule.iter().any(|(t, m)| *m == MidiMsg::note_off(0, 64) && *t > t0 + ms(3900)));
        // повторный тик в том же цикле ничего не дублирует
        assert!(l.tick(t0 + ms(2500), ms(20)).schedule.is_empty());

        // наложение
        l.press(t0 + ms(2500), t0 + ms(2000), t0 + ms(4000), len);
        assert_eq!(l.state, LoopState::Overdub);
        l.capture(t0 + ms(2600), MidiMsg::note_on(0, 67, 100));
        l.capture(t0 + ms(2700), MidiMsg::note_off(0, 67));
        l.press(t0 + ms(3000), t0 + ms(2000), t0 + ms(4000), len);
        assert_eq!(l.layers(), 2);
        let fx = l.tick(t0 + ms(3990), ms(20));
        assert!(fx.schedule.contains(&(t0 + ms(4600), MidiMsg::note_on(0, 67, 100))));

        // отмена слоя: оставшийся слой доигрывает текущий цикл
        let fx = l.undo(t0 + ms(4500));
        assert_eq!(l.layers(), 1);
        assert!(fx.silence);
        assert!(fx.schedule.iter().all(|(t, _)| *t > t0 + ms(4500)));
        assert!(fx.schedule.iter().any(|(_, m)| *m == MidiMsg::note_off(0, 64)), "закрытие ноты в конце цикла");
        assert!(l.clear().silence);
        assert_eq!(l.state, LoopState::Empty);
    }

    #[test]
    fn looper_arms_until_next_bar() {
        let t0 = Instant::now();
        let len = ms(2000);
        let mut l = Looper::default();
        l.press(t0 + ms(900), t0, t0 + ms(2000), len);
        assert_eq!(l.state, LoopState::Armed);
        l.capture(t0 + ms(1000), MidiMsg::note_on(0, 60, 100));
        l.tick(t0 + ms(2000), ms(20));
        assert_eq!(l.state, LoopState::Recording);
        assert_eq!(l.next_deadline(ms(20)), Some(t0 + ms(4000)));
    }
}
