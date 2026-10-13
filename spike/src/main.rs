//! Этап 0: технический спайк.
//! Проверяем: SDL3 видит разные геймпады с единой раскладкой, rustysynth играет
//! через WASAPI, и какая получается задержка от нажатия до звука.
//!
//! Запуск:  cargo run --release -- [--buffer N]
//! Раскладка как в прототипе. Start — сводка по задержке, Ctrl+C — выход.

use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::RingBuffer;
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};
use sdl3::event::Event;
use sdl3::gamepad::{Axis, Button, Gamepad};

const BASE_NOTE: i32 = 60;
const SCALE: [i32; 8] = [0, 2, 4, 5, 7, 9, 11, 12];
const INSTRUMENTS: [(u8, &str); 8] = [
    (0, "Фортепиано"), (11, "Вибрафон"), (24, "Гитара"), (40, "Скрипка"),
    (56, "Труба"), (73, "Флейта"), (79, "Окарина"), (80, "Синт-лид"),
];
const DEADZONE: f32 = 0.15;
const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// MIDI-сообщение для аудиопотока. `t_ns` — момент события ввода по часам SDL (0 = не замерять).
#[derive(Clone, Copy)]
struct Msg {
    status: u8,
    d1: u8,
    d2: u8,
    t_ns: u64,
}

/// Замер одной ноты: от события SDL до начала синтеза, и от синтеза до выхода на ЦАП.
#[derive(Clone, Copy)]
struct Latency {
    to_synth_ms: f32,
    to_dac_ms: f32,
}

fn ticks_ns() -> u64 {
    unsafe { sdl3::sys::timer::SDL_GetTicksNS() }
}

fn note_name(n: i32) -> String {
    format!("{}{}", NOTE_NAMES[n as usize % 12], n / 12 - 1)
}

fn deadzone(v: f32) -> f32 {
    if v.abs() < DEADZONE { 0.0 } else { v.signum() * (v.abs() - DEADZONE) / (1.0 - DEADZONE) }
}

fn degree_of(button: Button) -> Option<usize> {
    Some(match button {
        Button::DPadLeft => 0,
        Button::DPadDown => 1,
        Button::DPadRight => 2,
        Button::DPadUp => 3,
        Button::West => 4,
        Button::South => 5,
        Button::East => 6,
        Button::North => 7,
        _ => return None,
    })
}

fn soundfont_path() -> PathBuf {
    let local = PathBuf::from("assets/GeneralUser-GS.sf2");
    if local.exists() {
        local
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/GeneralUser-GS.sf2")
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let buffer: u32 = args
        .iter()
        .position(|a| a == "--buffer")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(256);

    // ---------- MIDI-выходы (проверяем, что midir работает) ----------
    let midi = midir::MidiOutput::new("spike")?;
    println!("MIDI-выходы:");
    for p in midi.ports() {
        println!("  {}", midi.port_name(&p).unwrap_or_default());
    }

    // ---------- Звук ----------
    let t = std::time::Instant::now();
    let sf = Arc::new(SoundFont::new(&mut File::open(soundfont_path())?)?);
    println!("SoundFont загружен за {:.0} мс", t.elapsed().as_secs_f32() * 1000.0);

    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("нет аудиовыхода")?;
    let supported = device.default_output_config()?;
    if supported.sample_format() != cpal::SampleFormat::F32 {
        return Err(format!("спайк умеет только f32, а устройство отдаёт {:?}", supported.sample_format()).into());
    }
    let mut config = supported.config();
    if let cpal::SupportedBufferSize::Range { min, max } = supported.buffer_size() {
        println!("Допустимый буфер: {min}..{max} кадров");
        config.buffer_size = cpal::BufferSize::Fixed(buffer.clamp(*min, *max));
    }
    let rate = config.sample_rate;
    let channels = config.channels as usize;
    println!(
        "Аудио: {} ({:?}), {} Гц, {} кан., буфер {:?}",
        device.description().map(|d| d.name().to_string()).unwrap_or_default(),
        host.id(),
        rate,
        channels,
        config.buffer_size
    );

    let mut settings = SynthesizerSettings::new(rate as i32);
    settings.block_size = 64;
    let mut synth = Synthesizer::new(&sf, &settings)?;

    let (mut to_audio, mut from_main) = RingBuffer::<Msg>::new(1024);
    let (mut stats_tx, mut stats_rx) = RingBuffer::<Latency>::new(256);
    let mut left = vec![0f32; 4096];
    let mut right = vec![0f32; 4096];

    let stream = device.build_output_stream(
        config,
        move |out: &mut [f32], info: &cpal::OutputCallbackInfo| {
            let ts = info.timestamp();
            let to_dac = ts.playback.duration_since(ts.callback).as_secs_f32() * 1000.0;
            let now = ticks_ns();
            while let Ok(m) = from_main.pop() {
                synth.process_midi_message((m.status & 0x0F) as i32, (m.status & 0xF0) as i32, m.d1 as i32, m.d2 as i32);
                if m.t_ns != 0 {
                    let to_synth = now.saturating_sub(m.t_ns) as f32 / 1e6;
                    let _ = stats_tx.push(Latency { to_synth_ms: to_synth, to_dac_ms: to_dac });
                }
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
        |e| eprintln!("ошибка аудио: {e}"),
        None,
    )?;
    stream.play()?;

    let mut send = |status: u8, d1: u8, d2: u8, t_ns: u64| {
        let _ = to_audio.push(Msg { status, d1, d2, t_ns });
    };

    // ---------- Геймпады ----------
    sdl3::hint::set("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1");
    let sdl = sdl3::init()?;
    let gamepads = sdl.gamepad()?;
    let mut pump = sdl.event_pump()?;
    let mut open: HashMap<sdl3::joystick::JoystickId, Gamepad> = HashMap::new();

    let mut held: HashMap<Button, i32> = HashMap::new();
    let (mut oct_down, mut oct_up, mut sharp) = (false, false, false);
    let mut instr = 0usize;
    let (mut last_bend, mut last_mod, mut last_expr) = (8192i32, 0i32, -1i32);
    let mut samples: Vec<Latency> = Vec::new();

    send(0xC0, INSTRUMENTS[0].0, 0, 0);
    send(0xB0, 11, 40, 0);
    println!("\nЖду геймпад... (Start — сводка по задержке, Ctrl+C — выход)");

    'main: loop {
        let Some(first) = pump.wait_event_timeout(Duration::from_millis(10)) else {
            while let Ok(l) = stats_rx.pop() {
                println!("   задержка: ввод→синтез {:.1} мс + синтез→ЦАП {:.1} мс = {:.1} мс",
                    l.to_synth_ms, l.to_dac_ms, l.to_synth_ms + l.to_dac_ms);
                samples.push(l);
            }
            continue;
        };
        let events: Vec<Event> = std::iter::once(first).chain(pump.poll_iter()).collect();
        for event in events {
            match event {
                Event::Quit { .. } => break 'main,
                Event::GamepadAdded { which, .. } => {
                    if let Ok(pad) = gamepads.open(which) {
                        println!("+ подключён: {} [{:?}]", pad.name().unwrap_or_default(), pad.r#type());
                        open.insert(which, pad);
                    }
                }
                Event::GamepadRemoved { which, .. } => {
                    if let Some(pad) = open.remove(&which) {
                        println!("- отключён: {}", pad.name().unwrap_or_default());
                    }
                    for (_, n) in held.drain() {
                        send(0x80, n as u8, 0, 0);
                    }
                }
                Event::GamepadButtonDown { timestamp, button, .. } => {
                    if let Some(deg) = degree_of(button) {
                        if held.contains_key(&button) {
                            continue;
                        }
                        let shift = 12 * (oct_up as i32 - oct_down as i32) + sharp as i32;
                        let note = (BASE_NOTE + SCALE[deg] + shift).clamp(0, 127);
                        held.insert(button, note);
                        send(0x90, note as u8, 100, timestamp);
                        println!("♪ {}", note_name(note));
                    } else {
                        match button {
                            Button::LeftShoulder => oct_down = true,
                            Button::RightShoulder => oct_up = true,
                            Button::Back => {
                                for (_, n) in held.drain() {
                                    send(0x80, n as u8, 0, 0);
                                }
                                instr = (instr + 1) % INSTRUMENTS.len();
                                send(0xC0, INSTRUMENTS[instr].0, 0, 0);
                                println!("Инструмент: {}", INSTRUMENTS[instr].1);
                            }
                            Button::Start => print_summary(&samples),
                            _ => {}
                        }
                    }
                }
                Event::GamepadButtonUp { button, .. } => {
                    if let Some(n) = held.remove(&button) {
                        send(0x80, n as u8, 0, 0);
                    }
                    match button {
                        Button::LeftShoulder => oct_down = false,
                        Button::RightShoulder => oct_up = false,
                        _ => {}
                    }
                }
                Event::GamepadAxisMotion { axis, value, .. } => {
                    let v = value as f32 / 32767.0;
                    match axis {
                        Axis::TriggerLeft => sharp = v >= 0.5,
                        Axis::TriggerRight => {
                            let e = (40.0 + 87.0 * v.clamp(0.0, 1.0)) as i32;
                            if e != last_expr {
                                send(0xB0, 11, e as u8, 0);
                                last_expr = e;
                            }
                        }
                        Axis::RightX => {
                            let b = (8192 + (deadzone(v) * 8191.0) as i32).clamp(0, 16383);
                            if (b - last_bend).abs() >= 32 || (b == 8192 && last_bend != 8192) {
                                send(0xE0, (b & 0x7F) as u8, (b >> 7) as u8, 0);
                                last_bend = b;
                            }
                        }
                        Axis::RightY => {
                            let m = (deadzone(v).abs() * 127.0) as i32;
                            if (m - last_mod).abs() >= 2 || (m == 0 && last_mod != 0) {
                                send(0xB0, 1, m as u8, 0);
                                last_mod = m;
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    print_summary(&samples);
    Ok(())
}

fn print_summary(samples: &[Latency]) {
    if samples.is_empty() {
        println!("Замеров пока нет — понажимай ноты.");
        return;
    }
    let mut total: Vec<f32> = samples.iter().map(|l| l.to_synth_ms + l.to_dac_ms).collect();
    total.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let avg = total.iter().sum::<f32>() / total.len() as f32;
    println!(
        "== Задержка по {} нотам: мин {:.1} / средн {:.1} / макс {:.1} мс (без опроса самого геймпада) ==",
        total.len(), total[0], avg, total[total.len() - 1]
    );
}
