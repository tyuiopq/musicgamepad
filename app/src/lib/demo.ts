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
    profile: typeof location !== "undefined" && new URLSearchParams(location.search).get("profile") === "ableton" ? "Ableton Live: клавиши" : "Классика",
    music: scene.music as Status["music"],
    layers: ["Основной"],
    controls: scene.controls as Status["controls"],
    axes: scene.axes,
    transpose: -12,
    degree_shift: 0,
    notes: scene.notes,
    pressed: scene.pressed as Status["pressed"],
    parts: [{ name: "Мелодия", channel: 0, program: 0, instrument: "Grand Piano", error: null }],
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
    transport: {
      settings: {
        bpm: 100,
        beats_per_bar: 4,
        metronome: true,
        metronome_volume: 0.6,
        count_in: true,
        quantize: "off",
        loop_bars: 2,
        export_wav: false,
        send_clock: false,
        send_mmc: false,
      },
      bar: 3,
      beat: 2,
      record: "idle",
      count_in_left: null,
      record_seconds: 0,
      looper: { state: "playing", layers: 2, progress: 0.35, bars: 2 },
      exporting: 0,
      last_wav: null,
    },
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

const useAbleton = typeof location !== "undefined" && new URLSearchParams(location.search).get("profile") === "ableton";
const profile = structuredClone((useAbleton ? scene.ableton : scene.profile) as Profile);
// ?samples — у партии свой кит из сэмплов (посмотреть редактор пэдов)
if (typeof location !== "undefined" && new URLSearchParams(location.search).has("samples")) {
  profile.parts[0].channel = 9;
  profile.parts[0].source = {
    type: "samples",
    pads: [
      { note: 36, file: "samples/my-kit/kick_deep.wav", gain_db: 0, pitch: 0, choke: 0 },
      { note: 38, file: "samples/my-kit/snare_tight.wav", gain_db: -3, pitch: 0, choke: 0 },
      { note: 42, file: "samples/my-kit/hat_closed.wav", gain_db: -6, pitch: 0, choke: 1 },
      { note: 46, file: "samples/my-kit/hat_open.wav", gain_db: -6, pitch: 0, choke: 1 },
    ],
  };
}

export const demo: Record<string, (args?: Record<string, unknown>) => unknown> = {
  get_status: () => structuredClone(status),
  get_profile: () => profile,
  list_profiles: (): ProfileEntry[] => scene.library,
  get_settings: () => ({ pad_profiles: { play_station: "Импровизация: пентатоника" } }),
  list_midi_ports: () => ["loopMIDI Port", "Microsoft GS Wavetable Synth"],
  preview_profile: (args) => (args?.layer === "chords" ? scene.chords : scene.controls),
  apply_profile: () => undefined,
  get_presets: () => scene.presets,
  get_presets_for: () => scene.presets,
  list_sound_library: () => [{ file: "Arachno.sf2", name: "Arachno" }],
  midi_monitor: (args) => demoMonitor(Number(args?.since ?? 0)),
  list_recordings: () => [
    { name: "melody_2026-10-14_18-20-03", mid: "a.mid", wav: "a.wav", modified: "2026-10-14 18:20", bytes: 812345 },
    { name: "melody_2026-10-13_15-02-11", mid: "b.mid", wav: null, modified: "2026-10-13 15:02", bytes: 2344 },
  ],
  save_profile: () => undefined,
  toggle_record: () => {
    status = { ...status, recording: !status.recording };
    emit();
  },
};

/** Демо MIDI-монитора: немного правдоподобного потока. */
let monitorSeq = 0;
function demoMonitor(since: number) {
  const out = [];
  while (monitorSeq < since + 3) {
    monitorSeq++;
    const kind = monitorSeq % 6;
    const bytes =
      kind === 0
        ? [0x90, 57, 100]
        : kind === 3
          ? [0x80, 57, 0]
          : kind === 5
            ? [0xe0, 0, 0x48]
            : [0xb0, 74, 60 + (monitorSeq % 30)];
    out.push({ seq: monitorSeq, ms: monitorSeq * 37, bytes });
  }
  return out;
}

export function demoListen(cb: (s: Status) => void) {
  listeners.push(cb);
  return () => listeners.splice(listeners.indexOf(cb), 1);
}
