//! Ядро: ввод с геймпада, раскладки, звук, MIDI.

pub mod audio;
pub mod engine;
pub mod input;
pub mod mapper;
pub mod midi;
pub mod midi_out;
pub mod model;
pub mod presets;
pub mod theory;

pub use engine::{Command, Engine, EngineConfig, PadChoice, Settings, Status};
