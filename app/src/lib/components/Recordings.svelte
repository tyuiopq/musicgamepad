<script lang="ts">
  // Экран «Записи»: файлы из папки записей.
  import { api, type RecordingEntry, type Status } from "$lib/api";

  let { status, run }: { status: Status; run: (action: () => Promise<unknown>) => void } = $props();

  let items = $state<RecordingEntry[]>([]);
  let confirm = $state<string | null>(null);

  async function refresh() {
    items = await api.listRecordings();
  }

  // Новая запись или готовый WAV — обновить список
  $effect(() => {
    void status.last_recording;
    void status.transport.last_wav;
    refresh();
  });

  async function remove(name: string) {
    if (confirm !== name) {
      confirm = name;
      return;
    }
    confirm = null;
    await api.deleteRecording(name);
    await refresh();
  }

  const size = (b: number) => (b > 1024 * 1024 ? `${(b / 1024 / 1024).toFixed(1)} МБ` : `${Math.max(1, Math.round(b / 1024))} КБ`);
</script>

<section class="recordings">
  <header>
    <h1>Записи</h1>
    <button type="button" class="ghost" onclick={() => run(api.openRecordings)}>Открыть папку</button>
  </header>
  <p class="muted">
    .mid открывается в любой музыкальной программе: каждая партия — на своей дорожке, темп и размер — как в метрономе.
  </p>

  {#each items as r (r.name)}
    <div class="row">
      <div class="info">
        <b>{r.name}</b>
        <span class="muted small">{r.modified} · {size(r.bytes)}</span>
      </div>
      <div class="actions">
        {#if r.mid}<button type="button" class="ghost" onclick={() => run(() => api.openRecording(r.mid!))}>MIDI</button>{/if}
        {#if r.wav}<button type="button" class="ghost" onclick={() => run(() => api.openRecording(r.wav!))}>▶ WAV</button>{/if}
        <button type="button" class="ghost danger" onclick={() => run(() => remove(r.name))}>
          {confirm === r.name ? "Точно удалить?" : "Удалить"}
        </button>
      </div>
    </div>
  {:else}
    <p class="muted empty">Пока пусто. Нажми «● Запись» на экране «Игра» или Start на геймпаде.</p>
  {/each}
</section>

<style>
  .recordings {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 900px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 16px;
  }
  h1 {
    margin: 0;
    font-family: var(--font-display);
    font-size: 22px;
  }
  p {
    margin: 0 0 8px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 12px 14px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 12px;
  }
  .info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .info b {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .ghost {
    min-height: 36px;
    padding: 0 14px;
    border-radius: 10px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--text);
    font: inherit;
  }
  .ghost:hover {
    border-color: var(--accent);
  }
  .danger {
    color: var(--error);
  }
  .small {
    font-size: 13px;
  }
  .empty {
    padding: 24px 0;
  }
</style>
