// Типы повторяют структуры движка (engine/src/engine.rs, model.rs, input.rs).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

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
  layers: string[];
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
  setPadChoice: (choice: PadChoice) => invoke<void>("set_pad_choice", { choice }),
  setSynthEnabled: (enabled: boolean) => invoke<void>("set_synth_enabled", { enabled }),
  setMidiOut: (port: string | null) => invoke<void>("set_midi_out", { port }),
  toggleRecord: () => invoke<void>("toggle_record"),
  panic: () => invoke<void>("panic"),
  openRecordings: () => invoke<void>("open_recordings"),
  onStatus: (cb: (s: Status) => void): Promise<UnlistenFn> => listen<Status>("status", (e) => cb(e.payload)),
};
