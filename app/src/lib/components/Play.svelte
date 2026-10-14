<script lang="ts">
  // Экран «Игра»: что звучит, схема геймпада, запись/метроном/лупер, лента нот или MIDI-монитор.
  // Сетка жёсткая: блок нот фиксированной ширины, метки обрезаются, колонка справа постоянной ширины.
  import { demoOption, type PadFamily, type Profile, type Status } from "$lib/api";
  import { effectLabel } from "$lib/describe";
  import { keyLabel, noteName, solfege } from "$lib/labels";
  import BankSetup from "./BankSetup.svelte";
  import Gamepad from "./Gamepad.svelte";
  import Monitor from "./Monitor.svelte";
  import PianoRoll from "./PianoRoll.svelte";
  import SetupCard from "./SetupCard.svelte";
  import Transport from "./Transport.svelte";

  let {
    status,
    profile,
    family,
    run,
  }: { status: Status; profile: Profile | null; family: PadFamily; run: (action: () => Promise<unknown>) => void } = $props();

  let bottom = $state<"roll" | "monitor">(demoOption("bottom") === "monitor" ? "monitor" : "roll");
  const tonic = $derived(status.music ? (status.music.octave + 1) * 12 + status.music.root : 60);

  /** До четырёх нот крупно, остальное — «+N». */
  const shown = $derived(status.notes.slice(0, 4));
  const extra = $derived(Math.max(0, status.notes.length - 4));

  const offsetText = $derived.by(() => {
    const parts: string[] = [];
    if (status.transpose) parts.push(`${status.transpose > 0 ? "+" : "−"}${Math.abs(status.transpose)} пт.`);
    if (status.degree_shift) parts.push(`${status.degree_shift > 0 ? "+" : "−"}${Math.abs(status.degree_shift)} ступ.`);
    return parts.join(" · ");
  });

  const activeMods = $derived(
    status.controls.flatMap((v) => {
      if (v.role !== "modifier" || !v.active || v.input.kind === "axis") return [];
      const key =
        v.input.kind === "button"
          ? ({ kind: "button", button: v.input.button } as const)
          : ({ kind: "axis_switch", axis: v.input.axis, direction: v.input.direction } as const);
      return [{ key: keyLabel(key, family), label: effectLabel(v.effect, profile) }];
    }),
  );
</script>

<div class="play">
  {#if status.synth.enabled && !status.synth.soundfont}
    <section class="card">
      <BankSetup {status} {run} />
    </section>
  {/if}
  <header class="nowbar">
    <div class="now" aria-live="polite">
      {#if shown.length === 0}
        <span class="idle">Нажми кнопку на геймпаде</span>
      {:else if shown.length === 1}
        <span class="note">{noteName(shown[0])}</span>
        <span class="solfege">{solfege(shown[0])}</span>
      {:else}
        <span class="notes">{shown.map(noteName).join(" ")}{extra ? ` +${extra}` : ""}</span>
      {/if}
    </div>
    <div class="info">
      <div class="line">
        {#each status.parts as part (part.channel)}
          <span class="chip" title={part.error ?? ""} class:bad={!!part.error}>{part.instrument}</span>
        {/each}
        <span class="chip faint">{status.profile}</span>
      </div>
      <div class="line">
        {#each status.layers.slice(1) as layer (layer)}<span class="chip mod">Слой: {layer}</span>{/each}
        {#each activeMods as m (m.key + m.label)}<span class="chip mod">{m.key} · {m.label}</span>{/each}
        {#if offsetText}<span class="chip accent">Сдвиг {offsetText}</span>{/if}
        {#if !status.layers.slice(1).length && !activeMods.length && !offsetText}
          <span class="xs faint">Модификаторы не зажаты</span>
        {/if}
      </div>
    </div>
  </header>

  {#if profile && profile.name === status.profile}
    <SetupCard {profile} {status} {run} />
  {/if}

  <div class="center">
    <section class="card pad-card">
      {#if status.pads.length === 0}
        <p class="hint">Геймпад не подключён — ниже раскладка профиля «{status.profile}».</p>
      {/if}
      <Gamepad controls={status.controls} pressed={status.pressed} axes={status.axes} {family} {profile} />
    </section>
    <aside class="side">
      <Transport transport={status.transport} {run} external={status.midi.clock_external ?? false} />
    </aside>
  </div>

  <section class="card bottom">
    <div class="seg tabs" role="tablist" aria-label="Нижняя панель">
      <button type="button" role="tab" aria-selected={bottom === "roll"} class:on={bottom === "roll"} onclick={() => (bottom = "roll")}>
        Последние ноты
      </button>
      <button type="button" role="tab" aria-selected={bottom === "monitor"} class:on={bottom === "monitor"} onclick={() => (bottom = "monitor")}>
        MIDI-монитор
      </button>
    </div>
    <div class="bottom-body">
      {#if bottom === "roll"}
        <PianoRoll notes={status.notes} {tonic} />
      {:else}
        <Monitor {family} />
      {/if}
    </div>
  </section>
</div>

<style>
  .play {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .nowbar {
    display: grid;
    grid-template-columns: 260px minmax(0, 1fr);
    gap: 20px;
    align-items: center;
    height: 72px;
  }
  .now {
    display: flex;
    align-items: baseline;
    gap: 10px;
    overflow: hidden;
    white-space: nowrap;
  }
  .note {
    font-family: var(--font-display);
    font-size: 48px;
    font-weight: 700;
    line-height: 1;
    color: var(--accent);
  }
  .notes {
    font-family: var(--font-display);
    font-size: 24px;
    font-weight: 700;
    color: var(--accent);
  }
  .solfege {
    font-size: 20px;
  }
  .idle {
    color: var(--muted);
    font-size: 16px;
  }
  .info {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .line {
    display: flex;
    gap: 6px;
    align-items: center;
    height: 26px;
    overflow: hidden;
    mask-image: linear-gradient(to right, #000 calc(100% - 40px), transparent);
  }
  .chip.faint {
    color: var(--muted);
  }
  .chip.bad {
    border-color: var(--error);
    color: var(--error);
  }
  .center {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 300px;
    gap: 16px;
    align-items: start;
  }
  @media (max-width: 1080px) {
    .center {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .pad-card {
    justify-content: center;
    min-height: 100%;
  }
  .bottom {
    gap: 10px;
  }
  .tabs {
    align-self: flex-start;
    width: 300px;
  }
  .bottom-body {
    height: 210px;
    display: flex;
    flex-direction: column;
  }
  .bottom-body > :global(*) {
    flex: 1;
  }
</style>
