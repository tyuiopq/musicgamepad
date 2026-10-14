<script lang="ts">
  // Знакомство при первом запуске: звук, геймпад, как играть.
  import { api, type PadFamily, type Status } from "$lib/api";
  import { familyName, keyLabel, noteName } from "$lib/labels";
  import BankSetup from "./BankSetup.svelte";

  let {
    status,
    family,
    run,
    onclose,
  }: {
    status: Status;
    family: PadFamily;
    run: (action: () => Promise<unknown>) => void;
    onclose: () => void;
  } = $props();

  const STEPS = ["Привет", "Звук", "Геймпад", "Как играть"] as const;
  let step = $state(0);

  const pad = $derived(status.pads.find((p) => p.id === status.active_pad) ?? status.pads[0] ?? null);
  // Нажали кнопку на геймпаде — запоминаем, чтобы показать «работает»
  let heard = $state<number | null>(null);
  let touched = $state(false);
  $effect(() => {
    if (step !== 2) return;
    if (status.pressed.length) touched = true;
    if (status.notes.length) heard = status.notes[0];
  });

  const k = (button: string) => keyLabel({ kind: "button", button }, family);
  const trig = (axis: string) => keyLabel({ kind: "axis_switch", axis, direction: "positive" }, family);

  function finish() {
    run(() => api.setOnboarded(true));
    onclose();
  }
</script>

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Знакомство с MusicGamepad">
    <ol class="steps" aria-label="Шаги">
      {#each STEPS as name, i (name)}
        <li class:on={i === step} class:done={i < step}>{name}</li>
      {/each}
    </ol>

    <div class="body">
      {#if step === 0}
        <h2>Геймпад — это инструмент</h2>
        <p>Кнопки играют ноты, курки и стики делают звук живым: громче, тише, подтяжка, вибрато. Можно играть встроенным звуком или отправлять MIDI в Ableton, FL Studio и другие программы.</p>
        <p class="muted">Займёт минуту: проверим звук и геймпад и покажем, где что.</p>
      {:else if step === 1}
        <h2>Звук</h2>
        <BankSetup {status} {run} />
      {:else if step === 2}
        <h2>Геймпад</h2>
        {#if pad}
          <p>Подключён: <b>{pad.name}</b> <span class="muted">({familyName[pad.family]})</span>{status.pads.length > 1 ? ` и ещё ${status.pads.length - 1}` : ""}.</p>
          {#if heard !== null}
            <p class="ok">✓ Работает — это была нота {noteName(heard)}. Понажимай ещё!</p>
          {:else if touched}
            <p class="ok">✓ Геймпад слышно. Если звука нет — проверь громкость или шаг «Звук».</p>
          {:else}
            <p class="hint-big">Нажми кнопку {k("south")} на геймпаде.</p>
          {/if}
        {:else}
          <p class="hint-big">Подключи геймпад — проводом или по Bluetooth. Подойдут Xbox, PlayStation, Nintendo и большинство других.</p>
        {/if}
        <p class="muted">Несколько геймпадов можно включить сразу — в «Настройках», у каждого может быть свой профиль.</p>
      {:else}
        <h2>Как играть</h2>
        <ul class="tips">
          <li><b>Кнопки и крестовина</b> — ноты лада. Тонику и лад меняешь на экране «Профиль», фальшивых нот не бывает.</li>
          <li><b>{k("left_shoulder")} / {k("right_shoulder")}</b> — октава ниже / выше, пока зажаты.</li>
          <li><b>{trig("right_trigger")}</b> — громкость, <b>{trig("left_trigger")}</b> — полутон выше, <b>правый стик</b> — подтяжка и вибрато.</li>
          <li><b>Нажатие левого стика</b> — слой аккордов: те же кнопки играют аккорды.</li>
          <li><b>{k("start")}</b> — запись, <b>{k("back")}</b> — следующий инструмент.</li>
          <li>Всё это — профиль «Классика». Любую кнопку можно переназначить на экране <b>«Профиль»</b>, а готовые раскладки для DAW и синтезаторов лежат в <b>библиотеке</b>.</li>
          <li>Забыл, что где? Кнопка <b>?</b> слева вверху или F1.</li>
        </ul>
      {/if}
    </div>

    <footer>
      {#if step < STEPS.length - 1}
        <button type="button" class="btn-link" onclick={finish}>Пропустить</button>
      {/if}
      <span class="spacer"></span>
      {#if step > 0}<button type="button" class="btn btn-ghost" onclick={() => step--}>Назад</button>{/if}
      {#if step < STEPS.length - 1}
        <button type="button" class="btn btn-primary" onclick={() => step++}>Дальше</button>
      {:else}
        <button type="button" class="btn btn-primary" onclick={finish}>Играть</button>
      {/if}
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 20;
    background: rgb(0 0 0 / 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }
  .dialog {
    width: min(620px, 100%);
    min-height: 420px;
    max-height: 100%;
    overflow-y: auto;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: 16px;
    padding: 22px 26px;
    display: flex;
    flex-direction: column;
    gap: 18px;
    box-sizing: border-box;
  }
  .steps {
    display: flex;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .steps li {
    flex: 1;
    padding-top: 8px;
    border-top: 3px solid var(--surface-3);
    font-size: 12px;
    color: var(--muted);
  }
  .steps li.done {
    border-color: var(--accent-soft);
  }
  .steps li.on {
    border-color: var(--accent);
    color: var(--text);
  }
  .body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  h2 {
    margin: 0;
    font-family: var(--font-display);
    font-size: 24px;
  }
  p {
    margin: 0;
    line-height: 1.5;
  }
  .ok {
    color: var(--accent);
  }
  .hint-big {
    font-size: 17px;
  }
  .tips {
    margin: 0;
    padding-left: 18px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    line-height: 1.45;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .spacer {
    flex: 1;
  }
</style>
