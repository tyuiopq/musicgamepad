<script lang="ts">
  import { api, type ProfileEntry, type Status } from "$lib/api";
  import { musicName } from "$lib/describe";
  import { familyName } from "$lib/labels";

  let {
    status,
    profiles,
    ports,
    run,
    refreshPorts,
    view,
    onview,
    editorDirty,
  }: {
    status: Status;
    profiles: ProfileEntry[];
    ports: string[];
    run: (action: () => Promise<unknown>) => void;
    refreshPorts: () => Promise<void>;
    view: "play" | "edit" | "records";
    onview: (v: "play" | "edit" | "records") => void;
    editorDirty: boolean;
  } = $props();

  const auto = $derived(status.pad_choice.mode === "auto");
  const fixedName = $derived(status.pad_choice.mode === "fixed" ? status.pad_choice.name : null);
  const fixedMissing = $derived(fixedName !== null && !status.pads.some((p) => p.name === fixedName));
  const fileName = (path: string | null) => path?.split(/[\\/]/).pop() ?? null;

  const latency = $derived(status.synth.latency);
  const latencyTitle = $derived(
    latency
      ? [
          latency.input !== null ? `ввод ${latency.input.toFixed(1)} мс` : "ввод — нажми кнопку",
          `ожидание блока ${latency.block.toFixed(1)} мс`,
          `вывод ${latency.output.toFixed(1)} мс`,
          "без времени передачи самого геймпада",
        ].join(" · ")
      : "",
  );
</script>

<nav>
  <div class="logo">MusicGamepad</div>

  <div class="views" role="tablist" aria-label="Экран">
    <button type="button" role="tab" aria-selected={view === "play"} class:on={view === "play"} onclick={() => onview("play")}>
      Игра
    </button>
    <button type="button" role="tab" aria-selected={view === "edit"} class:on={view === "edit"} onclick={() => onview("edit")}>
      Профиль{#if editorDirty}<span class="dirty" title="Есть несохранённые изменения">•</span>{/if}
    </button>
    <button type="button" role="tab" aria-selected={view === "records"} class:on={view === "records"} onclick={() => onview("records")}>
      Записи
    </button>
  </div>

  <section>
    <div class="section-title">Геймпад</div>
    {#each status.pads as pad (pad.id)}
      {@const active = pad.id === status.active_pad}
      <button
        class="pad"
        class:active
        aria-pressed={active}
        onclick={() => run(() => api.setPadChoice({ mode: "fixed", name: pad.name }))}
        title="{familyName[pad.family]} · {pad.model}"
      >
        <span class="led"></span>
        <span class="name">{pad.name}</span>
      </button>
    {:else}
      <p class="muted small">Подключи геймпад — он появится здесь.</p>
    {/each}
    {#if fixedMissing}
      <p class="muted small">«{fixedName}» не подключён.</p>
    {/if}
    {#if status.pads.length > 0}
      <label class="check">
        <input
          type="checkbox"
          checked={auto}
          onchange={(e) =>
            run(() =>
              api.setPadChoice(
                e.currentTarget.checked
                  ? { mode: "auto" }
                  : { mode: "fixed", name: status.pads.find((p) => p.id === status.active_pad)?.name ?? status.pads[0].name },
              ),
            )}
        />
        Играет тот, где нажали
      </label>
    {/if}
    {#if status.input_error}<p class="error">{status.input_error}</p>{/if}
  </section>

  <section>
    <div class="section-title">Профиль</div>
    <select
      value={status.profile}
      disabled={editorDirty}
      title={editorDirty ? "Сначала сохрани или отмени правки профиля" : ""}
      onchange={(e) => run(() => api.selectProfile(e.currentTarget.value))}
    >
      {#each profiles as p (p.name)}
        <option value={p.name}>{p.name}{p.modified ? " (изменён)" : ""}</option>
      {/each}
    </select>
    {#if status.music}
      <div class="fact"><span class="muted">Лад</span><span>{musicName(status.music)}</span></div>
    {/if}
  </section>

  <section>
    <div class="section-title">Звук</div>
    <label class="check">
      <input
        type="checkbox"
        checked={status.synth.enabled}
        onchange={(e) => run(() => api.setSynthEnabled(e.currentTarget.checked))}
      />
      Встроенный синтезатор
    </label>
    {#if status.synth.error}<p class="error">{status.synth.error}</p>{/if}
    {#if status.synth.enabled}
      <label class="check" title="Эксклюзивный режим Windows: звук идёт напрямую в устройство">
        <input
          type="checkbox"
          checked={status.synth.requested_mode === "exclusive"}
          onchange={(e) => run(() => api.setAudioMode(e.currentTarget.checked ? "exclusive" : "shared"))}
        />
        Низкая задержка
      </label>
      {#if status.synth.fallback}
        <p class="error">Не включилась: {status.synth.fallback}. Играю в обычном режиме.</p>
      {:else if status.synth.mode === "exclusive"}
        <p class="muted small">Другие программы на «{status.synth.device}» сейчас не звучат.</p>
      {/if}
      {#if latency}
        <p class="latency" title={latencyTitle}>
          Задержка ≈ <b class:good={latency.total < 15} class:bad={latency.total >= 25}>{Math.round(latency.total)} мс</b>
        </p>
      {/if}
    {/if}
    <div class="row">
      <select
        aria-label="MIDI-выход"
        value={status.midi.port ?? ""}
        onchange={(e) => run(() => api.setMidiOut(e.currentTarget.value || null))}
      >
        <option value="">MIDI-выход: нет</option>
        {#each ports as p (p)}<option value={p}>{p}</option>{/each}
      </select>
      <button class="icon" aria-label="Обновить список MIDI-портов" onclick={() => run(refreshPorts)}>↻</button>
    </div>
    {#if status.midi.error}<p class="error">{status.midi.error}</p>{/if}
    {#if status.synth.enabled}
      <label class="volume">
        <span class="muted">Громкость</span>
        <input
          type="range"
          min="0"
          max="2"
          step="0.05"
          value={status.synth.master_volume}
          oninput={(e) => run(() => api.setMasterVolume(Number(e.currentTarget.value)))}
        />
        <b>{Math.round(status.synth.master_volume * 100)}%</b>
      </label>
    {/if}
    <div class="bank">
      <span class="muted small" title={status.synth.soundfont ?? ""}>
        Банк: {fileName(status.synth.soundfont) ?? "не найден"}
      </span>
      <button type="button" class="link small" onclick={() => run(api.chooseSoundfont)}>Сменить…</button>
      {#if status.synth.custom_soundfont}
        <button type="button" class="link small" onclick={() => run(api.resetSoundfont)}>По умолчанию</button>
      {/if}
    </div>
  </section>

  <div class="spacer"></div>

  {#if status.last_recording && !status.recording}
    <button class="link small" onclick={() => run(api.openRecordings)} title={status.last_recording}>
      Сохранено: {fileName(status.last_recording)}
    </button>
  {/if}
  <button class="rec" class:on={status.transport.record !== "idle"} onclick={() => run(api.toggleRecord)}>
    <span class="dot"></span>{status.transport.record === "count_in"
      ? `Отсчёт… ${status.transport.count_in_left ?? ""}`
      : status.recording
        ? "Остановить запись"
        : "Запись"}
  </button>
</nav>

<style>
  nav {
    width: 260px;
    flex-shrink: 0;
    box-sizing: border-box;
    min-height: 100vh;
    padding: 22px 18px;
    background: var(--panel);
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 24px;
  }
  .views {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 4px;
    padding: 4px;
    border-radius: 12px;
    background: var(--bg);
    margin-top: -8px;
  }
  .views button {
    min-height: 36px;
    padding: 0 2px;
    font-size: 14px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--muted);
  }
  .views button.on {
    background: var(--key);
    color: var(--text);
  }
  .views .dirty {
    color: var(--accent);
    margin-left: 4px;
  }
  .logo {
    font-family: var(--font-display);
    font-weight: 700;
    font-size: 17px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .pad {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 0 12px;
    border-radius: 10px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--muted);
    text-align: left;
  }
  .pad .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pad .led {
    width: 8px;
    height: 8px;
    flex-shrink: 0;
    border-radius: 50%;
    border: 1px solid var(--muted);
    box-sizing: border-box;
  }
  .pad.active {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
  }
  .pad.active .led {
    background: var(--accent);
    border-color: var(--accent);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: 14px;
    cursor: pointer;
  }
  select {
    min-height: 40px;
    padding: 0 10px;
    border-radius: 10px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
    min-width: 0;
  }
  .row {
    display: flex;
    gap: 6px;
  }
  .row select {
    flex: 1;
  }
  .icon {
    width: 40px;
    border-radius: 10px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
  }
  .fact {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 14px;
  }
  .small {
    font-size: 13px;
    margin: 0;
  }
  .spacer {
    flex: 1;
  }
  .volume {
    display: grid;
    grid-template-columns: auto 1fr 44px;
    align-items: center;
    gap: 8px;
    font-size: 14px;
  }
  .volume input {
    accent-color: var(--accent);
    min-width: 0;
  }
  .volume b {
    text-align: right;
    font-weight: 600;
  }
  .bank {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 10px;
  }
  .bank span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }
  .bank .link {
    color: var(--mod);
  }
  .latency {
    margin: 0;
    font-size: 14px;
    color: var(--muted);
  }
  .latency b {
    color: var(--text);
  }
  .latency b.good {
    color: var(--face-a);
  }
  .latency b.bad {
    color: var(--error);
  }
  .link {
    background: none;
    border: none;
    color: var(--muted);
    text-align: left;
    padding: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .link:hover {
    color: var(--text);
  }
  .rec {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    min-height: 48px;
    border-radius: 12px;
    border: 1px solid var(--rec);
    background: transparent;
    color: var(--rec-text);
    font-weight: 600;
  }
  .rec .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--rec);
  }
  .rec.on {
    background: var(--rec);
    color: #fff;
  }
  .rec.on .dot {
    background: #fff;
    animation: blink 1s infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.3;
    }
  }
  button:hover,
  select:hover {
    border-color: var(--accent);
  }
</style>
