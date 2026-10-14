//! Эксклюзивный вывод WASAPI: свой аудиопоток, период ~3 мс.
//! Устройство на это время принадлежит только нам — другие программы на нём не звучат.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;

use anyhow::anyhow;
use rtrb::Consumer;
use rustysynth::SoundFont;
use wasapi::{
    calculate_period_100ns, initialize_mta, AudioClient, AudioRenderClient, DeviceEnumerator, Direction, Handle,
    SampleType, StreamMode, WasapiError, WaveFormat,
};
use windows::Win32::Media::Audio::{
    AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED, AUDCLNT_E_DEVICE_IN_USE, AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED,
};

use crate::audio::{AudioShared, SynthCore};
use crate::midi::MidiMsg;

/// Сколько ждать события от устройства, прежде чем счесть его пропавшим.
const EVENT_TIMEOUT_MS: u32 = 500;

pub(crate) struct Info {
    pub device: String,
    pub sample_rate: u32,
    pub period_frames: u32,
}

/// Формат сэмплов, который принял драйвер.
#[derive(Clone, Copy)]
enum Sample {
    F32,
    I24In32,
    I24,
    I16,
}

pub(crate) struct ExclusiveOutput {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ExclusiveOutput {
    /// Открывает устройство в отдельном потоке и ждёт, получилось ли.
    pub(crate) fn start(
        font: Arc<SoundFont>,
        rx: Consumer<MidiMsg>,
        shared: Arc<AudioShared>,
    ) -> anyhow::Result<(Self, Info)> {
        let stop = Arc::new(AtomicBool::new(false));
        let (init_tx, init_rx) = mpsc::sync_channel(1);
        let thread_stop = stop.clone();
        let thread = std::thread::Builder::new()
            .name("audio".into())
            .spawn(move || run(font, rx, shared, thread_stop, init_tx))?;
        match init_rx.recv() {
            Ok(Ok(info)) => Ok((Self { stop, thread: Some(thread) }, info)),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err(anyhow!("аудиопоток завершился при запуске")),
        }
    }
}

impl Drop for ExclusiveOutput {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Stream {
    client: AudioClient,
    render: AudioRenderClient,
    event: Handle,
    sample: Sample,
    channels: usize,
}

fn run(
    font: Arc<SoundFont>,
    rx: Consumer<MidiMsg>,
    shared: Arc<AudioShared>,
    stop: Arc<AtomicBool>,
    init_tx: mpsc::SyncSender<anyhow::Result<Info>>,
) {
    let (stream, info) = match open() {
        Ok(v) => v,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            return;
        }
    };
    let mut core = match SynthCore::new(&font, info.sample_rate, rx, shared.clone()) {
        Ok(c) => c,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            return;
        }
    };
    raise_priority();

    let mut bytes = Vec::new();
    // Буфер заполняется тишиной до старта, иначе первый период щёлкает
    let prefill = || -> anyhow::Result<()> {
        let frames = stream.client.get_available_space_in_frames()? as usize;
        let silence = vec![0u8; frames * stream.channels * sample_bytes(stream.sample)];
        stream.render.write_to_device(frames, &silence, None)?;
        stream.client.start_stream()?;
        Ok(())
    };
    if let Err(e) = prefill() {
        let _ = init_tx.send(Err(e));
        return;
    }
    // Один период играет, второй готовится: задержка вывода ≈ два периода
    let period_s = info.period_frames as f64 / info.sample_rate as f64;
    shared.output_latency_us.store((2.0 * period_s * 1e6) as u32, Ordering::Relaxed);
    let _ = init_tx.send(Ok(info));

    while !stop.load(Ordering::Relaxed) {
        let mut step = || -> anyhow::Result<()> {
            stream.event.wait_for_event(EVENT_TIMEOUT_MS)?;
            let frames = stream.client.get_available_space_in_frames()? as usize;
            if frames == 0 {
                return Ok(());
            }
            let (left, right) = core.render(frames);
            encode(left, right, stream.channels, stream.sample, &mut bytes);
            stream.render.write_to_device(frames, &bytes, None)?;
            Ok(())
        };
        if step().is_err() {
            shared.failed.store(true, Ordering::Relaxed);
            break;
        }
    }
    let _ = stream.client.stop_stream();
}

fn open() -> anyhow::Result<(Stream, Info)> {
    let _ = initialize_mta();
    let device = DeviceEnumerator::new()?.get_default_device(&Direction::Render)?;
    let name = device.get_friendlyname().unwrap_or_else(|_| "Звук".into());
    let mut client = device.get_iaudioclient()?;
    let rate = client.get_mixformat()?.get_samplespersec() as usize;

    // Форматы по предпочтению; драйвер может поправить запрошенный
    let candidates = [
        (32, 24, SampleType::Int),
        (32, 32, SampleType::Float),
        (24, 24, SampleType::Int),
        (16, 16, SampleType::Int),
    ];
    let format = candidates
        .iter()
        .find_map(|(store, valid, kind)| {
            client.is_supported_exclusive_with_quirks(&WaveFormat::new(*store, *valid, kind, rate, 2, None)).ok()
        })
        .ok_or_else(|| anyhow!("устройство «{name}» не поддерживает эксклюзивный режим"))?;
    let sample = match (format.get_subformat()?, format.get_bitspersample(), format.get_validbitspersample()) {
        (SampleType::Float, 32, _) => Sample::F32,
        (SampleType::Int, 32, _) => Sample::I24In32,
        (SampleType::Int, 24, _) => Sample::I24,
        (SampleType::Int, 16, _) => Sample::I16,
        (kind, bits, _) => return Err(anyhow!("неожиданный формат устройства: {kind:?} {bits} бит")),
    };
    let channels = format.get_nchannels() as usize;

    let (_, min_period) = client.get_device_period()?;
    let period = client.calculate_aligned_period_near(min_period, Some(128), &format).unwrap_or(min_period);
    match client.initialize_client(&format, &Direction::Render, &StreamMode::EventsExclusive { period_hns: period }) {
        Ok(()) => {}
        // Драйвер хочет другой размер буфера: берём предложенный и пробуем ещё раз (так велит документация)
        Err(WasapiError::Windows(e)) if e.code() == AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
            let frames = client.get_buffer_size()?;
            let aligned = calculate_period_100ns(frames as i64, rate as i64);
            client = device.get_iaudioclient()?;
            client.initialize_client(&format, &Direction::Render, &StreamMode::EventsExclusive { period_hns: aligned })?;
        }
        Err(WasapiError::Windows(e)) if e.code() == AUDCLNT_E_DEVICE_IN_USE => {
            return Err(anyhow!("устройство «{name}» занято другой программой"));
        }
        Err(WasapiError::Windows(e)) if e.code() == AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED => {
            return Err(anyhow!("эксклюзивный режим запрещён в свойствах устройства «{name}»"));
        }
        Err(e) => return Err(e.into()),
    }
    let event = client.set_get_eventhandle()?;
    let render = client.get_audiorenderclient()?;
    let period_frames = client.get_buffer_size()?;
    let info = Info { device: name, sample_rate: rate as u32, period_frames };
    Ok((Stream { client, render, event, sample, channels }, info))
}

/// Просим Windows планировать поток как профессиональное аудио (MMCSS).
fn raise_priority() {
    use windows::core::w;
    use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
    let mut index = 0u32;
    // Если не вышло — работаем с обычным приоритетом
    let _ = unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut index) };
}

fn sample_bytes(s: Sample) -> usize {
    match s {
        Sample::F32 | Sample::I24In32 => 4,
        Sample::I24 => 3,
        Sample::I16 => 2,
    }
}

/// Стерео f32 → байты формата устройства; лишние каналы получают левый/правый по очереди.
fn encode(left: &[f32], right: &[f32], channels: usize, sample: Sample, out: &mut Vec<u8>) {
    out.clear();
    for i in 0..left.len() {
        for c in 0..channels {
            let v = if c % 2 == 0 { left[i] } else { right[i] }.clamp(-1.0, 1.0);
            match sample {
                Sample::F32 => out.extend_from_slice(&v.to_le_bytes()),
                Sample::I24In32 => out.extend_from_slice(&(((v * 8_388_607.0) as i32) << 8).to_le_bytes()),
                Sample::I24 => out.extend_from_slice(&((v * 8_388_607.0) as i32).to_le_bytes()[..3]),
                Sample::I16 => out.extend_from_slice(&((v * 32_767.0) as i16).to_le_bytes()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_full_scale() {
        let mut out = Vec::new();
        encode(&[1.0], &[-1.0], 2, Sample::I16, &mut out);
        assert_eq!(out, [0xFF, 0x7F, 0x01, 0x80]);
        encode(&[1.0], &[0.0], 2, Sample::I24In32, &mut out);
        assert_eq!(&out[..4], &[0x00, 0xFF, 0xFF, 0x7F]);
        encode(&[0.5], &[0.5], 2, Sample::I24, &mut out);
        assert_eq!(out.len(), 6);
    }
}
