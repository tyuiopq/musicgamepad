//! Внешний MIDI-выход (виртуальный порт → DAW, аппаратный синтезатор и т.п.).
//! На Windows виртуальный порт создаёт loopMIDI, на macOS — сама программа (CoreMIDI это умеет).

use anyhow::anyhow;
use midir::{MidiOutput, MidiOutputConnection};

use crate::midi::MidiMsg;

const CLIENT: &str = "MusicGamepad";

/// Свой виртуальный порт (только macOS): в DAW он виден как вход «MusicGamepad».
pub const VIRTUAL_PORT: &str = "MusicGamepad (виртуальный порт)";

pub fn list_ports() -> Vec<String> {
    let Ok(out) = MidiOutput::new(CLIENT) else { return Vec::new() };
    let mut ports: Vec<String> = out.ports().iter().filter_map(|p| out.port_name(p).ok()).collect();
    if cfg!(target_os = "macos") {
        ports.insert(0, VIRTUAL_PORT.to_string());
    }
    ports
}

pub struct MidiOut {
    conn: MidiOutputConnection,
    pub port: String,
}

impl MidiOut {
    pub fn connect(port: &str) -> anyhow::Result<Self> {
        let out = MidiOutput::new(CLIENT)?;
        #[cfg(target_os = "macos")]
        if port == VIRTUAL_PORT {
            use midir::os::unix::VirtualOutput;
            let conn = out.create_virtual(CLIENT).map_err(|e| anyhow!("виртуальный MIDI-порт: {e}"))?;
            return Ok(Self { conn, port: port.to_string() });
        }
        let p = out
            .ports()
            .into_iter()
            .find(|p| out.port_name(p).is_ok_and(|n| n == port))
            .ok_or_else(|| anyhow!("MIDI-порт «{port}» не найден"))?;
        let conn = out.connect(&p, "out").map_err(|e| anyhow!("MIDI-порт «{port}»: {e}"))?;
        Ok(Self { conn, port: port.to_string() })
    }

    /// Произвольные байты (SysEx, например MMC).
    pub fn send_raw(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.conn.send(bytes).map_err(|e| anyhow!("MIDI-порт «{}»: {e}", self.port))
    }

    pub fn send(&mut self, msg: MidiMsg) -> anyhow::Result<()> {
        self.conn.send(msg.as_slice()).map_err(|e| anyhow!("MIDI-порт «{}»: {e}", self.port))
    }
}
