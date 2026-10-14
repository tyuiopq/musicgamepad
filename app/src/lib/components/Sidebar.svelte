<script lang="ts">
  // Боковая панель: разделы, что подключено и куда идёт звук, запись и «Тишина».
  import { api, type ProfileEntry, type Status } from "$lib/api";
  import { familyName } from "$lib/labels";
  import Icon from "./Icon.svelte";

  type View = "play" | "edit" | "records" | "settings";

  let {
    status,
    profiles,
    run,
    view,
    onview,
    editorDirty,
    onlibrary,
  }: {
    status: Status;
    profiles: ProfileEntry[];
    run: (action: () => Promise<unknown>) => void;
    view: View;
    onview: (v: View) => void;
    editorDirty: boolean;
    onlibrary: () => void;
  } = $props();

  const NAV: [View, string, "play" | "profile" | "records" | "settings"][] = [
    ["play", "Игра", "play"],
    ["edit", "Профиль", "profile"],
    ["records", "Записи", "records"],
    ["settings", "Настройки", "settings"],
  ];

  const activePad = $derived(status.pads.find((p) => p.id === status.active_pad) ?? null);
  const padValue = $derived(status.pad_choice.mode === "auto" ? "__auto" : status.pad_choice.name);

  function time(sec: number): string {
    return `${Math.floor(sec / 60)}:${String(Math.floor(sec % 60)).padStart(2, "0")}`;
  }
  const t = $derived(status.transport);
  const recLabel = $derived(
    t.record === "count_in" ? `Отсчёт · ${t.count_in_left ?? ""}` : t.record === "recording" ? `Стоп · ${time(t.record_seconds)}` : "Запись",
  );

  const soundText = $derived(
    !status.synth.enabled
      ? "выключен"
      : status.synth.error
        ? "ошибка"
        : status.synth.latency
          ? `≈ ${Math.round(status.synth.latency.total)} мс`
          : "включён",
  );
</script>

<nav class="sidebar" aria-label="Разделы">
  <div class="brand">MusicGamepad</div>

  <div class="nav">
    {#each NAV as [id, label, icon] (id)}
      <button type="button" class:on={view === id} aria-current={view === id ? "page" : undefined} onclick={() => onview(id)}>
        <Icon name={icon} />
        <span>{label}</span>
        {#if id === "edit" && editorDirty}<span class="badge" title="Есть несохранённые изменения"></span>{/if}
      </button>
    {/each}
  </div>

  <section class="block">
    <div class="card-title">Геймпад</div>
    <div class="pad">
      <span class="led" class:on={!!activePad}></span>
      <div class="stack" style="--gap: 0">
        <span class="ellipsis">{activePad?.name ?? "не подключён"}</span>
        {#if activePad}<span class="xs muted">{familyName[activePad.family]}</span>{/if}
      </div>
    </div>
    {#if status.pads.length > 1}
      <select
        class="select select-sm"
        aria-label="Каким геймпадом играть"
        value={padValue}
        onchange={(e) =>
          run(() =>
            api.setPadChoice(e.currentTarget.value === "__auto" ? { mode: "auto" } : { mode: "fixed", name: e.currentTarget.value }),
          )}
      >
        <option value="__auto">Тот, где нажали</option>
        {#each status.pads as p (p.id)}<option value={p.name}>{p.name}</option>{/each}
      </select>
    {/if}
  </section>

  <section class="block">
    <div class="card-title">Профиль</div>
    <div class="row" style="--gap: 6px">
      <select
        class="select select-sm"
        style="flex: 1"
        aria-label="Профиль"
        value={status.profile}
        disabled={editorDirty}
        title={editorDirty ? "Сначала сохрани или отмени правки профиля" : ""}
        onchange={(e) => run(() => api.selectProfile(e.currentTarget.value))}
      >
        {#each profiles as p (p.name)}<option value={p.name}>{p.name}</option>{/each}
      </select>
      <button type="button" class="btn btn-sm btn-icon" style="width: 30px" title="Библиотека профилей" aria-label="Библиотека профилей" onclick={onlibrary}>
        <Icon name="library" size={16} />
      </button>
    </div>
  </section>

  <button type="button" class="block out" onclick={() => onview("settings")} title="Звук и MIDI — в «Настройках»">
    <span class="card-title">Выход</span>
    <span class="kv"><span class="muted">Звук</span><span class:warn={!!status.synth.error}>{soundText}</span></span>
    <span class="kv"><span class="muted">MIDI</span><span class="ellipsis" class:warn={!!status.midi.error}>{status.midi.port ?? "нет"}</span></span>
  </button>

  <div class="spacer"></div>

  <div class="actions">
    <button type="button" class="btn btn-rec btn-lg btn-block" class:on={t.record !== "idle"} onclick={() => run(api.toggleRecord)}>
      <span class="dot"></span>{recLabel}
    </button>
    <button type="button" class="btn btn-ghost btn-block" title="Заглушить все ноты (All Notes Off)" onclick={() => run(api.panic)}>
      Тишина
    </button>
  </div>
</nav>

<style>
  .sidebar {
    position: sticky;
    top: 0;
    width: 232px;
    height: 100vh;
    flex-shrink: 0;
    box-sizing: border-box;
    padding: 18px 14px;
    background: var(--surface-1);
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 14px;
    overflow-y: auto;
  }
  .brand {
    padding: 0 6px;
    font-family: var(--font-display);
    font-weight: 700;
    font-size: 16px;
  }
  .nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .nav button {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 38px;
    padding: 0 10px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--muted);
    text-align: left;
    font-size: 14px;
  }
  .nav button:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  .nav button.on {
    color: var(--text);
    background: var(--surface-3);
  }
  .nav button.on :global(svg) {
    color: var(--accent);
  }
  .badge {
    width: 7px;
    height: 7px;
    margin-left: auto;
    border-radius: 50%;
    background: var(--accent);
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: var(--r-md);
    background: var(--surface-2);
    border: 1px solid var(--line);
    min-width: 0;
  }
  .pad {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .led {
    width: 8px;
    height: 8px;
    flex-shrink: 0;
    border-radius: 50%;
    background: var(--line-strong);
  }
  .led.on {
    background: var(--ok);
  }
  .out {
    text-align: left;
    color: var(--text);
    font-size: 13px;
  }
  .out:hover {
    border-color: var(--line-strong);
  }
  .kv {
    display: grid;
    grid-template-columns: 48px minmax(0, 1fr);
    gap: 8px;
  }
  .kv > :last-child {
    text-align: right;
  }
  .warn {
    color: var(--error);
  }
  .actions {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
</style>
