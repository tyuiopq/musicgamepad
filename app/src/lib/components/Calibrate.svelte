<script lang="ts">
  // Мастер калибровки: центр стиков в покое и полный ход стиков и курков — для играющего геймпада.
  import { api, type AxisCal, type Calibration, type Status } from "$lib/api";

  let {
    status,
    saved,
    run,
    onsaved,
  }: {
    status: Status;
    /** Сохранённые калибровки по именам геймпадов. */
    saved: Record<string, Calibration>;
    run: (action: () => Promise<unknown>) => void;
    onsaved: () => void;
  } = $props();

  const AXES: [string, string][] = [
    ["left_x", "Левый стик ↔"],
    ["left_y", "Левый стик ↕"],
    ["right_x", "Правый стик ↔"],
    ["right_y", "Правый стик ↕"],
    ["left_trigger", "Левый курок"],
    ["right_trigger", "Правый курок"],
  ];
  const isTrigger = (a: string) => a.endsWith("trigger");

  const pad = $derived(status.pads.find((p) => p.id === status.active_pad) ?? status.pads[0] ?? null);
  const raw = (a: string) => status.raw_axes?.[a] ?? 0;

  let step = $state<"idle" | "center" | "range">("idle");
  let axes = $state<Record<string, AxisCal>>({});

  function begin() {
    step = "center";
  }

  function takeCenter() {
    axes = Object.fromEntries(AXES.map(([a]) => [a, { center: raw(a), min: raw(a), max: raw(a) }]));
    step = "range";
  }

  // Пока идёт замер хода — раздвигаем границы по сырым значениям
  $effect(() => {
    if (step !== "range") return;
    const values = status.raw_axes ?? {};
    for (const [a] of AXES) {
      const v = values[a];
      const c = axes[a];
      if (v === undefined || !c) continue;
      if (v < c.min) c.min = v;
      if (v > c.max) c.max = v;
    }
  });

  function save() {
    if (!pad) return;
    const name = pad.name;
    run(async () => {
      await api.setCalibration(name, { axes: $state.snapshot(axes) });
      step = "idle";
      onsaved();
    });
  }

  function reset() {
    if (!pad) return;
    const name = pad.name;
    run(async () => {
      await api.setCalibration(name, null);
      onsaved();
    });
  }

  /** Положение 0..1 на полоске (стики −1..1, курки 0..1). */
  const pos = (a: string, v: number) => (isTrigger(a) ? v : (v + 1) / 2) * 100;
</script>

{#if pad}
  <div class="stack" style="--gap: 8px">
    <div class="row">
      <span class="grow xs muted">
        {saved[pad.name] ? `«${pad.name}» откалиброван.` : `«${pad.name}» без калибровки — стики и курки как есть.`}
      </span>
      {#if step === "idle"}
        {#if saved[pad.name]}<button type="button" class="btn btn-sm btn-ghost" onclick={reset}>Сбросить</button>{/if}
        <button type="button" class="btn btn-sm" onclick={begin}>Откалибровать</button>
      {:else}
        <button type="button" class="btn btn-sm btn-ghost" onclick={() => (step = "idle")}>Отмена</button>
      {/if}
    </div>

    {#if step === "center"}
      <p class="hint">1. Отпусти стики и курки и нажми «Дальше» — так запомнится их положение в покое.</p>
      <div><button type="button" class="btn btn-sm btn-primary" onclick={takeCenter}>Дальше</button></div>
    {:else if step === "range"}
      <p class="hint">2. Покрути оба стика по кругу до упора и до конца выжми курки. Потом — «Сохранить».</p>
      <div class="bars">
        {#each AXES as [a, label] (a)}
          {@const c = axes[a]}
          <span class="xs">{label}</span>
          <div class="bar">
            {#if c}
              <span class="range" style:left="{pos(a, c.min)}%" style:width="{pos(a, c.max) - pos(a, c.min)}%"></span>
              <span class="mark center" style:left="{pos(a, c.center)}%"></span>
            {/if}
            <span class="mark now" style:left="{pos(a, raw(a))}%"></span>
          </div>
        {/each}
      </div>
      <div><button type="button" class="btn btn-sm btn-primary" onclick={save}>Сохранить</button></div>
    {/if}
  </div>
{:else}
  <p class="hint">Подключи геймпад, чтобы его откалибровать.</p>
{/if}

<style>
  .grow {
    flex: 1;
  }
  .bars {
    display: grid;
    grid-template-columns: 120px 1fr;
    align-items: center;
    gap: 6px 10px;
  }
  .bar {
    position: relative;
    height: 10px;
    border-radius: 5px;
    background: var(--surface-2);
    border: 1px solid var(--line);
  }
  .range {
    position: absolute;
    top: 0;
    bottom: 0;
    background: var(--accent-soft);
    border-radius: 5px;
  }
  .mark {
    position: absolute;
    top: -3px;
    bottom: -3px;
    width: 2px;
    margin-left: -1px;
  }
  .mark.center {
    background: var(--muted);
  }
  .mark.now {
    background: var(--accent);
  }
</style>
