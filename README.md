# MusicGamepad

Геймпад как музыкальный инструмент: кнопки играют ноты, стики и курки управляют
подтяжкой, вибрато и громкостью. Звук — встроенный синтезатор или MIDI-порт для DAW.

## Структура

| Папка | Что там |
|---|---|
| `engine/` | Ядро на Rust: геймпады (SDL3), раскладки, синтезатор, MIDI, запись |
| `app/` | Приложение: Tauri 2 + Svelte |
| `assets/` | Звуковой банк (не в git, см. `assets/README.md`) |
| `vendor/sdl3-src/` | Исходники SDL — собираются из пути без кириллицы |
| `spike/` | Спайк этапа 0, для справки |
| `gamepad_instrument.py` | Исходный прототип на Python |

Ядро движка — `engine/src/mapper.rs`: события геймпада → MIDI. Модель профиля
(слои, модификаторы, роли кнопок) — `engine/src/model.rs`.

## Подготовка (один раз)

1. Rust (stable, MSVC), Node.js, pnpm, Visual Studio с C++.
2. Скачать звуковой банк в `assets/` (ссылка в `assets/README.md`).
3. Создать `.cargo/config.toml` с путём к CMake из Visual Studio:
   ```toml
   [env]
   CMAKE = "C:/Program Files/Microsoft Visual Studio/18/Community/Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe"
   ```
4. `cd app && pnpm install`

## Запуск

```
cd app
pnpm tauri dev                  # разработка, с горячей перезагрузкой интерфейса
pnpm tauri build --no-bundle    # релиз: target/release/musicgamepad.exe
```

Тесты ядра и запуск движка без окна:

```
cargo test -p engine
cargo run -p engine --example headless --release -- 30   # 30 секунд печатает состояние
```

## Где программа хранит данные

- Настройки: `%APPDATA%\com.musicgamepad.app\settings.json`
- Свои профили: `%APPDATA%\com.musicgamepad.app\profiles\*.json`
- Записи: `Документы\MusicGamepad\`
