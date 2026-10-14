<script lang="ts">
  // Вкладка «Звук»: партии профиля, их инструменты и звучание.
  import { onMount } from "svelte";
  import { api, type BankEntry, type Input, type Part, type PresetInfo, type Profile, type SamplePad, type Sound, type MidiStatus } from "$lib/api";
  import { noteName } from "$lib/labels";
  import { setBinding } from "$lib/profileEdit.svelte";
  import { addPart, canAddPart, defaultSound, drumName, hasDrums, isDrums, partUsage, presetName, removePart } from "$lib/sound";
  import InstrumentBrowser from "./InstrumentBrowser.svelte";

  let {
    profile,
    presets,
    onchange,
    onaudition,
    ontest,
    midi = null,
  }: {
    profile: Profile;
    /** Состояние MIDI-выходов — показать, подключился ли выход партии. */
    midi?: MidiStatus | null;
    presets: PresetInfo[];
    onchange: () => void;
    onaudition: (part: number) => void;
    /** Отправить сообщения (после применения свежих правок). */
    ontest: (msgs: [number, number[]][]) => void;
  } = $props();

  let index = $state(0);
  const part = $derived(profile.parts[Math.min(index, profile.parts.length - 1)]);
  let confirmRemove = $state(false);

  // ---------- источник звука партии ----------
  let banks = $state<BankEntry[]>([]);
  let partPresets = $state<PresetInfo[]>([]);
  let loadError = $state<string | null>(null);
  let ports = $state<string[]>([]);
  onMount(() => {
    api.soundLibrary().then((b) => (banks = b));
    api.midiPorts().then((p) => (ports = p));
  });

  const source = $derived(part?.source ?? { type: "default" as const });
  const fontFile = $derived(source.type === "sound_font" ? source.file : null);

  // Инструменты банка выбранной партии
  $effect(() => {
    const file = fontFile;
    loadError = null;
    if (!file) {
      partPresets = presets;
      return;
    }
    api.presetsFor(file).then(
      (p) => (partPresets = p),
      (e) => {
        partPresets = [];
        loadError = String(e);
      },
    );
  });

  function setSource(type: "default" | "sound_font" | "samples") {
    edit((p) => {
      if (type === "default") delete p.source;
      else if (type === "sound_font") p.source = { type: "sound_font", file: banks[0]?.file ?? "" };
      else p.source = { type: "samples", pads: [] };
    });
  }

  async function addBank() {
    const file = await api.addSoundfont();
    if (!file) return;
    banks = await api.soundLibrary();
    edit((p) => (p.source = { type: "sound_font", file }));
  }

  const fileName = (f: string) => f.split(/[\\/]/).pop() ?? f;
  const pads = $derived(source.type === "samples" ? source.pads : []);

  function editPads(fn: (pads: SamplePad[]) => void) {
    edit((p) => {
      if (p.source?.type === "samples") fn(p.source.pads);
    });
  }

  /** Следующая свободная нота: с 36, как у барабанов. */
  function freeNote(list: SamplePad[]): number {
    let n = 36;
    while (list.some((x) => x.note === n) && n < 127) n++;
    return n;
  }

  async function addSample() {
    const file = await api.chooseSample();
    if (file) editPads((list) => list.push({ note: freeNote(list), file, gain_db: 0, pitch: 0, choke: 0 }));
  }

  async function addKit() {
    const files = await api.kitFromFolder();
    if (files.length) editPads((list) => files.forEach((file) => list.push({ note: freeNote(list), file, gain_db: 0, pitch: 0, choke: 0 })));
  }

  function testPad(note: number) {
    const ch = part.channel;
    ontest([
      [0, [0x90 | ch, note, 110]],
      [300, [0x80 | ch, note, 0]],
    ]);
  }

  /** Первые 8 пэдов — на четыре кнопки и крестовину основного слоя. */
  const LAYOUT: Input[] = ["south", "east", "west", "north", "dpad_left", "dpad_down", "dpad_right", "dpad_up"].map(
    (button) => ({ kind: "button", button }),
  );
  function layoutPads() {
    const base = profile.layers[0];
    pads.slice(0, 8).forEach((pad, i) => {
      setBinding(base, LAYOUT[i], { kind: "note", note: { type: "absolute", note: pad.note }, part: index, velocity: 110, latch: "hold" });
    });
    onchange();
    laidOut = true;
  }
  let laidOut = $state(false);

  function partLabel(p: Part): string {
    if (p.source?.type === "samples") return `Сэмплы (${p.source.pads.length})`;
    if (p.source?.type === "sound_font") return `${fileName(p.source.file)} · ${p.program + 1}`;
    return presetName(presets, p.bank, p.program);
  }


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
        <span>{partLabel(p)}</span>
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
    <section class="settings" class:wide={source.type === "samples"}>
      <div class="head">
        <input aria-label="Название партии" value={part.name} oninput={(e) => edit((p) => (p.name = e.currentTarget.value))} maxlength="30" />
        <button type="button" class="primary" onclick={() => onaudition(index)}>▶ Послушать</button>
      </div>
      <p class="instrument">{source.type === "samples" ? `Сэмплы: ${pads.length}` : presetName(partPresets, part.bank, part.program)}</p>
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
      <label class="channel" title="Партию можно отправить в другой порт — например, бас на аппаратный синтезатор, а клавиши в DAW">
        <span>MIDI-выход</span>
        <select
          value={part.port ?? ""}
          onchange={(e) => {
            const v = e.currentTarget.value;
            edit((p) => {
              if (v) p.port = v;
              else delete p.port;
            });
          }}
        >
          <option value="">Основной (из «Настроек»)</option>
          {#each ports as p (p)}<option value={p}>{p}</option>{/each}
          {#if part.port && !ports.includes(part.port)}<option value={part.port}>{part.port} — не найден</option>{/if}
        </select>
      </label>
      {#if part.port && midi?.extra?.find((x) => x.port === part.port)?.error}
        <p class="hint warn">{midi.extra.find((x) => x.port === part.port)?.error}</p>
      {/if}

      <div class="source">
        <span class="section-title">Звук партии</span>
        <div class="seg">
          {#each [["default", "Основной банк"], ["sound_font", "Свой .sf2"], ["samples", "Сэмплы"]] as [t, text] (t)}
            <button type="button" class:on={source.type === t} aria-pressed={source.type === t} onclick={() => setSource(t as "default" | "sound_font" | "samples")}>
              {text}
            </button>
          {/each}
        </div>
        {#if source.type === "sound_font"}
          <div class="row">
            <select
              aria-label="Банк"
              value={source.file}
              onchange={(e) => {
                const file = e.currentTarget.value;
                edit((p) => (p.source = { type: "sound_font", file }));
              }}
            >
              {#if !banks.some((b) => b.file === source.file)}<option value={source.file}>{fileName(source.file) || "— выбери банк —"}</option>{/if}
              {#each banks as b (b.file)}<option value={b.file}>{b.name}</option>{/each}
            </select>
            <button type="button" class="ghost" onclick={addBank}>Добавить .sf2…</button>
          </div>
          {#if loadError}<p class="hint warn">{loadError}</p>{/if}
          {#if !banks.length}
            <p class="hint">В библиотеке пока нет своих банков — добавь .sf2 кнопкой выше (файл скопируется в библиотеку).</p>
          {/if}
        {:else if source.type === "samples"}
          <p class="hint">One-shot: звук играет до конца. Группа глушения: новый звук группы глушит прежний (открытый хэт — закрытый).</p>
          <div class="pads">
            {#each pads as pad, i (i)}
              <div class="pad">
                <button type="button" class="play" aria-label="Послушать" onclick={() => testPad(pad.note)}>▶</button>
                <label class="note">
                  <input
                    type="number"
                    min="0"
                    max="127"
                    value={pad.note}
                    aria-label="Нота"
                    onchange={(e) => editPads((l) => (l[i].note = Math.max(0, Math.min(127, Number(e.currentTarget.value)))))}
                  />
                  <small>{(isDrums(part) && drumName(pad.note)) || noteName(pad.note)}</small>
                </label>
                <span class="file" title={pad.file}>{fileName(pad.file)}</span>
                <button type="button" class="remove" aria-label="Убрать" onclick={() => editPads((l) => l.splice(i, 1))}>✕</button>
                <div class="opts">
                <label class="mini" title="Громкость, дБ">
                  <input type="range" min="-24" max="6" step="1" value={pad.gain_db} oninput={(e) => editPads((l) => (l[i].gain_db = Number(e.currentTarget.value)))} />
                  <small>{pad.gain_db > 0 ? "+" : ""}{pad.gain_db} дБ</small>
                </label>
                <label class="mini" title="Высота, полутоны">
                  <input type="number" min="-24" max="24" value={pad.pitch} onchange={(e) => editPads((l) => (l[i].pitch = Number(e.currentTarget.value)))} />
                  <small>пт.</small>
                </label>
                <select aria-label="Группа глушения" value={pad.choke} onchange={(e) => editPads((l) => (l[i].choke = Number(e.currentTarget.value)))}>
                  <option value={0}>глуш. —</option>
                  {#each [1, 2, 3, 4] as g (g)}<option value={g}>глуш. {g}</option>{/each}
                </select>
                </div>
              </div>
            {:else}
              <p class="hint">Пэдов пока нет.</p>
            {/each}
          </div>
          <div class="row">
            <button type="button" class="ghost" onclick={addSample}>+ Сэмпл…</button>
            <button type="button" class="ghost" onclick={addKit}>Кит из папки…</button>
            {#if pads.length}
              <button type="button" class="ghost" onclick={layoutPads} title="Первые 8 пэдов — на A, B, X, Y и крестовину основного слоя">
                Разложить по кнопкам
              </button>
            {/if}
          </div>
          {#if laidOut}<p class="hint">Готово: пэды на кнопках A, B, X, Y и крестовине.</p>{/if}
        {/if}
      </div>

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

    {#if source.type !== "samples"}
    <section class="instruments">
      <div class="section-title">Инструмент{fontFile ? ` · ${fileName(fontFile)}` : ""}</div>
      <InstrumentBrowser presets={partPresets} {part} onpick={pick} onstar={star} />
      {#if !isDrums(part)}
        <p class="hint">
          ★ — быстрый список: по нему ходит команда «следующий инструмент»{part.instruments.length
            ? ` (сейчас ${part.instruments.length})`
            : ""}.
        </p>
      {/if}
    </section>
    {/if}
  {/if}
</div>

<style>
  .sound {
    display: grid;
    grid-template-columns: 220px minmax(340px, 1fr) 300px;
    gap: 20px;
    align-items: start;
  }
  @media (max-width: 1180px) {
    .sound {
      grid-template-columns: 220px minmax(0, 1fr);
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
  .settings.wide {
    grid-column: span 2;
  }
  .source {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 0;
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
  }
  .source .seg {
    display: flex;
    gap: 4px;
  }
  .source .seg button {
    flex: 1;
    min-height: 32px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--muted);
    font-size: 13px;
  }
  .source .seg button.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
  }
  .source .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .source select,
  .pad input[type="number"] {
    font: inherit;
    color: var(--text);
    min-height: 32px;
    padding: 0 6px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
    min-width: 0;
  }
  .source .row select {
    flex: 1;
  }
  .pads {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .pad {
    display: grid;
    grid-template-columns: 32px 92px minmax(0, 1fr) 30px;
    align-items: center;
    gap: 4px 6px;
    padding: 6px 0;
    border-bottom: 1px solid var(--line);
    font-size: 13px;
  }
  .pad .opts {
    grid-column: 2 / -1;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }
  .pad .opts .mini {
    flex: 0 0 auto;
  }
  .pad .opts .mini:first-child {
    flex: 1 1 70px;
  }
  .pad .opts select {
    flex: 0 0 auto;
  }
  .pad .note {
    display: flex;
    flex-direction: column;
  }
  .pad .note input {
    width: 56px;
  }
  .pad small {
    color: var(--muted);
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pad .file {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pad .mini {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .pad .mini input[type="range"] {
    accent-color: var(--accent);
    min-width: 0;
  }
  .pad .mini input[type="number"] {
    width: 48px;
  }
  .pad .play,
  .pad .remove {
    width: 30px;
    height: 30px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: transparent;
  }
  .pad .remove:hover {
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
