<script lang="ts">
  import { onMount } from "svelte";
  import { api, demoOption, type PadFamily, type Profile, type ProfileEntry, type Status } from "$lib/api";
  import Editor from "$lib/components/editor/Editor.svelte";
  import Gamepad from "$lib/components/Gamepad.svelte";
  import Library from "$lib/components/Library.svelte";
  import Monitor from "$lib/components/Monitor.svelte";
  import SetupCard from "$lib/components/SetupCard.svelte";
  import PianoRoll from "$lib/components/PianoRoll.svelte";
  import Recordings from "$lib/components/Recordings.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import Transport from "$lib/components/Transport.svelte";
  import { effectLabel } from "$lib/describe";
  import { keyLabel, noteName, solfege } from "$lib/labels";

  let status = $state<Status | null>(null);
  let profile = $state<Profile | null>(null);
  let profiles = $state<ProfileEntry[]>([]);
  let ports = $state<string[]>([]);
  let error = $state<string | null>(null);
  let view = $state<"play" | "edit" | "records">((demoOption("view") as "edit" | "records" | null) ?? "play");
  let editorDirty = $state(false);
  let libraryOpen = $state(demoOption("library") !== null);
  let bottom = $state<"roll" | "monitor">(demoOption("bottom") === "monitor" ? "monitor" : "roll");

  /** Суммарный сдвиг от модификаторов: «−12 пт · +1 ступ.» */
  const offsetText = $derived.by(() => {
    if (!status) return "";
    const parts: string[] = [];
    const t = status.transpose;
    if (t) parts.push(`${t > 0 ? "+" : "−"}${Math.abs(t)} пт.`);
    const d = status.degree_shift;
    if (d) parts.push(`${d > 0 ? "+" : "−"}${Math.abs(d)} ступ.`);
    return parts.join(" · ");
  });

  async function refreshProfiles() {
    profiles = await api.profiles();
  }

  const activePad = $derived(status?.pads.find((p) => p.id === status?.active_pad) ?? null);
  const family = $derived<PadFamily>(activePad?.family ?? "xbox");
  const tonic = $derived(status?.music ? (status.music.octave + 1) * 12 + status.music.root : 60);

  // Включённые модификаторы — для строки над схемой
  const activeMods = $derived(
    (status?.controls ?? []).flatMap((v) => {
      if (v.role !== "modifier" || !v.active || v.input.kind === "axis") return [];
      const key =
        v.input.kind === "button"
          ? ({ kind: "button", button: v.input.button } as const)
          : ({ kind: "axis_switch", axis: v.input.axis, direction: v.input.direction } as const);
      return [{ key: keyLabel(key, family), label: effectLabel(v.effect, profile) }];
    }),
  );

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

  // При смене профиля подтягиваем его целиком (названия слоёв и т.п.)
  $effect(() => {
    const name = status?.profile;
    if (name && name !== profile?.name) api.profile().then((p) => (profile = p));
  });

  onMount(() => {
    const unlisten = api.onStatus((s) => (status = s));
    api.status().then((s) => (status ??= s));
    refreshProfiles();
    refreshPorts();
    return () => {
      unlisten.then((f) => f());
    };
  });
</script>

{#if !status}
  <p class="loading muted">Запускаю движок…</p>
{:else}
  <div class="app">
    <Sidebar {status} {profiles} {ports} {run} {refreshPorts} {view} onview={(v) => (view = v)} {editorDirty} onlibrary={() => (libraryOpen = true)} />
    {#if libraryOpen}
      <Library {profiles} current={status.profile} onclose={() => (libraryOpen = false)} onprofiles={refreshProfiles} {run} />
    {/if}

    <main>
      <!-- Редактор не уничтожается при уходе с экрана, иначе потерялись бы несохранённые правки -->
      <div class="view" hidden={view !== "edit"}>
        <Editor {status} {family} {profiles} onprofiles={refreshProfiles} ondirty={(d) => (editorDirty = d)} />
      </div>
      {#if view === "play"}
      <header>
        <div class="now" aria-live="polite">
          {#if status.notes.length === 0}
            <span class="hint">Нажми кнопку на геймпаде</span>
          {:else if status.notes.length === 1}
            <span class="big">{noteName(status.notes[0])}</span>
            <span class="solfege">{solfege(status.notes[0])}</span>
          {:else}
            <span class="big several">{status.notes.map(noteName).join(" ")}</span>
          {/if}
        </div>
        <div class="chips">
          {#each status.parts as part (part.channel)}
            <span class="chip">{part.instrument}</span>
          {/each}
          {#each status.layers.slice(1) as layer (layer)}
            <span class="chip mod">Слой: {layer}</span>
          {/each}
          {#each activeMods as m (m.key + m.label)}
            <span class="chip mod">{m.key} · {m.label}</span>
          {/each}
          {#if offsetText}
            <span class="chip offset" title="Суммарный сдвиг от зажатых модификаторов">Сдвиг: {offsetText}</span>
          {/if}
        </div>
        <button type="button" class="panic" title="Заглушить все ноты и сбросить защёлки (All Notes Off)" onclick={() => run(api.panic)}>
          Тишина
        </button>
      </header>

      {#if error}<p class="error">{error}</p>{/if}

      <section class="pad-area">
        {#if status.pads.length === 0}
          <p class="muted empty">Геймпад не подключён. Ниже — раскладка профиля «{status.profile}».</p>
        {/if}
        <Gamepad controls={status.controls} pressed={status.pressed} axes={status.axes} {family} {profile} />
      </section>

      {#if profile && profile.name === status.profile}
        <SetupCard {profile} {status} {run} />
      {/if}
      <Transport transport={status.transport} {run} />

      <section class="roll">
        <div class="roll-head" role="tablist" aria-label="Нижняя панель">
          <button type="button" role="tab" aria-selected={bottom === "roll"} class:on={bottom === "roll"} onclick={() => (bottom = "roll")}>
            Последние ноты
          </button>
          <button type="button" role="tab" aria-selected={bottom === "monitor"} class:on={bottom === "monitor"} onclick={() => (bottom = "monitor")}>
            MIDI-монитор
          </button>
        </div>
        {#if bottom === "roll"}
          <PianoRoll notes={status.notes} {tonic} />
        {:else}
          <Monitor />
        {/if}
      </section>
      {/if}
      {#if view === "records"}
        <Recordings {status} {run} />
      {/if}
    </main>
  </div>
{/if}

<style>
  .view[hidden] {
    display: none;
  }
  .loading {
    padding: 24px;
  }
  .app {
    display: flex;
    min-height: 100vh;
  }
  main {
    flex: 1;
    min-width: 0;
    padding: 22px 28px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 20px;
    min-height: 64px;
  }
  .now {
    display: flex;
    align-items: baseline;
    gap: 12px;
    min-width: 200px;
  }
  .big {
    font-family: var(--font-display);
    font-size: 56px;
    font-weight: 700;
    line-height: 1;
    color: var(--accent);
  }
  .big.several {
    font-size: 32px;
  }
  .solfege {
    font-size: 22px;
  }
  .hint {
    color: var(--muted);
    font-size: 18px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .pad-area {
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .empty {
    margin: 0 0 8px;
  }
  .roll {
    flex: 1;
    min-height: 180px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 14px;
    padding: 12px 14px 14px;
  }
  .panic {
    margin-left: auto;
    min-height: 38px;
    padding: 0 16px;
    border-radius: 10px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--text);
    font: inherit;
  }
  .panic:hover {
    border-color: var(--error);
    color: var(--error);
  }
  .chip.offset {
    border-color: var(--accent);
    color: var(--accent);
  }
  .roll-head button {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
    cursor: pointer;
  }
  .roll-head button.on {
    color: var(--text);
  }
  .roll-head {
    display: flex;
    gap: 18px;
    font-size: 12px;
  }
  .roll :global(.roll) {
    flex: 1;
  }
</style>
