//! Что умеет звуковая карта по задержке: периоды в общем, low-latency и эксклюзивном режимах.
//! cargo run -p engine --example latency_probe --release
//! Только Windows: замеряет WASAPI.

#[cfg(windows)]
mod probe {
    use std::time::Duration;

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use wasapi::*;

    fn ms(hns: i64) -> f64 {
        hns as f64 / 10_000.0
    }

    pub fn main() {
        initialize_mta().ok().unwrap();
        let device = DeviceEnumerator::new().unwrap().get_default_device(&Direction::Render).unwrap();
        println!("Устройство: {}", device.get_friendlyname().unwrap_or_default());
        let client = device.get_iaudioclient().unwrap();
        let mix = client.get_mixformat().unwrap();
        let rate = mix.get_samplespersec() as f64;
        println!("Микшер: {} Гц, {} кан., {} бит", mix.get_samplespersec(), mix.get_nchannels(), mix.get_bitspersample());

        let (def, min) = client.get_device_period().unwrap();
        println!("\nПериод устройства: обычный {:.2} мс, минимальный (эксклюзив) {:.2} мс", ms(def), ms(min));

        match client.get_shared_mode_engine_period(&mix) {
            Ok(p) => println!(
                "Общий режим, IAudioClient3: по умолчанию {:.2} мс, минимум {:.2} мс, шаг {:.2} мс ({}–{} кадров)",
                p.default as f64 * 1000.0 / rate,
                p.min as f64 * 1000.0 / rate,
                p.fundamental as f64 * 1000.0 / rate,
                p.min,
                p.max
            ),
            Err(e) => println!("IAudioClient3 недоступен: {e}"),
        }

        // Пробуем эксклюзивный режим с минимальным периодом в нескольких форматах
        println!("\nЭксклюзивный режим:");
        for (bits, valid, kind) in [(32, 32, SampleType::Float), (32, 24, SampleType::Int), (24, 24, SampleType::Int), (16, 16, SampleType::Int)] {
            let want = WaveFormat::new(bits, valid, &kind, mix.get_samplespersec() as usize, 2, None);
            let mut c = device.get_iaudioclient().unwrap();
            let fmt = match c.is_supported_exclusive_with_quirks(&want) {
                Ok(f) => f,
                Err(_) => {
                    println!("  {kind:?}{valid}/{bits}: формат не поддерживается");
                    continue;
                }
            };
            let period = c.calculate_aligned_period_near(min, Some(128), &fmt).unwrap_or(min);
            let r = c.initialize_client(&fmt, &Direction::Render, &StreamMode::EventsExclusive { period_hns: period });
            match r {
                Ok(()) => println!(
                    "  {kind:?}{valid}/{bits}: OK, период {:.2} мс, буфер {} кадров",
                    ms(period),
                    c.get_buffer_size().unwrap_or(0)
                ),
                Err(e) => println!("  {kind:?}{valid}/{bits}: не удалось — {e}"),
            }
        }

        // Как сейчас: cpal, общий режим, оценка «от колбэка до ЦАП»
        let host = cpal::default_host();
        let dev = host.default_output_device().unwrap();
        let mut cfg = dev.default_output_config().unwrap().config();
        cfg.buffer_size = cpal::BufferSize::Fixed(480);
        let (tx, rx) = std::sync::mpsc::channel::<f64>();
        let stream = dev
            .build_output_stream(
                cfg,
                move |out: &mut [f32], info: &cpal::OutputCallbackInfo| {
                    out.fill(0.0);
                    let t = info.timestamp();
                    let _ = tx.send(t.playback.duration_since(t.callback).as_secs_f64() * 1000.0);
                },
                |_| {},
                None,
            )
            .unwrap();
        stream.play().unwrap();
        std::thread::sleep(Duration::from_millis(500));
        let v: Vec<f64> = rx.try_iter().skip(5).collect();
        let avg = v.iter().sum::<f64>() / v.len().max(1) as f64;
        println!("\nСейчас (cpal, общий режим): от колбэка до ЦАП ≈ {avg:.1} мс, колбэков за 0,5 с: {}", v.len() + 5);
    }
}

fn main() {
    #[cfg(windows)]
    probe::main();
    #[cfg(not(windows))]
    eprintln!("latency_probe работает только на Windows");
}
