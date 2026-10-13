// Короткие подписи «что делает кнопка» для схемы геймпада.
import type { ControlView, Input, ModEffect, Music, Profile, SwitchKey } from "./api";
import { noteName } from "./labels";

const ROOTS = ["До", "До♯", "Ре", "Ре♯", "Ми", "Фа", "Фа♯", "Соль", "Соль♯", "Ля", "Ля♯", "Си"];

const SCALES: Record<string, string> = {
  major: "мажор",
  natural_minor: "минор",
  harmonic_minor: "гармонический минор",
  melodic_minor: "мелодический минор",
  major_pentatonic: "мажорная пентатоника",
  minor_pentatonic: "минорная пентатоника",
  blues: "блюз",
  dorian: "дорийский",
  phrygian: "фригийский",
  lydian: "лидийский",
  mixolydian: "миксолидийский",
  locrian: "локрийский",
  chromatic: "хроматика",
};

export function musicName(m: Music): string {
  const scale = m.scale.type === "preset" ? (SCALES[m.scale.preset] ?? m.scale.preset) : "свой лад";
  return `${ROOTS[m.root % 12]} ${scale}`;
}

function signed(n: number): string {
  return n > 0 ? `+${n}` : `−${-n}`;
}

export function effectLabel(e: ModEffect, profile: Profile | null): string {
  switch (e.type) {
    case "semitones":
      return e.semitones === 1 ? "♯ полутон" : e.semitones === -1 ? "♭ полутон" : `${signed(e.semitones)} полутона`;
    case "octaves":
      return `октава ${signed(e.octaves)}`;
    case "degrees":
      return `ступень ${signed(e.degrees)}`;
    case "layer":
      return profile?.layers.find((l) => l.id === e.layer)?.name.toLowerCase() ?? e.layer;
    case "velocity":
      return `сила ${e.velocity}`;
    case "sustain":
      return "педаль";
  }
}

const CC_NAMES: Record<number, string> = {
  1: "вибрато",
  2: "дыхание",
  7: "громкость",
  10: "панорама",
  11: "громкость",
  64: "педаль",
  71: "резонанс",
  74: "яркость",
  91: "реверб",
  93: "хорус",
};

/** Подпись роли: для ноты — её имя, для остального — что делает. */
export function roleLabel(v: ControlView, profile: Profile | null): string {
  switch (v.role) {
    case "note":
      return v.notes.length ? noteName(v.notes[0]) : "—";
    case "modifier":
      return effectLabel(v.effect, profile);
    case "command":
      switch (v.command.type) {
        case "next_instrument":
          return "инструмент →";
        case "prev_instrument":
          return "← инструмент";
        case "toggle_record":
          return "запись";
        case "panic":
          return "тишина";
      }
      break;
    case "control":
      switch (v.target.type) {
        case "pitch_bend":
          return "подтяжка";
        case "cc":
          return CC_NAMES[v.target.cc] ?? `CC ${v.target.cc}`;
        case "velocity":
          return "сила";
      }
  }
  return "";
}

export function sameInput(a: Input, b: Input): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind === "button" && b.kind === "button") return a.button === b.button;
  if (a.kind === "axis" && b.kind === "axis") return a.axis === b.axis;
  if (a.kind === "axis_switch" && b.kind === "axis_switch") return a.axis === b.axis && a.direction === b.direction;
  return false;
}

export function sameKey(a: SwitchKey, b: SwitchKey): boolean {
  if (a.kind === "button" && b.kind === "button") return a.button === b.button;
  if (a.kind === "axis_switch" && b.kind === "axis_switch") return a.axis === b.axis && a.direction === b.direction;
  return false;
}
