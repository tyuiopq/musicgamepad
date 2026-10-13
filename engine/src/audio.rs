//! Встроенный синтезатор: SoundFont через rustysynth, вывод через WASAPI.
//! MIDI передаётся в аудиопоток через lock-free очередь — аудиопоток никогда не ждёт.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Producer, RingBuffer};
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};
use serde::Serialize;

use crate::midi::MidiMsg;

/// Желаемый размер аудиобуфера в кадрах. Драйвер может дать больше.
const PREFERRED_BUFFER: u32 = 256;
/// Внутренний блок синтеза: чем меньше, тем точнее время нот внутри буфера.
const SYNTH_BLOCK: usize = 64;

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

pub struct Synth {
    _stream: cpal::Stream,
    tx: Producer<MidiMsg>,
    failed: Arc<AtomicBool>,
    pub device: String,
    pub buffer_frames: Option<u32>,
    pub sample_rate: u32,
}

impl Synth {
    pub fn start(bank: &SoundBank) -> anyhow::Result<Self> {
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
        let mut buffer_frames = None;
        if let cpal::SupportedBufferSize::Range { min, max } = supported.buffer_size() {
            let frames = PREFERRED_BUFFER.clamp(*min, *max);
            config.buffer_size = cpal::BufferSize::Fixed(frames);
            buffer_frames = Some(frames);
        }
        let channels = config.channels as usize;
        let sample_rate = config.sample_rate;

        let mut settings = SynthesizerSettings::new(sample_rate as i32);
        settings.block_size = SYNTH_BLOCK;
        let mut synth = Synthesizer::new(&bank.font, &settings).map_err(|e| anyhow!("синтезатор: {e:?}"))?;

        let (tx, mut rx) = RingBuffer::<MidiMsg>::new(4096);
        let failed = Arc::new(AtomicBool::new(false));
        let failed_cb = failed.clone();
        let mut left = vec![0f32; 4096];
        let mut right = vec![0f32; 4096];

        let stream = device.build_output_stream(
            config,
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                while let Ok(m) = rx.pop() {
                    let [status, d1, d2] = m.bytes;
                    synth.process_midi_message((status & 0x0F) as i32, (status & 0xF0) as i32, d1 as i32, d2 as i32);
                }
                let frames = out.len() / channels;
                if left.len() < frames {
                    left.resize(frames, 0.0);
                    right.resize(frames, 0.0);
                }
                synth.render(&mut left[..frames], &mut right[..frames]);
                for (i, frame) in out.chunks_mut(channels).enumerate() {
                    for (c, s) in frame.iter_mut().enumerate() {
                        *s = if c % 2 == 0 { left[i] } else { right[i] };
                    }
                }
            },
            move |_| failed_cb.store(true, Ordering::Relaxed),
            None,
        )?;
        stream.play()?;
        Ok(Self { _stream: stream, tx, failed, device: device_name, buffer_frames, sample_rate })
    }

    pub fn send(&mut self, msg: MidiMsg) {
        let _ = self.tx.push(msg);
    }

    /// Поток звука сломался (например, устройство отключили) — нужно пересоздать.
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }
}
