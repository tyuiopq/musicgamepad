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
  }: {
    status: Status;
    profiles: ProfileEntry[];
    ports: string[];
    run: (action: () => Promise<unknown>) => void;
    refreshPorts: () => Promise<void>;
  } = $props();

  const auto = $derived(status.pad_choice.mode === "auto");
  const fixedName = $derived(status.pad_choice.mode === "fixed" ? status.pad_choice.name : null);
  const fixedMissing = $derived(fixedName !== null && !status.pads.some((p) => p.name === fixedName));
  const fileName = (path: string | null) => path?.split(/[\\/]/).pop() ?? null;
</script>

<nav>
  <div class="logo">MusicGamepad</div>

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
    <select value={status.profile} onchange={(e) => run(() => api.selectProfile(e.currentTarget.value))}>
      {#each profiles as p (p.name)}
        <option value={p.name}>{p.name}</option>
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
    <p class="muted small" title={status.synth.soundfont ?? ""}>
      {fileName(status.synth.soundfont) ?? "банк не найден"}
      {#if status.synth.buffer_ms}· буфер {status.synth.buffer_ms.toFixed(0)} мс{/if}
    </p>
  </section>

  <div class="spacer"></div>

  {#if status.last_recording && !status.recording}
    <button class="link small" onclick={() => run(api.openRecordings)} title={status.last_recording}>
      Сохранено: {fileName(status.last_recording)}
    </button>
  {/if}
  <button class="rec" class:on={status.recording} onclick={() => run(api.toggleRecord)}>
    <span class="dot"></span>{status.recording ? "Остановить запись" : "Запись"}
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
