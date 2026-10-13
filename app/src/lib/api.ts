// Типы повторяют структуры движка (engine/src/engine.rs, model.rs, input.rs).
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Открыто ли окно внутри приложения. В обычном браузере работает демо-режим (lib/demo.ts). */
const inApp = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

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

export type SwitchKey =
  | { kind: "button"; button: string }
  | { kind: "axis_switch"; axis: string; direction: "positive" | "negative" };

export type Input =
  | { kind: "button"; button: string }
  | { kind: "axis_switch"; axis: string; direction: "positive" | "negative"; threshold: number }
  | { kind: "axis"; axis: string };

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

/** Что делает вход прямо сейчас (Mapper::preview). */
export type ControlView = { input: Input; layer: string } & (
  | { role: "note"; notes: number[]; chord: boolean }
  | { role: "modifier"; effect: ModEffect; latch: "hold" | "toggle"; active: boolean }
  | { role: "command"; command: Command }
  | { role: "control"; target: ControlTarget }
);

export interface Music {
  root: number;
  octave: number;
  scale: { type: "preset"; preset: string } | { type: "custom"; steps: number[] };
}

export interface ProfileLayer {
  id: string;
  name: string;
}

export interface Profile {
  name: string;
  music: Music;
  layers: ProfileLayer[];
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
    buffer_ms: number | null;
    error: string | null;
  };
  midi: { port: string | null; error: string | null };
  recording: boolean;
  last_recording: string | null;
}

export interface ProfileEntry {
  name: string;
  builtin: boolean;
}

export const api = {
  status: () => invoke<Status>("get_status"),
  midiPorts: () => invoke<string[]>("list_midi_ports"),
  profiles: () => invoke<ProfileEntry[]>("list_profiles"),
  selectProfile: (name: string) => invoke<void>("select_profile", { name }),
  profile: () => invoke<Profile>("get_profile"),
  setPadChoice: (choice: PadChoice) => invoke<void>("set_pad_choice", { choice }),
  setSynthEnabled: (enabled: boolean) => invoke<void>("set_synth_enabled", { enabled }),
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
