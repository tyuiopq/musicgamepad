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

  /** Отправить пробные сообщения — тоже после применения свежих правок. */
  async function sendTest(msgs: [number, number[]][]) {
    if (timer) {
      clearTimeout(timer);
      await push();
    }
    await api.sendTest(msgs);
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
    <header class="card head">
      <div class="title-row">
        <input class="name" aria-label="Название профиля" bind:value={draft.name} oninput={changed} maxlength="60" />
        <span class="status" class:dirty>{dirty ? "Не сохранено" : "Сохранено"}</span>
        <span class="spacer"></span>
        {#if dirty}<button type="button" class="btn btn-ghost" onclick={revert}>Отменить</button>{/if}
        <button type="button" class="btn btn-primary" disabled={!dirty} onclick={save}>Сохранить</button>
        <button type="button" class="btn btn-ghost" onclick={copy} title="Сделать копию профиля и править её">Копия</button>
        {#if entry?.builtin && entry.modified}
          <button type="button" class="btn btn-danger" onclick={remove}>{confirmDelete ? "Точно вернуть?" : "Вернуть заводской"}</button>
        {:else if entry && !entry.builtin}
          <button type="button" class="btn btn-danger" onclick={remove}>{confirmDelete ? "Точно удалить?" : "Удалить"}</button>
        {/if}
      </div>
      <div class="toolbar">
        <div class="seg tabs" role="tablist" aria-label="Раздел профиля">
          <button type="button" role="tab" aria-selected={tab === "buttons"} class:on={tab === "buttons"} onclick={() => (tab = "buttons")}>Кнопки</button>
          <button type="button" role="tab" aria-selected={tab === "sound"} class:on={tab === "sound"} onclick={() => (tab = "sound")}>Звук</button>
        </div>
        <span class="spacer"></span>
        <label class="mini">
          <span>Тоника</span>
          <select
            class="select select-sm"
            value={draft.music.root}
            onchange={(e) => {
              draft!.music.root = Number(e.currentTarget.value);
              changed();
            }}
          >
            {#each ROOT_NAMES as name, i (i)}<option value={i}>{name}</option>{/each}
          </select>
        </label>
        <label class="mini">
          <span>Лад</span>
          <select
            class="select select-sm"
            value={draft.music.scale.type === "preset" ? draft.music.scale.preset : "major"}
            onchange={(e) => {
              draft!.music.scale = { type: "preset", preset: e.currentTarget.value as ScalePreset };
              changed();
            }}
          >
            {#each SCALE_NAMES as [id, name] (id)}<option value={id}>{name}</option>{/each}
          </select>
        </label>
        <label class="mini">
          <span>Октава</span>
          <select
            class="select select-sm"
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
    </header>

    {#if error}<p class="error">{error}</p>{/if}

    {#if tab === "sound"}
      <SoundPanel profile={draft} {presets} onchange={changed} onaudition={audition} ontest={sendTest} />
    {:else}
      <div class="work">
        <section class="card pad">
          <div class="layer-row">
            <div class="layers" role="tablist" aria-label="Слои раскладки">
              {#each draft.layers as l (l.id)}
                <button type="button" role="tab" aria-selected={l.id === layerId} class:on={l.id === layerId} onclick={() => selectLayer(l.id)}>
                  {l.name}
                </button>
              {/each}
              <button type="button" class="add" onclick={() => selectLayer(newLayer())}>+ Слой</button>
            </div>
            <button type="button" class="btn btn-sm" class:listening onclick={() => (listening ? (listening = false) : startListening())}>
              {listening ? "Нажми на геймпаде… (отмена)" : "Назначить с геймпада"}
            </button>
          </div>

          {#if layer.id !== "base"}
            {@const triggers = layerTriggers(draft, layer.id)}
            <div class="layer-bar">
              <div class="row">
                <input class="input input-sm" style="width: 180px" aria-label="Название слоя" bind:value={layer.name} oninput={changed} maxlength="40" />
                <span class="hint ellipsis" style="flex: 1">
                  {#if layer.combo?.length}
                    Включается комбинацией слоёв.
                  {:else if triggers.length}
                    Включается: {triggers
                      .map((b) => `${inputLabel(b.input)}${b.action.kind === "modifier" && b.action.latch === "toggle" ? " (переключатель)" : " (пока зажата)"}`)
                      .join(", ")}
                  {:else}
                    Пока ничем не включается — назначь модификатор «Слой» в основном слое.
                  {/if}
                </span>
                <button type="button" class="btn btn-sm btn-danger" onclick={() => removeLayer(layer.id)}>Удалить слой</button>
              </div>
              <details class="combo">
                <summary>
                  {layer.combo?.length
                    ? `Комбинация: ${layer.combo.map((id) => draft!.layers.find((l) => l.id === id)?.name ?? id).join(" + ")}`
                    : "Сделать комбинацией слоёв…"}
                </summary>
                <p class="hint">
                  Слой включится сам, когда включены все отмеченные (например, LB + RB). Комбинация важнее своих частей; среди остальных
                  главнее тот, что правее во вкладках.
                </p>
                <div class="row wrap" style="--gap: 14px">
                  {#each draft.layers.filter((l) => l.id !== "base" && l.id !== layer.id) as other (other.id)}
                    <label class="row" style="--gap: 6px">
                      <input
                        type="checkbox"
                        checked={layer.combo?.includes(other.id) ?? false}
                        onchange={(e) => {
                          const set = new Set(layer.combo ?? []);
                          if (e.currentTarget.checked) set.add(other.id);
                          else set.delete(other.id);
                          layer.combo = [...set];
                          changed();
                        }}
                      />
                      {other.name}
                    </label>
                  {/each}
                </div>
              </details>
            </div>
          {/if}

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
          <p class="hint center">
            {layer.id !== "base" ? "Бледные — назначения из основного слоя. " : ""}Кликни по кнопке, курку или стику, чтобы настроить.
          </p>
        </section>

        <aside class="card inspect">
          {#if selected}
            <Inspector
              control={selected}
              {layer}
              profile={draft}
              {family}
              axes={status.axes}
              {presets}
              midiPort={status.midi.port}
              onchange={onBinding}
              onnewlayer={newLayer}
            />
          {:else}
            <p class="hint">Выбери кнопку, курок или стик на схеме — или нажми «Назначить с геймпада» и коснись нужного.</p>
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
    gap: 16px;
  }
  .head {
    gap: 14px;
  }
  .title-row,
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .name {
    flex: 0 1 380px;
    min-width: 160px;
    height: 40px;
    padding: 0 10px;
    margin-left: -10px;
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    background: transparent;
    font-family: var(--font-display);
    font-size: 20px;
    font-weight: 700;
  }
  .name:hover,
  .name:focus {
    border-color: var(--line-strong);
    outline: none;
  }
  .status {
    width: 110px;
    font-size: 13px;
    color: var(--muted);
  }
  .status.dirty {
    color: var(--accent);
  }
  .tabs {
    width: 220px;
  }
  .mini {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: var(--muted);
  }
  .work {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 380px;
    gap: 16px;
    align-items: start;
  }
  @media (max-width: 1100px) {
    .work {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .layer-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .layers {
    flex: 1;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    min-width: 0;
  }
  .layers button {
    height: 30px;
    padding: 0 12px;
    border-radius: 999px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--muted);
    font-size: 13px;
  }
  .layers button:hover {
    color: var(--text);
  }
  .layers button.on {
    border-color: var(--mod);
    background: var(--mod-soft);
    color: var(--text);
  }
  .layers .add {
    border-style: dashed;
  }
  .listening {
    border-color: var(--accent);
    color: var(--accent);
    animation: pulse 1.2s infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.6;
    }
  }
  .layer-bar {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    border-radius: var(--r-md);
    background: var(--surface-2);
    border: 1px solid var(--line);
  }
  .combo summary {
    cursor: pointer;
    font-size: 13px;
    color: var(--mod);
  }
  .combo[open] summary {
    margin-bottom: 6px;
  }
  .combo label {
    font-size: 13px;
  }
  .combo input {
    accent-color: var(--accent);
  }
  .center {
    text-align: center;
  }
  .inspect {
    position: sticky;
    top: 20px;
    max-height: calc(100vh - 40px);
    overflow-y: auto;
  }
</style>
