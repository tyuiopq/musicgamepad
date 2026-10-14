//! Геймпады через SDL3: подключение, отключение и нормализованные события.
//! Весь SDL живёт в одном потоке — в том, где создан `GamepadInput`.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::anyhow;
use sdl3::event::Event;
use sdl3::gamepad::{self as sg, Gamepad, GamepadType};
use sdl3::joystick::JoystickId;
use serde::Serialize;

use crate::mapper::InputEvent;
use crate::model::{Axis, Button};

/// Семейство геймпада — по нему интерфейс выбирает подписи кнопок (A/B/X/Y или ✕○□△).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PadFamily {
    Xbox,
    PlayStation,
    Nintendo,
    Generic,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PadInfo {
    pub id: u32,
    pub name: String,
    /// Имя, по которому геймпад узнаётся между запусками: у второго такого же — «… #2».
    pub key: String,
    pub family: PadFamily,
    /// Точная модель по версии SDL (XboxOne, PS5, …).
    pub model: String,
}

pub enum RawEvent {
    Added(PadInfo),
    Removed(u32),
    /// `t_ns` — когда SDL получил событие, по часам `ticks_ns()`.
    Input { pad: u32, event: InputEvent, t_ns: u64 },
}

pub struct GamepadInput {
    _sdl: sdl3::Sdl,
    subsystem: sdl3::GamepadSubsystem,
    pump: sdl3::EventPump,
    pads: HashMap<u32, (Gamepad, PadInfo)>,
}

impl GamepadInput {
    pub fn new() -> anyhow::Result<Self> {
        // Геймпад должен работать, даже когда окно не в фокусе (играешь поверх DAW)
        sdl3::hint::set("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1");
        let sdl = sdl3::init().map_err(|e| anyhow!("SDL: {e}"))?;
        let subsystem = sdl.gamepad().map_err(|e| anyhow!("SDL gamepad: {e}"))?;
        let pump = sdl.event_pump().map_err(|e| anyhow!("SDL events: {e}"))?;
        Ok(Self { _sdl: sdl, subsystem, pump, pads: HashMap::new() })
    }

    /// Подключённые геймпады в порядке подключения.
    pub fn pads(&self) -> Vec<PadInfo> {
        let mut v: Vec<PadInfo> = self.pads.values().map(|(_, info)| info.clone()).collect();
        v.sort_by_key(|p| p.id);
        for i in 0..v.len() {
            let n = v[..i].iter().filter(|p| p.name == v[i].name).count();
            v[i].key = if n == 0 { v[i].name.clone() } else { format!("{} #{}", v[i].name, n + 1) };
        }
        v
    }

    /// Ждёт события до `timeout` (просыпается сразу, как что-то произошло) и забирает все накопившиеся.
    pub fn wait(&mut self, timeout: Duration, out: &mut Vec<RawEvent>) {
        let Some(first) = self.pump.wait_event_timeout(timeout) else { return };
        let mut events = vec![first];
        events.extend(self.pump.poll_iter());
        for event in events {
            self.convert(event, out);
        }
    }

    fn convert(&mut self, event: Event, out: &mut Vec<RawEvent>) {
        match event {
            Event::GamepadAdded { which, .. } => {
                if self.pads.contains_key(&which.raw()) {
                    return;
                }
                if let Ok(pad) = self.subsystem.open(which) {
                    let info = pad_info(which, &pad);
                    out.push(RawEvent::Added(info.clone()));
                    self.pads.insert(which.raw(), (pad, info));
                }
            }
            Event::GamepadRemoved { which, .. } => {
                if self.pads.remove(&which.raw()).is_some() {
                    out.push(RawEvent::Removed(which.raw()));
                }
            }
            Event::GamepadButtonDown { which, button, timestamp } | Event::GamepadButtonUp { which, button, timestamp } => {
                let down = matches!(event, Event::GamepadButtonDown { .. });
                if let Some(button) = map_button(button) {
                    let event = InputEvent::Button { button, down };
                    out.push(RawEvent::Input { pad: which.raw(), event, t_ns: timestamp });
                }
            }
            Event::GamepadAxisMotion { which, axis, value, timestamp } => {
                let (axis, value) = map_axis(axis, value);
                out.push(RawEvent::Input { pad: which.raw(), event: InputEvent::Axis { axis, value }, t_ns: timestamp });
            }
            _ => {}
        }
    }
}

/// Часы SDL, в которых проставлены метки времени событий.
pub fn ticks_ns() -> u64 {
    unsafe { sdl3::sys::timer::SDL_GetTicksNS() }
}

fn pad_info(id: JoystickId, pad: &Gamepad) -> PadInfo {
    let kind = pad.r#type();
    let family = match kind {
        GamepadType::Xbox360 | GamepadType::XboxOne => PadFamily::Xbox,
        GamepadType::PS3 | GamepadType::PS4 | GamepadType::PS5 => PadFamily::PlayStation,
        GamepadType::NintendoSwitchPro
        | GamepadType::NintendoSwitchJoyconLeft
        | GamepadType::NintendoSwitchJoyconRight
        | GamepadType::NintendoSwitchJoyconPair => PadFamily::Nintendo,
        _ => PadFamily::Generic,
    };
    let name = pad.name().unwrap_or_else(|| "Геймпад".into());
    PadInfo {
        id: id.raw(),
        key: name.clone(),
        name,
        family,
        model: format!("{kind:?}"),
    }
}

fn map_button(b: sg::Button) -> Option<Button> {
    Some(match b {
        sg::Button::South => Button::South,
        sg::Button::East => Button::East,
        sg::Button::West => Button::West,
        sg::Button::North => Button::North,
        sg::Button::DPadUp => Button::DpadUp,
        sg::Button::DPadDown => Button::DpadDown,
        sg::Button::DPadLeft => Button::DpadLeft,
        sg::Button::DPadRight => Button::DpadRight,
        sg::Button::LeftShoulder => Button::LeftShoulder,
        sg::Button::RightShoulder => Button::RightShoulder,
        sg::Button::LeftStick => Button::LeftStick,
        sg::Button::RightStick => Button::RightStick,
        sg::Button::Back => Button::Back,
        sg::Button::Start => Button::Start,
        sg::Button::Guide => Button::Guide,
        sg::Button::Touchpad => Button::Touchpad,
        sg::Button::Misc1 => Button::Misc1,
        sg::Button::LeftPaddle1 => Button::LeftPaddle1,
        sg::Button::LeftPaddle2 => Button::LeftPaddle2,
        sg::Button::RightPaddle1 => Button::RightPaddle1,
        sg::Button::RightPaddle2 => Button::RightPaddle2,
        _ => return None,
    })
}

/// SDL: стики −32768..32767 (вниз — плюс), курки 0..32767.
/// У нас: вверх — плюс, всё приведено к −1..1 / 0..1.
fn map_axis(a: sg::Axis, raw: i16) -> (Axis, f32) {
    let v = (raw as f32 / 32767.0).clamp(-1.0, 1.0);
    match a {
        sg::Axis::LeftX => (Axis::LeftX, v),
        sg::Axis::LeftY => (Axis::LeftY, -v),
        sg::Axis::RightX => (Axis::RightX, v),
        sg::Axis::RightY => (Axis::RightY, -v),
        sg::Axis::TriggerLeft => (Axis::LeftTrigger, v.max(0.0)),
        sg::Axis::TriggerRight => (Axis::RightTrigger, v.max(0.0)),
    }
}
