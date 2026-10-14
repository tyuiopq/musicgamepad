// Типы повторяют структуры движка (engine/src/engine.rs, model.rs, input.rs).
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Открыто ли окно внутри приложения. В обычном браузере работает демо-режим (lib/demo.ts). */
const inApp = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Параметр адреса для демо-режима (например, ?view=edit); в приложении всегда null. */
export function demoOption(name: string): string | null {
  return inApp || typeof location === "undefined" ? null : new URLSearchParams(location.search).get(name);
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (inApp) return tauriInvoke<T>(cmd, args);
  const { demo } = await import("./demo");
  return (demo[cmd]?.(args) ?? undefined) as T;
}

export type PadFamily = "xbox" | "play_station" | "nintendo" | "generic";

export interface PadInfo {
  id: number;
  name: string;
  /** Имя для назначений: у второго такого же геймпада — «… #2». */
  key?: string;
  family: PadFamily;
  model: string;
}

export type PadChoice = { mode: "auto" } | { mode: "fixed"; name: string } | { mode: "all" };

/** Дополнительный геймпад в режиме «все сразу». */
export interface GuestStatus {
  pad: number;
  profile: string;
  notes: number[];
}

export type Direction = "positive" | "negative";

export type SwitchKey =
  | { kind: "button"; button: string }
  | { kind: "axis_switch"; axis: string; direction: Direction };

export type Input =
  | { kind: "button"; button: string }
  | { kind: "axis_switch"; axis: string; direction: Direction; threshold: number }
  | { kind: "axis"; axis: string };

export type ScalePreset =
  | "major"
  | "natural_minor"
  | "harmonic_minor"
  | "melodic_minor"
  | "major_pentatonic"
  | "minor_pentatonic"
  | "blues"
  | "dorian"
  | "phrygian"
  | "lydian"
  | "mixolydian"
  | "locrian"
  | "chromatic";

export type ChordKind = "triad" | "seventh" | "ninth" | "sus2" | "sus4" | "sixth" | "power" | "custom";

export type NoteSpec =
  | { type: "degree"; degree: number }
  | { type: "absolute"; note: number }
  | {
      type: "chord";
      degree: number;
      chord: ChordKind;
      /** Для custom: интервалы от основного тона в полутонах. */
      intervals?: number[];
      inversion?: number;
      open?: boolean;
      bass?: boolean;
    };

export type Latch = "hold" | "toggle";
export type AxisMode = "bipolar" | "absolute" | "positive" | "negative";

export type CurveKind = "power" | "s";

export interface AxisShape {
  deadzone: number;
  curve: number;
  curve_kind?: CurveKind;
  invert: boolean;
  mode: AxisMode;
  /** Сглаживание, мс. */
  smoothing: number;
}

export type ModEffect =
  | { type: "semitones"; semitones: number }
  | { type: "octaves"; octaves: number }
  | { type: "degrees"; degrees: number }
  | { type: "layer"; layer: string }
  | { type: "velocity"; velocity: number }
  | { type: "sustain" };

export type Command =
  | { type: "next_instrument"; part: number }
  | { type: "prev_instrument"; part: number }
  | { type: "toggle_record" }
  | { type: "panic" }
  | { type: "toggle_metronome" }
  | { type: "set_program"; part: number; program: number; bank: number }
  | { type: "loop_record" }
  | { type: "loop_undo" }
  | { type: "loop_clear" }
  | { type: "tap_tempo" };

export type Quantize = "off" | "quarter" | "eighth" | "sixteenth" | "eighth_triplet";

export interface TransportSettings {
  bpm: number;
  beats_per_bar: number;
  metronome: boolean;
  metronome_volume: number;
  count_in: boolean;
  quantize: Quantize;
  loop_bars: number;
  export_wav: boolean;
  send_clock: boolean;
  send_mmc: boolean;
  /** Живое квантование: ноты подтягиваются к доле, пока идут часы. */
  live_quantize?: Quantize;
}

export type MonitorInput = { kind: "button"; button: string; down: boolean } | { kind: "axis"; axis: string; value: number };

export interface MonitorEntry {
  seq: number;
  ms: number;
  /** Ушедшее сообщение; у записей о входах пусто. */
  bytes: number[];
  input?: MonitorInput;
}

export type LoopState = "empty" | "armed" | "recording" | "playing" | "overdub";

export interface TransportStatus {
  settings: TransportSettings;
  bar: number | null;
  beat: number | null;
  record: "idle" | "count_in" | "recording";
  count_in_left: number | null;
  record_seconds: number;
  looper: { state: LoopState; layers: number; progress: number; bars: number };
  exporting: number;
  last_wav: string | null;
}

export interface RecordingEntry {
  name: string;
  mid: string | null;
  wav: string | null;
  modified: string;
  bytes: number;
}

export type ControlTarget =
  | { type: "pitch_bend" }
  | { type: "cc"; cc: number; min: number; max: number }
  | { type: "velocity"; min: number; max: number }
  | { type: "pressure"; min: number; max: number };

export type Action =
  | {
      kind: "note";
      note: NoteSpec;
      part: number;
      velocity: number;
      latch: Latch;
      /** Фиксированная длина, мс (0 / нет — пока зажата). */
      length_ms?: number;
      velocity_from?: VelocitySource;
    }
  | { kind: "cc"; cc: number; part: number; on: number; off: number; latch: Latch; step: number }
  | { kind: "modifier"; effect: ModEffect; latch: Latch }
  | { kind: "command"; command: Command }
  | { kind: "control"; target: ControlTarget; shape: AxisShape; part: number; takeover: boolean; relative: boolean };

/** Сила нажатия: постоянная или от оси (от `min` до `velocity`). */
export type VelocitySource = { type: "fixed" } | { type: "axis"; axis: string; min: number } | { type: "last_axis"; min: number };

export interface Binding {
  input: Input;
  action: Action;
}

/** Что делает вход прямо сейчас (Mapper::preview). */
export type ControlView = { input: Input; layer: string } & (
  | { role: "note"; notes: number[]; chord: boolean; latch: Latch; on: boolean }
  | { role: "cc"; cc: number; step: number; latch: Latch; on: boolean }
  | { role: "modifier"; effect: ModEffect; latch: Latch; active: boolean }
  | { role: "command"; command: Command }
  | { role: "control"; target: ControlTarget }
);

export interface Music {
  root: number;
  octave: number;
  scale: { type: "preset"; preset: ScalePreset } | { type: "custom"; steps: number[] };
}

export interface Layer {
  id: string;
  name: string;
  bindings: Binding[];
  /** Слой-комбинация: включается, когда включены все перечисленные слои. */
  combo?: string[];
  /** Сдвиг нот слоя, полутоны. */
  transpose?: number;
}

/** Звучание партии: стандартные MIDI-контроллеры, 64 — «как в банке» (кроме панорамы и посылов). */
export interface Sound {
  pan: number;
  reverb: number;
  chorus: number;
  attack: number;
  release: number;
  brightness: number;
  resonance: number;
}

export interface SamplePad {
  note: number;
  file: string;
  gain_db: number;
  pitch: number;
  choke: number;
}

export type PartSource =
  | { type: "default" }
  | { type: "sound_font"; file: string }
  | { type: "samples"; pads: SamplePad[] };

export interface BankEntry {
  file: string;
  name: string;
}

export interface Part {
  name: string;
  channel: number;
  program: number;
  bank: number;
  volume: number;
  bend_range: number;
  instruments: number[];
  transpose: number;
  sound: Sound;
  /** Свой MIDI-выход партии; нет поля — основной из настроек. */
  port?: string | null;
  /** Нет поля — основной банк. */
  source?: PartSource;
}

export interface PresetInfo {
  bank: number;
  program: number;
  name: string;
}

export interface SetupStep {
  text: string;
  learn?: Input;
}

export interface ProfileMeta {
  category: string;
  target: string;
  description: string;
  setup: SetupStep[];
}

export interface Profile {
  name: string;
  music: Music;
  parts: Part[];
  layers: Layer[];
  meta?: ProfileMeta;
}

export interface AxisCal {
  center: number;
  min: number;
  max: number;
}

export interface Calibration {
  axes: Partial<Record<string, AxisCal>>;
}

export interface MidiStatus {
  port: string | null;
  error: string | null;
  /** Свои выходы партий. */
  extra?: { port: string; error: string | null }[];
  clock_in?: string | null;
  clock_in_error?: string | null;
  /** Приходит MIDI Clock — темп задаёт DAW. */
  clock_external?: boolean;
  clock_playing?: boolean;
}

export interface AppSettings {
  clock_in?: string | null;
  /** Профили дополнительных геймпадов: ключ геймпада → профиль. */
  pad_assign?: Record<string, string>;
  pad_profiles: Record<string, string>;
  calibration?: Record<string, Calibration>;
  ui?: { accent: string | null; secondary: string | null; onboarded?: boolean };
}

export interface Touched {
  key: SwitchKey;
  seq: number;
}

export interface PartStatus {
  name: string;
  channel: number;
  program: number;
  bank: number;
  instrument: string;
  error: string | null;
}

export interface Status {
  pads: PadInfo[];
  active_pad: number | null;
  pad_choice: PadChoice;
  input_error: string | null;
  profile: string;
  music: Music | null;
  layers: string[];
  controls: ControlView[];
  axes: Partial<Record<string, number>>;
  guests?: GuestStatus[];
  /** Оси до калибровки. */
  raw_axes?: Partial<Record<string, number>>;
  transpose: number;
  degree_shift: number;
  notes: number[];
  pressed: SwitchKey[];
  parts: PartStatus[];
  synth: {
    enabled: boolean;
    soundfont: string | null;
    device: string | null;
    requested_mode: AudioMode;
    master_volume: number;
    custom_soundfont: boolean;
    mode: AudioMode | null;
    fallback: string | null;
    error: string | null;
    latency: Latency | null;
  };
  midi: MidiStatus;
  recording: boolean;
  last_recording: string | null;
  touched: Touched | null;
  transport: TransportStatus;
}

export type AudioMode = "exclusive" | "shared";

/** Оценка «нажал → услышал», мс. */
export interface Latency {
  input: number | null;
  block: number;
  output: number;
  total: number;
}

export interface ProfileEntry {
  name: string;
  builtin: boolean;
  /** Встроенный профиль изменён — можно вернуть заводской. */
  modified: boolean;
  /** builtin, daw, synth или mine. */
  category: string;
  target: string;
  description: string;
}

export const api = {
  status: () => invoke<Status>("get_status"),
  midiPorts: () => invoke<string[]>("list_midi_ports"),
  midiInPorts: () => invoke<string[]>("list_midi_in_ports"),
  setClockIn: (port: string | null) => invoke<void>("set_clock_in", { port }),
  profiles: () => invoke<ProfileEntry[]>("list_profiles"),
  selectProfile: (name: string) => invoke<void>("select_profile", { name }),
  profile: () => invoke<Profile>("get_profile"),
  previewProfile: (profile: Profile, layer: string) => invoke<ControlView[]>("preview_profile", { profile, layer }),
  applyProfile: (profile: Profile) => invoke<void>("apply_profile", { profile }),
  saveProfile: (profile: Profile, previous: string | null) => invoke<void>("save_profile", { profile, previous }),
  deleteProfile: (name: string) => invoke<string>("delete_profile", { name }),
  exportProfile: (name: string) => invoke<string | null>("export_profile", { name }),
  importProfile: () => invoke<string | null>("import_profile"),
  settings: () => invoke<AppSettings>("get_settings"),
  setCalibration: (pad: string, calibration: Calibration | null) => invoke<void>("set_calibration", { pad, calibration }),
  setPadProfile: (family: string, profile: string | null) => invoke<void>("set_pad_profile", { family, profile }),
  setUiColors: (accent: string, secondary: string) => invoke<void>("set_ui_colors", { accent, secondary }),
  setPadAssign: (pad: string, profile: string | null) => invoke<void>("set_pad_assign", { pad, profile }),
  setPadChoice: (choice: PadChoice) => invoke<void>("set_pad_choice", { choice }),
  setSynthEnabled: (enabled: boolean) => invoke<void>("set_synth_enabled", { enabled }),
  setAudioMode: (mode: AudioMode) => invoke<void>("set_audio_mode", { mode }),
  setMasterVolume: (volume: number) => invoke<void>("set_master_volume", { volume }),
  chooseSoundfont: () => invoke<string | null>("choose_soundfont"),
  downloadBank: () => invoke<string>("download_bank"),
  cancelBankDownload: () => invoke<void>("cancel_bank_download"),
  setOnboarded: (done: boolean) => invoke<void>("set_onboarded", { done }),
  onBankProgress: async (cb: (p: { done: number; total: number | null }) => void): Promise<UnlistenFn> => {
    if (inApp) return listen<{ done: number; total: number | null }>("bank_progress", (e) => cb(e.payload));
    return () => {};
  },
  resetSoundfont: () => invoke<void>("reset_soundfont"),
  presets: () => invoke<PresetInfo[]>("get_presets"),
  presetsFor: (file: string | null) => invoke<PresetInfo[]>("get_presets_for", { file }),
  soundLibrary: () => invoke<BankEntry[]>("list_sound_library"),
  addSoundfont: () => invoke<string | null>("add_soundfont"),
  chooseSample: () => invoke<string | null>("choose_sample"),
  kitFromFolder: () => invoke<string[]>("kit_from_folder"),
  openSoundLibrary: () => invoke<void>("open_sound_library"),
  audition: (part: number) => invoke<void>("audition", { part }),
  setMidiOut: (port: string | null) => invoke<void>("set_midi_out", { port }),
  toggleRecord: () => invoke<void>("toggle_record"),
  setTransport: (settings: TransportSettings) => invoke<void>("set_transport", { settings }),
  transportAction: (action: Command) => invoke<void>("transport_action", { action }),
  listRecordings: () => invoke<RecordingEntry[]>("list_recordings"),
  midiMonitor: (since: number) => invoke<MonitorEntry[]>("midi_monitor", { since }),
  sendTest: (messages: [number, number[]][]) => invoke<void>("send_test", { messages }),
  openRecording: (path: string) => invoke<void>("open_recording", { path }),
  deleteRecording: (name: string) => invoke<void>("delete_recording", { name }),
  panic: () => invoke<void>("panic"),
  openRecordings: () => invoke<void>("open_recordings"),
  openLogs: () => invoke<void>("open_logs"),
  /** Записать в журнал ошибок; сама никогда не падает. */
  logUi: (level: "error" | "warn", message: string) => invoke<void>("log_ui", { level, message }).catch(() => undefined),
  onStatus: async (cb: (s: Status) => void): Promise<UnlistenFn> => {
    if (inApp) return listen<Status>("status", (e) => cb(e.payload));
    const { demoListen } = await import("./demo");
    return demoListen(cb);
  },
};
