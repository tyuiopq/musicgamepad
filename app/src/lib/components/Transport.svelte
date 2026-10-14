<script lang="ts">
  // Колонка справа на экране игры: запись, метроном, лупер. Все элементы на постоянных местах —
  // меняются только подписи, ширина не прыгает.
  import { api, type Quantize, type TransportSettings, type TransportStatus } from "$lib/api";

  let {
    transport,
    run,
    external = false,
  }: {
    transport: TransportStatus;
    run: (action: () => Promise<unknown>) => void;
    /** Темп приходит по MIDI Clock от DAW. */
    external?: boolean;
  } = $props();

  const ts = $derived(transport.settings);
  const looper = $derived(transport.looper);
  let taps: number[] = [];
  let showOptions = $state(false);

  function set(fn: (s: TransportSettings) => void) {
    const s = { ...ts };
    fn(s);
    run(() => api.setTransport(s));
  }

  function setBpm(v: number) {
    if (Number.isFinite(v)) set((s) => (s.bpm = Math.round(Math.min(300, Math.max(20, v)))));
  }

  function tap() {
    const now = performance.now();
    taps = [...taps.filter((t) => now - t < 2500), now].slice(-5);
    if (taps.length >= 3) {
      const gaps = taps.slice(1).map((t, i) => t - taps[i]);
      setBpm(60000 / (gaps.reduce((a, b) => a + b, 0) / gaps.length));
    }
  }

  const time = (sec: number) => `${Math.floor(sec / 60)}:${String(Math.floor(sec % 60)).padStart(2, "0")}`;
  const fileName = (p: string | null) => p?.split(/[\\/]/).pop() ?? "";

  const recordLine = $derived(
    transport.record === "count_in"
      ? `Отсчёт: ещё ${transport.count_in_left ?? ""}`
      : transport.record === "recording"
        ? `Идёт запись · ${time(transport.record_seconds)}`
        : transport.exporting > 0
          ? "Сохраняю WAV…"
          : transport.last_wav
            ? `WAV: ${fileName(transport.last_wav)}`
            : "Start на геймпаде или кнопка выше",
  );

  const LOOP_LABEL: Record<string, string> = {
    empty: "Записать петлю",
    armed: "Ждёт начала такта…",
    recording: "Пишу петлю…",
    playing: "Наложить слой",
    overdub: "Закончить слой",
  };
  const loopBusy = $derived(looper.state !== "empty");

  const QUANTIZE: [Quantize, string][] = [
    ["off", "Нет"],
    ["quarter", "1/4"],
    ["eighth", "1/8"],
    ["sixteenth", "1/16"],
    ["eighth_triplet", "Триоли 1/8"],
  ];
  const bars = (n: number) => `${n} ${n === 1 ? "такт" : n < 5 ? "такта" : "тактов"}`;
</script>

<div class="panel">
  <section class="card">
    <div class="card-head">
      <h3 class="card-title">Запись</h3>
      <button type="button" class="btn-link" aria-expanded={showOptions} onclick={() => (showOptions = !showOptions)}>
        {showOptions ? "Скрыть" : "Параметры"}
      </button>
    </div>
    <button
      type="button"
      class="btn btn-rec btn-lg btn-block"
      class:on={transport.record !== "idle"}
      class:pulse={transport.record === "count_in"}
      onclick={() => run(api.toggleRecord)}
    >
      <span class="dot"></span>{transport.record === "idle" ? "Записать" : "Остановить"}
    </button>
    <p class="hint ellipsis line">{recordLine}</p>
    {#if showOptions}
      <div class="stack options" style="--gap: 8px">
        <label class="toggle"
          >Отсчёт такта перед записью
          <input type="checkbox" class="switch" checked={ts.count_in} onchange={(e) => set((s) => (s.count_in = e.currentTarget.checked))} /></label
        >
        <label class="toggle"
          >Сохранять ещё и WAV
          <input type="checkbox" class="switch" checked={ts.export_wav} onchange={(e) => set((s) => (s.export_wav = e.currentTarget.checked))} /></label
        >
        <label class="field" style="--label-w: 110px">
          <span class="label">Квантование</span>
          <select class="select select-sm" value={ts.quantize} onchange={(e) => set((s) => (s.quantize = e.currentTarget.value as Quantize))}>
            {#each QUANTIZE as [q, text] (q)}<option value={q}>{text}</option>{/each}
          </select>
        </label>
        <label class="field" style="--label-w: 110px" title="Пока идут часы (метроном, запись, лупер, Play в DAW), нажатия подтягиваются к ближайшей доле сетки">
          <span class="label">Квантовать игру</span>
          <select class="select select-sm" value={ts.live_quantize ?? "off"} onchange={(e) => set((s) => (s.live_quantize = e.currentTarget.value as Quantize))}>
            {#each QUANTIZE as [q, text] (q)}<option value={q}>{text}</option>{/each}
          </select>
        </label>
      </div>
    {/if}
  </section>

  <section class="card">
    <div class="card-head">
      <h3 class="card-title">Метроном</h3>
      <input
        type="checkbox"
        class="switch"
        aria-label="Метроном"
        checked={ts.metronome}
        onchange={() => run(() => api.transportAction({ type: "toggle_metronome" }))}
      />
    </div>
    <div class="bpm">
      <button type="button" class="btn btn-sm btn-icon" style="width: 30px" aria-label="Медленнее" disabled={external} onclick={() => setBpm(ts.bpm - 1)}>−</button>
      <input class="input input-sm" type="number" min="20" max="300" aria-label="Темп" disabled={external} value={ts.bpm} onchange={(e) => setBpm(Number(e.currentTarget.value))} />
      <button type="button" class="btn btn-sm btn-icon" style="width: 30px" aria-label="Быстрее" disabled={external} onclick={() => setBpm(ts.bpm + 1)}>+</button>
      <span class="xs muted">BPM</span>
      {#if external}
        <span class="chip" title="Темп приходит по MIDI Clock">от DAW</span>
      {:else}
        <button type="button" class="btn btn-sm" title="Постучи 3–4 раза в темпе" onclick={tap}>Тап</button>
      {/if}
    </div>
    <div class="row">
      <select class="select select-sm" aria-label="Размер" value={ts.beats_per_bar} onchange={(e) => set((s) => (s.beats_per_bar = Number(e.currentTarget.value)))}>
        {#each [2, 3, 4, 5, 6, 7] as n (n)}<option value={n}>{n}/4</option>{/each}
      </select>
      <div class="beats" aria-label={transport.beat ? `Такт ${transport.bar}, доля ${transport.beat}` : "Часы стоят"}>
        {#each Array.from({ length: 7 }, (_, i) => i + 1) as b (b)}
          <span class="beat" class:hidden={b > ts.beats_per_bar} class:first={b === 1} class:lit={transport.beat === b}></span>
        {/each}
      </div>
    </div>
    <label class="field" style="--label-w: 72px; --value-w: 36px">
      <span class="label">Громкость</span>
      <input type="range" min="0" max="1" step="0.05" value={ts.metronome_volume} oninput={(e) => set((s) => (s.metronome_volume = Number(e.currentTarget.value)))} />
      <span class="value">{Math.round(ts.metronome_volume * 100)}%</span>
    </label>
  </section>

  <section class="card">
    <div class="card-head">
      <h3 class="card-title">Лупер</h3>
      <span class="xs muted">слоёв: {looper.layers}</span>
    </div>
    <button
      type="button"
      class="btn btn-block"
      class:loop-on={loopBusy}
      class:pulse={looper.state === "recording" || looper.state === "overdub" || looper.state === "armed"}
      onclick={() => run(() => api.transportAction({ type: "loop_record" }))}
    >
      {LOOP_LABEL[looper.state]}
    </button>
    <div class="progress" aria-hidden="true"><div style="width: {Math.round(looper.progress * 100)}%"></div></div>
    <div class="row">
      <select class="select select-sm" style="width: 112px" aria-label="Длина петли" disabled={loopBusy} value={ts.loop_bars} onchange={(e) => set((s) => (s.loop_bars = Number(e.currentTarget.value)))}>
        {#each [1, 2, 4, 8] as n (n)}<option value={n}>{bars(n)}</option>{/each}
      </select>
      <span class="spacer"></span>
      <button type="button" class="btn btn-sm" disabled={!loopBusy} onclick={() => run(() => api.transportAction({ type: "loop_undo" }))}>Отменить</button>
      <button type="button" class="btn btn-sm btn-danger" disabled={!loopBusy} onclick={() => run(() => api.transportAction({ type: "loop_clear" }))}>
        Очистить
      </button>
    </div>
  </section>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .line {
    min-height: 18px;
  }
  .options {
    padding-top: 10px;
    border-top: 1px solid var(--line);
  }
  .bpm {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .bpm input {
    width: 64px;
    padding: 0 4px;
    -moz-appearance: textfield;
    appearance: textfield;
    text-align: center;
    font-family: var(--font-mono);
    font-weight: 700;
  }
  .bpm .btn:last-child {
    margin-left: auto;
  }
  .beats {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
  }
  .beat {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--surface-3);
  }
  .beat.first {
    width: 13px;
    height: 13px;
  }
  .beat.lit {
    background: var(--accent);
  }
  .beat.hidden {
    visibility: hidden;
  }
  .bpm input::-webkit-inner-spin-button,
  .bpm input::-webkit-outer-spin-button {
    -webkit-appearance: none;
    margin: 0;
  }
  .loop-on {
    border-color: var(--mod);
    background: var(--mod-soft);
  }
  .pulse {
    animation: pulse 0.6s infinite alternate;
  }
  @keyframes pulse {
    to {
      opacity: 0.6;
    }
  }
  .progress {
    height: 6px;
    border-radius: 3px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .progress div {
    height: 100%;
    background: var(--mod);
  }
</style>
