// Демо-режим: страница открыта в обычном браузере, без движка.
// Показывает сцену из настоящего ядра (demo-scene.json — вывод `cargo run -p engine --example preview_json`)
// и проигрывает короткую мелодию, чтобы ожила лента нот. Нужен для работы над дизайном.
import type { Profile, ProfileEntry, Status } from "./api";
import scene from "./demo-scene.json";

const MELODY = [48, 50, 52, 53, 55, 53, 52, 57];

function baseStatus(): Status {
  return {
    pads: [
      { id: 1, name: "Xbox 360 Controller", family: "xbox", model: "Xbox360" },
      { id: 2, name: "DualSense Wireless Controller", family: "play_station", model: "PS5" },
    ],
    active_pad: 1,
    pad_choice: { mode: "auto" },
    input_error: null,
    profile: "Классика",
    music: scene.music as Status["music"],
    layers: ["Основной"],
    controls: scene.controls as Status["controls"],
    axes: scene.axes,
    transpose: -12,
    notes: scene.notes,
    pressed: scene.pressed as Status["pressed"],
    parts: [{ name: "Мелодия", channel: 0, program: 0, instrument: "Grand Piano" }],
    synth: {
      enabled: true,
      soundfont: "C:\\assets\\GeneralUser-GS.sf2",
      device: "Динамики",
      requested_mode: "exclusive",
      master_volume: 1,
      custom_soundfont: false,
      mode: "exclusive",
      fallback: null,
      error: null,
      latency: { input: 1.2, block: 1.5, output: 6, total: 8.7 },
    },
    midi: { port: null, error: null },
    recording: false,
    last_recording: "C:\\Users\\me\\Documents\\MusicGamepad\\melody_2026-10-13_15-02-11.mid",
    touched: null,
  };
}

let status = baseStatus();
const listeners: ((s: Status) => void)[] = [];

function emit() {
  for (const l of listeners) l(structuredClone(status));
}

// Мелодия по кругу: нота звучит ~0,6 с, пауза ~0,25 с
let step = 0;
function tick() {
  const note = MELODY[step % MELODY.length];
  step++;
  status = { ...status, notes: [note] };
  emit();
  setTimeout(() => {
    status = { ...status, notes: step % MELODY.length === 0 ? scene.notes : [] };
    emit();
  }, 600);
}
if (typeof window !== "undefined" && new URLSearchParams(location.search).has("play")) {
  setInterval(tick, 850);
}

const profile = scene.profile as Profile;

export const demo: Record<string, (args?: Record<string, unknown>) => unknown> = {
  get_status: () => structuredClone(status),
  get_profile: () => profile,
  list_profiles: (): ProfileEntry[] => [{ name: "Классика", builtin: true, modified: false }],
  list_midi_ports: () => ["loopMIDI Port", "Microsoft GS Wavetable Synth"],
  preview_profile: (args) => (args?.layer === "chords" ? scene.chords : scene.controls),
  apply_profile: () => undefined,
  get_presets: () => scene.presets,
  save_profile: () => undefined,
  toggle_record: () => {
    status = { ...status, recording: !status.recording };
    emit();
  },
};

export function demoListen(cb: (s: Status) => void) {
  listeners.push(cb);
  return () => listeners.splice(listeners.indexOf(cb), 1);
}
