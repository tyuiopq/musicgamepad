<script lang="ts">
  // Карточка «как подключить» для профилей под DAW и синтезаторы: шаги и кнопки MIDI Learn.
  import { api, type Profile, type Status } from "$lib/api";
  import { learnForInput } from "$lib/learn";

  let { profile, status, run }: { profile: Profile; status: Status; run: (action: () => Promise<unknown>) => void } = $props();

  const steps = $derived(profile.meta?.setup ?? []);
  const key = $derived(`setup:${profile.name}`);
  let open = $state(true);
  let done = $state<number[]>([]);

  // Состояние карточки — своё у каждого профиля (запоминается в этом браузере)
  $effect(() => {
    try {
      const saved = JSON.parse(localStorage.getItem(key) ?? "null");
      open = saved?.open ?? true;
      done = saved?.done ?? [];
    } catch {
      open = true;
      done = [];
    }
  });

  function save() {
    try {
      localStorage.setItem(key, JSON.stringify({ open, done }));
    } catch {
      /* без хранилища просто не запоминаем */
    }
  }

  function toggle(i: number) {
    done = done.includes(i) ? done.filter((x) => x !== i) : [...done, i];
    save();
  }

  const wantsDaw = $derived(profile.meta?.category === "daw" || profile.meta?.category === "synth");
</script>

{#if steps.length}
  <section class="setup">
    <button
      type="button"
      class="head"
      aria-expanded={open}
      onclick={() => {
        open = !open;
        save();
      }}
    >
      <span class="title">Как подключить: {profile.meta?.target}</span>
      <span class="muted">{done.length}/{steps.length} {open ? "▴" : "▾"}</span>
    </button>
    {#if open}
      {#if wantsDaw && (status.synth.enabled || !status.midi.port)}
        <p class="warn">
          {#if !status.midi.port}Сейчас MIDI-выход не выбран — DAW ничего не получит.{/if}
          {#if status.synth.enabled}
            Встроенный звук включён и будет звучать вместе с DAW.
            <button type="button" class="link" onclick={() => run(() => api.setSynthEnabled(false))}>Выключить встроенный звук</button>
          {/if}
        </p>
      {/if}
      <ol>
        {#each steps as s, i (i)}
          {@const msgs = s.learn ? learnForInput(s.learn, profile) : []}
          <li class:done={done.includes(i)}>
            <label class="check">
              <input type="checkbox" checked={done.includes(i)} onchange={() => toggle(i)} />
              <span>{s.text}</span>
            </label>
            {#if msgs.length}
              <button type="button" class="ghost" disabled={!status.midi.port} onclick={() => run(() => api.sendTest(msgs))}>
                Отправить для MIDI Learn
              </button>
            {/if}
          </li>
        {/each}
      </ol>
    {/if}
  </section>
{/if}

<style>
  .setup {
    background: var(--panel);
    border: 1px solid var(--mod);
    border-radius: 14px;
    padding: 10px 14px;
  }
  .head {
    width: 100%;
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: none;
    border: none;
    padding: 0;
    color: var(--text);
    font: inherit;
  }
  .title {
    font-weight: 600;
    color: var(--mod);
  }
  ol {
    margin: 10px 0 0;
    padding-left: 22px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    font-size: 14px;
  }
  li.done span {
    color: var(--muted);
    text-decoration: line-through;
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: baseline;
    flex: 1 1 320px;
    cursor: pointer;
  }
  .ghost {
    min-height: 30px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 13px;
  }
  .ghost:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .warn {
    margin: 8px 0 0;
    font-size: 13px;
    color: var(--accent);
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    color: var(--mod);
    font: inherit;
    text-decoration: underline;
  }
</style>
