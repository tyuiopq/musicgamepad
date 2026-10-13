// Правки профиля в редакторе. Все функции меняют профиль на месте (он — $state).
import type { Action, Binding, Input, Layer, Profile } from "./api";
import { sameInput } from "./describe";

export type Role = "none" | "note" | "chord" | "modifier" | "command" | "control";

export function findBinding(layer: Layer, input: Input): Binding | undefined {
  return layer.bindings.find((b) => sameInput(b.input, input));
}

/** Назначить `action` на `input` в слое; `null` — убрать назначение. */
export function setBinding(layer: Layer, input: Input, action: Action | null) {
  const i = layer.bindings.findIndex((b) => sameInput(b.input, input));
  if (action === null) {
    if (i >= 0) layer.bindings.splice(i, 1);
  } else if (i >= 0) {
    layer.bindings[i] = { input, action };
  } else {
    layer.bindings.push({ input, action });
  }
}

export function roleOf(action: Action | null | undefined): Role {
  if (!action) return "none";
  if (action.kind === "note") return action.note.type === "chord" ? "chord" : "note";
  return action.kind;
}

const isTrigger = (input: Input) =>
  (input.kind === "axis" || input.kind === "axis_switch") && input.axis.endsWith("trigger");

/** Разумное назначение по умолчанию при выборе роли. */
export function defaultAction(role: Role, input: Input): Action | null {
  switch (role) {
    case "none":
      return null;
    case "note":
      return { kind: "note", note: { type: "degree", degree: 0 }, part: 0, velocity: 100 };
    case "chord":
      return { kind: "note", note: { type: "chord", degree: 0, chord: "triad" }, part: 0, velocity: 90 };
    case "modifier":
      return { kind: "modifier", effect: { type: "semitones", semitones: 1 }, latch: "hold" };
    case "command":
      return { kind: "command", command: { type: "next_instrument", part: 0 } };
    case "control":
      return {
        kind: "control",
        target: { type: "cc", cc: 1, min: 0, max: 127 },
        shape: isTrigger(input)
          ? { deadzone: 0.02, curve: 1, invert: false, mode: "positive" }
          : { deadzone: 0.15, curve: 1, invert: false, mode: "bipolar" },
        part: 0,
      };
  }
}

export function addLayer(profile: Profile): Layer {
  let n = profile.layers.length;
  let id = `layer${n}`;
  while (profile.layers.some((l) => l.id === id)) id = `layer${++n}`;
  const layer: Layer = { id, name: `Слой ${n}`, bindings: [] };
  profile.layers.push(layer);
  return layer;
}

/** Удаляет слой и модификаторы, которые его включали. */
export function deleteLayer(profile: Profile, id: string) {
  profile.layers = profile.layers.filter((l) => l.id !== id);
  for (const layer of profile.layers) {
    layer.bindings = layer.bindings.filter(
      (b) => !(b.action.kind === "modifier" && b.action.effect.type === "layer" && b.action.effect.layer === id),
    );
  }
}

/** Кто включает слой — чтобы подсказать, как в него попасть. */
export function layerTriggers(profile: Profile, id: string): Binding[] {
  return profile.layers.flatMap((l) =>
    l.bindings.filter((b) => b.action.kind === "modifier" && b.action.effect.type === "layer" && b.action.effect.layer === id),
  );
}
