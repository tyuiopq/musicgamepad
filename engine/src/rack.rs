//! «Стойка» инструментов: основной синтезатор, дополнительные (свои .sf2 у партий) и сэмплер.
//! Каждый MIDI-канал направляется в свой инструмент. Одна и та же стойка играет вживую
//! (в аудиопотоке) и при экспорте в WAV — поэтому запись звучит так же, как игра.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context};
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

use crate::midi::MidiMsg;

/// Внутренний блок синтеза: чем меньше, тем точнее время нот внутри буфера.
const SYNTH_BLOCK: usize = 64;
/// Сколько звуков сэмплера одновременно (старые глушатся).
const MAX_VOICES: usize = 64;
/// Быстрое затухание при глушении, секунды — без щелчка.
const CHOKE_FADE_S: f32 = 0.006;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Main,
    Font(usize),
    Sampler,
}

pub fn new_synth(font: &Arc<SoundFont>, sample_rate: u32) -> anyhow::Result<Synthesizer> {
    let mut settings = SynthesizerSettings::new(sample_rate as i32);
    settings.block_size = SYNTH_BLOCK;
    Synthesizer::new(font, &settings).map_err(|e| anyhow!("синтезатор: {e:?}"))
}

// ---------- сэмплы ----------

/// Звук из файла: стерео, f32.
pub struct SampleData {
    pub rate: u32,
    pub frames: Vec<[f32; 2]>,
}

impl SampleData {
    /// WAV (PCM 8/16/24/32 бит или float), моно или стерео.
    pub fn load_wav(path: &Path) -> anyhow::Result<Self> {
        let mut reader = hound::WavReader::open(path).with_context(|| format!("не открыть {}", path.display()))?;
        let spec = reader.spec();
        let ch = spec.channels.max(1) as usize;
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
            hound::SampleFormat::Int => {
                let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
                reader.samples::<i32>().map(|s| s.map(|v| v as f32 * scale)).collect::<Result<_, _>>()?
            }
        };
        let frames = samples
            .chunks(ch)
            .map(|f| if ch == 1 { [f[0], f[0]] } else { [f[0], f[1]] })
            .collect();
        Ok(Self { rate: spec.sample_rate, frames })
    }
}

/// Пэд сэмплера: что играет нота.
#[derive(Clone)]
pub struct Pad {
    pub data: Arc<SampleData>,
    /// Громкость, линейная.
    pub gain: f32,
    /// Сдвиг высоты, полутоны.
    pub pitch: f32,
    /// Группа глушения: новый звук группы глушит прежние (0 — без группы).
    pub choke: u8,
}

struct Voice {
    pad: Pad,
    channel: u8,
    pos: f64,
    step: f64,
    gain: f32,
    /// Оставшееся затухание (1 → 0), если звук глушат.
    fade: Option<f32>,
}

#[derive(Clone, Copy)]
struct ChannelState {
    volume: f32,
    expression: f32,
    pan: f32,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self { volume: 100.0 / 127.0, expression: 1.0, pan: 0.5 }
    }
}

/// One-shot сэмплер: нота запускает звук целиком, Note Off его не обрывает.
pub struct Sampler {
    rate: u32,
    pads: HashMap<(u8, u8), Pad>,
    voices: Vec<Voice>,
    channels: [ChannelState; 16],
}

impl Sampler {
    pub fn new(rate: u32, pads: HashMap<(u8, u8), Pad>) -> Self {
        Self { rate, pads, voices: Vec::with_capacity(MAX_VOICES), channels: [ChannelState::default(); 16] }
    }

    pub fn process(&mut self, msg: MidiMsg) {
        let ch = msg.channel();
        match msg.status() {
            0x90 if msg.bytes[2] > 0 => self.note_on(ch, msg.bytes[1], msg.bytes[2]),
            0xB0 => {
                let v = msg.bytes[2] as f32 / 127.0;
                let state = &mut self.channels[ch as usize];
                match msg.bytes[1] {
                    7 => state.volume = v,
                    11 => state.expression = v,
                    10 => state.pan = v,
                    120 | 123 => self.fade_where(|voice| voice.channel == ch),
                    121 => *state = ChannelState::default(),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn note_on(&mut self, ch: u8, note: u8, vel: u8) {
        let Some(pad) = self.pads.get(&(ch, note)).cloned() else { return };
        if pad.choke != 0 {
            self.fade_where(|v| v.channel == ch && v.pad.choke == pad.choke);
        }
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0);
        }
        let step = pad.data.rate as f64 / self.rate as f64 * 2f64.powf(pad.pitch as f64 / 12.0);
        let gain = pad.gain * vel as f32 / 127.0;
        self.voices.push(Voice { pad, channel: ch, pos: 0.0, step, gain, fade: None });
    }

    fn fade_where(&mut self, pick: impl Fn(&Voice) -> bool) {
        for v in self.voices.iter_mut().filter(|v| pick(v)) {
            v.fade.get_or_insert(1.0);
        }
    }

    /// Добавить звук к `left`/`right`.
    pub fn render_add(&mut self, left: &mut [f32], right: &mut [f32]) {
        let fade_step = 1.0 / (CHOKE_FADE_S * self.rate as f32);
        for v in &mut self.voices {
            let ch = self.channels[v.channel as usize];
            let level = v.gain * ch.volume * ch.expression;
            let (pl, pr) = ((1.0 - ch.pan).min(0.5) * 2.0, ch.pan.min(0.5) * 2.0);
            let frames = &v.pad.data.frames;
            for i in 0..left.len() {
                let idx = v.pos as usize;
                if idx + 1 >= frames.len() {
                    v.pos = frames.len() as f64;
                    break;
                }
                let t = (v.pos - idx as f64) as f32;
                let a = frames[idx];
                let b = frames[idx + 1];
                let mut g = level;
                if let Some(f) = &mut v.fade {
                    g *= *f;
                    *f -= fade_step;
                    if *f <= 0.0 {
                        v.pos = frames.len() as f64;
                        break;
                    }
                }
                left[i] += (a[0] + (b[0] - a[0]) * t) * g * pl;
                right[i] += (a[1] + (b[1] - a[1]) * t) * g * pr;
                v.pos += v.step;
            }
        }
        self.voices.retain(|v| (v.pos as usize) + 1 < v.pad.data.frames.len());
    }
}

// ---------- стойка ----------

/// Что поменять в стойке: маршруты каналов, дополнительные синтезаторы, пэды сэмплера.
/// Собирается в потоке движка (загрузка файлов), применяется в аудиопотоке мгновенно.
pub struct RackConfig {
    pub routes: [Route; 16],
    pub extra: Vec<Synthesizer>,
    pub pads: HashMap<(u8, u8), Pad>,
}

pub struct Rack {
    rate: u32,
    main: Synthesizer,
    extra: Vec<Synthesizer>,
    sampler: Sampler,
    routes: [Route; 16],
    l: Vec<f32>,
    r: Vec<f32>,
}

impl Rack {
    pub fn new(main: Synthesizer, rate: u32) -> Self {
        Self {
            rate,
            main,
            extra: Vec::new(),
            sampler: Sampler::new(rate, HashMap::new()),
            routes: [Route::Main; 16],
            l: vec![0.0; 4096],
            r: vec![0.0; 4096],
        }
    }

    /// Основной синтезатор и его состояние остаются; остальное заменяется.
    pub fn apply(&mut self, cfg: RackConfig) {
        self.routes = cfg.routes;
        self.extra = cfg.extra;
        self.sampler = Sampler::new(self.rate, cfg.pads);
    }

    pub fn process(&mut self, msg: MidiMsg) {
        let [status, d1, d2] = msg.bytes;
        let (ch, cmd) = ((status & 0x0F) as i32, (status & 0xF0) as i32);
        match self.routes[(status & 0x0F) as usize] {
            Route::Main => self.main.process_midi_message(ch, cmd, d1 as i32, d2 as i32),
            Route::Font(i) => {
                if let Some(s) = self.extra.get_mut(i) {
                    s.process_midi_message(ch, cmd, d1 as i32, d2 as i32);
                }
            }
            Route::Sampler => self.sampler.process(msg),
        }
    }

    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        self.main.render(&mut left[..n], &mut right[..n]);
        if self.l.len() < n {
            self.l.resize(n, 0.0);
            self.r.resize(n, 0.0);
        }
        for s in &mut self.extra {
            s.render(&mut self.l[..n], &mut self.r[..n]);
            for i in 0..n {
                left[i] += self.l[i];
                right[i] += self.r[i];
            }
        }
        self.sampler.render_add(&mut left[..n], &mut right[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(len: usize) -> Arc<SampleData> {
        Arc::new(SampleData { rate: 48_000, frames: (0..len).map(|i| [((i % 10) as f32 / 10.0), 0.5]).collect() })
    }

    fn sampler(choke: u8) -> Sampler {
        let mut pads = HashMap::new();
        pads.insert((9, 42), Pad { data: tone(48_000), gain: 1.0, pitch: 0.0, choke });
        pads.insert((9, 46), Pad { data: tone(48_000), gain: 1.0, pitch: 0.0, choke });
        pads.insert((9, 36), Pad { data: tone(100), gain: 1.0, pitch: 12.0, choke: 0 });
        Sampler::new(48_000, pads)
    }

    fn render(s: &mut Sampler, n: usize) -> f32 {
        let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
        s.render_add(&mut l, &mut r);
        r.iter().map(|x| x.abs()).sum::<f32>()
    }

    #[test]
    fn one_shot_ignores_note_off_and_ends_itself() {
        let mut s = sampler(0);
        s.process(MidiMsg::note_on(9, 36, 127));
        s.process(MidiMsg::note_off(9, 36));
        assert!(render(&mut s, 10) > 0.0, "Note Off не обрывает");
        // 100 кадров на октаву выше — кончится за ~50
        render(&mut s, 100);
        assert!(s.voices.is_empty());
    }

    #[test]
    fn choke_group_fades_previous_sound() {
        let mut s = sampler(1);
        s.process(MidiMsg::note_on(9, 42, 127));
        render(&mut s, 100);
        s.process(MidiMsg::note_on(9, 46, 127));
        render(&mut s, 1000); // > 6 мс затухания
        assert_eq!(s.voices.len(), 1, "закрытый хэт заглушён открытым");
        let mut free = sampler(0);
        free.process(MidiMsg::note_on(9, 42, 127));
        free.process(MidiMsg::note_on(9, 46, 127));
        render(&mut free, 1000);
        assert_eq!(free.voices.len(), 2, "без группы звучат оба");
    }

    #[test]
    fn velocity_and_channel_volume_scale_output() {
        let mut loud = sampler(0);
        loud.process(MidiMsg::note_on(9, 42, 127));
        let a = render(&mut loud, 100);
        let mut soft = sampler(0);
        soft.process(MidiMsg::cc(9, 7, 50));
        soft.process(MidiMsg::note_on(9, 42, 64));
        let b = render(&mut soft, 100);
        assert!(b < a * 0.3, "{b} < {a}·0.3");
        assert_eq!(render(&mut sampler(0), 100), 0.0, "незанятая нота — тишина");
    }
}
