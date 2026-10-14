<script lang="ts">
  // Подсказка «?»: что делают кнопки прямо сейчас и короткий словарик.
  import type { ControlView, PadFamily, Profile, Status } from "$lib/api";
  import { roleLabel } from "$lib/describe";
  import { inputName, noteName } from "$lib/labels";

  let {
    status,
    profile,
    family,
    onclose,
    onwelcome,
  }: {
    status: Status;
    profile: Profile | null;
    family: PadFamily;
    onclose: () => void;
    onwelcome: () => void;
  } = $props();

  const GROUPS: [string, (v: ControlView) => boolean][] = [
    ["Ноты и аккорды", (v) => v.role === "note"],
    ["Модификаторы и слои", (v) => v.role === "modifier"],
    ["Курки и стики", (v) => v.role === "control"],
    ["Команды и кнопки для DAW", (v) => v.role === "command" || v.role === "cc"],
  ];

  function what(v: ControlView): string {
    if (v.role === "note") {
      const notes = v.notes.map(noteName).join(" ");
      return `${v.chord ? "аккорд " : ""}${notes}${v.latch === "toggle" ? " · защёлка" : ""}`;
    }
    const text = roleLabel(v, profile);
    return v.role === "modifier" && v.latch === "toggle" ? `${text} · переключатель` : text;
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="drawer" role="dialog" aria-modal="true" aria-label="Подсказка">
    <header>
      <h2>Подсказка</h2>
      <button type="button" class="btn btn-ghost btn-icon" aria-label="Закрыть" onclick={onclose}>✕</button>
    </header>

    <section>
      <h3 class="card-title">Что делают кнопки сейчас</h3>
      <p class="muted xs">
        Профиль «{status.profile}»{status.layers.length > 1 ? `, включены слои: ${status.layers.slice(1).join(", ")}` : ""}. Зажми модификатор — список
        поменяется.
      </p>
      {#each GROUPS as [title, test] (title)}
        {@const items = status.controls.filter(test)}
        {#if items.length}
          <h4>{title}</h4>
          <dl>
            {#each items as v, i (i)}
              <dt>{inputName(v.input, family)}</dt>
              <dd>{what(v)}</dd>
            {/each}
          </dl>
        {/if}
      {/each}
    </section>

    <section>
      <h3 class="card-title">Словарик</h3>
      <dl class="words">
        <dt>Лад и ступени</dt>
        <dd>Кнопки играют не конкретные ноты, а ступени лада от тоники — поэтому мимо не сыграть. Тоника и лад — в шапке экрана «Профиль».</dd>
        <dt>Модификатор</dt>
        <dd>Пока зажат (или после нажатия, если это переключатель) меняет остальные кнопки: октава, полутон, слой, сила, педаль.</dd>
        <dt>Слой</dt>
        <dd>Вторая раскладка поверх основной. Что в слое не назначено — работает как в основном.</dd>
        <dt>Партия</dt>
        <dd>Инструмент на своём MIDI-канале. Партий может быть несколько — например, бас и барабаны одновременно.</dd>
        <dt>Защёлка</dt>
        <dd>Нажал — звучит, нажал ещё раз — выключилось. Удобно для педалей и долгих аккордов.</dd>
      </dl>
    </section>

    <section>
      <h3 class="card-title">Клавиши</h3>
      <dl class="words">
        <dt>F1</dt>
        <dd>эта подсказка</dd>
        <dt>Ctrl+Z / Ctrl+Y</dt>
        <dd>отменить / вернуть правку на экране «Профиль»</dd>
      </dl>
    </section>

    <button type="button" class="btn" onclick={onwelcome}>Показать знакомство заново</button>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 15;
    background: rgb(0 0 0 / 0.45);
    display: flex;
    justify-content: flex-end;
  }
  .drawer {
    width: min(440px, 100%);
    height: 100%;
    overflow-y: auto;
    background: var(--panel);
    border-left: 1px solid var(--line-strong);
    padding: 18px 22px 24px;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font-family: var(--font-display);
    font-size: 22px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h4 {
    margin: 10px 0 2px;
    font-size: 13px;
    color: var(--muted);
    font-weight: 600;
  }
  dl {
    margin: 0;
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 14px;
    font-size: 14px;
  }
  dt {
    font-weight: 600;
    white-space: nowrap;
  }
  dd {
    margin: 0;
    color: var(--muted);
  }
  .words {
    grid-template-columns: 1fr;
    gap: 2px;
  }
  .words dd {
    margin-bottom: 8px;
  }
  p {
    margin: 0;
  }
</style>
