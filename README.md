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
pnpm tauri build --no-bundle    # только exe: target/release/musicgamepad.exe
pnpm tauri build                # установщик: target/release/bundle/nsis/MusicGamepad_*_x64-setup.exe
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

## Поставка

Установщик (NSIS, на русском, без прав администратора) не содержит звуковой банк: при первом
запуске программа предлагает скачать GeneralUser GS (32 МБ) или выбрать свой `.sf2`. Банк из
`assets/` подхватывается только отладочной сборкой.

Иконка — `app/src-tauri/installer/icon.svg`; после правки: отрендерить в PNG 1024×1024 и
`pnpm tauri icon src-tauri/installer/icon-1024.png`. Картинки установщика — `installer/*.bmp`.

Журнал ошибок: `%APPDATA%\com.musicgamepad.app\logs\musicgamepad.log`
(кнопка-значок в шапке «Настроек» открывает папку).
