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
  family: PadFamily;
  model: string;
}

export type PadChoice = { mode: "auto" } | { mode: "fixed"; name: string };

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

export type ChordKind = "triad" | "seventh" | "power";

export type NoteSpec =
  | { type: "degree"; degree: number }
  | { type: "absolute"; note: number }
  | { type: "chord"; degree: number; chord: ChordKind };

export type Latch = "hold" | "toggle";
export type AxisMode = "bipolar" | "absolute" | "positive" | "negative";

export interface AxisShape {
  deadzone: number;
  curve: number;
  invert: boolean;
  mode: AxisMode;
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
  | { type: "panic" };

export type ControlTarget =
  | { type: "pitch_bend" }
  | { type: "cc"; cc: number; min: number; max: number }
  | { type: "velocity"; min: number; max: number };

export type Action =
  | { kind: "note"; note: NoteSpec; part: number; velocity: number }
  | { kind: "modifier"; effect: ModEffect; latch: Latch }
  | { kind: "command"; command: Command }
  | { kind: "control"; target: ControlTarget; shape: AxisShape; part: number };

export interface Binding {
  input: Input;
  action: Action;
}

/** Что делает вход прямо сейчас (Mapper::preview). */
export type ControlView = { input: Input; layer: string } & (
  | { role: "note"; notes: number[]; chord: boolean }
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
}

export interface PresetInfo {
  bank: number;
  program: number;
  name: string;
}

export interface Profile {
  name: string;
  music: Music;
  parts: Part[];
  layers: Layer[];
}

export interface Touched {
  key: SwitchKey;
  seq: number;
}

export interface PartStatus {
  name: string;
  channel: number;
  program: number;
  instrument: string;
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
  transpose: number;
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
  midi: { port: string | null; error: string | null };
  recording: boolean;
  last_recording: string | null;
  touched: Touched | null;
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
}

export const api = {
  status: () => invoke<Status>("get_status"),
  midiPorts: () => invoke<string[]>("list_midi_ports"),
  profiles: () => invoke<ProfileEntry[]>("list_profiles"),
  selectProfile: (name: string) => invoke<void>("select_profile", { name }),
  profile: () => invoke<Profile>("get_profile"),
  previewProfile: (profile: Profile, layer: string) => invoke<ControlView[]>("preview_profile", { profile, layer }),
  applyProfile: (profile: Profile) => invoke<void>("apply_profile", { profile }),
  saveProfile: (profile: Profile, previous: string | null) => invoke<void>("save_profile", { profile, previous }),
  deleteProfile: (name: string) => invoke<string>("delete_profile", { name }),
  setPadChoice: (choice: PadChoice) => invoke<void>("set_pad_choice", { choice }),
  setSynthEnabled: (enabled: boolean) => invoke<void>("set_synth_enabled", { enabled }),
  setAudioMode: (mode: AudioMode) => invoke<void>("set_audio_mode", { mode }),
  setMasterVolume: (volume: number) => invoke<void>("set_master_volume", { volume }),
  chooseSoundfont: () => invoke<string | null>("choose_soundfont"),
  resetSoundfont: () => invoke<void>("reset_soundfont"),
  presets: () => invoke<PresetInfo[]>("get_presets"),
  audition: (part: number) => invoke<void>("audition", { part }),
  setMidiOut: (port: string | null) => invoke<void>("set_midi_out", { port }),
  toggleRecord: () => invoke<void>("toggle_record"),
  panic: () => invoke<void>("panic"),
  openRecordings: () => invoke<void>("open_recordings"),
  onStatus: async (cb: (s: Status) => void): Promise<UnlistenFn> => {
    if (inApp) return listen<Status>("status", (e) => cb(e.payload));
    const { demoListen } = await import("./demo");
    return demoListen(cb);
  },
};
