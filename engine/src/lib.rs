//! Ядро: ввод с геймпада, раскладки, звук, MIDI.

pub mod audio;
pub mod calibration;
#[cfg(windows)]
mod audio_exclusive;
pub mod engine;
pub mod input;
pub mod mapper;
pub mod midi;
pub mod midi_out;
pub mod model;
pub mod presets;
pub mod rack;
pub mod theory;
pub mod transport;
pub mod wav;

pub use audio::AudioMode;
pub use engine::{Command, Engine, EngineConfig, PadChoice, Settings, Status};
