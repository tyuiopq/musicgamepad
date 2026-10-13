<script lang="ts">
  import { onMount } from "svelte";
  import { api, type PadFamily, type ProfileEntry, type Status } from "$lib/api";
  import { familyName, keyLabel, noteName, solfege } from "$lib/labels";

  let status = $state<Status | null>(null);
  let ports = $state<string[]>([]);
  let profiles = $state<ProfileEntry[]>([]);
  let error = $state<string | null>(null);

  const activePad = $derived(status?.pads.find((p) => p.id === status?.active_pad) ?? null);
  const family = $derived<PadFamily>(activePad?.family ?? "xbox");
  const fileName = (path: string | null) => path?.split(/[\\/]/).pop() ?? null;

  async function run(action: () => Promise<unknown>) {
    try {
      error = null;
      await action();
    } catch (e) {
      error = String(e);
    }
  }

  async function refreshPorts() {
    ports = await api.midiPorts();
  }

  onMount(() => {
    const unlisten = api.onStatus((s) => (status = s));
    api.status().then((s) => (status ??= s));
    refreshPorts();
    api.profiles().then((p) => (profiles = p));
    return () => {
      unlisten.then((f) => f());
    };
  });
</script>

<main>
  <header>
    <h1>MusicGamepad</h1>
    {#if status}
      <label class="profile">
        Профиль
        <select
          value={status.profile}
          onchange={(e) => run(() => api.selectProfile(e.currentTarget.value))}
        >
          {#each profiles as p (p.name)}
            <option value={p.name}>{p.name}{p.builtin ? "" : " ★"}</option>
          {/each}
        </select>
      </label>
      <button class="rec" class:on={status.recording} onclick={() => run(api.toggleRecord)}>
        {status.recording ? "■ Стоп" : "● Запись"}
      </button>
      <button class="ghost" title="Заглушить все ноты" onclick={() => run(api.panic)}>Тишина</button>
    {/if}
  </header>

  {#if error}<p class="error">{error}</p>{/if}

  {#if !status}
    <p class="muted">Запускаю движок…</p>
  {:else}
    <section class="now">
      <div class="notes" class:silent={status.notes.length === 0}>
        {#if status.notes.length}
          {#each status.notes as n (n)}
            <span class="note"><b>{noteName(n)}</b><small>{solfege(n)}</small></span>
          {/each}
        {:else}
          <span class="placeholder">Нажми кнопку на геймпаде</span>
        {/if}
      </div>
      <div class="facts">
        {#each status.parts as part (part.channel)}
          <div><span class="muted">{part.name}:</span> {part.instrument}</div>
        {/each}
        <div>
          <span class="muted">Слои:</span>
          {#each status.layers as layer, i (layer)}
            <span class="chip" class:accent={i > 0}>{layer}</span>
          {/each}
        </div>
        <div>
          <span class="muted">Сдвиг:</span>
          {status.transpose > 0 ? "+" : ""}{status.transpose} полутонов
        </div>
        <div class="pressed">
          <span class="muted">Зажато:</span>
          {#each status.pressed as key (JSON.stringify(key))}
            <span class="chip">{keyLabel(key, family)}</span>
          {:else}
            <span class="muted">—</span>
          {/each}
        </div>
      </div>
    </section>

    <div class="grid">
      <section>
        <h2>Геймпад</h2>
        {#if status.input_error}<p class="error">{status.input_error}</p>{/if}
        <label class="option">
          <input
            type="radio"
            name="pad"
            checked={status.pad_choice.mode === "auto"}
            onchange={() => run(() => api.setPadChoice({ mode: "auto" }))}
          />
          <span>Автоматически <small class="muted">— тот, на котором играешь</small></span>
        </label>
        {#each status.pads as pad (pad.id)}
          <label class="option">
            <input
              type="radio"
              name="pad"
              checked={status.pad_choice.mode === "fixed" && status.pad_choice.name === pad.name}
              onchange={() => run(() => api.setPadChoice({ mode: "fixed", name: pad.name }))}
            />
            <span>
              {pad.name}
              <small class="muted">{familyName[pad.family]} · {pad.model}</small>
              {#if pad.id === status.active_pad}<span class="chip accent">играет</span>{/if}
            </span>
          </label>
        {:else}
          <p class="muted">Подключи геймпад — он появится здесь.</p>
        {/each}
        {#if status.pad_choice.mode === "fixed" && !status.pads.some((p) => status?.pad_choice.mode === "fixed" && p.name === status.pad_choice.name)}
          <p class="muted">Выбранный «{status.pad_choice.name}» не подключён.</p>
        {/if}
      </section>

      <section>
        <h2>Звук</h2>
        <label class="option">
          <input
            type="checkbox"
            checked={status.synth.enabled}
            onchange={(e) => run(() => api.setSynthEnabled(e.currentTarget.checked))}
          />
          <span>Встроенный синтезатор</span>
        </label>
        {#if status.synth.error}<p class="error">{status.synth.error}</p>{/if}
        <dl>
          <dt>Банк</dt>
          <dd>{fileName(status.synth.soundfont) ?? "—"}</dd>
          <dt>Устройство</dt>
          <dd>{status.synth.device ?? "—"}</dd>
          <dt>Буфер</dt>
          <dd>{status.synth.buffer_ms ? `${status.synth.buffer_ms.toFixed(1)} мс` : "—"}</dd>
        </dl>
      </section>

      <section>
        <h2>MIDI-выход</h2>
        <div class="row">
          <select
            value={status.midi.port ?? ""}
            onchange={(e) => run(() => api.setMidiOut(e.currentTarget.value || null))}
          >
            <option value="">Выключен</option>
            {#each ports as p (p)}<option value={p}>{p}</option>{/each}
          </select>
          <button class="ghost" title="Обновить список" onclick={() => run(refreshPorts)}>↻</button>
        </div>
        {#if status.midi.error}<p class="error">{status.midi.error}</p>{/if}
        <p class="muted small">
          Чтобы играть в FL Studio или другой DAW, создай виртуальный порт в loopMIDI и выбери его
          здесь. Встроенный синтезатор тогда лучше выключить.
        </p>
      </section>

      <section>
        <h2>Запись</h2>
        <p>
          {#if status.recording}
            <span class="rec-dot"></span> Идёт запись…
          {:else if status.last_recording}
            Сохранено: <b>{fileName(status.last_recording)}</b>
          {:else}
            <span class="muted">Start на геймпаде или кнопка «Запись» наверху.</span>
          {/if}
        </p>
        <button class="ghost" onclick={() => run(api.openRecordings)}>Открыть папку записей</button>
      </section>
    </div>
  {/if}
</main>

<style>
  :global(:root) {
    --bg: #14141c;
    --panel: #1d1d28;
    --line: #2c2c3a;
    --text: #e8e8f0;
    --muted: #8a8aa0;
    --accent: #7c9cff;
    --rec: #ff5a6a;
    --error: #ff8a7a;
    color-scheme: dark;
    font-family: "Segoe UI", system-ui, sans-serif;
    font-size: 15px;
    color: var(--text);
    background: var(--bg);
  }
  :global(body) {
    margin: 0;
  }
  main {
    padding: 20px 24px;
    max-width: 1000px;
    margin: 0 auto;
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 16px;
  }
  h1 {
    font-size: 20px;
    margin: 0 auto 0 0;
    letter-spacing: 0.02em;
  }
  h2 {
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
    margin: 0 0 12px;
  }
  section {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 16px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 12px;
    margin-top: 12px;
  }
  .now {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
    align-items: center;
  }
  .notes {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    min-height: 84px;
    align-items: center;
  }
  .note {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 10px 14px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    border: 1px solid var(--accent);
  }
  .note b {
    font-size: 26px;
  }
  .placeholder {
    color: var(--muted);
  }
  .facts {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .chip {
    display: inline-block;
    padding: 1px 8px;
    margin: 0 4px 2px 0;
    border-radius: 999px;
    border: 1px solid var(--line);
    font-size: 13px;
  }
  .chip.accent {
    border-color: var(--accent);
    color: var(--accent);
  }
  .option {
    display: flex;
    gap: 10px;
    align-items: baseline;
    padding: 6px 0;
    cursor: pointer;
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .row select {
    flex: 1;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 12px;
    margin: 8px 0 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 13px;
  }
  .error {
    color: var(--error);
  }
  select,
  button {
    font: inherit;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 6px 10px;
  }
  button {
    cursor: pointer;
  }
  button:hover,
  select:hover {
    border-color: var(--accent);
  }
  .rec.on {
    background: var(--rec);
    border-color: var(--rec);
    color: white;
  }
  .rec-dot {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--rec);
    animation: blink 1s infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.3;
    }
  }
  .profile {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--muted);
  }
  @media (max-width: 700px) {
    .now {
      grid-template-columns: 1fr;
    }
  }
</style>
