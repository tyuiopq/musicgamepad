<script lang="ts">
  // Вкладка «Звук»: партии профиля, их инструменты и звучание.
  import type { Part, PresetInfo, Profile, Sound } from "$lib/api";
  import { addPart, canAddPart, defaultSound, hasDrums, isDrums, partUsage, presetName, removePart } from "$lib/sound";
  import InstrumentBrowser from "./InstrumentBrowser.svelte";

  let {
    profile,
    presets,
    onchange,
    onaudition,
  }: {
    profile: Profile;
    presets: PresetInfo[];
    onchange: () => void;
    onaudition: (part: number) => void;
  } = $props();

  let index = $state(0);
  const part = $derived(profile.parts[Math.min(index, profile.parts.length - 1)]);
  let confirmRemove = $state(false);

  function edit(fn: (p: Part) => void) {
    fn(part);
    onchange();
  }

  function setSound<K extends keyof Sound>(key: K, value: Sound[K]) {
    edit((p) => (p.sound[key] = value));
  }

  function add(drums: boolean) {
    const i = addPart(profile, drums);
    if (i >= 0) {
      index = i;
      onchange();
    }
  }

  function remove() {
    if (!confirmRemove) {
      confirmRemove = true;
      return;
    }
    removePart(profile, index);
    index = 0;
    confirmRemove = false;
    onchange();
  }

  function pick(p: PresetInfo) {
    edit((x) => {
      x.bank = p.bank;
      x.program = p.program;
    });
    onaudition(index);
  }

  function star(program: number) {
    edit((x) => {
      x.instruments = x.instruments.includes(program)
        ? x.instruments.filter((i) => i !== program)
        : [...x.instruments, program].sort((a, b) => a - b);
    });
  }

  // ---------- подписи ----------
  const factor = (v: number) => Math.pow(2, ((v - 64) / 64) * 3);
  function timeText(v: number): string {
    if (v === 64) return "как в банке";
    const f = factor(v);
    return f > 1 ? `×${f.toFixed(1)} дольше` : `×${(1 / f).toFixed(1)} короче`;
  }
  function brightText(v: number): string {
    if (v === 64) return "как в банке";
    const oct = ((v - 64) / 64) * 3;
    return `${oct > 0 ? "+" : "−"}${Math.abs(oct).toFixed(1)} окт.`;
  }
  function resonanceText(v: number): string {
    if (v === 64) return "как в банке";
    const db = ((v - 64) / 64) * 12;
    return `${db > 0 ? "+" : "−"}${Math.abs(db).toFixed(0)} дБ`;
  }
  function panText(v: number): string {
    if (v === 64) return "центр";
    return v < 64 ? `левее ${Math.round(((64 - v) / 64) * 100)}%` : `правее ${Math.round(((v - 64) / 63) * 100)}%`;
  }
  const percent = (v: number) => `${Math.round((v / 127) * 100)}%`;

  const SLIDERS: { key: keyof Sound; label: string; text: (v: number) => string }[] = [
    { key: "pan", label: "Панорама", text: panText },
    { key: "reverb", label: "Реверберация", text: percent },
    { key: "chorus", label: "Хорус", text: percent },
    { key: "attack", label: "Атака", text: timeText },
    { key: "release", label: "Затухание", text: timeText },
    { key: "brightness", label: "Яркость", text: brightText },
    { key: "resonance", label: "Резонанс", text: resonanceText },
  ];
</script>

{#snippet slider(label: string, value: number, set: (v: number) => void, text: string, min = 0, max = 127)}
  <label class="slider">
    <span>{label}</span>
    <input type="range" {min} {max} {value} oninput={(e) => set(Number(e.currentTarget.value))} />
    <b>{text}</b>
  </label>
{/snippet}

{#snippet stepper(label: string, value: number, min: number, max: number, set: (v: number) => void, text: string)}
  <div class="stepper">
    <span>{label}</span>
    <button type="button" aria-label="Меньше" disabled={value <= min} onclick={() => set(value - 1)}>−</button>
    <b>{text}</b>
    <button type="button" aria-label="Больше" disabled={value >= max} onclick={() => set(value + 1)}>+</button>
  </div>
{/snippet}

<div class="sound">
  <aside class="parts">
    <div class="section-title">Партии</div>
    {#each profile.parts as p, i (i)}
      <button type="button" class="part" class:on={i === index} aria-pressed={i === index} onclick={() => ((index = i), (confirmRemove = false))}>
        <b>{p.name}</b>
        <span>{presetName(presets, p.bank, p.program)}</span>
        <small>{isDrums(p) ? "ударные" : `канал ${p.channel + 1}`} · назначений: {partUsage(profile, i)}</small>
      </button>
    {/each}
    <div class="add">
      <button type="button" class="ghost" disabled={!canAddPart(profile)} onclick={() => add(false)}>+ Партия</button>
      {#if !hasDrums(profile)}
        <button type="button" class="ghost" onclick={() => add(true)}>+ Ударные</button>
      {/if}
    </div>
    <p class="hint">Партии звучат одновременно. Какой партией играет кнопка — выбирается во вкладке «Кнопки».</p>
  </aside>

  {#if part}
    <section class="settings">
      <div class="head">
        <input aria-label="Название партии" value={part.name} oninput={(e) => edit((p) => (p.name = e.currentTarget.value))} maxlength="30" />
        <button type="button" class="primary" onclick={() => onaudition(index)}>▶ Послушать</button>
      </div>
      <p class="instrument">{presetName(presets, part.bank, part.program)}</p>
      <label class="channel">
        <span>MIDI-канал</span>
        <select
          value={part.channel}
          onchange={(e) => {
            const ch = Number(e.currentTarget.value);
            edit((p) => {
              p.channel = ch;
              if (ch === 9 && p.bank !== 128) p.bank = 128;
              if (ch !== 9 && p.bank === 128) p.bank = 0;
            });
          }}
        >
          {#each Array.from({ length: 16 }, (_, i) => i) as ch (ch)}
            {@const owner = profile.parts.find((x, j) => x.channel === ch && j !== index)}
            <option value={ch}>{ch + 1}{ch === 9 ? " — ударные (GM)" : ""}{owner ? ` — занят: ${owner.name}` : ""}</option>
          {/each}
        </select>
      </label>
      {#if profile.parts.some((x, j) => x.channel === part.channel && j !== index)}
        <p class="hint warn">На этом канале уже есть партия: инструменты будут мешать друг другу.</p>
      {/if}

      {@render slider("Громкость", part.volume, (v) => edit((p) => (p.volume = v)), percent(part.volume))}
      {#each SLIDERS as s (s.key)}
        {@render slider(s.label, part.sound[s.key], (v) => setSound(s.key, v), s.text(part.sound[s.key]))}
      {/each}
      <button type="button" class="link" onclick={() => edit((p) => (p.sound = defaultSound()))}>Сбросить звучание</button>

      {#if !isDrums(part)}
        {@render stepper(
          "Транспонирование",
          part.transpose,
          -24,
          24,
          (v) => edit((p) => (p.transpose = v)),
          part.transpose === 0 ? "нет" : `${part.transpose > 0 ? "+" : "−"}${Math.abs(part.transpose)} пт.`,
        )}
        {@render stepper("Диапазон подтяжки", part.bend_range, 1, 24, (v) => edit((p) => (p.bend_range = v)), `±${part.bend_range} пт.`)}
      {/if}

      {#if profile.parts.length > 1}
        <button type="button" class="ghost danger" onclick={remove}>
          {confirmRemove ? `Точно удалить? Назначения перейдут на «${profile.parts[index === 0 ? 1 : 0].name}»` : "Удалить партию"}
        </button>
      {/if}
    </section>

    <section class="instruments">
      <div class="section-title">Инструмент</div>
      <InstrumentBrowser {presets} {part} onpick={pick} onstar={star} />
      {#if !isDrums(part)}
        <p class="hint">
          ★ — быстрый список: по нему ходит команда «следующий инструмент»{part.instruments.length
            ? ` (сейчас ${part.instruments.length})`
            : ""}.
        </p>
      {/if}
    </section>
  {/if}
</div>

<style>
  .sound {
    display: grid;
    grid-template-columns: minmax(200px, 240px) minmax(280px, 1fr) minmax(260px, 1fr);
    gap: 20px;
    align-items: start;
  }
  @media (max-width: 1100px) {
    .sound {
      grid-template-columns: minmax(200px, 240px) minmax(0, 1fr);
    }
    .instruments {
      grid-column: 1 / -1;
    }
  }
  .parts,
  .settings,
  .instruments {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .settings,
  .instruments {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 14px;
    padding: 16px;
  }
  .part {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid var(--line-strong);
    background: transparent;
    text-align: left;
  }
  .part span {
    font-size: 13px;
  }
  .part small {
    font-size: 12px;
    color: var(--muted);
  }
  .part.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .add {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .head {
    display: flex;
    gap: 10px;
  }
  .head input {
    flex: 1;
    min-width: 0;
    font: inherit;
    font-weight: 600;
    color: var(--text);
    min-height: 36px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
  }
  .instrument {
    margin: 0;
    font-family: var(--font-display);
    font-size: 18px;
    color: var(--accent);
  }
  .channel {
    display: grid;
    grid-template-columns: 120px 1fr;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    color: var(--muted);
  }
  .channel select {
    font: inherit;
    color: var(--text);
    min-height: 34px;
    padding: 0 8px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
  }
  .warn {
    color: var(--error);
  }
  .slider {
    display: grid;
    grid-template-columns: 120px 1fr 104px;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    color: var(--muted);
  }
  .slider input {
    accent-color: var(--accent);
    min-width: 0;
  }
  .slider b {
    color: var(--text);
    font-weight: 600;
    text-align: right;
    white-space: nowrap;
  }
  .stepper {
    display: grid;
    grid-template-columns: 120px 36px 1fr 36px;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--muted);
  }
  .stepper b {
    text-align: center;
    color: var(--text);
  }
  .stepper button {
    width: 36px;
    height: 36px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
    font-size: 18px;
  }
  .stepper button:disabled {
    opacity: 0.35;
    cursor: default;
  }
  button.ghost,
  button.primary {
    min-height: 36px;
    padding: 0 14px;
    border-radius: 10px;
    border: 1px solid var(--line-strong);
    background: transparent;
  }
  button.ghost:disabled {
    opacity: 0.4;
    cursor: default;
  }
  button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--bg);
    font-weight: 600;
    white-space: nowrap;
  }
  button.danger {
    color: var(--error);
  }
  .link {
    align-self: flex-start;
    background: none;
    border: none;
    padding: 0;
    color: var(--mod);
    font-size: 13px;
  }
  .hint {
    margin: 0;
    font-size: 13px;
    color: var(--muted);
  }
</style>
