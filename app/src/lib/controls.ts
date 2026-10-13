// Контролы геймпада для редактора и их «слоты» — входы, на которые можно что-то назначить.
import type { Direction, Input, SwitchKey } from "./api";

export type ControlId =
  | { kind: "button"; button: string }
  | { kind: "trigger"; axis: "left_trigger" | "right_trigger" }
  | { kind: "stick"; side: "left" | "right" };

export interface Slot {
  /** Подпись слота в инспекторе. */
  title: string;
  /** Шаблон входа; у кнопки-на-оси порог берётся из назначения или по умолчанию. */
  input: Input;
  /** Плавный контроллер (ось) или «кнопочный» вход. */
  continuous: boolean;
}

export const DEFAULT_THRESHOLD = 0.5;

const axisSwitch = (axis: string, direction: Direction): Input => ({
  kind: "axis_switch",
  axis,
  direction,
  threshold: DEFAULT_THRESHOLD,
});

export function slotsOf(c: ControlId): Slot[] {
  switch (c.kind) {
    case "button":
      return [{ title: "Нажатие", input: { kind: "button", button: c.button }, continuous: false }];
    case "trigger":
      return [
        { title: "Как кнопка (нажать сильнее порога)", input: axisSwitch(c.axis, "positive"), continuous: false },
        { title: "Как плавный контроллер", input: { kind: "axis", axis: c.axis }, continuous: true },
      ];
    case "stick": {
      const x = `${c.side}_x`;
      const y = `${c.side}_y`;
      return [
        { title: "Нажатие стика", input: { kind: "button", button: `${c.side}_stick` }, continuous: false },
        { title: "Влево-вправо", input: { kind: "axis", axis: x }, continuous: true },
        { title: "Вверх-вниз", input: { kind: "axis", axis: y }, continuous: true },
        { title: "Наклон вверх", input: axisSwitch(y, "positive"), continuous: false },
        { title: "Наклон вниз", input: axisSwitch(y, "negative"), continuous: false },
        { title: "Наклон влево", input: axisSwitch(x, "negative"), continuous: false },
        { title: "Наклон вправо", input: axisSwitch(x, "positive"), continuous: false },
      ];
    }
  }
}

/** К какому контролу относится касание с геймпада. */
export function controlOfKey(key: SwitchKey): ControlId {
  if (key.kind === "button") {
    if (key.button === "left_stick") return { kind: "stick", side: "left" };
    if (key.button === "right_stick") return { kind: "stick", side: "right" };
    return { kind: "button", button: key.button };
  }
  if (key.axis === "left_trigger" || key.axis === "right_trigger") return { kind: "trigger", axis: key.axis };
  return { kind: "stick", side: key.axis.startsWith("left") ? "left" : "right" };
}

export function sameControl(a: ControlId | null, b: ControlId | null): boolean {
  if (!a || !b || a.kind !== b.kind) return false;
  if (a.kind === "button" && b.kind === "button") return a.button === b.button;
  if (a.kind === "trigger" && b.kind === "trigger") return a.axis === b.axis;
  if (a.kind === "stick" && b.kind === "stick") return a.side === b.side;
  return false;
}

/** Кнопки, которых нет на схеме, — выбираются списком. */
export const EXTRA_BUTTONS = ["guide", "touchpad", "misc1", "left_paddle1", "left_paddle2", "right_paddle1", "right_paddle2"];
