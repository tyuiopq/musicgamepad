<script lang="ts">
  // Экран «Раскладка»: правка профиля. Правки сразу применяются к движку (их слышно),
  // а на диск попадают по кнопке «Сохранить».
  import { onMount } from "svelte";
  import {
    api,
    demoOption,
    type Binding,
    type ControlView,
    type Input,
    type PadFamily,
    type PresetInfo,
    type Profile,
    type ProfileEntry,
    type ScalePreset,
    type Status,
  } from "$lib/api";
  import { controlOfKey, type ControlId } from "$lib/controls";
  import { keyLabel } from "$lib/labels";
  import { ROOT_NAMES, SCALE_NAMES } from "$lib/music";
  import { addLayer, deleteLayer, layerTriggers, setBinding } from "$lib/profileEdit";
  import Gamepad from "../Gamepad.svelte";
  import Inspector from "./Inspector.svelte";
  import SoundPanel from "./SoundPanel.svelte";

  let {
    status,
    family,
    profiles,
    onprofiles,
    ondirty,
  }: {
    status: Status;
    family: PadFamily;
    profiles: ProfileEntry[];
    onprofiles: () => Promise<void>;
    ondirty: (dirty: boolean) => void;
  } = $props();

  let draft = $state<Profile | null>(null);
  /** Под каким именем профиль лежит на диске (null — новая копия, ещё не сохранена). */
  let savedName = $state<string | null>(null);
  /** К какому профилю вернуться по «Отменить». */
  let revertName = $state<string | null>(null);
  let dirty = $state(false);
  let layerId = $state("base");
  let selected = $state<ControlId | null>(demoSelection());

  function demoSelection(): ControlId | null {
    const s = demoOption("select");
    if (!s) return null;
    if (s.endsWith("trigger")) return { kind: "trigger", axis: s as "left_trigger" | "right_trigger" };
    if (s === "left" || s === "right") return { kind: "stick", side: s };
    return { kind: "button", button: s };
  }
  let listening = $state(false);
  let listenFrom = 0;
  let preview = $state<ControlView[]>([]);
  let error = $state<string | null>(null);
  let confirmDelete = $state(false);
  let tab = $state<"buttons" | "sound">(demoOption("tab") === "sound" ? "sound" : "buttons");
  let presets = $state<PresetInfo[]>([]);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const layer = $derived(draft?.layers.find((l) => l.id === layerId) ?? draft?.layers[0] ?? null);
  const entry = $derived(profiles.find((p) => p.name === savedName) ?? null);

  $effect(() => ondirty(dirty));

  async function load() {
    const p = await api.profile();
    draft = p;
    savedName = p.name;
    revertName = p.name;
    dirty = false;
    confirmDelete = false;
    if (!p.layers.some((l) => l.id === layerId)) layerId = "base";
    await refreshPreview();
  }

  async function refreshPreview() {
    if (!draft) return;
    try {
      preview = await api.previewProfile($state.snapshot(draft) as Profile, layerId);
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  /** Что-то поменяли: через мгновение применяем к движку и обновляем схему. */
  function changed() {
    dirty = true;
    clearTimeout(timer);
    timer = setTimeout(push, 120);
  }

  async function push() {
    timer = undefined;
    if (!draft) return;
    const snap = $state.snapshot(draft) as Profile;
    try {
      await api.applyProfile(snap);
      preview = await api.previewProfile(snap, layerId);
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  /** Послушать партию — сначала применить свежие правки, чтобы звучала новая настройка. */
  async function audition(part: number) {
    if (timer) {
      clearTimeout(timer);
      await push();
    }
    await api.audition(part);
  }

  function selectLayer(id: string) {
    layerId = id;
    refreshPreview();
  }

  function onBinding(slotInput: Input, b: Binding | null) {
    if (!layer) return;
    setBinding(layer, b?.input ?? slotInput, b?.action ?? null);
    changed();
  }

  function newLayer(): string {
    if (!draft) return "base";
    const l = addLayer(draft);
    changed();
    return l.id;
  }

  function removeLayer(id: string) {
    if (!draft) return;
    deleteLayer(draft, id);
    layerId = "base";
    changed();
  }

  async function save() {
    if (!draft) return;
    try {
      await api.saveProfile($state.snapshot(draft) as Profile, savedName);
      savedName = draft.name;
      revertName = draft.name;
      dirty = false;
      error = null;
      await onprofiles();
    } catch (e) {
      error = String(e);
    }
  }

  async function revert() {
    if (!revertName) return;
    try {
      await api.selectProfile(revertName);
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  function copy() {
    if (!draft) return;
    let name = `${draft.name} (копия)`;
    for (let i = 2; profiles.some((p) => p.name === name); i++) name = `${draft.name} (копия ${i})`;
    draft.name = name;
    savedName = null;
    changed();
  }

  async function remove() {
    if (!savedName) return;
    if (!confirmDelete) {
      confirmDelete = true;
      return;
    }
    try {
      await api.deleteProfile(savedName);
      await onprofiles();
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  function startListening() {
    listening = true;
    listenFrom = status.touched?.seq ?? 0;
  }

  // «Назначить с геймпада»: выбираем то, что нажали
  $effect(() => {
    const t = status.touched;
    if (listening && t && t.seq > listenFrom) {
      selected = controlOfKey(t.key);
      listening = false;
    }
  });

  // Профиль сменили снаружи (не в редакторе) — перечитываем
  $effect(() => {
    if (draft && !dirty && status.profile !== draft.name) load();
  });

  function inputLabel(input: Input): string {
    if (input.kind === "button") return keyLabel({ kind: "button", button: input.button }, family);
    if (input.kind === "axis_switch") return keyLabel({ kind: "axis_switch", axis: input.axis, direction: input.direction }, family);
    return input.axis;
  }

  onMount(() => {
    load();
    api.presets().then((p) => (presets = p));
    return () => clearTimeout(timer);
  });
</script>

{#if !draft || !layer}
  <p class="muted">Загружаю профиль…</p>
{:else}
  <div class="editor">
    <header class="top">
      <input
        class="name"
        aria-label="Название профиля"
        bind:value={draft.name}
        oninput={changed}
        maxlength="60"
      />
      <span class="state" class:dirty>{dirty ? "Есть несохранённые изменения" : "Сохранено"}</span>
      <div class="actions">
        {#if dirty}
          <button type="button" class="ghost" onclick={revert}>Отменить</button>
        {/if}
        <button type="button" class="primary" disabled={!dirty} onclick={save}>Сохранить</button>
        <button type="button" class="ghost" onclick={copy} title="Сделать копию профиля и править её">Копия</button>
        {#if entry?.builtin && entry.modified}
          <button type="button" class="ghost danger" onclick={remove}>
            {confirmDelete ? "Точно вернуть?" : "Вернуть заводской"}
          </button>
        {:else if entry && !entry.builtin}
          <button type="button" class="ghost danger" onclick={remove}>{confirmDelete ? "Точно удалить?" : "Удалить"}</button>
        {/if}
      </div>
    </header>

    {#if error}<p class="error">{error}</p>{/if}

    <div class="music">
      <label>
        <span class="muted">Тоника</span>
        <select
          value={draft.music.root}
          onchange={(e) => {
            draft!.music.root = Number(e.currentTarget.value);
            changed();
          }}
        >
          {#each ROOT_NAMES as name, i (i)}<option value={i}>{name}</option>{/each}
        </select>
      </label>
      <label>
        <span class="muted">Лад</span>
        <select
          value={draft.music.scale.type === "preset" ? draft.music.scale.preset : "major"}
          onchange={(e) => {
            draft!.music.scale = { type: "preset", preset: e.currentTarget.value as ScalePreset };
            changed();
          }}
        >
          {#each SCALE_NAMES as [id, name] (id)}<option value={id}>{name}</option>{/each}
        </select>
      </label>
      <label>
        <span class="muted">Октава</span>
        <select
          value={draft.music.octave}
          onchange={(e) => {
            draft!.music.octave = Number(e.currentTarget.value);
            changed();
          }}
        >
          {#each [1, 2, 3, 4, 5, 6] as o (o)}<option value={o}>{o}{o === 4 ? " (первая)" : ""}</option>{/each}
        </select>
      </label>
    </div>

    <div class="tabs" role="tablist" aria-label="Раздел профиля">
      <button type="button" role="tab" aria-selected={tab === "buttons"} class:on={tab === "buttons"} onclick={() => (tab = "buttons")}>
        Кнопки
      </button>
      <button type="button" role="tab" aria-selected={tab === "sound"} class:on={tab === "sound"} onclick={() => (tab = "sound")}>
        Звук
      </button>
    </div>

    {#if tab === "sound"}
      <SoundPanel profile={draft} {presets} onchange={changed} onaudition={audition} />
    {:else}
    <nav class="layers" aria-label="Слои раскладки">
      {#each draft.layers as l (l.id)}
        <button type="button" class:on={l.id === layerId} aria-pressed={l.id === layerId} onclick={() => selectLayer(l.id)}>
          {l.name}
        </button>
      {/each}
      <button type="button" class="add" onclick={() => selectLayer(newLayer())}>+ Слой</button>
    </nav>

    {#if layer.id !== "base"}
      {@const triggers = layerTriggers(draft, layer.id)}
      <div class="layer-bar">
        <input aria-label="Название слоя" bind:value={layer.name} oninput={changed} maxlength="40" />
        <span class="muted">
          {#if triggers.length}
            Включается: {triggers
              .map((b) => `${inputLabel(b.input)}${b.action.kind === "modifier" && b.action.latch === "toggle" ? " (переключатель)" : " (пока зажата)"}`)
              .join(", ")}
          {:else}
            Пока ничем не включается — назначь модификатор «Слой» на кнопку в основном слое.
          {/if}
        </span>
        <button type="button" class="ghost danger" onclick={() => removeLayer(layer.id)}>Удалить слой</button>
      </div>
    {/if}

    <div class="work">
      <section class="pad">
        <div class="pad-tools">
          <button type="button" class="ghost" class:listening onclick={() => (listening ? (listening = false) : startListening())}>
            {listening ? "Нажми кнопку на геймпаде… (отмена)" : "Назначить с геймпада"}
          </button>
          {#if layer.id !== "base"}
            <span class="muted small">Бледные — из основного слоя</span>
          {/if}
        </div>
        <Gamepad
          controls={preview}
          pressed={status.pressed}
          axes={status.axes}
          {family}
          profile={draft}
          {selected}
          onselect={(c) => (selected = c)}
          focusLayer={layer.id}
        />
      </section>

      <aside class="inspect">
        {#if selected}
          <Inspector control={selected} {layer} profile={draft} {family} axes={status.axes} onchange={onBinding} onnewlayer={newLayer} />
        {:else}
          <p class="muted">Выбери кнопку, курок или стик на схеме — или нажми «Назначить с геймпада» и коснись нужного.</p>
        {/if}
      </aside>
    </div>
    {/if}
  </div>
{/if}

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
  }
  .name {
    font: inherit;
    font-family: var(--font-display);
    font-size: 22px;
    font-weight: 700;
    color: var(--text);
    background: transparent;
    border: 1px solid transparent;
    border-radius: 8px;
    padding: 4px 8px;
    min-width: 0;
    flex: 0 1 360px;
  }
  .name:hover,
  .name:focus {
    border-color: var(--line-strong);
  }
  .state {
    font-size: 13px;
    color: var(--muted);
  }
  .state.dirty {
    color: var(--accent);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-left: auto;
  }
  button.primary,
  button.ghost {
    min-height: 36px;
    padding: 0 14px;
    border-radius: 10px;
    border: 1px solid var(--line-strong);
    background: transparent;
  }
  button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--bg);
    font-weight: 600;
  }
  button.primary:disabled {
    background: transparent;
    color: var(--muted);
    border-color: var(--line-strong);
    cursor: default;
  }
  button.ghost:hover {
    border-color: var(--accent);
  }
  button.danger {
    color: var(--error);
  }
  .tabs {
    display: flex;
    gap: 4px;
    padding: 4px;
    border-radius: 12px;
    background: var(--panel);
    align-self: flex-start;
  }
  .tabs button {
    min-height: 36px;
    padding: 0 18px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--muted);
  }
  .tabs button.on {
    background: var(--key);
    color: var(--text);
  }
  .music {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
  }
  .music label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
  }
  select,
  .layer-bar input {
    min-height: 36px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
    color: var(--text);
    font: inherit;
  }
  .layers {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    border-bottom: 1px solid var(--line);
    padding-bottom: 10px;
  }
  .layers button {
    min-height: 36px;
    padding: 0 14px;
    border-radius: 999px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--muted);
  }
  .layers button.on {
    border-color: var(--mod);
    background: var(--mod-soft);
    color: var(--text);
  }
  .layers .add {
    border-style: dashed;
  }
  .layer-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }
  .layer-bar span {
    flex: 1 1 240px;
  }
  .work {
    display: flex;
    flex-wrap: wrap;
    gap: 20px;
    align-items: flex-start;
  }
  .pad {
    flex: 999 1 480px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .pad-tools {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .listening {
    border-color: var(--accent) !important;
    color: var(--accent);
    animation: pulse 1.2s infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.6;
    }
  }
  .inspect {
    flex: 1 1 340px;
    max-width: 420px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 14px;
    padding: 16px;
    box-sizing: border-box;
  }
  .small {
    font-size: 13px;
  }
</style>
