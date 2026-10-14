<script lang="ts">
  // Схема геймпада: на каждой кнопке — что она делает прямо сейчас.
  // В редакторе (`onselect` задан) контролы кликабельны, выбранный подсвечен,
  // а назначения, унаследованные из базового слоя, приглушены.
  import type { ControlView, Input, PadFamily, Profile, SwitchKey } from "$lib/api";
  import { EXTRA_BUTTONS, sameControl, type ControlId } from "$lib/controls";
  import { roleLabel, sameInput, sameKey } from "$lib/describe";
  import { keyLabel } from "$lib/labels";

  let {
    controls,
    pressed,
    axes,
    family,
    profile,
    selected = null,
    onselect,
    focusLayer = null,
  }: {
    controls: ControlView[];
    pressed: SwitchKey[];
    axes: Partial<Record<string, number>>;
    family: PadFamily;
    profile: Profile | null;
    selected?: ControlId | null;
    onselect?: (c: ControlId) => void;
    /** Слой, который редактируется: назначения из других слоёв показываются приглушённо. */
    focusLayer?: string | null;
  } = $props();

  const W = 640;
  const H = 460;
  let width = $state(W);
  const scale = $derived(Math.min(1, width / W));
  const editable = $derived(!!onselect);
  const tag = $derived(editable ? "button" : "div");

  const btn = (button: string): Input => ({ kind: "button", button });

  function view(input: Input): ControlView | undefined {
    return controls.find((v) => sameInput(v.input, input));
  }

  function keyOf(input: Input): SwitchKey | null {
    if (input.kind === "button") return { kind: "button", button: input.button };
    if (input.kind === "axis_switch") return { kind: "axis_switch", axis: input.axis, direction: input.direction };
    return null;
  }

  function isPressed(input: Input): boolean {
    const k = keyOf(input);
    if (!k) return false;
    return pressed.some((p) => sameKey(p, k));
  }

  /** Состояние кнопки для стиля: звучит / модификатор включён / нажата / покой. */
  function stateOf(input: Input): "note" | "mod" | "pressed" | "idle" {
    const v = view(input);
    const down = isPressed(input);
    if (v?.role === "modifier" && (v.active || down)) return "mod";
    if (v?.role === "note" && v.on) return "note";
    if (v?.role === "cc" && (v.on || down)) return "mod";
    if (down) return v?.role === "note" ? "note" : "pressed";
    return "idle";
  }

  function label(input: Input): string {
    const v = view(input);
    return v ? roleLabel(v, profile) : "";
  }

  /** Назначение пришло не из редактируемого слоя. */
  function inherited(...inputs: Input[]): boolean {
    if (!focusLayer || focusLayer === "base") return false;
    const views = inputs.map(view).filter((v) => v !== undefined);
    return views.length > 0 && views.every((v) => v.layer !== focusLayer);
  }

  function pick(c: ControlId) {
    onselect?.(c);
  }

  // ---------- курки ----------
  function triggerInfo(axis: "left_trigger" | "right_trigger") {
    const sw: Input = { kind: "axis_switch", axis, direction: "positive", threshold: 0 };
    const ax: Input = { kind: "axis", axis };
    const main = view(sw) ? sw : ax;
    const id: ControlId = { kind: "trigger", axis };
    return {
      id,
      glyph: keyLabel({ kind: "axis_switch", axis, direction: "positive" }, family),
      label: label(main),
      state: stateOf(sw),
      value: axes[axis] ?? 0,
      inherited: inherited(sw, ax),
    };
  }
  const lt = $derived(triggerInfo("left_trigger"));
  const rt = $derived(triggerInfo("right_trigger"));

  // ---------- стики ----------
  function stickInfo(side: "left" | "right") {
    const x = `${side}_x`;
    const y = `${side}_y`;
    const parts: string[] = [];
    const lx = label({ kind: "axis", axis: x });
    const ly = label({ kind: "axis", axis: y });
    if (lx) parts.push(`↔ ${lx}`);
    if (ly) parts.push(`↕ ${ly}`);
    const switches: Input[] = [];
    for (const [axis, dir, arrow] of [
      [y, "positive", "↑"],
      [y, "negative", "↓"],
      [x, "negative", "←"],
      [x, "positive", "→"],
    ] as const) {
      const input: Input = { kind: "axis_switch", axis, direction: dir, threshold: 0 };
      switches.push(input);
      const l = label(input);
      if (l) parts.push(`${arrow} ${l}`);
    }
    const click = btn(`${side}_stick`);
    const id: ControlId = { kind: "stick", side };
    return {
      id,
      glyph: keyLabel({ kind: "button", button: `${side}_stick` }, family),
      move: parts.join(" · "),
      click: label(click),
      clickState: stateOf(click),
      x: axes[x] ?? 0,
      y: axes[y] ?? 0,
      inherited: inherited(click, { kind: "axis", axis: x }, { kind: "axis", axis: y }, ...switches),
    };
  }
  const ls = $derived(stickInfo("left"));
  const rs = $derived(stickInfo("right"));

  // ---------- крестовина и кнопки ----------
  const DPAD = [
    { button: "dpad_up", x: 56, y: 0 },
    { button: "dpad_left", x: 0, y: 56 },
    { button: "dpad_right", x: 112, y: 56 },
    { button: "dpad_down", x: 56, y: 112 },
  ];
  const FACE = [
    { button: "north", x: 62, y: 0 },
    { button: "west", x: 0, y: 62 },
    { button: "east", x: 124, y: 62 },
    { button: "south", x: 62, y: 124 },
  ];

  const FACE_COLORS: Record<PadFamily, Record<string, string>> = {
    xbox: { north: "var(--face-y)", west: "var(--face-x)", east: "var(--face-b)", south: "var(--face-a)" },
    generic: { north: "var(--face-y)", west: "var(--face-x)", east: "var(--face-b)", south: "var(--face-a)" },
    play_station: { north: "#5fd3a6", west: "#f08ac0", east: "#ff7b7b", south: "#7aa8ff" },
    nintendo: { north: "var(--muted)", west: "var(--muted)", east: "var(--muted)", south: "var(--muted)" },
  };

  // Кнопки вне схемы: при игре — только назначенные, в редакторе — все
  const extras = $derived(
    editable
      ? EXTRA_BUTTONS
      : EXTRA_BUTTONS.filter((b) => controls.some((v) => sameInput(v.input, btn(b)))),
  );

  const isSel = (c: ControlId) => sameControl(selected, c);
  const buttonId = (button: string): ControlId => ({ kind: "button", button });
</script>

{#snippet face(button: string, x: number, y: number, round: boolean, glyphColor?: string)}
  {@const input = btn(button)}
  {@const st = stateOf(input)}
  {@const v = view(input)}
  {@const glyph = keyLabel({ kind: "button", button }, family)}
  <svelte:element
    this={tag}
    type={editable ? "button" : undefined}
    role={editable ? "button" : undefined}
    class="cell {st}"
    class:round
    class:unbound={!v}
    class:selected={isSel(buttonId(button))}
    class:inherited={inherited(input)}
    style="left: {x}px; top: {y}px"
    title={v ? `${glyph}: ${label(input)}` : `${glyph}: не назначено`}
    onclick={() => pick(buttonId(button))}
  >
    <small style={glyphColor && st === "idle" ? `color: ${glyphColor}` : ""}>{glyph}</small>
    <b class:long={label(input).length > 4}>{label(input) || "·"}</b>
  </svelte:element>
{/snippet}

{#snippet bumper(button: string, side: "left" | "right")}
  {@const input = btn(button)}
  <svelte:element
    this={tag}
    type={editable ? "button" : undefined}
    role={editable ? "button" : undefined}
    class="bumper {stateOf(input)} {side}"
    class:selected={isSel(buttonId(button))}
    class:inherited={inherited(input)}
    onclick={() => pick(buttonId(button))}
  >
    <b>{keyLabel({ kind: "button", button }, family)}</b><span>{label(input) || "—"}</span>
  </svelte:element>
{/snippet}

{#snippet trigger(t: ReturnType<typeof triggerInfo>, side: "left" | "right")}
  <svelte:element
    this={tag}
    type={editable ? "button" : undefined}
    role={editable ? "button" : undefined}
    class="trigger {t.state} {side}"
    class:selected={isSel(t.id)}
    class:inherited={t.inherited}
    onclick={() => pick(t.id)}
  >
    <div class="fill" style="width: {Math.round(t.value * 100)}%"></div>
    <b>{t.glyph}</b><span>{t.label || "—"}</span>
  </svelte:element>
{/snippet}

{#snippet stick(s: ReturnType<typeof stickInfo>, left: number)}
  <svelte:element
    this={tag}
    type={editable ? "button" : undefined}
    role={editable ? "button" : undefined}
    class="stick {s.clickState}"
    class:selected={isSel(s.id)}
    class:inherited={s.inherited}
    style="left: {left}px"
    aria-label={s.glyph}
    onclick={() => pick(s.id)}
  >
    <span class="glyph">{s.glyph}</span>
    <div class="dot" style="left: {42 + s.x * 28 - 8}px; top: {42 - s.y * 28 - 8}px"></div>
  </svelte:element>
  <div class="stick-label" style="left: {left + 42 - 110}px">
    {#if s.move}<div>{s.move}</div>{/if}
    {#if s.click}<div class="click">нажатие — {s.click}</div>{/if}
  </div>
{/snippet}

<div class="wrap" bind:clientWidth={width} style="height: {H * scale}px">
  <div class="pad" style="width: {W}px; height: {H}px; transform: scale({scale})">
    {@render trigger(lt, "left")}
    {@render trigger(rt, "right")}
    {@render bumper("left_shoulder", "left")}
    {@render bumper("right_shoulder", "right")}
    <div class="body"></div>

    <div class="group" style="left: 92px; top: 128px">
      {#each DPAD as d (d.button)}
        {@render face(d.button, d.x, d.y, false)}
      {/each}
    </div>

    <div class="group" style="left: 374px; top: 116px">
      {#each FACE as f (f.button)}
        {@render face(f.button, f.x, f.y, true, FACE_COLORS[family][f.button])}
      {/each}
    </div>

    <div class="center">
      {#each ["back", "start"] as b (b)}
        {@const input = btn(b)}
        <svelte:element
          this={tag}
          type={editable ? "button" : undefined}
          role={editable ? "button" : undefined}
          class="small {stateOf(input)}"
          class:selected={isSel(buttonId(b))}
          class:inherited={inherited(input)}
          title={keyLabel({ kind: "button", button: b }, family)}
          onclick={() => pick(buttonId(b))}
        >
          <div class="pill"></div>
          <small>{label(input) || keyLabel({ kind: "button", button: b }, family)}</small>
        </svelte:element>
      {/each}
    </div>

    {@render stick(ls, 196)}
    {@render stick(rs, 360)}
  </div>
</div>

{#if extras.length}
  <div class="extras">
    {#each extras as b (b)}
      {@const input = btn(b)}
      <svelte:element
        this={tag}
        type={editable ? "button" : undefined}
        role={editable ? "button" : undefined}
        class="chip {stateOf(input)}"
        class:selected={isSel(buttonId(b))}
        class:inherited={inherited(input)}
        onclick={() => pick(buttonId(b))}
      >
        <b>{keyLabel({ kind: "button", button: b }, family)}</b>
        {label(input) || "—"}
      </svelte:element>
    {/each}
  </div>
{/if}

<style>
  .wrap {
    width: 100%;
    display: flex;
    justify-content: center;
  }
  .pad {
    position: relative;
    flex-shrink: 0;
    transform-origin: top center;
  }
  .body {
    position: absolute;
    left: 20px;
    top: 90px;
    width: 600px;
    height: 350px;
    border-radius: 150px 150px 190px 190px / 120px 120px 180px 180px;
    background: var(--raised);
    border: 1px solid var(--line-strong);
  }
  .group {
    position: absolute;
    width: 180px;
    height: 180px;
  }

  .cell {
    position: absolute;
    width: 56px;
    height: 56px;
    border-radius: 10px;
    background: var(--key);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    transition:
      background 60ms,
      box-shadow 60ms;
  }
  .cell.round {
    border-radius: 50%;
  }
  .cell small {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
  }
  .cell b {
    font-size: 17px;
  }
  .cell b.long {
    font-size: 10px;
    font-weight: 600;
    text-align: center;
    line-height: 1.1;
    padding: 0 3px;
  }
  .cell.unbound b {
    color: var(--muted);
  }

  .note {
    background: var(--accent);
    color: var(--bg);
    box-shadow: 0 0 0 6px color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .cell.note small {
    color: var(--bg);
  }
  .mod {
    background: var(--mod-soft);
    border-color: var(--mod) !important;
    box-shadow: inset 0 0 0 1px var(--mod);
  }
  .pressed {
    background: var(--line-strong);
  }

  .bumper,
  .trigger {
    position: absolute;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    border: 1px solid var(--line-strong);
    background: #1d1d26;
    font-size: 14px;
    white-space: nowrap;
  }
  .bumper {
    top: 46px;
    width: 144px;
    height: 34px;
    border-radius: 10px;
  }
  .bumper.left {
    left: 80px;
  }
  .bumper.right {
    right: 80px;
  }
  .bumper b,
  .trigger b {
    font-size: 13px;
    color: var(--muted);
  }
  .bumper.mod b,
  .trigger.mod b {
    color: var(--mod);
  }
  .trigger {
    top: 0;
    width: 124px;
    height: 38px;
    border-radius: 14px 14px 6px 6px;
    overflow: hidden;
  }
  .trigger.left {
    left: 90px;
  }
  .trigger.right {
    right: 90px;
  }
  .trigger .fill {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    background: var(--accent-soft);
  }
  .trigger b,
  .trigger span {
    position: relative;
  }

  .center {
    position: absolute;
    left: 236px;
    top: 150px;
    width: 168px;
    display: flex;
    justify-content: space-between;
  }
  .small {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 84px;
    background: none !important;
    box-shadow: none !important;
  }
  .small .pill {
    width: 30px;
    height: 18px;
    border-radius: 9px;
    background: var(--key);
  }
  .small.pressed .pill,
  .small.note .pill {
    background: var(--accent);
  }
  .small small {
    white-space: nowrap;
    font-size: 11px;
    color: var(--muted);
    text-align: center;
  }

  .stick {
    position: absolute;
    top: 312px;
    width: 84px;
    height: 84px;
    border-radius: 50%;
    background: var(--key);
    border: 1px solid var(--line-strong);
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
  }
  .stick .dot {
    position: absolute;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0.9;
  }
  .stick .glyph {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
    opacity: 0.6;
  }
  .stick.mod {
    border-color: var(--mod);
    background: var(--mod-soft);
  }
  .stick-label {
    position: absolute;
    top: 402px;
    width: 220px;
    text-align: center;
    font-size: 12px;
    line-height: 1.4;
    color: var(--muted);
  }

  .extras {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
    margin-top: 8px;
    font-size: 13px;
  }
  .extras b {
    color: var(--muted);
  }

  /* ---------- редактор ---------- */
  button.cell,
  button.bumper,
  button.trigger,
  button.stick,
  button.small,
  button.chip {
    font: inherit;
    color: inherit;
    padding: 0;
    cursor: pointer;
  }
  button.cell,
  button.small {
    border: none;
  }
  button.chip {
    padding: 4px 12px;
    background: transparent;
  }
  button:hover {
    filter: brightness(1.25);
  }
  .selected {
    outline: 2px solid var(--accent);
    outline-offset: 3px;
  }
  .inherited {
    opacity: 0.45;
  }
</style>
