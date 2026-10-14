# Изменения MusicGamepad

Копия rustysynth 1.3.7 (лицензия MIT, https://github.com/sinshu/rustysynth) с одной доработкой:
синтезатор понимает звуковые контроллеры General MIDI 2.

| CC | Что делает | Где |
|---|---|---|
| 71 | резонанс фильтра: ±12 дБ к резонансу из банка | `channel.rs`, `voice.rs` |
| 72 | время затухания новых нот: ×1/8…×8 | `channel.rs`, `region_ex.rs` |
| 73 | время атаки новых нот: ×1/8…×8 | `channel.rs`, `region_ex.rs` |
| 74 | яркость: частота среза ±3 октавы, действует и на звучащие ноты | `channel.rs`, `voice.rs` |

Значение 64 — без изменений. Проверка: `cargo run -p engine --example sound_controls_check --release`.
