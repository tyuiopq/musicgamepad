<script lang="ts">
  // Библиотека профилей: готовые раскладки по разделам, импорт и экспорт файлом.
  import { untrack } from "svelte";
  import { api, type ProfileEntry } from "$lib/api";

  let {
    profiles,
    current,
    onclose,
    onprofiles,
    run,
  }: {
    profiles: ProfileEntry[];
    current: string;
    onclose: () => void;
    onprofiles: () => Promise<void>;
    run: (action: () => Promise<unknown>) => void;
  } = $props();

  const SECTIONS: [string, string, string][] = [
    ["builtin", "Встроенный звук", "Играют сразу, без DAW."],
    ["daw", "DAW", "Через loopMIDI в Ableton, FL Studio и др. У каждого — пошаговая карточка подключения."],
    ["synth", "Синтезаторы", "Для VST-синтов в любой DAW: ручки привязываются через MIDI Learn."],
    ["mine", "Мои", "Сохранённые и импортированные профили."],
  ];
  // Открываемся на разделе текущего профиля (один раз, дальше раздел выбирает пользователь)
  let section = $state(untrack(() => profiles.find((p) => p.name === current)?.category ?? "builtin"));
  let note = $state<string | null>(null);

  const visible = $derived(profiles.filter((p) => p.category === section));

  async function pick(name: string) {
    await api.selectProfile(name);
    onclose();
  }

  async function importFile() {
    const name = await api.importProfile();
    if (name) {
      await onprofiles();
      note = `Импортирован и включён: «${name}»`;
      section = "mine";
    }
  }

  async function exportFile(name: string) {
    const path = await api.exportProfile(name);
    if (path) note = `Сохранено: ${path}`;
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Библиотека профилей">
    <header>
      <h2>Библиотека профилей</h2>
      <button type="button" class="ghost" onclick={() => run(importFile)}>Импорт из файла…</button>
      <button type="button" class="close" aria-label="Закрыть" onclick={onclose}>✕</button>
    </header>

    <div class="tabs" role="tablist" aria-label="Разделы">
      {#each SECTIONS as [id, title] (id)}
        <button type="button" role="tab" aria-selected={section === id} class:on={section === id} onclick={() => (section = id)}>
          {title}
          <span class="count">{profiles.filter((p) => p.category === id).length}</span>
        </button>
      {/each}
    </div>
    <p class="muted hint">{SECTIONS.find(([id]) => id === section)?.[2]}</p>
    {#if note}<p class="note">{note}</p>{/if}

    <div class="cards">
      {#each visible as p (p.name)}
        <article class="card" class:current={p.name === current}>
          <div class="top">
            <h3>{p.name}</h3>
            {#if p.target}<span class="target">{p.target}</span>{/if}
          </div>
          {#if p.description}<p class="desc">{p.description}</p>{/if}
          {#if p.modified}<p class="muted small">Изменён вами — вернуть заводской можно в «Профиль».</p>{/if}
          <div class="actions">
            {#if p.name === current}
              <span class="chip on">Сейчас включён</span>
            {:else}
              <button type="button" class="primary" onclick={() => run(() => pick(p.name))}>Включить</button>
            {/if}
            <button type="button" class="ghost" title="Сохранить в файл, чтобы поделиться" onclick={() => run(() => exportFile(p.name))}>
              Экспорт…
            </button>
          </div>
        </article>
      {:else}
        <p class="muted">Пока пусто. Сохрани свой профиль в «Профиль» или импортируй файл.</p>
      {/each}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
    background: rgb(0 0 0 / 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }
  .dialog {
    width: min(960px, 100%);
    max-height: 100%;
    overflow-y: auto;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: 16px;
    padding: 20px 24px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    box-sizing: border-box;
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  h2 {
    margin: 0 auto 0 0;
    font-family: var(--font-display);
    font-size: 20px;
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .tabs button {
    min-height: 36px;
    padding: 0 14px;
    border-radius: 999px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--muted);
  }
  .tabs button.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
  }
  .count {
    margin-left: 6px;
    font-size: 12px;
    opacity: 0.7;
  }
  .hint,
  .note {
    margin: 0;
    font-size: 13px;
  }
  .note {
    color: var(--mod);
    overflow-wrap: anywhere;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px 16px;
    border-radius: 12px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
  }
  .card.current {
    border-color: var(--accent);
  }
  .top {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  h3 {
    margin: 0;
    font-size: 16px;
  }
  .target {
    font-size: 12px;
    color: var(--mod);
  }
  .desc {
    margin: 0;
    font-size: 13px;
    color: var(--muted);
    line-height: 1.4;
  }
  .actions {
    margin-top: auto;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  button.primary,
  button.ghost {
    min-height: 34px;
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
  button.ghost:hover {
    border-color: var(--accent);
  }
  .close {
    width: 36px;
    height: 36px;
    border-radius: 10px;
    border: 1px solid var(--line-strong);
    background: transparent;
  }
  .chip.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .small {
    font-size: 12px;
    margin: 0;
  }
</style>
