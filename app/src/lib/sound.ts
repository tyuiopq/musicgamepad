// Партии и инструменты: группы General MIDI, названия ударных, правки партий профиля.
import type { Part, PresetInfo, Profile } from "./api";

export const DRUM_CHANNEL = 9;
/** В банках SoundFont ударные установки лежат в банке 128. */
export const DRUM_BANK = 128;

/** Группы General MIDI: по 8 программ в каждой. */
export const GM_FAMILIES = [
  "Фортепиано",
  "Хроматическая перкуссия",
  "Органы",
  "Гитары",
  "Басы",
  "Струнные",
  "Ансамбли и хоры",
  "Медные",
  "Язычковые",
  "Флейты",
  "Синтезатор: соло",
  "Синтезатор: пэды",
  "Синтезатор: эффекты",
  "Этнические",
  "Перкуссия",
  "Звуковые эффекты",
];

const DRUMS: Record<number, string> = {
  35: "Бочка 2",
  36: "Бочка",
  37: "Римшот",
  38: "Малый барабан",
  39: "Хлопок",
  40: "Малый 2",
  41: "Том низкий 2",
  42: "Хэт закрытый",
  43: "Том низкий",
  44: "Хэт педалью",
  45: "Том средний 2",
  46: "Хэт открытый",
  47: "Том средний",
  48: "Том высокий 2",
  49: "Крэш",
  50: "Том высокий",
  51: "Райд",
  52: "Чайна",
  53: "Райд, купол",
  54: "Бубен",
  55: "Сплэш",
  56: "Ковбел",
  57: "Крэш 2",
  58: "Вибраслэп",
  59: "Райд 2",
  60: "Бонго высокий",
  61: "Бонго низкий",
  62: "Конга, глушёная",
  63: "Конга высокая",
  64: "Конга низкая",
  65: "Тимбалес высокий",
  66: "Тимбалес низкий",
  67: "Агого высокий",
  68: "Агого низкий",
  69: "Кабаса",
  70: "Маракасы",
  71: "Свисток короткий",
  72: "Свисток длинный",
  73: "Гуиро короткий",
  74: "Гуиро длинный",
  75: "Клавес",
  76: "Вуд-блок высокий",
  77: "Вуд-блок низкий",
  78: "Куика, глушёная",
  79: "Куика, открытая",
  80: "Треугольник, глушёный",
  81: "Треугольник, открытый",
};

export function drumName(note: number): string | null {
  return DRUMS[note] ?? null;
}

export const isDrums = (p: Part | undefined) => p?.channel === DRUM_CHANNEL;

export function presetName(presets: PresetInfo[], bank: number, program: number): string {
  const p = presets.find((x) => x.bank === bank && x.program === program) ?? presets.find((x) => x.bank === 0 && x.program === program);
  return p?.name ?? `Программа ${program + 1}`;
}

export interface PresetGroup {
  title: string;
  presets: PresetInfo[];
}

/** Инструменты для партии: у ударной — установки, у мелодической — по группам GM, затем вариации. */
export function groupPresets(presets: PresetInfo[], drums: boolean, query: string): PresetGroup[] {
  const q = query.trim().toLowerCase();
  const match = (p: PresetInfo) => !q || p.name.toLowerCase().includes(q) || String(p.program + 1) === q;
  if (drums) {
    return [{ title: "Ударные установки", presets: presets.filter((p) => p.bank === DRUM_BANK && match(p)) }].filter(
      (g) => g.presets.length,
    );
  }
  const groups: PresetGroup[] = GM_FAMILIES.map((title, i) => ({
    title,
    presets: presets.filter((p) => p.bank === 0 && Math.floor(p.program / 8) === i && match(p)),
  }));
  groups.push({
    title: "Вариации",
    presets: presets.filter((p) => p.bank !== 0 && p.bank !== DRUM_BANK && match(p)),
  });
  return groups.filter((g) => g.presets.length);
}

export function defaultSound(): Part["sound"] {
  return { pan: 64, reverb: 40, chorus: 0, attack: 64, release: 64, brightness: 64, resonance: 64 };
}

function freeChannel(profile: Profile): number | null {
  const used = new Set(profile.parts.map((p) => p.channel));
  for (let ch = 0; ch < 16; ch++) if (ch !== DRUM_CHANNEL && !used.has(ch)) return ch;
  return null;
}

export const canAddPart = (profile: Profile) => freeChannel(profile) !== null;
export const hasDrums = (profile: Profile) => profile.parts.some(isDrums);

export function addPart(profile: Profile, drums: boolean): number {
  const channel = drums ? DRUM_CHANNEL : freeChannel(profile);
  if (channel === null) return -1;
  profile.parts.push({
    name: drums ? "Ударные" : `Партия ${profile.parts.length + 1}`,
    channel,
    program: 0,
    bank: drums ? DRUM_BANK : 0,
    volume: 100,
    bend_range: 2,
    instruments: [],
    transpose: 0,
    sound: defaultSound(),
  });
  return profile.parts.length - 1;
}

/** Удаляет партию; назначения на неё переходят на первую партию, номера остальных сдвигаются. */
export function removePart(profile: Profile, index: number) {
  if (profile.parts.length <= 1) return;
  profile.parts.splice(index, 1);
  const remap = (part: number) => (part === index ? 0 : part > index ? part - 1 : part);
  for (const layer of profile.layers) {
    for (const b of layer.bindings) {
      const a = b.action;
      if (a.kind === "note" || a.kind === "control") a.part = remap(a.part);
      if (a.kind === "command" && (a.command.type === "next_instrument" || a.command.type === "prev_instrument")) {
        a.command.part = remap(a.command.part);
      }
    }
  }
}

/** Сколько назначений играет этой партией. */
export function partUsage(profile: Profile, index: number): number {
  return profile.layers
    .flatMap((l) => l.bindings)
    .filter((b) => (b.action.kind === "note" || b.action.kind === "control") && b.action.part === index).length;
}
