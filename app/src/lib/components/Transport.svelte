<script lang="ts">
  // Панель записи: запись с отсчётом, метроном, лупер и настройки сохранения.
  import { api, type Quantize, type TransportSettings, type TransportStatus } from "$lib/api";

  let { transport, run }: { transport: TransportStatus; run: (action: () => Promise<unknown>) => void } = $props();

  const ts = $derived(transport.settings);
  const looper = $derived(transport.looper);
  let showSettings = $state(false);
  let taps: number[] = [];

  function set(fn: (s: TransportSettings) => void) {
    const s = { ...ts };
    fn(s);
    run(() => api.setTransport(s));
  }

  function setBpm(v: number) {
    if (Number.isFinite(v)) set((s) => (s.bpm = Math.round(Math.min(300, Math.max(20, v)))));
  }

  /** Тап-темп: темп по последним нажатиям. */
  function tap() {
    const now = performance.now();
    taps = [...taps.filter((t) => now - t < 2500), now].slice(-5);
    if (taps.length >= 3) {
      const gaps = taps.slice(1).map((t, i) => t - taps[i]);
      setBpm(60000 / (gaps.reduce((a, b) => a + b, 0) / gaps.length));
    }
  }

  function time(sec: number): string {
    const m = Math.floor(sec / 60);
    const s = Math.floor(sec % 60);
    return `${m}:${String(s).padStart(2, "0")}`;
  }

  const recordLabel = $derived(
    transport.record === "count_in"
      ? `Отсчёт… ${transport.count_in_left ?? ""}`
      : transport.record === "recording"
        ? `■ ${time(transport.record_seconds)}`
        : "● Запись",
  );

  const LOOP_LABELS: Record<string, string> = {
    empty: "⟳ Записать петлю",
    armed: "Ждёт начала такта…",
    recording: "Пишу петлю…",
    playing: "＋ Наложить слой",
    overdub: "✓ Закончить слой",
  };

  const QUANTIZE: [Quantize, string][] = [
    ["off", "Нет"],
    ["quarter", "1/4"],
    ["eighth", "1/8"],
    ["sixteenth", "1/16"],
    ["eighth_triplet", "Триоли 1/8"],
  ];

  const fileName = (p: string | null) => p?.split(/[\\/]/).pop() ?? "";
</script>

<section class="transport" aria-label="Запись и метроном">
  <div class="group">
    <button
      type="button"
      class="rec"
      class:on={transport.record !== "idle"}
      class:count={transport.record === "count_in"}
      onclick={() => run(api.toggleRecord)}
    >
      {recordLabel}
    </button>
  </div>

  <div class="group">
    <button
      type="button"
      class="toggle"
      class:on={ts.metronome}
      aria-pressed={ts.metronome}
      title="Метроном"
      onclick={() => run(() => api.transportAction({ type: "toggle_metronome" }))}
    >
      ♩ Метроном
    </button>
    <div class="bpm">
      <button type="button" aria-label="Медленнее" onclick={() => setBpm(ts.bpm - 1)}>−</button>
      <input
        type="number"
        min="20"
        max="300"
        aria-label="Темп, ударов в минуту"
        value={ts.bpm}
        onchange={(e) => setBpm(Number(e.currentTarget.value))}
      />
      <button type="button" aria-label="Быстрее" onclick={() => setBpm(ts.bpm + 1)}>+</button>
      <span class="muted">BPM</span>
    </div>
    <button type="button" class="ghost" title="Постучи 3–4 раза в темпе" onclick={tap}>Тап</button>
    <select aria-label="Размер" value={ts.beats_per_bar} onchange={(e) => set((s) => (s.beats_per_bar = Number(e.currentTarget.value)))}>
      {#each [2, 3, 4, 5, 6, 7] as n (n)}<option value={n}>{n}/4</option>{/each}
    </select>
    <div class="beats" aria-label={transport.beat ? `Такт ${transport.bar}, доля ${transport.beat}` : "Часы стоят"}>
      {#each Array.from({ length: ts.beats_per_bar }, (_, i) => i + 1) as b (b)}
        <span class="dot" class:first={b === 1} class:lit={transport.beat === b}></span>
      {/each}
    </div>
  </div>

  <div class="group looper">
    <button
      type="button"
      class="loop"
      class:active={looper.state !== "empty"}
      class:rec={looper.state === "recording" || looper.state === "overdub"}
      onclick={() => run(() => api.transportAction({ type: "loop_record" }))}
    >
      {LOOP_LABELS[looper.state]}
    </button>
    <div class="progress" aria-hidden="true"><div style="width: {Math.round(looper.progress * 100)}%"></div></div>
    {#if looper.state !== "empty"}
      <span class="muted small">слоёв: {looper.layers}</span>
      <button type="button" class="ghost" onclick={() => run(() => api.transportAction({ type: "loop_undo" }))}>Отменить</button>
      <button type="button" class="ghost" onclick={() => run(() => api.transportAction({ type: "loop_clear" }))}>Очистить</button>
    {:else}
      <select aria-label="Длина петли" value={ts.loop_bars} onchange={(e) => set((s) => (s.loop_bars = Number(e.currentTarget.value)))}>
        {#each [1, 2, 4, 8] as n (n)}<option value={n}>{n} {n === 1 ? "такт" : n < 5 ? "такта" : "тактов"}</option>{/each}
      </select>
    {/if}
  </div>

  <button
    type="button"
    class="ghost settings-toggle"
    aria-expanded={showSettings}
    onclick={() => (showSettings = !showSettings)}
    title="Настройки записи"
  >
    ⚙
  </button>

  {#if showSettings}
    <div class="settings">
      <label class="check"
        ><input type="checkbox" checked={ts.count_in} onchange={(e) => set((s) => (s.count_in = e.currentTarget.checked))} /> Отсчёт такта
        перед записью</label
      >
      <label class="field">
        <span class="muted">Квантование при сохранении</span>
        <select value={ts.quantize} onchange={(e) => set((s) => (s.quantize = e.currentTarget.value as Quantize))}>
          {#each QUANTIZE as [q, text] (q)}<option value={q}>{text}</option>{/each}
        </select>
      </label>
      <label class="check"
        ><input type="checkbox" checked={ts.export_wav} onchange={(e) => set((s) => (s.export_wav = e.currentTarget.checked))} /> Сохранять
        ещё и WAV</label
      >
      <label class="check" title="Пока идёт метроном или лупер — 24 тика на долю, Start при запуске, Stop при остановке. В Ableton: Preferences → Link/Tempo/MIDI → у входа включить Sync">
        <input type="checkbox" checked={ts.send_clock} onchange={(e) => set((s) => (s.send_clock = e.currentTarget.checked))} /> MIDI Clock в DAW
      </label>
      <label class="check" title="Record/Stop через MIDI Machine Control. Понимают, например, Reaper и Cubase; Ableton — нет (для него назначь CC-кнопку через MIDI Map)">
        <input type="checkbox" checked={ts.send_mmc} onchange={(e) => set((s) => (s.send_mmc = e.currentTarget.checked))} /> MMC (запись в DAW)
      </label>
      <label class="field">
        <span class="muted">Громкость метронома</span>
        <input
          type="range"
          min="0"
          max="1"
          step="0.05"
          value={ts.metronome_volume}
          oninput={(e) => set((s) => (s.metronome_volume = Number(e.currentTarget.value)))}
        />
      </label>
    </div>
  {/if}

  {#if transport.exporting > 0}
    <p class="note muted">Сохраняю WAV…</p>
  {:else if transport.last_wav}
    <p class="note muted">WAV: {fileName(transport.last_wav)}</p>
  {/if}
</section>

<style>
  .transport {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 18px;
    padding: 10px 14px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 14px;
  }
  .group {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  button,
  select,
  input {
    font: inherit;
    color: var(--text);
  }
  .rec,
  .toggle,
  .loop,
  .ghost {
    min-height: 38px;
    padding: 0 14px;
    border-radius: 10px;
    border: 1px solid var(--line-strong);
    background: transparent;
    white-space: nowrap;
  }
  .rec {
    border-color: var(--rec);
    color: var(--rec-text);
    font-weight: 600;
    min-width: 120px;
  }
  .rec.on {
    background: var(--rec);
    color: #fff;
  }
  .rec.count {
    background: transparent;
    color: var(--rec-text);
    animation: pulse 0.6s infinite alternate;
  }
  .toggle.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .loop.active {
    border-color: var(--mod);
    background: var(--mod-soft);
  }
  .loop.rec {
    border-color: var(--rec);
    color: var(--rec-text);
    background: transparent;
    animation: pulse 0.6s infinite alternate;
  }
  @keyframes pulse {
    to {
      opacity: 0.55;
    }
  }
  .bpm {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .bpm button {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
  }
  .bpm input {
    width: 56px;
    height: 32px;
    text-align: center;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
    font-family: var(--font-mono);
    font-weight: 700;
  }
  select {
    min-height: 36px;
    padding: 0 8px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
  }
  .beats {
    display: flex;
    gap: 6px;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--key);
  }
  .dot.first {
    width: 14px;
    height: 14px;
    margin-top: -2px;
  }
  .dot.lit {
    background: var(--accent);
  }
  .progress {
    width: 120px;
    height: 6px;
    border-radius: 3px;
    background: var(--key);
    overflow: hidden;
  }
  .progress div {
    height: 100%;
    background: var(--mod);
  }
  .settings-toggle {
    margin-left: auto;
    padding: 0 12px;
  }
  .settings {
    flex-basis: 100%;
    display: flex;
    flex-wrap: wrap;
    gap: 10px 24px;
    padding-top: 8px;
    border-top: 1px solid var(--line);
    font-size: 14px;
  }
  .check,
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .field input[type="range"] {
    accent-color: var(--accent);
  }
  .small {
    font-size: 13px;
  }
  .note {
    flex-basis: 100%;
    margin: 0;
    font-size: 13px;
  }
</style>
