//! Встроенный синтезатор: SoundFont через rustysynth.
//!
//! Два вывода звука:
//! - эксклюзивный WASAPI (`audio_exclusive.rs`) — период ~3 мс, но устройство занято только нами;
//! - общий режим через cpal — уживается с другими программами, но Windows добавляет ~30 мс.
//!
//! MIDI передаётся в аудиопоток через lock-free очередь — аудиопоток никогда не ждёт.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer, RingBuffer};
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};
use serde::{Deserialize, Serialize};

use crate::midi::MidiMsg;

/// Желаемый размер буфера в общем режиме, кадров. Драйвер может дать больше.
const PREFERRED_BUFFER: u32 = 256;
/// Внутренний блок синтеза: чем меньше, тем точнее время нот внутри буфера.
const SYNTH_BLOCK: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioMode {
    /// Эксклюзивный режим: минимальная задержка, другие программы на устройстве молчат.
    #[default]
    Exclusive,
    /// Общий режим Windows: звучит вместе с другими программами, задержка больше.
    Shared,
}

#[derive(Clone, Debug, Serialize)]
pub struct PresetInfo {
    pub bank: u16,
    pub program: u8,
    pub name: String,
}

pub struct SoundBank {
    pub path: PathBuf,
    pub font: Arc<SoundFont>,
    pub presets: Vec<PresetInfo>,
}

impl SoundBank {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let mut file = std::fs::File::open(path).with_context(|| format!("не открыть {}", path.display()))?;
        let font = SoundFont::new(&mut file).map_err(|e| anyhow!("{}: {e:?}", path.display()))?;
        let mut presets: Vec<PresetInfo> = font
            .get_presets()
            .iter()
            .map(|p| PresetInfo {
                bank: p.get_bank_number() as u16,
                program: p.get_patch_number() as u8,
                name: p.get_name().trim().to_string(),
            })
            .collect();
        presets.sort_by_key(|p| (p.bank, p.program));
        Ok(Self { path: path.to_path_buf(), font: Arc::new(font), presets })
    }

    pub fn preset_name(&self, bank: u16, program: u8) -> Option<&str> {
        let find = |b| self.presets.iter().find(|p| p.bank == b && p.program == program);
        find(bank).or_else(|| find(0)).map(|p| p.name.as_str())
    }

    /// Первый найденный .sf2: явно указанный путь, затем папки по порядку.
    /// В сборке для разработки последним вариантом идёт `assets/` проекта.
    pub fn find(explicit: Option<&Path>, dirs: &[PathBuf]) -> Option<PathBuf> {
        if let Some(p) = explicit.filter(|p| p.is_file()) {
            return Some(p.to_path_buf());
        }
        let mut dirs = dirs.to_vec();
        if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
            dirs.push(exe_dir.join("sounds"));
        }
        if let Some(repo) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() {
            dirs.push(repo.join("assets"));
        }
        dirs.iter().find_map(|d| first_sf2(d))
    }
}

fn first_sf2(dir: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("sf2")))
        .collect();
    files.sort();
    files.into_iter().next()
}

/// Щелчок метронома: короткий затухающий тон, микшируется поверх синтезатора.
struct Click {
    pos: u32,
    len: u32,
    step: f32,
    amp: f32,
    decay: f32,
}

impl Click {
    fn new(level: u8, accent: bool, sample_rate: u32) -> Self {
        let rate = sample_rate as f32;
        let freq = if accent { 1600.0 } else { 1100.0 };
        Self {
            pos: 0,
            len: (0.04 * rate) as u32,
            step: std::f32::consts::TAU * freq / rate,
            amp: level as f32 / 127.0 * 0.5,
            decay: (-1.0 / (0.006 * rate)).exp(),
        }
    }
}

/// Синтезатор внутри аудиопотока: забирает MIDI из очереди и считает звук.
pub(crate) struct SynthCore {
    synth: Synthesizer,
    rx: Consumer<MidiMsg>,
    shared: Arc<AudioShared>,
    sample_rate: u32,
    clicks: Vec<Click>,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl SynthCore {
    pub(crate) fn new(
        font: &Arc<SoundFont>,
        sample_rate: u32,
        rx: Consumer<MidiMsg>,
        shared: Arc<AudioShared>,
    ) -> anyhow::Result<Self> {
        let mut settings = SynthesizerSettings::new(sample_rate as i32);
        settings.block_size = SYNTH_BLOCK;
        let synth = Synthesizer::new(font, &settings).map_err(|e| anyhow!("синтезатор: {e:?}"))?;
        Ok(Self { synth, rx, shared, sample_rate, clicks: Vec::with_capacity(4), left: vec![0.0; 4096], right: vec![0.0; 4096] })
    }

    /// Применить пришедший MIDI и посчитать `frames` кадров.
    pub(crate) fn render(&mut self, frames: usize) -> (&[f32], &[f32]) {
        while let Ok(m) = self.rx.pop() {
            let [status, d1, d2] = m.bytes;
            if status == crate::transport::CLICK_STATUS {
                self.clicks.push(Click::new(d1, d2 != 0, self.sample_rate));
                continue;
            }
            self.synth.process_midi_message((status & 0x0F) as i32, (status & 0xF0) as i32, d1 as i32, d2 as i32);
        }
        if self.left.len() < frames {
            self.left.resize(frames, 0.0);
            self.right.resize(frames, 0.0);
        }
        self.synth.render(&mut self.left[..frames], &mut self.right[..frames]);
        let gain = self.shared.gain();
        if gain != 1.0 {
            self.left[..frames].iter_mut().for_each(|s| *s *= gain);
            self.right[..frames].iter_mut().for_each(|s| *s *= gain);
        }
        for c in &mut self.clicks {
            let mut env = c.amp * c.decay.powi(c.pos as i32);
            for i in 0..frames {
                if c.pos >= c.len {
                    break;
                }
                let v = (c.pos as f32 * c.step).sin() * env;
                self.left[i] += v;
                self.right[i] += v;
                env *= c.decay;
                c.pos += 1;
            }
        }
        self.clicks.retain(|c| c.pos < c.len);
        (&self.left[..frames], &self.right[..frames])
    }
}

/// Состояние аудиопотока, которое видит движок.
pub(crate) struct AudioShared {
    pub failed: AtomicBool,
    /// Оценка задержки вывода (от синтеза до ЦАП), мкс.
    pub output_latency_us: AtomicU32,
    /// Общая громкость (биты f32).
    gain_bits: AtomicU32,
}

impl AudioShared {
    pub(crate) fn new(gain: f32) -> Self {
        Self { failed: AtomicBool::new(false), output_latency_us: AtomicU32::new(0), gain_bits: AtomicU32::new(gain.to_bits()) }
    }

    pub(crate) fn gain(&self) -> f32 {
        f32::from_bits(self.gain_bits.load(Ordering::Relaxed))
    }

    pub(crate) fn set_gain(&self, gain: f32) {
        self.gain_bits.store(gain.to_bits(), Ordering::Relaxed);
    }
}

/// Держит вывод живым; при удалении звук останавливается.
#[allow(dead_code)]
enum Backend {
    Shared(cpal::Stream),
    #[cfg(windows)]
    Exclusive(crate::audio_exclusive::ExclusiveOutput),
}

pub struct Synth {
    _backend: Backend,
    tx: Producer<MidiMsg>,
    shared: Arc<AudioShared>,
    pub mode: AudioMode,
    pub device: String,
    pub sample_rate: u32,
    /// Период вывода, кадров.
    pub period_frames: u32,
    /// Почему не получилось включить эксклюзивный режим (если откатились на общий).
    pub fallback: Option<String>,
}

impl Synth {
    /// Запускает вывод в режиме `mode`. Если эксклюзивный не удался — откатывается на общий.
    pub fn start(bank: &SoundBank, mode: AudioMode, gain: f32) -> anyhow::Result<Self> {
        #[cfg(windows)]
        if mode == AudioMode::Exclusive {
            match Self::start_exclusive(bank, gain) {
                Ok(s) => return Ok(s),
                Err(e) => {
                    let mut s = Self::start_shared(bank, gain)?;
                    s.fallback = Some(e.to_string());
                    return Ok(s);
                }
            }
        }
        Self::start_shared(bank, gain)
    }

    #[cfg(windows)]
    fn start_exclusive(bank: &SoundBank, gain: f32) -> anyhow::Result<Self> {
        let (tx, rx) = RingBuffer::<MidiMsg>::new(4096);
        let shared = Arc::new(AudioShared::new(gain));
        let (out, info) = crate::audio_exclusive::ExclusiveOutput::start(bank.font.clone(), rx, shared.clone())?;
        Ok(Self {
            _backend: Backend::Exclusive(out),
            tx,
            shared,
            mode: AudioMode::Exclusive,
            device: info.device,
            sample_rate: info.sample_rate,
            period_frames: info.period_frames,
            fallback: None,
        })
    }

    fn start_shared(bank: &SoundBank, gain: f32) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| anyhow!("нет устройства вывода звука"))?;
        let device_name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "Звук".into());

        let supported = device.default_output_config()?;
        let supported = if supported.sample_format() == cpal::SampleFormat::F32 {
            supported
        } else {
            device
                .supported_output_configs()?
                .filter(|c| c.sample_format() == cpal::SampleFormat::F32)
                .find_map(|c| c.try_with_sample_rate(supported.sample_rate()))
                .ok_or_else(|| anyhow!("устройство «{device_name}» не поддерживает float-вывод"))?
        };
        let mut config = supported.config();
        let mut period_frames = 0;
        if let cpal::SupportedBufferSize::Range { min, max } = supported.buffer_size() {
            period_frames = PREFERRED_BUFFER.clamp(*min, *max);
            config.buffer_size = cpal::BufferSize::Fixed(period_frames);
        }
        let channels = config.channels as usize;
        let sample_rate = config.sample_rate;

        let (tx, rx) = RingBuffer::<MidiMsg>::new(4096);
        let shared = Arc::new(AudioShared::new(gain));
        let mut core = SynthCore::new(&bank.font, sample_rate, rx, shared.clone())?;
        let cb_shared = shared.clone();
        let err_shared = shared.clone();

        let stream = device.build_output_stream(
            config,
            move |out: &mut [f32], info: &cpal::OutputCallbackInfo| {
                let frames = out.len() / channels;
                // Задержка вывода: от колбэка до ЦАП плюс сам буфер
                let ts = info.timestamp();
                let ahead = ts.playback.duration_since(ts.callback).as_secs_f64();
                let buffer = frames as f64 / sample_rate as f64;
                cb_shared.output_latency_us.store(((ahead + buffer) * 1e6) as u32, Ordering::Relaxed);

                let (left, right) = core.render(frames);
                for (i, frame) in out.chunks_mut(channels).enumerate() {
                    for (c, s) in frame.iter_mut().enumerate() {
                        *s = if c % 2 == 0 { left[i] } else { right[i] };
                    }
                }
            },
            move |_| err_shared.failed.store(true, Ordering::Relaxed),
            None,
        )?;
        stream.play()?;
        Ok(Self {
            _backend: Backend::Shared(stream),
            tx,
            shared,
            mode: AudioMode::Shared,
            device: device_name,
            sample_rate,
            period_frames,
            fallback: None,
        })
    }

    pub fn send(&mut self, msg: MidiMsg) {
        let _ = self.tx.push(msg);
    }

    /// Общая громкость: 1 — как есть.
    pub fn set_gain(&self, gain: f32) {
        self.shared.set_gain(gain);
    }

    /// Поток звука сломался (например, устройство отключили) — нужно пересоздать.
    pub fn failed(&self) -> bool {
        self.shared.failed.load(Ordering::Relaxed)
    }

    /// Оценка задержки от синтеза до ЦАП, мс.
    pub fn output_latency_ms(&self) -> Option<f32> {
        let us = self.shared.output_latency_us.load(Ordering::Relaxed);
        (us > 0).then_some(us as f32 / 1000.0)
    }

    /// Период вывода, мс. В среднем нота ждёт ближайшего аудиоблока половину периода.
    pub fn period_ms(&self) -> f32 {
        self.period_frames as f32 * 1000.0 / self.sample_rate.max(1) as f32
    }
}
