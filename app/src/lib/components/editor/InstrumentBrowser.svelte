<script lang="ts">
  // Выбор инструмента партии из звукового банка: группы General MIDI, поиск, избранное.
  import type { Part, PresetInfo } from "$lib/api";
  import { groupPresets, isDrums } from "$lib/sound";

  let {
    presets,
    part,
    onpick,
    onstar,
  }: {
    presets: PresetInfo[];
    part: Part;
    onpick: (p: PresetInfo) => void;
    onstar: (program: number) => void;
  } = $props();

  let query = $state("");
  const drums = $derived(isDrums(part));
  const groups = $derived(groupPresets(presets, drums, query));
  const isCurrent = (p: PresetInfo) => p.bank === part.bank && p.program === part.program;
</script>

<div class="browser">
  <input type="search" placeholder="Найти инструмент…" aria-label="Поиск инструмента" bind:value={query} />
  <div class="list">
    {#each groups as g (g.title)}
      <div class="group">
        <div class="section-title">{g.title}</div>
        {#each g.presets as p (`${p.bank}:${p.program}`)}
          <div class="row" class:current={isCurrent(p)}>
            <button type="button" class="pick" aria-pressed={isCurrent(p)} onclick={() => onpick(p)}>
              <span class="num">{p.program + 1}</span>
              <span class="name">{p.name}</span>
            </button>
            {#if !drums && p.bank === 0}
              {@const starred = part.instruments.includes(p.program)}
              <button
                type="button"
                class="star"
                class:on={starred}
                aria-pressed={starred}
                aria-label={starred ? "Убрать из быстрого списка" : "В быстрый список"}
                title="Быстрый список: по нему ходит команда «следующий инструмент»"
                onclick={() => onstar(p.program)}
              >
                ★
              </button>
            {/if}
          </div>
        {/each}
      </div>
    {:else}
      <p class="muted">Ничего не нашлось.</p>
    {/each}
  </div>
</div>

<style>
  .browser {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
  }
  input {
    font: inherit;
    color: var(--text);
    min-height: 36px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
  }
  .list {
    max-height: 420px;
    overflow-y: auto;
    padding-right: 4px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .group .section-title {
    margin-bottom: 4px;
    font-size: 11px;
  }
  .row {
    display: flex;
    align-items: center;
    border-radius: 8px;
  }
  .row.current {
    background: var(--accent-soft);
  }
  .pick {
    flex: 1;
    display: flex;
    gap: 10px;
    align-items: baseline;
    min-height: 32px;
    padding: 0 8px;
    border: none;
    background: transparent;
    text-align: left;
    min-width: 0;
  }
  .row:hover {
    background: var(--key);
  }
  .row.current .name {
    color: var(--accent);
    font-weight: 600;
  }
  .num {
    width: 28px;
    flex-shrink: 0;
    color: var(--muted);
    font-size: 12px;
    text-align: right;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .star {
    width: 32px;
    height: 32px;
    border: none;
    background: transparent;
    color: var(--line-strong);
    font-size: 16px;
  }
  .star.on {
    color: var(--accent);
  }
  .star:hover {
    color: var(--accent);
  }
</style>
