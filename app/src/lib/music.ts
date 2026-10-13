// Лады и ступени — повторяет engine/src/theory.rs для подписей в редакторе.
import type { Music, ScalePreset } from "./api";

export const SCALE_STEPS: Record<ScalePreset, number[]> = {
  major: [0, 2, 4, 5, 7, 9, 11],
  natural_minor: [0, 2, 3, 5, 7, 8, 10],
  harmonic_minor: [0, 2, 3, 5, 7, 8, 11],
  melodic_minor: [0, 2, 3, 5, 7, 9, 11],
  major_pentatonic: [0, 2, 4, 7, 9],
  minor_pentatonic: [0, 3, 5, 7, 10],
  blues: [0, 3, 5, 6, 7, 10],
  dorian: [0, 2, 3, 5, 7, 9, 10],
  phrygian: [0, 1, 3, 5, 7, 8, 10],
  lydian: [0, 2, 4, 6, 7, 9, 11],
  mixolydian: [0, 2, 4, 5, 7, 9, 10],
  locrian: [0, 1, 3, 5, 6, 8, 10],
  chromatic: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
};

export const SCALE_NAMES: [ScalePreset, string][] = [
  ["major", "Мажор"],
  ["natural_minor", "Минор"],
  ["harmonic_minor", "Гармонический минор"],
  ["melodic_minor", "Мелодический минор"],
  ["major_pentatonic", "Мажорная пентатоника"],
  ["minor_pentatonic", "Минорная пентатоника"],
  ["blues", "Блюз"],
  ["dorian", "Дорийский"],
  ["phrygian", "Фригийский"],
  ["lydian", "Лидийский"],
  ["mixolydian", "Миксолидийский"],
  ["locrian", "Локрийский"],
  ["chromatic", "Хроматика"],
];

export const ROOT_NAMES = ["До", "До♯", "Ре", "Ре♯", "Ми", "Фа", "Фа♯", "Соль", "Соль♯", "Ля", "Ля♯", "Си"];

export function scaleSteps(m: Music): number[] {
  const raw = m.scale.type === "preset" ? SCALE_STEPS[m.scale.preset] : m.scale.steps;
  const steps = [...new Set(raw.map((s) => ((s % 12) + 12) % 12))].sort((a, b) => a - b);
  return steps.length ? steps : SCALE_STEPS.major;
}

export function tonic(m: Music): number {
  return (m.octave + 1) * 12 + m.root;
}

/** MIDI-нота ступени лада (как Music::degree_note в ядре). */
export function degreeNote(m: Music, degree: number): number {
  const steps = scaleSteps(m);
  const n = steps.length;
  const i = ((degree % n) + n) % n;
  return tonic(m) + steps[i] + 12 * Math.floor(degree / n);
}

export const CHORD_DEGREES: Record<string, number[]> = {
  triad: [0, 2, 4],
  seventh: [0, 2, 4, 6],
  power: [0, 4, 7],
};
