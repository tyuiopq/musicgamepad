<script lang="ts">
  // Звуковой банк: есть ли он, а если нет — скачать бесплатный или выбрать свой .sf2.
  import { onMount } from "svelte";
  import { api, type Status } from "$lib/api";

  let { status, run }: { status: Status; run: (action: () => Promise<unknown>) => void } = $props();

  let downloading = $state(false);
  let done = $state(0);
  let total = $state<number | null>(null);
  let error = $state<string | null>(null);

  const bank = $derived(status.synth.soundfont?.split(/[\\/]/).pop() ?? null);
  const percent = $derived(total ? Math.min(100, Math.round((done / total) * 100)) : null);
  const mb = (b: number) => (b / 1024 / 1024).toFixed(1);

  onMount(() => {
    const off = api.onBankProgress((p) => {
      done = p.done;
      total = p.total;
    });
    return () => {
      off.then((f) => f());
    };
  });

  async function download() {
    error = null;
    done = 0;
    total = null;
    downloading = true;
    try {
      await api.downloadBank();
    } catch (e) {
      error = String(e);
    } finally {
      downloading = false;
    }
  }
</script>

<div class="bank stack" style="--gap: 10px">
  {#if bank}
    <p class="ok">✓ Звуковой банк на месте: <b>{bank}</b></p>
  {:else if downloading}
    <div class="stack" style="--gap: 6px">
      <div class="row">
        <span class="grow">Скачиваю GeneralUser GS… {percent !== null ? `${percent}%` : `${mb(done)} МБ`}</span>
        <button type="button" class="btn btn-sm btn-ghost" onclick={() => run(api.cancelBankDownload)}>Отмена</button>
      </div>
      <div class="bar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={percent ?? 0}>
        <span style:width="{percent ?? 0}%"></span>
      </div>
    </div>
  {:else}
    <p>
      Для встроенного звука нужен <b>звуковой банк</b> (.sf2) — набор записанных инструментов: рояль, гитары, барабаны и ещё больше сотни.
    </p>
    <div class="row wrap">
      <button type="button" class="btn btn-primary" onclick={download}>Скачать бесплатный банк · 32 МБ</button>
      <button type="button" class="btn" onclick={() => run(api.chooseSoundfont)}>Выбрать свой .sf2…</button>
    </div>
    <p class="hint">
      GeneralUser GS от S. Christian Collins — бесплатный, можно использовать в своей музыке. Без банка программа всё равно работает как MIDI-контроллер
      для DAW.
    </p>
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
</div>

<style>
  .grow {
    flex: 1;
  }
  .ok {
    color: var(--text);
  }
  .bar {
    height: 8px;
    border-radius: 4px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  p {
    margin: 0;
  }
</style>
