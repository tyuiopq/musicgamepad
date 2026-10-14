//! MIDI-вход для синхронизации: принимаем MIDI Clock и Start/Continue/Stop от DAW.

use std::sync::mpsc::Sender;
use std::time::Instant;

use anyhow::anyhow;
use midir::{Ignore, MidiInput, MidiInputConnection};

const CLIENT: &str = "MusicGamepad";

pub fn list_ports() -> Vec<String> {
    let Ok(inp) = MidiInput::new(CLIENT) else { return Vec::new() };
    inp.ports().iter().filter_map(|p| inp.port_name(p).ok()).collect()
}

/// Сообщения синхронизации: тик (24 на долю), старт с начала, продолжить, стоп.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockEvent {
    Tick,
    Start,
    Continue,
    Stop,
}

pub struct ClockIn {
    _conn: MidiInputConnection<()>,
    pub port: String,
}

impl ClockIn {
    /// Подключиться к порту; события уходят в `tx` с моментом прихода.
    pub fn connect(port: &str, tx: Sender<(ClockEvent, Instant)>) -> anyhow::Result<Self> {
        let mut inp = MidiInput::new(CLIENT)?;
        // Часы — это «время»: без них нечего принимать. SysEx и Active Sensing не нужны.
        inp.ignore(Ignore::SysexAndActiveSense);
        let p = inp
            .ports()
            .into_iter()
            .find(|p| inp.port_name(p).is_ok_and(|n| n == port))
            .ok_or_else(|| anyhow!("MIDI-вход «{port}» не найден"))?;
        let conn = inp
            .connect(
                &p,
                "clock",
                move |_, bytes, _| {
                    let event = match bytes.first() {
                        Some(0xF8) => ClockEvent::Tick,
                        Some(0xFA) => ClockEvent::Start,
                        Some(0xFB) => ClockEvent::Continue,
                        Some(0xFC) => ClockEvent::Stop,
                        _ => return,
                    };
                    let _ = tx.send((event, Instant::now()));
                },
                (),
            )
            .map_err(|e| anyhow!("MIDI-вход «{port}»: {e}"))?;
        Ok(Self { _conn: conn, port: port.to_string() })
    }
}
