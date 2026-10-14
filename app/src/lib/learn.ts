// Что отправить для MIDI Learn: ровно те сообщения, которые шлёт назначение.
import type { Action, Input, Profile } from "./api";
import { sameInput } from "./describe";
import { CHORD_DEGREES, degreeNote, voice } from "./music";

type NoteAction = Extract<Action, { kind: "note" }>;

/** Звуки аккорда — как в ядре (theory::voice). */
export function chordNotes(profile: Profile, note: NoteAction["note"]): number[] {
  if (note.type !== "chord") return [];
  const root = degreeNote(profile.music, note.degree);
  const tones =
    note.chord === "custom"
      ? (note.intervals?.length ? note.intervals : [0, 4, 7]).map((i) => root + i)
      : CHORD_DEGREES[note.chord].map((d) => degreeNote(profile.music, note.degree + d));
  return voice(tones, note.inversion ?? 0, note.open ?? false, note.bass ?? false);
}

/** (задержка мс, байты) для отправки через api.sendTest. Пусто — учить нечего. */
export function learnMessages(action: Action | null | undefined, profile: Profile): [number, number[]][] {
  if (!action) return [];
  const partIndex = action.kind === "note" || action.kind === "control" || action.kind === "cc" ? action.part : 0;
  const part = profile.parts[partIndex];
  const ch = part?.channel ?? 0;
  const shift = part?.transpose ?? 0;
  switch (action.kind) {
    case "note": {
      const n = action.note;
      const notes =
        n.type === "degree"
          ? [degreeNote(profile.music, n.degree) + shift]
          : n.type === "absolute"
            ? [n.note]
            : chordNotes(profile, n).map((x) => x + shift);
      const first = Math.min(127, Math.max(0, notes[0]));
      return [
        [0, [0x90 | ch, first, 100]],
        [250, [0x80 | ch, first, 0]],
      ];
    }
    case "cc":
      return action.step
        ? [[0, [0xb0 | ch, action.cc, 64]]]
        : [
            [0, [0xb0 | ch, action.cc, action.on]],
            [150, [0xb0 | ch, action.cc, action.off]],
          ];
    case "control": {
      const t = action.target;
      if (t.type === "cc")
        return [
          [0, [0xb0 | ch, t.cc, 64]],
          [60, [0xb0 | ch, t.cc, 66]],
        ];
      if (t.type === "pressure") return [[0, [0xd0 | ch, 64]]];
      if (t.type === "pitch_bend")
        return [
          [0, [0xe0 | ch, 0, 0x50]],
          [120, [0xe0 | ch, 0, 0x40]],
        ];
      return [];
    }
    default:
      return [];
  }
}

/** То же для входа: ищем его назначение в базовом слое профиля. */
export function learnForInput(input: Input, profile: Profile): [number, number[]][] {
  const binding = profile.layers[0]?.bindings.find((b) => sameInput(b.input, input));
  return learnMessages(binding?.action, profile);
}
