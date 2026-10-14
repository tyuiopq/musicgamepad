//! Насколько точно поток движка попадает в доли: имитирует цикл движка
//! (ожидание событий SDL с таймаутом до следующей доли) и меряет опоздание щелчков.
//! cargo run -p engine --example timing_check --release

use std::time::{Duration, Instant};

use engine::transport::{Clock, Metronome, TransportSettings};

fn main() {
    #[cfg(windows)]
    unsafe {
        windows::Win32::Media::timeBeginPeriod(1);
    }
    let sdl = sdl3::init().unwrap();
    let _gp = sdl.gamepad().unwrap();
    let mut pump = sdl.event_pump().unwrap();

    let ts = TransportSettings { bpm: 240.0, metronome: true, ..Default::default() };
    let mut clock = Clock::default();
    let start = Instant::now();
    clock.start(start);
    let mut m = Metronome::default();
    m.sync(&clock, start, &ts);

    let mut late = Vec::new();
    let mut beat = 0u64;
    while beat < 40 {
        let now = Instant::now();
        let next = m.next_time(&clock, &ts).unwrap();
        let wait = next.saturating_duration_since(now).min(Duration::from_millis(8));
        let _ = pump.wait_event_timeout(wait);
        let now = Instant::now();
        for _ in m.due(&clock, now, &ts) {
            let ideal = clock.beat_time(beat, &ts).unwrap();
            late.push(now.duration_since(ideal).as_secs_f64() * 1000.0);
            beat += 1;
        }
    }
    late.sort_by(|a, b| a.total_cmp(b));
    let avg = late.iter().sum::<f64>() / late.len() as f64;
    println!(
        "Опоздание щелчков по {} долям: среднее {:.2} мс, медиана {:.2} мс, худшее {:.2} мс",
        late.len(),
        avg,
        late[late.len() / 2],
        late[late.len() - 1]
    );
}
