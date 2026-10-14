// Акцентные цвета: основной (ноты, главные кнопки) и второй (модификаторы, слои).
export interface Theme {
  accent: string;
  secondary: string;
}

export const DEFAULT_THEME: Theme = { accent: "#f2b84b", secondary: "#5cc8e8" };

export const PRESETS: { name: string; theme: Theme }[] = [
  { name: "Янтарь", theme: DEFAULT_THEME },
  { name: "Коралл", theme: { accent: "#ff7a59", secondary: "#62d6c8" } },
  { name: "Роза", theme: { accent: "#ff6b9a", secondary: "#8fb4ff" } },
  { name: "Фиалка", theme: { accent: "#a78bfa", secondary: "#5eead4" } },
  { name: "Небо", theme: { accent: "#4ea8ff", secondary: "#ffb454" } },
  { name: "Мята", theme: { accent: "#3ddc97", secondary: "#c4a1ff" } },
  { name: "Лайм", theme: { accent: "#b8e04a", secondary: "#6ab0ff" } },
  { name: "Графит", theme: { accent: "#e6e6ee", secondary: "#9fb0c8" } },
];

const HEX = /^#[0-9a-f]{6}$/i;

export function validTheme(t: Partial<Theme> | null | undefined): Theme {
  return {
    accent: t?.accent && HEX.test(t.accent) ? t.accent : DEFAULT_THEME.accent,
    secondary: t?.secondary && HEX.test(t.secondary) ? t.secondary : DEFAULT_THEME.secondary,
  };
}

/** Тёмный или светлый текст поверх цвета — по яркости. */
function onColor(hex: string): string {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  const lin = (c: number) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  const lum = 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
  return lum > 0.35 ? "#101015" : "#ffffff";
}

export function applyTheme(theme: Theme) {
  const t = validTheme(theme);
  const root = document.documentElement.style;
  root.setProperty("--accent", t.accent);
  root.setProperty("--on-accent", onColor(t.accent));
  root.setProperty("--mod", t.secondary);
  root.setProperty("--on-mod", onColor(t.secondary));
}
