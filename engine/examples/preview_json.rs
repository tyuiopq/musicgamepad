//! Печатает, что делает каждая кнопка в сцене «зажат LB, играет нижняя кнопка».
//! Нужен для демо-режима интерфейса: cargo run -p engine --example preview_json

use engine::mapper::{InputEvent, Mapper};
use engine::model::{Axis, Button};
use engine::presets;

fn main() {
    let mut m = Mapper::new(presets::classic());
    m.handle(InputEvent::Button { button: Button::LeftShoulder, down: true });
    m.handle(InputEvent::Button { button: Button::South, down: true });
    m.handle(InputEvent::Axis { axis: Axis::RightTrigger, value: 0.62 });
    m.handle(InputEvent::Axis { axis: Axis::RightX, value: 0.45 });
    let out = serde_json::json!({
        "controls": m.preview(),
        "pressed": m.pressed(),
        "notes": m.sounding_notes(),
        "axes": m.axis_values(),
        "music": m.profile().music,
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
