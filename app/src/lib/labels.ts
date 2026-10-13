// Подписи кнопок и нот для интерфейса.
import type { PadFamily, SwitchKey } from "./api";

type Labels = Record<string, string>;

const common: Labels = {
  dpad_up: "↑",
  dpad_down: "↓",
  dpad_left: "←",
  dpad_right: "→",
  touchpad: "Тачпад",
  misc1: "Доп.",
  left_paddle1: "P3",
  left_paddle2: "P4",
  right_paddle1: "P1",
  right_paddle2: "P2",
};

const xbox: Labels = {
  ...common,
  south: "A",
  east: "B",
  west: "X",
  north: "Y",
  left_shoulder: "LB",
  right_shoulder: "RB",
  left_stick: "LS",
  right_stick: "RS",
  back: "View",
  start: "Menu",
  guide: "Xbox",
  left_trigger: "LT",
  right_trigger: "RT",
};

const playstation: Labels = {
  ...common,
  south: "✕",
  east: "○",
  west: "□",
  north: "△",
  left_shoulder: "L1",
  right_shoulder: "R1",
  left_stick: "L3",
  right_stick: "R3",
  back: "Create",
  start: "Options",
  guide: "PS",
  left_trigger: "L2",
  right_trigger: "R2",
};

// У Nintendo буквы стоят иначе: нижняя кнопка — B, правая — A.
const nintendo: Labels = {
  ...common,
  south: "B",
  east: "A",
  west: "Y",
  north: "X",
  left_shoulder: "L",
  right_shoulder: "R",
  left_stick: "L-стик",
  right_stick: "R-стик",
  back: "−",
  start: "+",
  guide: "Home",
  left_trigger: "ZL",
  right_trigger: "ZR",
};

const byFamily: Record<PadFamily, Labels> = {
  xbox,
  play_station: playstation,
  nintendo,
  generic: xbox,
};

const axisNames: Record<string, [string, string]> = {
  left_x: ["Л.стик →", "Л.стик ←"],
  left_y: ["Л.стик ↑", "Л.стик ↓"],
  right_x: ["П.стик →", "П.стик ←"],
  right_y: ["П.стик ↑", "П.стик ↓"],
};

export function keyLabel(key: SwitchKey, family: PadFamily): string {
  const labels = byFamily[family];
  if (key.kind === "button") return labels[key.button] ?? key.button;
  if (key.axis === "left_trigger" || key.axis === "right_trigger") return labels[key.axis];
  const [pos, neg] = axisNames[key.axis] ?? [key.axis, key.axis];
  return key.direction === "positive" ? pos : neg;
}

export const familyName: Record<PadFamily, string> = {
  xbox: "Xbox",
  play_station: "PlayStation",
  nintendo: "Nintendo",
  generic: "Геймпад",
};

const NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const SOLFEGE = ["до", "до♯", "ре", "ре♯", "ми", "фа", "фа♯", "соль", "соль♯", "ля", "ля♯", "си"];

export function noteName(n: number): string {
  return `${NAMES[n % 12]}${Math.floor(n / 12) - 1}`;
}

export function solfege(n: number): string {
  return SOLFEGE[n % 12];
}
