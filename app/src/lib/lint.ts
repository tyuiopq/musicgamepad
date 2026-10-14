// Проверка раскладки: то, что скорее всего работает не так, как задумано.
import type { Binding, PadFamily, Profile } from "./api";
import { inputName } from "./labels";
import { DRUM_CHANNEL } from "./sound";

export interface Issue {
  level: "warn" | "info";
  text: string;
  /** Слой, к которому относится замечание (чтобы перейти к нему). */
  layer?: string;
}

/** Какой (канал, CC) задаёт назначение — для поиска «спорящих» за один CC. */
function ccOf(b: Binding, profile: Profile): string | null {
  const a = b.action;
  const ch = (part: number) => (profile.parts[part]?.channel ?? 0) + 1;
  if (a.kind === "cc") return `канал ${ch(a.part)}, CC${a.cc}`;
  if (a.kind === "control" && a.target.type === "cc") return `канал ${ch(a.part)}, CC${a.target.cc}`;
  if (a.kind === "control" && a.target.type === "pitch_bend") return `канал ${ch(a.part)}, подтяжка`;
  return null;
}

export function lintProfile(profile: Profile, family: PadFamily): Issue[] {
  const issues: Issue[] = [];
  const reached = new Set<string>(["base"]);
  for (const l of profile.layers) {
    for (const b of l.bindings) {
      if (b.action.kind === "modifier" && b.action.effect.type === "layer") reached.add(b.action.effect.layer);
    }
  }

  for (const layer of profile.layers) {
    const where = layer.id === "base" ? "" : ` (слой «${layer.name}»)`;
    // 1. Слой ничем не включается
    if (layer.id !== "base" && !reached.has(layer.id) && !layer.combo?.length) {
      issues.push({ level: "warn", text: `Слой «${layer.name}» ничем не включается — назначь модификатор «Слой» или сделай его комбинацией.`, layer: layer.id });
    }
    if (layer.id !== "base" && !layer.bindings.length) {
      issues.push({ level: "info", text: `Слой «${layer.name}» пуст — в нём действуют назначения основного слоя.`, layer: layer.id });
    }
    // 2. Ось одновременно плавный контроллер и кнопка
    const axes = new Set(layer.bindings.filter((b) => b.input.kind === "axis").map((b) => (b.input as { axis: string }).axis));
    for (const b of layer.bindings) {
      if (b.input.kind === "axis_switch" && axes.has(b.input.axis)) {
        issues.push({
          level: "info",
          text: `${inputName(b.input, family)}: и плавный контроллер, и кнопка с порогом — при сильном нажатии сработают оба${where}.`,
          layer: layer.id,
        });
      }
    }
    // 3. Несколько входов задают один и тот же CC
    const byCc = new Map<string, string[]>();
    for (const b of layer.bindings) {
      const key = ccOf(b, profile);
      if (key) byCc.set(key, [...(byCc.get(key) ?? []), inputName(b.input, family)]);
    }
    for (const [key, inputs] of byCc) {
      if (inputs.length > 1) {
        issues.push({ level: "warn", text: `${key}: задают сразу ${inputs.join(", ")} — значения будут перебивать друг друга${where}.`, layer: layer.id });
      }
    }
    for (const b of layer.bindings) {
      const a = b.action;
      // 4. Смена инструмента без быстрого списка
      if (a.kind === "command" && (a.command.type === "next_instrument" || a.command.type === "prev_instrument")) {
        const part = profile.parts[a.command.part];
        if (part && !part.instruments.length) {
          issues.push({
            level: "warn",
            text: `${inputName(b.input, family)} листает инструменты партии «${part.name}», но её быстрый список пуст — отметь инструменты ★ во вкладке «Звук».`,
            layer: layer.id,
          });
        }
      }
      // 5. Ступени лада на ударной партии
      if (a.kind === "note" && a.note.type !== "absolute" && profile.parts[a.part]?.channel === DRUM_CHANNEL) {
        issues.push({
          level: "info",
          text: `${inputName(b.input, family)} играет ступень лада на ударной партии — обычно барабанам нужна конкретная нота${where}.`,
          layer: layer.id,
        });
      }
      // 6. Ссылка на несуществующую партию
      if ((a.kind === "note" || a.kind === "control" || a.kind === "cc") && !profile.parts[a.part]) {
        issues.push({ level: "warn", text: `${inputName(b.input, family)} ссылается на удалённую партию${where}.`, layer: layer.id });
      }
    }
  }

  // 7. Две партии на одном канале
  const seen = new Map<number, string>();
  for (const p of profile.parts) {
    const other = seen.get(p.channel);
    if (other) issues.push({ level: "warn", text: `Партии «${other}» и «${p.name}» на одном канале ${p.channel + 1} — инструменты будут мешать друг другу.` });
    seen.set(p.channel, p.name);
  }
  return issues;
}
