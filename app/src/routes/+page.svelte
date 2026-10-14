<script lang="ts">
  import { onMount } from "svelte";
  import { api, demoOption, type PadFamily, type Profile, type ProfileEntry, type Status } from "$lib/api";
  import Editor from "$lib/components/editor/Editor.svelte";
  import Library from "$lib/components/Library.svelte";
  import Play from "$lib/components/Play.svelte";
  import Recordings from "$lib/components/Recordings.svelte";
  import Settings from "$lib/components/Settings.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import { applyTheme, DEFAULT_THEME, validTheme, type Theme } from "$lib/theme";

  type View = "play" | "edit" | "records" | "settings";

  let status = $state<Status | null>(null);
  let profile = $state<Profile | null>(null);
  let profiles = $state<ProfileEntry[]>([]);
  let error = $state<string | null>(null);
  let view = $state<View>((demoOption("view") as View | null) ?? "play");
  let editorDirty = $state(false);
  let libraryOpen = $state(demoOption("library") !== null);
  let theme = $state<Theme>(DEFAULT_THEME);

  const activePad = $derived(status?.pads.find((p) => p.id === status?.active_pad) ?? null);
  const family = $derived<PadFamily>(activePad?.family ?? "xbox");

  async function refreshProfiles() {
    profiles = await api.profiles();
  }

  async function run(action: () => Promise<unknown>) {
    try {
      error = null;
      await action();
    } catch (e) {
      error = String(e);
    }
  }

  // При смене профиля подтягиваем его целиком (названия слоёв, шаги подключения…)
  $effect(() => {
    const name = status?.profile;
    if (name && name !== profile?.name) api.profile().then((p) => (profile = p));
  });

  onMount(() => {
    const unlisten = api.onStatus((s) => (status = s));
    api.status().then((s) => (status ??= s));
    refreshProfiles();
    api.settings().then((s) => {
      theme = validTheme({ accent: s.ui?.accent ?? undefined, secondary: s.ui?.secondary ?? undefined });
      applyTheme(theme);
    });
    return () => {
      unlisten.then((f) => f());
    };
  });
</script>

{#if !status}
  <p class="loading muted">Запускаю движок…</p>
{:else}
  <div class="app">
    <Sidebar {status} {profiles} {run} {view} onview={(v) => (view = v)} {editorDirty} onlibrary={() => (libraryOpen = true)} />
    {#if libraryOpen}
      <Library {profiles} current={status.profile} onclose={() => (libraryOpen = false)} onprofiles={refreshProfiles} {run} />
    {/if}

    <main>
      {#if error}
        <div class="toast" role="alert">
          <span>{error}</span>
          <button type="button" class="btn btn-sm btn-ghost" onclick={() => (error = null)}>Закрыть</button>
        </div>
      {/if}

      <!-- Редактор не уничтожается при уходе с экрана, иначе потерялись бы несохранённые правки -->
      <div class="view" hidden={view !== "edit"}>
        <Editor {status} {family} {profiles} onprofiles={refreshProfiles} ondirty={(d) => (editorDirty = d)} active={view === "edit"} />
      </div>
      {#if view === "play"}
        <Play {status} {profile} {family} {run} />
      {:else if view === "records"}
        <Recordings {status} {run} />
      {:else if view === "settings"}
        <Settings {status} {profiles} {run} {theme} ontheme={(t) => (theme = t)} />
      {/if}
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
    padding: 20px 24px 24px;
    box-sizing: border-box;
    max-width: 1440px;
  }
  .view[hidden] {
    display: none;
  }
  .toast {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
    padding: 8px 8px 8px 14px;
    border-radius: var(--r-md);
    border: 1px solid var(--error);
    background: color-mix(in srgb, var(--error) 10%, var(--surface-1));
    color: var(--error);
    font-size: 13px;
  }
</style>
