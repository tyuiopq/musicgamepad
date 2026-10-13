<script lang="ts">
  import { onMount } from "svelte";
  import { api, type PadFamily, type Profile, type ProfileEntry, type Status } from "$lib/api";
  import Gamepad from "$lib/components/Gamepad.svelte";
  import PianoRoll from "$lib/components/PianoRoll.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import { effectLabel } from "$lib/describe";
  import { keyLabel, noteName, solfege } from "$lib/labels";

  let status = $state<Status | null>(null);
  let profile = $state<Profile | null>(null);
  let profiles = $state<ProfileEntry[]>([]);
  let ports = $state<string[]>([]);
  let error = $state<string | null>(null);

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
    api.profiles().then((p) => (profiles = p));
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
    <Sidebar {status} {profiles} {ports} {run} {refreshPorts} />

    <main>
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
        </div>
      </header>

      {#if error}<p class="error">{error}</p>{/if}

      <section class="pad-area">
        {#if status.pads.length === 0}
          <p class="muted empty">Геймпад не подключён. Ниже — раскладка профиля «{status.profile}».</p>
        {/if}
        <Gamepad {status} {family} {profile} />
      </section>

      <section class="roll">
        <div class="roll-head">
          <span class="section-title">Последние ноты</span>
          <span class="muted">10 с</span>
        </div>
        <PianoRoll notes={status.notes} {tonic} />
      </section>
    </main>
  </div>
{/if}

<style>
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
  .roll-head {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
  }
  .roll :global(.roll) {
    flex: 1;
  }
</style>
