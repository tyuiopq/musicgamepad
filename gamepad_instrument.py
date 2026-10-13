# -*- coding: utf-8 -*-
"""
Геймпад как музыкальный инструмент — прототип.

Установка:  pip install pygame-ce
Запуск:     python gamepad_instrument.py
Опции:      --list     показать MIDI-выходы
            --port N   выбрать MIDI-выход по номеру
            --debug    печатать номера кнопок и осей (для переназначения)
"""

import os
import time
import struct
import argparse

# Чтобы геймпад работал, даже когда окно программы не в фокусе (например, играешь в DAW)
os.environ["SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS"] = "1"

import pygame
import pygame.midi

# ===================== НАСТРОЙКИ =====================
# Номера кнопок и осей для Xbox-геймпада в pygame 2.
# Если у тебя другой геймпад — запусти с --debug и поправь цифры здесь.
BTN_A, BTN_B, BTN_X, BTN_Y = 0, 1, 2, 3
BTN_LB, BTN_RB = 4, 5
BTN_BACK, BTN_START = 6, 7
AXIS_RX, AXIS_RY = 2, 3
AXIS_LT, AXIS_RT = 4, 5

BASE_NOTE = 60                          # до первой октавы
SCALE = [0, 2, 4, 5, 7, 9, 11, 12]      # мажор: до ре ми фа соль ля си до

# Крестовина — нижние ступени, кнопки — верхние (зеркально)
HAT_DEGREE = {"left": 0, "down": 1, "right": 2, "up": 3}
BUTTON_DEGREE = {BTN_X: 4, BTN_A: 5, BTN_B: 6, BTN_Y: 7}

# Номера инструментов по стандарту General MIDI
INSTRUMENTS = [
    (0, "Фортепиано"), (11, "Вибрафон"), (24, "Гитара"), (40, "Скрипка"),
    (56, "Труба"), (73, "Флейта"), (79, "Окарина"), (80, "Синт-лид"),
]

DEADZONE = 0.15          # мёртвая зона стиков (против дрейфа)
SHARP_THRESHOLD = 0.5    # насколько зажать LT, чтобы нота стала диезом
VELOCITY = 100
NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
# =====================================================


def note_name(n):
    return f"{NOTE_NAMES[n % 12]}{n // 12 - 1}"


def apply_deadzone(v):
    if abs(v) < DEADZONE:
        return 0.0
    sign = 1 if v > 0 else -1
    return sign * (abs(v) - DEADZONE) / (1 - DEADZONE)


# ---------- Запись в .mid-файл ----------
def varlen(n):
    out = [n & 0x7F]
    n >>= 7
    while n:
        out.append((n & 0x7F) | 0x80)
        n >>= 7
    return bytes(reversed(out))


def write_midi(path, events, ppq=480, bpm=120):
    """events: список (секунды_от_начала, байты_сообщения)."""
    ticks_per_sec = ppq * bpm / 60
    track = bytearray()
    track += varlen(0) + bytes([0xFF, 0x51, 0x03]) + (60_000_000 // bpm).to_bytes(3, "big")
    last = 0
    for t, msg in events:
        tick = int(round(t * ticks_per_sec))
        track += varlen(max(0, tick - last)) + msg
        last = max(last, tick)
    track += varlen(0) + bytes([0xFF, 0x2F, 0x00])
    with open(path, "wb") as f:
        f.write(b"MThd" + struct.pack(">IHHH", 6, 0, 1, ppq))
        f.write(b"MTrk" + struct.pack(">I", len(track)) + track)


class Recorder:
    def __init__(self):
        self.events = None
        self.t0 = 0.0

    @property
    def active(self):
        return self.events is not None

    def start(self):
        self.events = []
        self.t0 = time.perf_counter()

    def add(self, msg):
        if self.active:
            self.events.append((time.perf_counter() - self.t0, bytes(msg)))

    def stop_and_save(self):
        events, self.events = self.events, None
        if not events or not any(m[0] & 0xF0 == 0x90 for _, m in events):
            return None
        name = time.strftime("melody_%Y%m%d_%H%M%S.mid")
        write_midi(name, events)
        return os.path.abspath(name)


# ---------- MIDI-выход ----------
class Synth:
    """Отправляет MIDI-сообщения и параллельно отдаёт их в запись."""

    def __init__(self, port_id, recorder):
        self.out = pygame.midi.Output(port_id, latency=0)
        self.rec = recorder

    def send(self, *msg):
        self.out.write_short(*msg)
        self.rec.add(msg)

    def note_on(self, note):
        self.send(0x90, note, VELOCITY)

    def note_off(self, note):
        self.send(0x80, note, 0)

    def cc(self, num, val):
        self.send(0xB0, num, val)

    def program(self, prog):
        self.send(0xC0, prog)

    def bend(self, val):
        self.send(0xE0, val & 0x7F, (val >> 7) & 0x7F)

    def panic(self):
        self.cc(123, 0)      # All Notes Off
        self.bend(8192)
        self.cc(1, 0)

    def close(self):
        self.out.close()


class Trigger:
    """Курок: сам калибрует положение покоя (у разных драйверов это -1 или 0)."""

    def __init__(self):
        self.rest = None
        self.value = 0.0

    def update(self, raw):
        if self.rest is None or raw < self.rest:
            self.rest = raw
        span = 1.0 - self.rest
        self.value = 0.0 if span <= 1e-6 else max(0.0, min(1.0, (raw - self.rest) / span))
        return self.value


# ---------- Логика инструмента ----------
class App:
    def __init__(self, port_id):
        self.rec = Recorder()
        self.synth = Synth(port_id, self.rec)
        self.joy = None
        self.held = {}             # источник (кнопка/направление) -> какая нота звучит
        self.hat_dirs = set()
        self.lt, self.rt = Trigger(), Trigger()
        self.octave_down = self.octave_up = False
        self.instr = 0
        self.last_bend = 8192
        self.last_mod = 0
        self.last_expr = -1
        self.status = "Подключи геймпад..."
        self.synth.program(INSTRUMENTS[0][0])
        self.set_expression(0.0, force=True)

    # --- ноты ---
    def octave_shift(self):
        return (1 if self.octave_up else 0) - (1 if self.octave_down else 0)

    def sharp(self):
        return self.lt.value >= SHARP_THRESHOLD

    def current_note(self, degree):
        n = BASE_NOTE + SCALE[degree] + 12 * self.octave_shift() + (1 if self.sharp() else 0)
        return max(0, min(127, n))

    def press(self, source, degree):
        if source in self.held:
            return
        note = self.current_note(degree)
        self.held[source] = note
        self.synth.note_on(note)

    def release(self, source):
        note = self.held.pop(source, None)
        if note is not None:
            # Гасим именно ту ноту, что включили, даже если с тех пор сменилась октава
            self.synth.note_off(note)

    def all_off(self):
        for note in self.held.values():
            self.synth.note_off(note)
        self.held.clear()
        self.hat_dirs.clear()
        self.synth.panic()
        self.last_bend, self.last_mod = 8192, 0

    # --- непрерывные контроллеры ---
    def set_expression(self, amount, force=False):
        val = int(40 + 87 * amount)          # отпущен RT — тише, зажат — громче
        if force or abs(val - self.last_expr) >= 2 or val in (40, 127):
            if val != self.last_expr or force:
                self.synth.cc(11, val)
                self.last_expr = val

    def on_axis(self, axis, raw):
        if axis == AXIS_LT:
            self.lt.update(raw)
        elif axis == AXIS_RT:
            self.set_expression(self.rt.update(raw))
        elif axis == AXIS_RX:                 # правый стик влево-вправо — питч-бенд
            x = apply_deadzone(raw)
            bend = max(0, min(16383, 8192 + int(x * 8191)))
            if abs(bend - self.last_bend) >= 32 or (bend == 8192 and self.last_bend != 8192):
                self.synth.bend(bend)
                self.last_bend = bend
        elif axis == AXIS_RY:                 # правый стик вверх-вниз — вибрато
            mod = int(abs(apply_deadzone(raw)) * 127)
            if abs(mod - self.last_mod) >= 2 or (mod == 0 and self.last_mod != 0):
                self.synth.cc(1, mod)
                self.last_mod = mod

    # --- кнопки ---
    def on_button(self, btn, down):
        if btn in BUTTON_DEGREE:
            if down:
                self.press(("btn", btn), BUTTON_DEGREE[btn])
            else:
                self.release(("btn", btn))
        elif btn == BTN_LB:
            self.octave_down = down
        elif btn == BTN_RB:
            self.octave_up = down
        elif btn == BTN_BACK and down:
            self.next_instrument()
        elif btn == BTN_START and down:
            self.toggle_record()

    def on_hat(self, value):
        x, y = value
        dirs = set()
        if x < 0: dirs.add("left")
        if x > 0: dirs.add("right")
        if y < 0: dirs.add("down")
        if y > 0: dirs.add("up")
        for d in self.hat_dirs - dirs:
            self.release(("hat", d))
        for d in dirs - self.hat_dirs:
            self.press(("hat", d), HAT_DEGREE[d])
        self.hat_dirs = dirs

    def next_instrument(self):
        self.all_off()
        self.instr = (self.instr + 1) % len(INSTRUMENTS)
        self.synth.program(INSTRUMENTS[self.instr][0])
        self.set_expression(self.rt.value, force=True)

    def toggle_record(self):
        if self.rec.active:
            path = self.rec.stop_and_save()
            self.status = f"Сохранено: {os.path.basename(path)}" if path else "Запись пустая, не сохранено"
            if path:
                print("Сохранено:", path)
        else:
            self.rec.start()
            # Стартовое состояние — чтобы файл звучал тем же инструментом
            self.rec.add((0xC0, INSTRUMENTS[self.instr][0]))
            self.rec.add((0xB0, 11, self.last_expr))
            self.status = "Идёт запись (Start — остановить)"

    # --- геймпад ---
    def on_device_added(self, index):
        if self.joy is None:
            self.joy = pygame.joystick.Joystick(index)
            self.status = f"Подключено: {self.joy.get_name()}"

    def on_device_removed(self, instance_id):
        if self.joy and self.joy.get_instance_id() == instance_id:
            self.all_off()
            self.joy = None
            self.status = "Геймпад отключился, подключи снова..."

    def owns(self, event):
        return self.joy is not None and getattr(event, "instance_id", None) == self.joy.get_instance_id()

    # --- окно ---
    def draw(self, screen, font):
        screen.fill((24, 24, 32))
        playing = " ".join(note_name(n) for n in sorted(set(self.held.values()))) or "—"
        lines = [
            (self.status, (230, 230, 230)),
            (f"Инструмент: {INSTRUMENTS[self.instr][1]}   (Back — сменить)", (230, 230, 230)),
            (f"Октава: {self.octave_shift():+d}    Диез: {'да' if self.sharp() else 'нет'}", (230, 230, 230)),
            (f"Звучит: {playing}", (140, 220, 255)),
            ("● REC" if self.rec.active else "Start — начать запись",
             (255, 90, 90) if self.rec.active else (150, 150, 150)),
        ]
        for i, (text, color) in enumerate(lines):
            screen.blit(font.render(text, True, color), (16, 14 + i * 34))
        pygame.display.flip()


def list_outputs():
    print("MIDI-выходы:")
    for i in range(pygame.midi.get_count()):
        _, name, _, is_out, _ = pygame.midi.get_device_info(i)
        if is_out:
            print(f"  [{i}] {name.decode(errors='replace')}")


def debug_print(event):
    if event.type in (pygame.JOYBUTTONDOWN, pygame.JOYBUTTONUP):
        print(f"кнопка {event.button} {'нажата' if event.type == pygame.JOYBUTTONDOWN else 'отпущена'}")
    elif event.type == pygame.JOYHATMOTION:
        print(f"крестовина {event.hat}: {event.value}")
    elif event.type == pygame.JOYAXISMOTION and abs(event.value) > 0.3:
        print(f"ось {event.axis}: {event.value:+.2f}")


def main():
    ap = argparse.ArgumentParser(description="Геймпад как музыкальный инструмент")
    ap.add_argument("--list", action="store_true", help="показать MIDI-выходы")
    ap.add_argument("--port", type=int, help="номер MIDI-выхода")
    ap.add_argument("--debug", action="store_true", help="печатать номера кнопок и осей")
    args = ap.parse_args()

    pygame.init()
    pygame.midi.init()

    if args.list:
        list_outputs()
        return

    port = args.port if args.port is not None else pygame.midi.get_default_output_id()
    if port < 0:
        print("Не найден ни один MIDI-выход. Запусти с --list и проверь.")
        return
    try:
        app = App(port)
    except Exception as e:
        print(f"Не получилось открыть MIDI-выход {port}: {e}")
        list_outputs()
        return

    screen = pygame.display.set_mode((600, 190))
    pygame.display.set_caption("Gamepad Instrument")
    font = pygame.font.SysFont("arial", 20)

    joy_events = (pygame.JOYBUTTONDOWN, pygame.JOYBUTTONUP, pygame.JOYAXISMOTION, pygame.JOYHATMOTION)
    running, dirty, last_draw = True, True, 0.0
    try:
        while running:
            # wait() просыпается сразу при событии — минимальная задержка без нагрузки на CPU
            first = pygame.event.wait(5)
            for event in [first] + pygame.event.get():
                t = event.type
                if t == pygame.NOEVENT:
                    continue
                if t == pygame.QUIT:
                    running = False
                elif t == pygame.JOYDEVICEADDED:
                    app.on_device_added(event.device_index)
                elif t == pygame.JOYDEVICEREMOVED:
                    app.on_device_removed(event.instance_id)
                elif t in joy_events and app.owns(event):
                    if args.debug:
                        debug_print(event)
                    if t == pygame.JOYBUTTONDOWN:
                        app.on_button(event.button, True)
                    elif t == pygame.JOYBUTTONUP:
                        app.on_button(event.button, False)
                    elif t == pygame.JOYAXISMOTION:
                        app.on_axis(event.axis, event.value)
                    elif t == pygame.JOYHATMOTION:
                        app.on_hat(event.value)
                dirty = True
            now = time.perf_counter()
            if dirty and now - last_draw > 1 / 30:
                app.draw(screen, font)
                dirty, last_draw = False, now
    except KeyboardInterrupt:
        pass
    finally:
        if app.rec.active:
            path = app.rec.stop_and_save()
            if path:
                print("Сохранено:", path)
        app.all_off()
        app.synth.close()
        pygame.midi.quit()
        pygame.quit()


if __name__ == "__main__":
    main()
