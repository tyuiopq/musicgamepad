<script lang="ts">
  // Экран «Настройки»: геймпады, звук, MIDI-выход, оформление.
  import { onMount } from "svelte";
  import { api, type Calibration, type ProfileEntry, type Status } from "$lib/api";
  import { familyName } from "$lib/labels";
  import { applyTheme, DEFAULT_THEME, PRESETS, validTheme, type Theme } from "$lib/theme";
  import Calibrate from "./Calibrate.svelte";
  import Icon from "./Icon.svelte";

  let {
    status,
    profiles,
    run,
    theme,
    ontheme,
  }: {
    status: Status;
    profiles: ProfileEntry[];
    run: (action: () => Promise<unknown>) => void;
    theme: Theme;
    ontheme: (t: Theme) => void;
  } = $props();

  let ports = $state<string[]>([]);
  let padProfiles = $state<Record<string, string>>({});
  let calibration = $state<Record<string, Calibration>>({});

  async function refreshSettings() {
    const s = await api.settings();
    padProfiles = s.pad_profiles ?? {};
    calibration = s.calibration ?? {};
  }

  async function refreshPorts() {
    ports = await api.midiPorts();
  }

  onMount(() => {
    refreshPorts();
    refreshSettings();
  });

  async function setPadProfile(family: string, name: string) {
    await api.setPadProfile(family, name || null);
    padProfiles = (await api.settings()).pad_profiles ?? {};
  }

  const FAMILIES = ["xbox", "play_station", "nintendo", "generic"] as const;
  const fileName = (p: string | null) => p?.split(/[\\/]/).pop() ?? null;
  const ts = $derived(status.transport.settings);
  const latency = $derived(status.synth.latency);

  function setTheme(t: Theme) {
    const v = validTheme(t);
    applyTheme(v);
    ontheme(v);
    run(() => api.setUiColors(v.accent, v.secondary));
  }
  const same = (a: Theme, b: Theme) => a.accent.toLowerCase() === b.accent.toLowerCase() && a.secondary.toLowerCase() === b.secondary.toLowerCase();
</script>

<div class="settings">
  <h1 class="h1">Настройки</h1>

  <div class="grid">
    <section class="card">
      <h2 class="card-title">Геймпады</h2>
      <div class="stack" style="--gap: 4px">
        <label class="radio">
          <input type="radio" name="pad" checked={status.pad_choice.mode === "auto"} onchange={() => run(() => api.setPadChoice({ mode: "auto" }))} />
          <span>Играет тот, на котором нажали</span>
        </label>
        {#each status.pads as pad (pad.id)}
          <label class="radio">
            <input
              type="radio"
              name="pad"
              checked={status.pad_choice.mode === "fixed" && status.pad_choice.name === pad.name}
              onchange={() => run(() => api.setPadChoice({ mode: "fixed", name: pad.name }))}
            />
            <span class="ellipsis">{pad.name}</span>
            <span class="xs muted">{familyName[pad.family]}{pad.id === status.active_pad ? " · играет" : ""}</span>
          </label>
        {:else}
          <p class="hint">Подключённых геймпадов нет.</p>
        {/each}
      </div>
      {#if status.input_error}<p class="error">{status.input_error}</p>{/if}
      <div class="sep"></div>
      <h3 class="card-title">Калибровка</h3>
      <p class="hint">Если стик в покое «плывёт» или курок не добирает до максимума.</p>
      <Calibrate {status} saved={calibration} {run} onsaved={refreshSettings} />
      <div class="sep"></div>
      <h3 class="card-title">Свой профиль для каждого типа</h3>
      <p class="hint">Взял геймпад другого типа — включился его профиль.</p>
      {#each FAMILIES as f (f)}
        <label class="field" style="--label-w: 110px">
          <span class="label">{familyName[f]}</span>
          <select class="select select-sm" value={padProfiles[f] ?? ""} onchange={(e) => run(() => setPadProfile(f, e.currentTarget.value))}>
            <option value="">не менять</option>
            {#each profiles as p (p.name)}<option value={p.name}>{p.name}</option>{/each}
          </select>
        </label>
      {/each}
    </section>

    <section class="card">
      <h2 class="card-title">Встроенный звук</h2>
      <label class="toggle">
        Встроенный синтезатор
        <input type="checkbox" class="switch" checked={status.synth.enabled} onchange={(e) => run(() => api.setSynthEnabled(e.currentTarget.checked))} />
      </label>
      <label class="toggle" title="Эксклюзивный режим Windows: звук идёт напрямую в устройство">
        Низкая задержка
        <input
          type="checkbox"
          class="switch"
          disabled={!status.synth.enabled}
          checked={status.synth.requested_mode === "exclusive"}
          onchange={(e) => run(() => api.setAudioMode(e.currentTarget.checked ? "exclusive" : "shared"))}
        />
      </label>
      <p class="hint">
        {#if status.synth.fallback}
          <span class="warn">Не включилась: {status.synth.fallback}. Играю в обычном режиме.</span>
        {:else if status.synth.mode === "exclusive"}
          Другие программы на «{status.synth.device}» сейчас не звучат.
        {:else}
          Обычный режим: звучит вместе с другими программами, задержка больше.
        {/if}
      </p>
      <div class="field" style="--value-w: 80px">
        <span class="label">Задержка</span>
        <span class="xs muted ellipsis">
          {#if latency}ввод {latency.input?.toFixed(1) ?? "—"} · блок {latency.block.toFixed(1)} · вывод {latency.output.toFixed(1)} мс{/if}
        </span>
        <span class="value" class:good={!!latency && latency.total < 15}>{latency ? `≈ ${Math.round(latency.total)} мс` : "—"}</span>
      </div>
      <label class="field" style="--value-w: 48px">
        <span class="label">Громкость</span>
        <input
          type="range"
          min="0"
          max="2"
          step="0.05"
          disabled={!status.synth.enabled}
          value={status.synth.master_volume}
          oninput={(e) => run(() => api.setMasterVolume(Number(e.currentTarget.value)))}
        />
        <span class="value">{Math.round(status.synth.master_volume * 100)}%</span>
      </label>
      <div class="field">
        <span class="label">Основной банк</span>
        <span class="ellipsis" title={status.synth.soundfont ?? ""}>{fileName(status.synth.soundfont) ?? "не найден"}</span>
      </div>
      <div class="row wrap">
        <button type="button" class="btn btn-sm" onclick={() => run(api.chooseSoundfont)}>Сменить…</button>
        {#if status.synth.custom_soundfont}
          <button type="button" class="btn btn-sm btn-ghost" onclick={() => run(api.resetSoundfont)}>По умолчанию</button>
        {/if}
        <button type="button" class="btn btn-sm btn-ghost" onclick={() => run(api.openSoundLibrary)}>Папка библиотеки звуков</button>
      </div>
      {#if status.synth.error}<p class="error">{status.synth.error}</p>{/if}
    </section>

    <section class="card">
      <h2 class="card-title">MIDI-выход</h2>
      <div class="row">
        <select
          class="select"
          style="flex: 1"
          aria-label="MIDI-выход"
          value={status.midi.port ?? ""}
          onchange={(e) => run(() => api.setMidiOut(e.currentTarget.value || null))}
        >
          <option value="">Не отправлять</option>
          {#each ports as p (p)}<option value={p}>{p}</option>{/each}
        </select>
        <button type="button" class="btn btn-icon" aria-label="Обновить список" title="Обновить список" onclick={() => run(refreshPorts)}>
          <Icon name="refresh" size={16} />
        </button>
      </div>
      {#if status.midi.error}<p class="error">{status.midi.error}</p>{/if}
      <p class="hint">
        Для DAW (Ableton, FL Studio…) создай порт в бесплатной программе loopMIDI и выбери его здесь. Встроенный синтезатор тогда лучше выключить.
      </p>
      <div class="sep"></div>
      <label class="toggle" title="Пока идёт метроном или лупер — 24 тика на долю, Start при запуске, Stop при остановке">
        <span class="stack" style="--gap: 2px"
          >MIDI Clock в DAW <span class="xs muted">В Ableton: у входа loopMIDI включи Sync</span></span
        >
        <input type="checkbox" class="switch" checked={ts.send_clock} onchange={(e) => run(() => api.setTransport({ ...ts, send_clock: e.currentTarget.checked }))} />
      </label>
      <label class="toggle">
        <span class="stack" style="--gap: 2px"
          >MMC: запись в DAW <span class="xs muted">Понимают Reaper, Cubase; Ableton — нет</span></span
        >
        <input type="checkbox" class="switch" checked={ts.send_mmc} onchange={(e) => run(() => api.setTransport({ ...ts, send_mmc: e.currentTarget.checked }))} />
      </label>
    </section>

    <section class="card">
      <h2 class="card-title">Оформление</h2>
      <div class="swatches">
        {#each PRESETS as p (p.name)}
          <button type="button" class="swatch" class:on={same(p.theme, theme)} aria-pressed={same(p.theme, theme)} onclick={() => setTheme(p.theme)}>
            <span class="pair">
              <span style="background: {p.theme.accent}"></span>
              <span style="background: {p.theme.secondary}"></span>
            </span>
            <span class="xs">{p.name}</span>
          </button>
        {/each}
      </div>
      <div class="sep"></div>
      <h3 class="card-title">Свои цвета</h3>
      <label class="field" style="--label-w: 190px; --value-w: 70px">
        <span class="label">Ноты и главные кнопки</span>
        <input type="color" class="color" value={theme.accent} oninput={(e) => setTheme({ ...theme, accent: e.currentTarget.value })} />
        <span class="value mono">{theme.accent}</span>
      </label>
      <label class="field" style="--label-w: 190px; --value-w: 70px">
        <span class="label">Модификаторы и слои</span>
        <input type="color" class="color" value={theme.secondary} oninput={(e) => setTheme({ ...theme, secondary: e.currentTarget.value })} />
        <span class="value mono">{theme.secondary}</span>
      </label>
      <div class="preview">
        <span class="btn btn-sm btn-primary">Кнопка</span>
        <span class="chip accent">A3 · ля</span>
        <span class="chip mod">LB · октава −1</span>
        <span class="spacer"></span>
        <button type="button" class="btn btn-sm btn-ghost" disabled={same(theme, DEFAULT_THEME)} onclick={() => setTheme(DEFAULT_THEME)}>Сбросить</button>
      </div>
    </section>
  </div>
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
    gap: 16px;
    align-items: start;
  }
  .sep {
    height: 1px;
    background: var(--line);
  }
  .radio {
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
    min-height: 32px;
    cursor: pointer;
  }
  .radio input {
    accent-color: var(--accent);
    margin: 0;
  }
  .warn {
    color: var(--error);
  }
  .good {
    color: var(--ok);
  }
  .swatches {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
  }
  .swatch {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 10px 6px;
    border-radius: var(--r-md);
    border: 1px solid var(--line-strong);
    background: var(--surface-2);
  }
  .swatch.on {
    border-color: var(--text);
    box-shadow: inset 0 0 0 1px var(--text);
  }
  .pair {
    display: flex;
  }
  .pair span {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 2px solid var(--surface-2);
  }
  .pair span + span {
    margin-left: -8px;
  }
  .color {
    width: 48px;
    height: 30px;
    padding: 0;
    border: 1px solid var(--line-strong);
    border-radius: var(--r-sm);
    background: none;
    cursor: pointer;
  }
  .mono {
    font-family: var(--font-mono);
    font-weight: 500;
    font-size: 12px;
  }
  .preview {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
</style>
